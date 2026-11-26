use super::compatibility::resolve_install_destination;
use super::detector::detect_all_agents;
use super::models::{
    CompatibilityLevel, InstalledManifestEntry, SecuritySeverity, SkillInstallHistoryRecord,
    SkillInstallPreviewResponse, SkillInstallPreviewTarget, SkillInstallTargetResult,
    SkillInstallationRecord, SkillUpdateCheckItem,
};
use super::parser::{parse_skill_md, sha256_bytes, validate_relative_path};
use super::storage::{
    delete_installation, get_skill_by_id, get_skill_files, get_skills_settings,
    insert_install_history, list_agents, list_installations, managed_backups_dir,
    upsert_installation,
};
use crate::db::storage::{now_unix_secs, Database};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn preview_install(
    db: &Database,
    skill_id: &str,
    agent_ids: &[String],
    scope: &str,
    project_path: Option<&str>,
) -> Result<SkillInstallPreviewResponse, String> {
    let skill = get_skill_by_id(db, skill_id)?
        .ok_or_else(|| format!("Skill not found: {}", skill_id))?;
    let files = get_skill_files(db, skill_id)?;
    let mut agents = list_agents(db)?;
    if agents.is_empty() {
        agents = detect_all_agents(db)?;
    }

    let source_hashes: HashMap<String, String> = files
        .iter()
        .map(|f| (f.relative_path.clone(), f.content_hash.clone()))
        .collect();

    let mut targets = Vec::new();
    for aid in agent_ids {
        let Some(agent) = agents.iter().find(|a| &a.id == aid) else {
            targets.push(SkillInstallPreviewTarget {
                agent_id: aid.clone(),
                agent_name: aid.clone(),
                destination_path: String::new(),
                compatibility: CompatibilityLevel::Unsupported,
                conflict_exists: false,
                conflict_same_hash: false,
                existing_files_count: 0,
                allowed: false,
                block_reason: Some("Agent not found".to_string()),
            });
            continue;
        };

        if agent.compatibility_level == CompatibilityLevel::Unsupported {
            targets.push(SkillInstallPreviewTarget {
                agent_id: agent.id.clone(),
                agent_name: agent.display_name.clone(),
                destination_path: String::new(),
                compatibility: CompatibilityLevel::Unsupported,
                conflict_exists: false,
                conflict_same_hash: false,
                existing_files_count: 0,
                allowed: false,
                block_reason: Some("Agent is marked Unsupported for this skill".to_string()),
            });
            continue;
        }

        match resolve_install_destination(agent, scope, project_path, &skill.slug) {
            Ok(dest) => {
                let conflict_exists = dest.exists();
                let mut existing_files_count = 0usize;
                let mut all_match = conflict_exists;
                if conflict_exists {
                    let mut existing_map = HashMap::new();
                    for entry in WalkDir::new(&dest).follow_links(false).into_iter().flatten() {
                        if entry.file_type().is_file() {
                            existing_files_count += 1;
                            if let Ok(rel) = entry.path().strip_prefix(&dest) {
                                let rel_s = rel.to_string_lossy().replace('\\', "/");
                                if let Ok(bytes) = std::fs::read(entry.path()) {
                                    existing_map.insert(rel_s, sha256_bytes(&bytes));
                                }
                            }
                        }
                    }
                    if existing_map.len() != source_hashes.len() {
                        all_match = false;
                    } else {
                        for (k, v) in &source_hashes {
                            if existing_map.get(k) != Some(v) {
                                all_match = false;
                                break;
                            }
                        }
                    }
                }
                targets.push(SkillInstallPreviewTarget {
                    agent_id: agent.id.clone(),
                    agent_name: agent.display_name.clone(),
                    destination_path: dest.to_string_lossy().to_string(),
                    compatibility: agent.compatibility_level.clone(),
                    conflict_exists,
                    conflict_same_hash: all_match,
                    existing_files_count,
                    allowed: true,
                    block_reason: None,
                });
            }
            Err(err) => {
                targets.push(SkillInstallPreviewTarget {
                    agent_id: agent.id.clone(),
                    agent_name: agent.display_name.clone(),
                    destination_path: String::new(),
                    compatibility: agent.compatibility_level.clone(),
                    conflict_exists: false,
                    conflict_same_hash: false,
                    existing_files_count: 0,
                    allowed: false,
                    block_reason: Some(err),
                });
            }
        }
    }

    Ok(SkillInstallPreviewResponse {
        skill_id: skill.id,
        skill_name: skill.name,
        security_report: skill.security_report,
        targets,
    })
}

/// Installs a skill into one or multiple agents with staging, backup, and hash verification.
/// `conflict_policy`: "backup_and_replace" | "update" | "keep_existing"
pub fn install_skill_to_agents(
    db: &Database,
    skill_id: &str,
    agent_ids: &[String],
    scope: &str,
    project_path: Option<&str>,
    conflict_policy: &str,
    confirm_high_risk: bool,
) -> Result<Vec<SkillInstallTargetResult>, String> {
    let skill = get_skill_by_id(db, skill_id)?
        .ok_or_else(|| format!("Skill not found: {}", skill_id))?;
    let files = get_skill_files(db, skill_id)?;
    if files.is_empty() {
        return Err("Skill package has no stored files; import or download it first".to_string());
    }

    let settings = get_skills_settings(db)?;
    let has_critical = skill.security_report.highest_severity == Some(SecuritySeverity::Critical);
    let has_high = skill.security_report.highest_severity == Some(SecuritySeverity::High);
    if settings.block_critical_by_default && has_critical {
        return Err("Blocked by policy: skill has Critical security findings".to_string());
    }
    if settings.require_confirm_high_risk && (has_critical || has_high) && !confirm_high_risk {
        return Err("Confirmation required: skill has High/Critical security findings".to_string());
    }

    let mut agents = list_agents(db)?;
    if agents.is_empty() {
        agents = detect_all_agents(db)?;
    }

    let mut results = Vec::new();
    let existing_installs = list_installations(db)?;
    let now = now_unix_secs();

    for aid in agent_ids {
        let Some(agent) = agents.iter().find(|a| &a.id == aid) else {
            results.push(SkillInstallTargetResult {
                agent_id: aid.clone(),
                agent_name: aid.clone(),
                destination_path: String::new(),
                success: false,
                action_taken: "skipped".to_string(),
                installation_id: None,
                error: Some("Unknown agent".to_string()),
            });
            continue;
        };

        if agent.compatibility_level == CompatibilityLevel::Unsupported {
            results.push(SkillInstallTargetResult {
                agent_id: agent.id.clone(),
                agent_name: agent.display_name.clone(),
                destination_path: String::new(),
                success: false,
                action_taken: "blocked".to_string(),
                installation_id: None,
                error: Some("Unsupported agent compatibility".to_string()),
            });
            continue;
        }

        let dest = match resolve_install_destination(agent, scope, project_path, &skill.slug) {
            Ok(p) => p,
            Err(e) => {
                results.push(SkillInstallTargetResult {
                    agent_id: agent.id.clone(),
                    agent_name: agent.display_name.clone(),
                    destination_path: String::new(),
                    success: false,
                    action_taken: "failed".to_string(),
                    installation_id: None,
                    error: Some(e),
                });
                continue;
            }
        };

        if dest.exists() && conflict_policy == "keep_existing" {
            results.push(SkillInstallTargetResult {
                agent_id: agent.id.clone(),
                agent_name: agent.display_name.clone(),
                destination_path: dest.to_string_lossy().to_string(),
                success: true,
                action_taken: "kept_existing".to_string(),
                installation_id: None,
                error: None,
            });
            continue;
        }

        let parent_dir = dest
            .parent()
            .ok_or_else(|| "Invalid destination parent directory".to_string())?;
        if let Err(e) = std::fs::create_dir_all(parent_dir) {
            results.push(SkillInstallTargetResult {
                agent_id: agent.id.clone(),
                agent_name: agent.display_name.clone(),
                destination_path: dest.to_string_lossy().to_string(),
                success: false,
                action_taken: "failed".to_string(),
                installation_id: None,
                error: Some(format!("Failed creating skills directory: {}", e)),
            });
            continue;
        }

        // Stage package in sibling temporary directory
        let stage_dir = parent_dir.join(format!(".raydrop-stage-{}-{}", skill.slug, uuid::Uuid::new_v4()));
        let stage_res = (|| -> Result<Vec<InstalledManifestEntry>, String> {
            std::fs::create_dir_all(&stage_dir).map_err(|e| e.to_string())?;
            let mut manifest = Vec::with_capacity(files.len());
            for f in &files {
                let clean_rel = validate_relative_path(&f.relative_path)?;
                let target_file = stage_dir.join(&clean_rel);
                if let Some(p) = target_file.parent() {
                    std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
                }
                let bytes = std::fs::read(&f.storage_path).map_err(|e| {
                    format!("Failed reading managed package file '{}': {}", f.relative_path, e)
                })?;
                let actual_hash = sha256_bytes(&bytes);
                if actual_hash != f.content_hash {
                    return Err(format!(
                        "Integrity check failed for '{}' in managed library",
                        f.relative_path
                    ));
                }
                std::fs::write(&target_file, &bytes).map_err(|e| e.to_string())?;
                manifest.push(InstalledManifestEntry {
                    relative_path: f.relative_path.clone(),
                    content_hash: actual_hash,
                    size: bytes.len() as u64,
                });
            }
            Ok(manifest)
        })();

        let manifest = match stage_res {
            Ok(m) => m,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&stage_dir);
                results.push(SkillInstallTargetResult {
                    agent_id: agent.id.clone(),
                    agent_name: agent.display_name.clone(),
                    destination_path: dest.to_string_lossy().to_string(),
                    success: false,
                    action_taken: "failed".to_string(),
                    installation_id: None,
                    error: Some(e),
                });
                continue;
            }
        };

        // Backup existing destination if present
        let mut backup_path_str: Option<String> = None;
        let action_name = if dest.exists() {
            match create_backup_of_dir(db, &skill.slug, &agent.id, &dest, settings.backup_retention_count) {
                Ok(bpath) => {
                    backup_path_str = Some(bpath.to_string_lossy().to_string());
                }
                Err(e) => {
                    let _ = std::fs::remove_dir_all(&stage_dir);
                    results.push(SkillInstallTargetResult {
                        agent_id: agent.id.clone(),
                        agent_name: agent.display_name.clone(),
                        destination_path: dest.to_string_lossy().to_string(),
                        success: false,
                        action_taken: "failed".to_string(),
                        installation_id: None,
                        error: Some(format!("Backup failed before replace: {}", e)),
                    });
                    continue;
                }
            }
            if let Err(e) = std::fs::remove_dir_all(&dest) {
                let _ = std::fs::remove_dir_all(&stage_dir);
                results.push(SkillInstallTargetResult {
                    agent_id: agent.id.clone(),
                    agent_name: agent.display_name.clone(),
                    destination_path: dest.to_string_lossy().to_string(),
                    success: false,
                    action_taken: "failed".to_string(),
                    installation_id: None,
                    error: Some(format!("Failed removing old skill directory: {}", e)),
                });
                continue;
            }
            if conflict_policy == "update" {
                "update"
            } else {
                "replace"
            }
        } else {
            "install"
        };

        if let Err(e) = std::fs::rename(&stage_dir, &dest) {
            // Fallback copy if rename fails across mountpoints/AV locks
            if let Err(copy_err) = copy_dir_recursive(&stage_dir, &dest) {
                let _ = std::fs::remove_dir_all(&stage_dir);
                results.push(SkillInstallTargetResult {
                    agent_id: agent.id.clone(),
                    agent_name: agent.display_name.clone(),
                    destination_path: dest.to_string_lossy().to_string(),
                    success: false,
                    action_taken: "failed".to_string(),
                    installation_id: None,
                    error: Some(format!("Failed moving staged files: {} / {}", e, copy_err)),
                });
                continue;
            }
            let _ = std::fs::remove_dir_all(&stage_dir);
        }

        let prev_install = existing_installs.iter().find(|i| {
            i.skill_id == skill.id
                && i.agent_id == agent.id
                && i.scope == scope
                && i.project_path.as_deref() == project_path
        });
        let inst_id = prev_install
            .map(|i| i.id.clone())
            .unwrap_or_else(|| format!("inst_{}", uuid::Uuid::new_v4().simple()));

        let inst_record = SkillInstallationRecord {
            id: inst_id.clone(),
            skill_id: skill.id.clone(),
            skill_name: skill.name.clone(),
            agent_id: agent.id.clone(),
            scope: scope.to_string(),
            project_path: project_path.map(|s| s.to_string()),
            install_path: dest.to_string_lossy().to_string(),
            installed_version: skill.version.clone(),
            installed_commit_sha: skill.commit_sha.clone(),
            installed_manifest: manifest,
            installed_at: prev_install.map(|i| i.installed_at).unwrap_or(now),
            updated_at: now,
            status: "installed".to_string(),
        };
        upsert_installation(db, &inst_record)?;

        let _ = insert_install_history(
            db,
            &SkillInstallHistoryRecord {
                id: format!("hist_{}", uuid::Uuid::new_v4().simple()),
                installation_id: inst_id.clone(),
                skill_id: skill.id.clone(),
                skill_name: skill.name.clone(),
                agent_id: agent.id.clone(),
                action: action_name.to_string(),
                previous_version: prev_install.and_then(|p| p.installed_version.clone()),
                new_version: skill.version.clone(),
                backup_path: backup_path_str,
                timestamp: now,
                result: "success".to_string(),
                error_message: None,
            },
        );

        results.push(SkillInstallTargetResult {
            agent_id: agent.id.clone(),
            agent_name: agent.display_name.clone(),
            destination_path: dest.to_string_lossy().to_string(),
            success: true,
            action_taken: action_name.to_string(),
            installation_id: Some(inst_id),
            error: None,
        });
    }

    Ok(results)
}

pub fn uninstall_skill(
    db: &Database,
    installation_id: &str,
    force_modified: bool,
) -> Result<Vec<String>, String> {
    let installs = list_installations(db)?;
    let inst = installs
        .into_iter()
        .find(|i| i.id == installation_id)
        .ok_or_else(|| format!("Installation not found: {}", installation_id))?;

    let dest = PathBuf::from(&inst.install_path);
    let mut modified_files = Vec::new();

    if dest.exists() {
        for entry in &inst.installed_manifest {
            if let Ok(rel) = validate_relative_path(&entry.relative_path) {
                let fp = dest.join(rel);
                if fp.exists() {
                    if let Ok(bytes) = std::fs::read(&fp) {
                        if sha256_bytes(&bytes) != entry.content_hash {
                            modified_files.push(entry.relative_path.clone());
                        }
                    }
                }
            }
        }

        if !modified_files.is_empty() && !force_modified {
            return Err(format!(
                "MODIFIED_FILES: {}",
                modified_files.join(", ")
            ));
        }

        // Only remove files owned by the recorded manifest
        for entry in &inst.installed_manifest {
            if let Ok(rel) = validate_relative_path(&entry.relative_path) {
                let fp = dest.join(rel);
                if fp.is_file() {
                    let _ = std::fs::remove_file(&fp);
                }
            }
        }
        // Remove empty subdirectories and dest only if now empty
        remove_empty_dirs_bottom_up(&dest);
    }

    delete_installation(db, installation_id)?;
    let _ = insert_install_history(
        db,
        &SkillInstallHistoryRecord {
            id: format!("hist_{}", uuid::Uuid::new_v4().simple()),
            installation_id: inst.id,
            skill_id: inst.skill_id,
            skill_name: inst.skill_name,
            agent_id: inst.agent_id,
            action: "uninstall".to_string(),
            previous_version: inst.installed_version,
            new_version: None,
            backup_path: None,
            timestamp: now_unix_secs(),
            result: "success".to_string(),
            error_message: None,
        },
    );

    Ok(modified_files)
}

pub fn rollback_installation(db: &Database, history_id: &str) -> Result<String, String> {
    let history = super::storage::list_install_history(db)?;
    let entry = history
        .into_iter()
        .find(|h| h.id == history_id)
        .ok_or_else(|| format!("History entry not found: {}", history_id))?;
    let backup_str = entry
        .backup_path
        .ok_or_else(|| "No backup associated with this history entry".to_string())?;
    let backup_dir = PathBuf::from(&backup_str);
    if !backup_dir.exists() || !backup_dir.is_dir() {
        return Err(format!("Backup directory no longer exists: {}", backup_str));
    }

    let installs = list_installations(db)?;
    let inst = installs
        .into_iter()
        .find(|i| i.id == entry.installation_id)
        .ok_or_else(|| "Installation record not found for rollback".to_string())?;

    let dest = PathBuf::from(&inst.install_path);
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    copy_dir_recursive(&backup_dir, &dest)?;

    let mut updated_inst = inst.clone();
    updated_inst.installed_version = entry.previous_version.clone();
    updated_inst.updated_at = now_unix_secs();
    updated_inst.status = "installed".to_string();
    upsert_installation(db, &updated_inst)?;

    let _ = insert_install_history(
        db,
        &SkillInstallHistoryRecord {
            id: format!("hist_{}", uuid::Uuid::new_v4().simple()),
            installation_id: inst.id,
            skill_id: inst.skill_id,
            skill_name: inst.skill_name,
            agent_id: inst.agent_id,
            action: "rollback".to_string(),
            previous_version: inst.installed_version,
            new_version: entry.previous_version,
            backup_path: Some(backup_str),
            timestamp: now_unix_secs(),
            result: "success".to_string(),
            error_message: None,
        },
    );

    Ok(dest.to_string_lossy().to_string())
}

pub fn get_scanned_projects(db: &Database) -> Vec<String> {
    db.get_setting("skills_scanned_projects")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

pub fn add_scanned_project(db: &Database, project_path: &str) -> Result<Vec<String>, String> {
    let p = Path::new(project_path.trim());
    if !p.exists() || !p.is_dir() {
        return Err(format!("Project directory does not exist: {}", project_path));
    }
    let canon = dunce::canonicalize(p)
        .map_err(|e| format!("Failed resolving project path: {}", e))?
        .to_string_lossy()
        .to_string();
    let mut list = get_scanned_projects(db);
    if !list.iter().any(|x| x.eq_ignore_ascii_case(&canon)) {
        list.push(canon);
        let json = serde_json::to_string(&list).map_err(|e| e.to_string())?;
        db.set_setting("skills_scanned_projects", &json)?;
    }
    Ok(list)
}

pub fn remove_scanned_project(db: &Database, project_path: &str) -> Result<Vec<String>, String> {
    let mut list = get_scanned_projects(db);
    list.retain(|x| !x.eq_ignore_ascii_case(project_path));
    let json = serde_json::to_string(&list).map_err(|e| e.to_string())?;
    db.set_setting("skills_scanned_projects", &json)?;
    Ok(list)
}

/// Scans agent global skill folders and saved project folders for pre-existing skills on disk.
pub fn scan_existing_skills_on_disk(db: &Database) -> Result<(), String> {
    let mut agents = list_agents(db)?;
    if agents.is_empty() {
        agents = detect_all_agents(db)?;
    }
    let existing = list_installations(db)?;
    let now = now_unix_secs();

    let mut scan_targets: Vec<(String, String, Option<String>, PathBuf)> = Vec::new();
    for agent in &agents {
        let global_dir = agent
            .configured_path
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&agent.global_skill_path);
        if !global_dir.trim().is_empty() {
            scan_targets.push((
                agent.id.clone(),
                "global".to_string(),
                None,
                PathBuf::from(global_dir),
            ));
        }
    }

    for proj in get_scanned_projects(db) {
        let proj_root = PathBuf::from(&proj);
        if !proj_root.is_dir() {
            continue;
        }
        for agent in &agents {
            let p_skills = proj_root.join(&agent.project_skill_path);
            scan_targets.push((
                agent.id.clone(),
                "project".to_string(),
                Some(proj.clone()),
                p_skills,
            ));
        }
    }

    for (agent_id, scope, project_path, skills_dir) in scan_targets {
        if !skills_dir.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&skills_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let skill_dir = entry.path();
            if !skill_dir.is_dir() {
                continue;
            }
            let dir_name = skill_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if dir_name.is_empty() || dir_name.starts_with('.') {
                continue;
            }
            // Only treat as a skill if it has SKILL.md (case-insensitive)
            let skill_md_path = ["SKILL.md", "skill.md"]
                .iter()
                .map(|f| skill_dir.join(f))
                .find(|p| p.is_file());
            let Some(md_path) = skill_md_path else {
                continue;
            };

            let canon_install = dunce::canonicalize(&skill_dir)
                .unwrap_or_else(|_| skill_dir.clone())
                .to_string_lossy()
                .to_string();

            let already_tracked = existing.iter().any(|i| {
                i.install_path.eq_ignore_ascii_case(&canon_install) && i.agent_id == agent_id
            });
            if already_tracked {
                continue;
            }

            let md_content = std::fs::read_to_string(&md_path).unwrap_or_default();
            let parsed = parse_skill_md(&md_content, &dir_name).ok();
            let skill_name = parsed
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| dir_name.clone());
            let version = parsed.as_ref().and_then(|p| p.version.clone());

            let mut manifest = Vec::new();
            for wf in WalkDir::new(&skill_dir).follow_links(false).into_iter().flatten() {
                if !wf.file_type().is_file() {
                    continue;
                }
                if let Ok(rel) = wf.path().strip_prefix(&skill_dir) {
                    let rel_s = rel.to_string_lossy().replace('\\', "/");
                    if let Ok(bytes) = std::fs::read(wf.path()) {
                        manifest.push(InstalledManifestEntry {
                            relative_path: rel_s,
                            content_hash: sha256_bytes(&bytes),
                            size: bytes.len() as u64,
                        });
                    }
                }
            }

            let path_hash = sha256_bytes(format!("{}::{}", agent_id, canon_install).as_bytes());
            let rec = SkillInstallationRecord {
                id: format!("ext_{}", &path_hash[..16]),
                skill_id: format!("ext_{}", &path_hash[..16]),
                skill_name,
                agent_id: agent_id.clone(),
                scope: scope.clone(),
                project_path: project_path.clone(),
                install_path: canon_install,
                installed_version: version,
                installed_commit_sha: None,
                installed_manifest: manifest,
                installed_at: now,
                updated_at: now,
                status: "external".to_string(),
            };
            let _ = upsert_installation(db, &rec);
        }
    }

    Ok(())
}

pub fn reconcile_installations(db: &Database) -> Result<Vec<SkillInstallationRecord>, String> {
    let _ = scan_existing_skills_on_disk(db);
    let installs = list_installations(db)?;
    let mut updated = Vec::new();
    for mut inst in installs {
        let dest = Path::new(&inst.install_path);
        let new_status = if !dest.exists() {
            "missing".to_string()
        } else {
            let mut modified = false;
            for entry in &inst.installed_manifest {
                if let Ok(rel) = validate_relative_path(&entry.relative_path) {
                    let fp = dest.join(rel);
                    match std::fs::read(&fp) {
                        Ok(bytes) => {
                            if sha256_bytes(&bytes) != entry.content_hash {
                                modified = true;
                                break;
                            }
                        }
                        Err(_) => {
                            modified = true;
                            break;
                        }
                    }
                }
            }
            if modified {
                "modified".to_string()
            } else if inst.status == "external" {
                "external".to_string()
            } else {
                "installed".to_string()
            }
        };
        if inst.status != new_status {
            inst.status = new_status;
            upsert_installation(db, &inst)?;
        }
        updated.push(inst);
    }
    Ok(updated)
}

pub fn check_updates_for_installed(db: &Database) -> Result<Vec<SkillUpdateCheckItem>, String> {
    let installs = list_installations(db)?;
    let mut out = Vec::new();
    for inst in installs {
        let Some(skill) = get_skill_by_id(db, &inst.skill_id)? else {
            continue;
        };
        let pkg_files = get_skill_files(db, &inst.skill_id)?;
        let latest_map: HashMap<String, String> = pkg_files
            .iter()
            .map(|f| (f.relative_path.clone(), f.content_hash.clone()))
            .collect();
        let inst_map: HashMap<String, String> = inst
            .installed_manifest
            .iter()
            .map(|m| (m.relative_path.clone(), m.content_hash.clone()))
            .collect();

        let mut changed_files = Vec::new();
        let mut added_files = Vec::new();
        let mut removed_files = Vec::new();

        for (p, h) in &latest_map {
            match inst_map.get(p) {
                None => added_files.push(p.clone()),
                Some(old_h) if old_h != h => changed_files.push(p.clone()),
                _ => {}
            }
        }
        for p in inst_map.keys() {
            if !latest_map.contains_key(p) {
                removed_files.push(p.clone());
            }
        }

        let dest = Path::new(&inst.install_path);
        let mut locally_modified_files = Vec::new();
        if dest.exists() {
            for m in &inst.installed_manifest {
                if let Ok(rel) = validate_relative_path(&m.relative_path) {
                    if let Ok(bytes) = std::fs::read(dest.join(rel)) {
                        if sha256_bytes(&bytes) != m.content_hash {
                            locally_modified_files.push(m.relative_path.clone());
                        }
                    }
                }
            }
        }

        let has_update = !changed_files.is_empty()
            || !added_files.is_empty()
            || !removed_files.is_empty()
            || (skill.commit_sha.is_some() && skill.commit_sha != inst.installed_commit_sha);

        out.push(SkillUpdateCheckItem {
            installation_id: inst.id,
            skill_id: inst.skill_id,
            skill_name: inst.skill_name,
            agent_id: inst.agent_id,
            current_commit_sha: inst.installed_commit_sha,
            latest_commit_sha: skill.commit_sha,
            has_update,
            locally_modified_files,
            changed_files,
            added_files,
            removed_files,
        });
    }
    Ok(out)
}

fn create_backup_of_dir(
    db: &Database,
    slug: &str,
    agent_id: &str,
    source_dir: &Path,
    retention: u32,
) -> Result<PathBuf, String> {
    let backups_root = managed_backups_dir(db)?;
    let skill_backup_parent = backups_root.join(format!("{}_{}", slug, agent_id));
    std::fs::create_dir_all(&skill_backup_parent).map_err(|e| e.to_string())?;
    let stamp = format!("{}", now_unix_secs());
    let dest = skill_backup_parent.join(&stamp);
    copy_dir_recursive(source_dir, &dest)?;

    // Prune old backups beyond retention
    if retention > 0 {
        if let Ok(entries) = std::fs::read_dir(&skill_backup_parent) {
            let mut dirs: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            dirs.sort();
            while dirs.len() > retention as usize {
                let oldest = dirs.remove(0);
                let _ = std::fs::remove_dir_all(oldest);
            }
        }
    }

    Ok(dest)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in WalkDir::new(src).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        let rel = entry.path().strip_prefix(src).map_err(|e| e.to_string())?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else if entry.file_type().is_file() {
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            std::fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn remove_empty_dirs_bottom_up(root: &Path) {
    if !root.is_dir() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                remove_empty_dirs_bottom_up(&p);
            }
        }
    }
    if let Ok(mut entries) = std::fs::read_dir(root) {
        if entries.next().is_none() {
            let _ = std::fs::remove_dir(root);
        }
    }
}
