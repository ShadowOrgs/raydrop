use super::{
    ingest_skill_package, PackageIngestInput, RawSkillFile, MAX_PACKAGE_FILES,
    MAX_PACKAGE_TOTAL_BYTES, MAX_SINGLE_FILE_BYTES,
};
use crate::db::storage::Database;
use crate::skills::models::SkillRecord;
use crate::skills::parser::validate_relative_path;
use crate::skills::storage::{get_skill_by_id, get_skill_files};
use std::io::{Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;

pub fn import_zip_archive(db: &Database, zip_path: &str) -> Result<SkillRecord, String> {
    let path = Path::new(zip_path);
    if !path.exists() || !path.is_file() {
        return Err(format!("ZIP archive does not exist: {}", zip_path));
    }

    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open ZIP: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Invalid ZIP archive: {}", e))?;

    if archive.len() > MAX_PACKAGE_FILES * 2 {
        return Err("ZIP archive contains too many entries".to_string());
    }

    let mut raw_entries: Vec<(String, Vec<u8>, bool)> = Vec::new();
    let mut total_bytes: u64 = 0;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Failed reading ZIP entry {}: {}", i, e))?;

        // Reject symlinks in ZIP
        if entry.unix_mode().map(|m| (m & 0o170000) == 0o120000).unwrap_or(false) {
            return Err(format!("Symlink rejected in ZIP entry: {}", entry.name()));
        }

        if entry.is_dir() {
            continue;
        }

        let clean = validate_relative_path(entry.name())?;
        let rel = clean.to_string_lossy().replace('\\', "/");
        if rel.starts_with("__MACOSX/") || rel.ends_with(".DS_Store") {
            continue;
        }

        if entry.size() > MAX_SINGLE_FILE_BYTES {
            return Err(format!(
                "ZIP entry '{}' exceeds max single file size ({} bytes)",
                rel, MAX_SINGLE_FILE_BYTES
            ));
        }

        let mut buf = Vec::new();
        let mut limited = (&mut entry).take(MAX_SINGLE_FILE_BYTES + 1);
        limited
            .read_to_end(&mut buf)
            .map_err(|e| format!("Failed extracting '{}': {}", rel, e))?;

        if buf.len() as u64 > MAX_SINGLE_FILE_BYTES {
            return Err(format!("ZIP entry '{}' exceeded decompressed size limit", rel));
        }

        total_bytes = total_bytes
            .checked_add(buf.len() as u64)
            .ok_or_else(|| "ZIP total size overflow".to_string())?;
        if total_bytes > MAX_PACKAGE_TOTAL_BYTES {
            return Err("ZIP archive exceeds maximum extracted size (25 MB)".to_string());
        }

        if raw_entries.len() >= MAX_PACKAGE_FILES {
            return Err("ZIP archive exceeds maximum file count".to_string());
        }

        let is_exec = entry.unix_mode().map(|m| (m & 0o111) != 0).unwrap_or(false);
        raw_entries.push((rel, buf, is_exec));
    }

    // Detect if all entries share a single wrapper folder containing SKILL.md
    let has_root_skill_md = raw_entries
        .iter()
        .any(|(r, _, _)| r.eq_ignore_ascii_case("SKILL.md"));

    let stripped_entries: Vec<RawSkillFile> = if has_root_skill_md {
        raw_entries
            .into_iter()
            .map(|(r, b, e)| RawSkillFile {
                relative_path: r,
                bytes: b,
                is_executable: e,
            })
            .collect()
    } else {
        // Find folder prefix ending with /SKILL.md
        let prefix = raw_entries
            .iter()
            .find_map(|(r, _, _)| {
                let lower = r.to_ascii_lowercase();
                if lower.ends_with("/skill.md") {
                    let idx = r.len() - "skill.md".len();
                    Some(r[..idx].to_string())
                } else {
                    None
                }
            })
            .ok_or_else(|| "SKILL.md not found in ZIP archive".to_string())?;

        raw_entries
            .into_iter()
            .filter_map(|(r, b, e)| {
                r.strip_prefix(&prefix).map(|stripped| RawSkillFile {
                    relative_path: stripped.to_string(),
                    bytes: b,
                    is_executable: e,
                })
            })
            .collect()
    };

    let fallback_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("zip-skill")
        .to_string();

    let source_url = format!("zip://{}", path.to_string_lossy().replace('\\', "/"));
    ingest_skill_package(
        db,
        PackageIngestInput {
            source_id: "src_zip".to_string(),
            source_name: "ZIP Archive Imports".to_string(),
            provider_type: "zip".to_string(),
            source_url,
            source_path: Some(zip_path.to_string()),
            repository_url: None,
            branch: None,
            commit_sha: None,
            license_hint: None,
            fallback_name,
            files: stripped_entries,
        },
    )
}

/// Packages a managed skill into a portable `.zip` archive ready for P2P sharing.
pub fn export_skill_to_zip(db: &Database, skill_id: &str) -> Result<String, String> {
    let skill = get_skill_by_id(db, skill_id)?
        .ok_or_else(|| format!("Skill not found: {}", skill_id))?;
    let files = get_skill_files(db, skill_id)?;
    if files.is_empty() {
        return Err("Skill package has no local files yet. Fetch or import it first before sharing.".to_string());
    }

    let shared_dir = db.data_dir().join("skills_shared");
    std::fs::create_dir_all(&shared_dir)
        .map_err(|e| format!("Failed creating skills_shared dir: {}", e))?;

    let zip_path = shared_dir.join(format!("{}-skill.zip", skill.slug));
    let file = std::fs::File::create(&zip_path)
        .map_err(|e| format!("Failed creating ZIP file: {}", e))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for f in &files {
        let clean_rel = validate_relative_path(&f.relative_path)?;
        let entry_name = format!(
            "{}/{}",
            skill.slug,
            clean_rel.to_string_lossy().replace('\\', "/")
        );
        let bytes = std::fs::read(&f.storage_path)
            .map_err(|e| format!("Failed reading '{}': {}", f.relative_path, e))?;
        writer
            .start_file(&entry_name, options)
            .map_err(|e| format!("Failed starting ZIP entry '{}': {}", entry_name, e))?;
        writer
            .write_all(&bytes)
            .map_err(|e| format!("Failed writing ZIP entry '{}': {}", entry_name, e))?;
    }

    writer
        .finish()
        .map_err(|e| format!("Failed finalizing ZIP archive: {}", e))?;

    Ok(zip_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use crate::skills::parser::validate_relative_path;

    #[test]
    fn test_zip_slip_path_rejected() {
        assert!(validate_relative_path("../../etc/passwd").is_err());
        assert!(validate_relative_path("foo/../../../bar").is_err());
    }
}
