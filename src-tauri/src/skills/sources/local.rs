use super::{ingest_skill_package, PackageIngestInput, RawSkillFile};
use crate::db::storage::Database;
use crate::skills::models::SkillRecord;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn import_local_folder(db: &Database, folder_path: &str) -> Result<SkillRecord, String> {
    let root = PathBuf::from(folder_path);
    if !root.exists() || !root.is_dir() {
        return Err(format!("Selected folder does not exist: {}", folder_path));
    }

    let canonical_root = dunce::canonicalize(&root)
        .map_err(|e| format!("Failed to resolve folder path: {}", e))?;

    // Check if SKILL.md is directly at root, or inside a single child directory
    let skill_root = if canonical_root.join("SKILL.md").is_file() {
        canonical_root.clone()
    } else {
        let mut found_sub: Option<PathBuf> = None;
        for entry in std::fs::read_dir(&canonical_root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let p = entry.path();
            if p.is_dir() && p.join("SKILL.md").is_file() {
                found_sub = Some(p);
                break;
            }
        }
        found_sub.ok_or_else(|| {
            "No SKILL.md found in selected folder or its immediate subfolders".to_string()
        })?
    };

    let mut files = Vec::new();
    for entry in WalkDir::new(&skill_root).follow_links(false) {
        let entry = entry.map_err(|e| format!("Failed walking folder: {}", e))?;
        let ft = entry.file_type();
        if ft.is_symlink() {
            return Err(format!(
                "Symlink entries are not allowed in MVP skill packages: {}",
                entry.path().display()
            ));
        }
        if !ft.is_file() {
            continue;
        }
        let path = entry.path();
        // Skip .git or node_modules noise
        let rel = path
            .strip_prefix(&skill_root)
            .map_err(|e| e.to_string())?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str.starts_with(".git/") || rel_str.starts_with("node_modules/") {
            continue;
        }

        let bytes = std::fs::read(path)
            .map_err(|e| format!("Failed to read '{}': {}", rel_str, e))?;
        files.push(RawSkillFile {
            relative_path: rel_str,
            bytes,
            is_executable: is_exec_file(path),
        });
    }

    let fallback_name = skill_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("local-skill")
        .to_string();

    let source_url = format!("local://{}", skill_root.to_string_lossy().replace('\\', "/"));
    ingest_skill_package(
        db,
        PackageIngestInput {
            source_id: "src_local".to_string(),
            source_name: "Local Folder Imports".to_string(),
            provider_type: "local".to_string(),
            source_url,
            source_path: Some(skill_root.to_string_lossy().to_string()),
            repository_url: None,
            branch: None,
            commit_sha: None,
            license_hint: None,
            fallback_name,
            files,
        },
    )
}

fn is_exec_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            return (meta.permissions().mode() & 0o111) != 0;
        }
    }
    let _ = path;
    false
}
