pub mod catalog;
pub mod github;
pub mod local;
pub mod zip;

use crate::db::storage::{now_unix_secs, Database};
use crate::skills::models::{SkillFileRecord, SkillRecord, SkillSourceRecord};
use crate::skills::parser::{
    classify_file_type, parse_skill_md, sha256_bytes, slugify_skill_name, stable_skill_id,
    validate_relative_path,
};
use crate::skills::security::{scan_skill_package, ScanInputFile};
use crate::skills::storage::{managed_library_dir, upsert_skill_with_files, upsert_source};
use std::path::Path;

pub const MAX_PACKAGE_FILES: usize = 300;
pub const MAX_PACKAGE_TOTAL_BYTES: u64 = 25 * 1024 * 1024; // 25 MB
pub const MAX_SINGLE_FILE_BYTES: u64 = 5 * 1024 * 1024; // 5 MB

pub struct RawSkillFile {
    pub relative_path: String,
    pub bytes: Vec<u8>,
    pub is_executable: bool,
}

pub struct PackageIngestInput {
    pub source_id: String,
    pub source_name: String,
    pub provider_type: String,
    pub source_url: String,
    pub source_path: Option<String>,
    pub repository_url: Option<String>,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub license_hint: Option<String>,
    pub fallback_name: String,
    pub files: Vec<RawSkillFile>,
}

/// Validates, scans, copies into managed local library, and persists a skill package.
pub fn ingest_skill_package(db: &Database, input: PackageIngestInput) -> Result<SkillRecord, String> {
    if input.files.is_empty() {
        return Err("Skill package contains no files".to_string());
    }
    if input.files.len() > MAX_PACKAGE_FILES {
        return Err(format!(
            "Skill package exceeds maximum file count ({} > {})",
            input.files.len(),
            MAX_PACKAGE_FILES
        ));
    }

    let mut total_bytes: u64 = 0;
    let mut validated: Vec<(String, Vec<u8>, String, bool)> = Vec::new();
    let mut skill_md_text: Option<String> = None;

    for rf in input.files {
        let size = rf.bytes.len() as u64;
        if size > MAX_SINGLE_FILE_BYTES {
            return Err(format!(
                "File '{}' exceeds max single file size ({} bytes)",
                rf.relative_path, MAX_SINGLE_FILE_BYTES
            ));
        }
        total_bytes = total_bytes
            .checked_add(size)
            .ok_or_else(|| "Package size overflow".to_string())?;
        if total_bytes > MAX_PACKAGE_TOTAL_BYTES {
            return Err(format!(
                "Skill package exceeds max total size ({} bytes)",
                MAX_PACKAGE_TOTAL_BYTES
            ));
        }

        let clean_rel = validate_relative_path(&rf.relative_path)?;
        let rel_str = clean_rel.to_string_lossy().replace('\\', "/");
        let ftype = classify_file_type(&clean_rel, &rf.bytes);

        if rel_str.eq_ignore_ascii_case("SKILL.md") {
            skill_md_text = Some(
                std::str::from_utf8(&rf.bytes)
                    .map_err(|_| "SKILL.md must be valid UTF-8".to_string())?
                    .to_string(),
            );
        }

        validated.push((rel_str, rf.bytes, ftype, rf.is_executable));
    }

    let md_content = skill_md_text
        .ok_or_else(|| "SKILL.md is required at the root of the skill package".to_string())?;
    let parsed = parse_skill_md(&md_content, &input.fallback_name)?;
    let slug = slugify_skill_name(&parsed.name);
    let skill_id = stable_skill_id(&input.source_url, &slug);
    let now = now_unix_secs();

    // Ensure source exists
    upsert_source(
        db,
        &SkillSourceRecord {
            id: input.source_id.clone(),
            name: input.source_name.clone(),
            provider_type: input.provider_type.clone(),
            source_url: input.source_url.clone(),
            enabled: true,
            last_synced_at: Some(now),
            last_error: None,
            created_at: now,
            updated_at: now,
        },
    )?;

    // Static security scan (never executes anything)
    let scan_inputs: Vec<ScanInputFile<'_>> = validated
        .iter()
        .map(|(rel, bytes, ftype, exec)| ScanInputFile {
            relative_path: rel.as_str(),
            file_type: ftype.as_str(),
            bytes: bytes.as_slice(),
            is_executable_mode: *exec,
        })
        .collect();
    let security_report = scan_skill_package(&scan_inputs, now);

    // Stage into managed library directory
    let lib_root = managed_library_dir(db)?;
    let pkg_dir = lib_root.join(&skill_id);
    if pkg_dir.exists() {
        std::fs::remove_dir_all(&pkg_dir)
            .map_err(|e| format!("Failed to clean existing managed package dir: {}", e))?;
    }
    std::fs::create_dir_all(&pkg_dir)
        .map_err(|e| format!("Failed to create managed package dir: {}", e))?;

    let mut file_records = Vec::with_capacity(validated.len());
    for (rel_str, bytes, ftype, _) in validated {
        let dest_path = pkg_dir.join(Path::new(&rel_str));
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent dir: {}", e))?;
        }
        std::fs::write(&dest_path, &bytes)
            .map_err(|e| format!("Failed to write managed file '{}': {}", rel_str, e))?;

        let hash = sha256_bytes(&bytes);
        file_records.push(SkillFileRecord {
            id: format!("{}_{}", skill_id, &hash[..12]),
            skill_id: skill_id.clone(),
            relative_path: rel_str,
            content_hash: hash,
            size: bytes.len() as u64,
            file_type: ftype,
            storage_path: dest_path.to_string_lossy().to_string(),
        });
    }

    let skill = SkillRecord {
        id: skill_id,
        source_id: input.source_id,
        name: parsed.name,
        slug,
        description: parsed.description,
        author: parsed.author,
        license: parsed.license.or(input.license_hint),
        repository_url: input.repository_url,
        source_url: input.source_url,
        source_path: input.source_path,
        version: parsed.version,
        branch: input.branch,
        commit_sha: input.commit_sha,
        categories: parsed.categories,
        tags: parsed.tags,
        compatibility: Vec::new(),
        security_status: security_report.status.clone(),
        security_report,
        popularity: None,
        local_package_path: Some(pkg_dir.to_string_lossy().to_string()),
        last_updated: Some(now),
        created_at: now,
        updated_at: now,
        is_favorite: false,
        installed_agents: Vec::new(),
    };

    upsert_skill_with_files(db, &skill, &file_records)?;
    Ok(skill)
}
