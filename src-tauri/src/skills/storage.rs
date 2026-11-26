use super::models::{
    AgentDetectedStatus, AgentRecord, CompatibilityLevel, SecurityReport, SkillCompatibilityEntry,
    SkillDetailResponse, SkillFilePreview, SkillFileRecord, SkillInstallHistoryRecord,
    SkillInstallationRecord, SkillPresetRecord, SkillRecord, SkillSourceRecord, SkillsSearchQuery,
    SkillsSettings,
};
use crate::db::storage::{now_unix_secs, Database};
use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};

pub fn managed_library_dir(db: &Database) -> Result<PathBuf, String> {
    let dir = db.data_dir().join("skills_library");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create skills_library dir: {}", e))?;
    Ok(dir)
}

pub fn managed_backups_dir(db: &Database) -> Result<PathBuf, String> {
    let dir = db.data_dir().join("skills_backups");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create skills_backups dir: {}", e))?;
    Ok(dir)
}

pub fn get_skills_settings(db: &Database) -> Result<SkillsSettings, String> {
    if let Some(raw) = db.get_setting("skills_library_settings")? {
        if let Ok(parsed) = serde_json::from_str::<SkillsSettings>(&raw) {
            return Ok(parsed);
        }
    }
    Ok(SkillsSettings::default())
}

pub fn save_skills_settings(db: &Database, settings: &SkillsSettings) -> Result<SkillsSettings, String> {
    let json = serde_json::to_string(settings).map_err(|e| e.to_string())?;
    db.set_setting("skills_library_settings", &json)?;
    Ok(settings.clone())
}

pub fn upsert_source(db: &Database, src: &SkillSourceRecord) -> Result<(), String> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO skill_sources
            (id, name, provider_type, source_url, enabled, last_synced_at, last_error, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                src.id,
                src.name,
                src.provider_type,
                src.source_url,
                if src.enabled { 1 } else { 0 },
                src.last_synced_at.map(|v| v as i64),
                src.last_error,
                src.created_at as i64,
                src.updated_at as i64
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn list_sources(db: &Database) -> Result<Vec<SkillSourceRecord>, String> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, provider_type, source_url, enabled, last_synced_at, last_error, created_at, updated_at
                 FROM skill_sources ORDER BY created_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SkillSourceRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    provider_type: row.get(2)?,
                    source_url: row.get(3)?,
                    enabled: row.get::<_, i64>(4)? != 0,
                    last_synced_at: row.get::<_, Option<i64>>(5)?.map(|v| v as u64),
                    last_error: row.get(6)?,
                    created_at: row.get::<_, i64>(7)? as u64,
                    updated_at: row.get::<_, i64>(8)? as u64,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    })
}

pub fn remove_source(db: &Database, source_id: &str) -> Result<(), String> {
    db.with_conn(|conn| {
        conn.execute("DELETE FROM skill_sources WHERE id = ?1", params![source_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn upsert_skill_with_files(
    db: &Database,
    skill: &SkillRecord,
    files: &[SkillFileRecord],
) -> Result<(), String> {
    db.with_conn(|conn| {
        let cats_json = serde_json::to_string(&skill.categories).unwrap_or_else(|_| "[]".into());
        let tags_json = serde_json::to_string(&skill.tags).unwrap_or_else(|_| "[]".into());
        let compat_json =
            serde_json::to_string(&skill.compatibility).unwrap_or_else(|_| "[]".into());
        let sec_json =
            serde_json::to_string(&skill.security_report).unwrap_or_else(|_| "{}".into());

        conn.execute(
            "INSERT OR REPLACE INTO skills_catalog
            (id, source_id, name, slug, description, author, license, repository_url, source_url, source_path,
             version, branch, commit_sha, categories_json, tags_json, compatibility_json, security_status,
             security_report_json, popularity, local_package_path, last_updated, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
            params![
                skill.id,
                skill.source_id,
                skill.name,
                skill.slug,
                skill.description,
                skill.author,
                skill.license,
                skill.repository_url,
                skill.source_url,
                skill.source_path,
                skill.version,
                skill.branch,
                skill.commit_sha,
                cats_json,
                tags_json,
                compat_json,
                skill.security_status,
                sec_json,
                skill.popularity.map(|v| v as i64),
                skill.local_package_path,
                skill.last_updated.map(|v| v as i64),
                skill.created_at as i64,
                skill.updated_at as i64,
            ],
        )
        .map_err(|e| e.to_string())?;

        if !files.is_empty() {
            conn.execute("DELETE FROM skill_files WHERE skill_id = ?1", params![skill.id])
                .map_err(|e| e.to_string())?;
            for f in files {
                conn.execute(
                    "INSERT INTO skill_files (id, skill_id, relative_path, content_hash, size, file_type, storage_path)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        f.id,
                        f.skill_id,
                        f.relative_path,
                        f.content_hash,
                        f.size as i64,
                        f.file_type,
                        f.storage_path
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })
}

pub fn search_skills(db: &Database, query: &SkillsSearchQuery) -> Result<Vec<SkillRecord>, String> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT c.id, c.source_id, c.name, c.slug, c.description, c.author, c.license,
                        c.repository_url, c.source_url, c.source_path, c.version, c.branch,
                        c.commit_sha, c.categories_json, c.tags_json, c.compatibility_json,
                        c.security_status, c.security_report_json, c.popularity, c.local_package_path,
                        c.last_updated, c.created_at, c.updated_at,
                        EXISTS(SELECT 1 FROM skill_favorites f WHERE f.skill_id = c.id) as is_fav
                 FROM skills_catalog c",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                let cats_str: String = row.get(13)?;
                let tags_str: String = row.get(14)?;
                let compat_str: String = row.get(15)?;
                let sec_str: String = row.get(17)?;
                Ok(SkillRecord {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    name: row.get(2)?,
                    slug: row.get(3)?,
                    description: row.get(4)?,
                    author: row.get(5)?,
                    license: row.get(6)?,
                    repository_url: row.get(7)?,
                    source_url: row.get(8)?,
                    source_path: row.get(9)?,
                    version: row.get(10)?,
                    branch: row.get(11)?,
                    commit_sha: row.get(12)?,
                    categories: serde_json::from_str(&cats_str).unwrap_or_default(),
                    tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                    compatibility: serde_json::from_str::<Vec<SkillCompatibilityEntry>>(&compat_str)
                        .unwrap_or_default(),
                    security_status: row.get(16)?,
                    security_report: serde_json::from_str::<SecurityReport>(&sec_str)
                        .unwrap_or_default(),
                    popularity: row.get::<_, Option<i64>>(18)?.map(|v| v as u64),
                    local_package_path: row.get(19)?,
                    last_updated: row.get::<_, Option<i64>>(20)?.map(|v| v as u64),
                    created_at: row.get::<_, i64>(21)? as u64,
                    updated_at: row.get::<_, i64>(22)? as u64,
                    is_favorite: row.get::<_, i64>(23)? != 0,
                    installed_agents: Vec::new(),
                })
            })
            .map_err(|e| e.to_string())?;

        let mut all = Vec::new();
        for r in rows {
            let mut rec = r.map_err(|e| e.to_string())?;
            let mut inst_stmt = conn
                .prepare("SELECT DISTINCT agent_id FROM skill_installations WHERE skill_id = ?1")
                .map_err(|e| e.to_string())?;
            let agent_rows = inst_stmt
                .query_map(params![rec.id], |ar| ar.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            for ar in agent_rows {
                if let Ok(aid) = ar {
                    rec.installed_agents.push(aid);
                }
            }
            all.push(rec);
        }

        let q = query
            .query
            .as_deref()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty());

        all.retain(|item| {
            if query.favorites_only && !item.is_favorite {
                return false;
            }
            if let Some(ref sid) = query.source_id {
                if !sid.is_empty() && &item.source_id != sid {
                    return false;
                }
            }
            if let Some(ref cat) = query.category {
                if !cat.is_empty()
                    && !item
                        .categories
                        .iter()
                        .any(|c| c.eq_ignore_ascii_case(cat))
                {
                    return false;
                }
            }
            if let Some(ref tag) = query.tag {
                if !tag.is_empty() && !item.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                    return false;
                }
            }
            if let Some(ref needle) = q {
                let in_name = item.name.to_ascii_lowercase().contains(needle);
                let in_desc = item.description.to_ascii_lowercase().contains(needle);
                let in_tags = item
                    .tags
                    .iter()
                    .any(|t| t.to_ascii_lowercase().contains(needle));
                let in_author = item
                    .author
                    .as_deref()
                    .map(|a| a.to_ascii_lowercase().contains(needle))
                    .unwrap_or(false);
                if !(in_name || in_desc || in_tags || in_author) {
                    return false;
                }
            }
            true
        });

        match query.sort_by.as_deref().unwrap_or("updated") {
            "alpha" => all.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase())),
            "popularity" => all.sort_by(|a, b| {
                b.popularity
                    .unwrap_or(0)
                    .cmp(&a.popularity.unwrap_or(0))
                    .then_with(|| b.updated_at.cmp(&a.updated_at))
            }),
            _ => all.sort_by(|a, b| {
                b.is_favorite
                    .cmp(&a.is_favorite)
                    .then_with(|| b.updated_at.cmp(&a.updated_at))
            }),
        }

        Ok(all)
    })
}

pub fn get_skill_by_id(db: &Database, skill_id: &str) -> Result<Option<SkillRecord>, String> {
    let all = search_skills(db, &SkillsSearchQuery::default())?;
    Ok(all.into_iter().find(|s| s.id == skill_id))
}

pub fn get_skill_files(db: &Database, skill_id: &str) -> Result<Vec<SkillFileRecord>, String> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, skill_id, relative_path, content_hash, size, file_type, storage_path
                 FROM skill_files WHERE skill_id = ?1 ORDER BY relative_path ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![skill_id], |row| {
                Ok(SkillFileRecord {
                    id: row.get(0)?,
                    skill_id: row.get(1)?,
                    relative_path: row.get(2)?,
                    content_hash: row.get(3)?,
                    size: row.get::<_, i64>(4)? as u64,
                    file_type: row.get(5)?,
                    storage_path: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    })
}

pub fn get_skill_detail(db: &Database, skill_id: &str) -> Result<SkillDetailResponse, String> {
    let skill = get_skill_by_id(db, skill_id)?
        .ok_or_else(|| format!("Skill not found: {}", skill_id))?;
    let files = get_skill_files(db, skill_id)?;
    let skill_md_content = files
        .iter()
        .find(|f| f.relative_path.eq_ignore_ascii_case("SKILL.md"))
        .and_then(|f| std::fs::read_to_string(&f.storage_path).ok());
    let installations = list_installations(db)?
        .into_iter()
        .filter(|i| i.skill_id == skill_id)
        .collect();
    Ok(SkillDetailResponse {
        skill,
        files,
        skill_md_content,
        installations,
    })
}

pub fn read_skill_file_preview(
    db: &Database,
    skill_id: &str,
    relative_path: &str,
) -> Result<SkillFilePreview, String> {
    let files = get_skill_files(db, skill_id)?;
    let file = files
        .into_iter()
        .find(|f| f.relative_path == relative_path)
        .ok_or_else(|| format!("File '{}' not found in skill {}", relative_path, skill_id))?;

    if file.file_type == "binary" {
        return Ok(SkillFilePreview {
            relative_path: file.relative_path,
            file_type: file.file_type,
            size: file.size,
            content_hash: file.content_hash,
            content: None,
            truncated: false,
        });
    }

    let bytes = std::fs::read(&file.storage_path)
        .map_err(|e| format!("Failed to read skill file: {}", e))?;
    const MAX_PREVIEW_BYTES: usize = 256 * 1024;
    let truncated = bytes.len() > MAX_PREVIEW_BYTES;
    let slice = &bytes[..bytes.len().min(MAX_PREVIEW_BYTES)];
    let content = String::from_utf8_lossy(slice).to_string();

    Ok(SkillFilePreview {
        relative_path: file.relative_path,
        file_type: file.file_type,
        size: file.size,
        content_hash: file.content_hash,
        content: Some(content),
        truncated,
    })
}

pub fn toggle_favorite(db: &Database, skill_id: &str) -> Result<bool, String> {
    db.with_conn(|conn| {
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM skill_favorites WHERE skill_id = ?1",
                params![skill_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if exists.is_some() {
            conn.execute(
                "DELETE FROM skill_favorites WHERE skill_id = ?1",
                params![skill_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(false)
        } else {
            conn.execute(
                "INSERT INTO skill_favorites (skill_id, created_at) VALUES (?1, ?2)",
                params![skill_id, now_unix_secs() as i64],
            )
            .map_err(|e| e.to_string())?;
            Ok(true)
        }
    })
}

pub fn delete_skill(db: &Database, skill_id: &str) -> Result<(), String> {
    let skill = get_skill_by_id(db, skill_id)?;
    db.with_conn(|conn| {
        conn.execute("DELETE FROM skill_files WHERE skill_id = ?1", params![skill_id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM skill_favorites WHERE skill_id = ?1", params![skill_id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM skills_catalog WHERE id = ?1", params![skill_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    })?;
    if let Some(s) = skill {
        if let Some(pkg_dir) = s.local_package_path {
            let p = Path::new(&pkg_dir);
            if p.exists() {
                let _ = std::fs::remove_dir_all(p);
            }
        }
    }
    Ok(())
}

pub fn upsert_agent(db: &Database, agent: &AgentRecord) -> Result<(), String> {
    db.with_conn(|conn| {
        let status_str = serde_json::to_string(&agent.detected_status).unwrap_or_else(|_| "\"NotDetected\"".into());
        let compat_str = serde_json::to_string(&agent.compatibility_level).unwrap_or_else(|_| "\"Compatible\"".into());
        conn.execute(
            "INSERT OR REPLACE INTO skill_agents
            (id, agent_type, display_name, detected_status, detection_reason, configured_path,
             global_skill_path, project_skill_path, compatibility_level, last_detected_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                agent.id,
                agent.agent_type,
                agent.display_name,
                status_str.trim_matches('"'),
                agent.detection_reason,
                agent.configured_path,
                agent.global_skill_path,
                agent.project_skill_path,
                compat_str.trim_matches('"'),
                agent.last_detected_at as i64,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn list_agents(db: &Database) -> Result<Vec<AgentRecord>, String> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, agent_type, display_name, detected_status, detection_reason, configured_path,
                        global_skill_path, project_skill_path, compatibility_level, last_detected_at
                 FROM skill_agents ORDER BY display_name ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                let status_raw: String = row.get(3)?;
                let compat_raw: String = row.get(8)?;
                let detected_status = match status_raw.as_str() {
                    "Detected" => AgentDetectedStatus::Detected,
                    "ManualConfiguration" => AgentDetectedStatus::ManualConfiguration,
                    "PathInvalid" => AgentDetectedStatus::PathInvalid,
                    "UnsupportedPlatform" => AgentDetectedStatus::UnsupportedPlatform,
                    _ => AgentDetectedStatus::NotDetected,
                };
                let compatibility_level = match compat_raw.as_str() {
                    "Native" => CompatibilityLevel::Native,
                    "Adapted" => CompatibilityLevel::Adapted,
                    "Unsupported" => CompatibilityLevel::Unsupported,
                    _ => CompatibilityLevel::Compatible,
                };
                Ok(AgentRecord {
                    id: row.get(0)?,
                    agent_type: row.get(1)?,
                    display_name: row.get(2)?,
                    detected_status,
                    detection_reason: row.get(4)?,
                    configured_path: row.get(5)?,
                    global_skill_path: row.get(6)?,
                    project_skill_path: row.get(7)?,
                    compatibility_level,
                    supports_global: true,
                    supports_project: true,
                    last_detected_at: row.get::<_, i64>(9)? as u64,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    })
}

pub fn upsert_installation(db: &Database, rec: &SkillInstallationRecord) -> Result<(), String> {
    db.with_conn(|conn| {
        let manifest_json =
            serde_json::to_string(&rec.installed_manifest).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "INSERT OR REPLACE INTO skill_installations
            (id, skill_id, skill_name, agent_id, scope, project_path, install_path,
             installed_version, installed_commit_sha, installed_manifest_json, installed_at, updated_at, status)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                rec.id,
                rec.skill_id,
                rec.skill_name,
                rec.agent_id,
                rec.scope,
                rec.project_path,
                rec.install_path,
                rec.installed_version,
                rec.installed_commit_sha,
                manifest_json,
                rec.installed_at as i64,
                rec.updated_at as i64,
                rec.status,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn list_installations(db: &Database) -> Result<Vec<SkillInstallationRecord>, String> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, skill_id, skill_name, agent_id, scope, project_path, install_path,
                        installed_version, installed_commit_sha, installed_manifest_json, installed_at, updated_at, status
                 FROM skill_installations ORDER BY updated_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                let m_str: String = row.get(9)?;
                Ok(SkillInstallationRecord {
                    id: row.get(0)?,
                    skill_id: row.get(1)?,
                    skill_name: row.get(2)?,
                    agent_id: row.get(3)?,
                    scope: row.get(4)?,
                    project_path: row.get(5)?,
                    install_path: row.get(6)?,
                    installed_version: row.get(7)?,
                    installed_commit_sha: row.get(8)?,
                    installed_manifest: serde_json::from_str(&m_str).unwrap_or_default(),
                    installed_at: row.get::<_, i64>(10)? as u64,
                    updated_at: row.get::<_, i64>(11)? as u64,
                    status: row.get(12)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    })
}

pub fn delete_installation(db: &Database, installation_id: &str) -> Result<(), String> {
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM skill_installations WHERE id = ?1",
            params![installation_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn insert_install_history(db: &Database, h: &SkillInstallHistoryRecord) -> Result<(), String> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO skill_install_history
            (id, installation_id, skill_id, skill_name, agent_id, action, previous_version, new_version, backup_path, timestamp, result, error_message)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                h.id,
                h.installation_id,
                h.skill_id,
                h.skill_name,
                h.agent_id,
                h.action,
                h.previous_version,
                h.new_version,
                h.backup_path,
                h.timestamp as i64,
                h.result,
                h.error_message,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn list_install_history(db: &Database) -> Result<Vec<SkillInstallHistoryRecord>, String> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, installation_id, skill_id, skill_name, agent_id, action, previous_version, new_version, backup_path, timestamp, result, error_message
                 FROM skill_install_history ORDER BY timestamp DESC LIMIT 200",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SkillInstallHistoryRecord {
                    id: row.get(0)?,
                    installation_id: row.get(1)?,
                    skill_id: row.get(2)?,
                    skill_name: row.get(3)?,
                    agent_id: row.get(4)?,
                    action: row.get(5)?,
                    previous_version: row.get(6)?,
                    new_version: row.get(7)?,
                    backup_path: row.get(8)?,
                    timestamp: row.get::<_, i64>(9)? as u64,
                    result: row.get(10)?,
                    error_message: row.get(11)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    })
}

pub fn list_presets(db: &Database) -> Result<Vec<SkillPresetRecord>, String> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, role_tag, is_builtin, created_at, updated_at
                 FROM skill_presets ORDER BY is_builtin DESC, name ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SkillPresetRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    role_tag: row.get(3)?,
                    is_builtin: row.get::<_, i64>(4)? != 0,
                    skill_ids: Vec::new(),
                    created_at: row.get::<_, i64>(5)? as u64,
                    updated_at: row.get::<_, i64>(6)? as u64,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            let mut p = r.map_err(|e| e.to_string())?;
            let mut istmt = conn
                .prepare("SELECT skill_id FROM skill_preset_items WHERE preset_id = ?1 ORDER BY position ASC")
                .map_err(|e| e.to_string())?;
            let irows = istmt
                .query_map(params![p.id], |ir| ir.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            for ir in irows {
                if let Ok(sid) = ir {
                    p.skill_ids.push(sid);
                }
            }
            out.push(p);
        }
        Ok(out)
    })
}

pub fn upsert_preset(db: &Database, preset: &SkillPresetRecord) -> Result<(), String> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO skill_presets (id, name, description, role_tag, is_builtin, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                preset.id,
                preset.name,
                preset.description,
                preset.role_tag,
                if preset.is_builtin { 1 } else { 0 },
                preset.created_at as i64,
                preset.updated_at as i64,
            ],
        )
        .map_err(|e| e.to_string())?;

        conn.execute("DELETE FROM skill_preset_items WHERE preset_id = ?1", params![preset.id])
            .map_err(|e| e.to_string())?;
        for (idx, sid) in preset.skill_ids.iter().enumerate() {
            conn.execute(
                "INSERT INTO skill_preset_items (preset_id, skill_id, position) VALUES (?1, ?2, ?3)",
                params![preset.id, sid, idx as i64],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    })
}

pub fn delete_preset(db: &Database, preset_id: &str) -> Result<(), String> {
    db.with_conn(|conn| {
        conn.execute("DELETE FROM skill_preset_items WHERE preset_id = ?1", params![preset_id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM skill_presets WHERE id = ?1", params![preset_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    })
}
