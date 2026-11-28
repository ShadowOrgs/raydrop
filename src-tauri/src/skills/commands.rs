use super::compatibility::resolve_skill_compatibility;
use super::detector::{configure_agent_manual_path, detect_all_agents, validate_agent_directory};
use super::installer::{
    add_scanned_project, check_updates_for_installed, get_scanned_projects,
    install_skill_to_agents, preview_install, reconcile_installations, remove_scanned_project,
    rollback_installation, uninstall_skill,
};
use super::models::{
    AgentRecord, DiscoveredRepoSkill, SkillCompatibilityEntry, SkillDetailResponse,
    SkillFilePreview, SkillInstallHistoryRecord, SkillInstallPreviewResponse,
    SkillInstallTargetResult, SkillInstallationRecord, SkillPresetRecord, SkillRecord,
    SkillSourceRecord, SkillUpdateCheckItem, SkillsSearchQuery, SkillsSettings,
};
use super::presets::ensure_builtin_presets;
use super::sources::catalog::{ensure_default_sources_and_catalog, refresh_source};
use super::sources::github::{discover_github_skills, import_github_skill};
use super::sources::local::import_local_folder;
use super::sources::zip::{export_skill_to_zip, import_zip_archive};
use super::storage::{
    delete_preset, delete_skill, get_skill_by_id, get_skill_detail, get_skill_files,
    get_skills_settings, list_agents, list_install_history, list_sources, read_skill_file_preview,
    remove_source, save_skills_settings, search_skills, toggle_favorite, upsert_preset,
    upsert_source,
};
use crate::db::storage::now_unix_secs;
use crate::state::AppState;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn skills_init_library(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    ensure_default_sources_and_catalog(&state.db)?;
    let _ = ensure_builtin_presets(&state.db)?;
    let _ = detect_all_agents(&state.db)?;
    Ok(())
}

#[tauri::command]
pub async fn skills_search(
    state: State<'_, Arc<AppState>>,
    query: Option<SkillsSearchQuery>,
) -> Result<Vec<SkillRecord>, String> {
    ensure_default_sources_and_catalog(&state.db)?;
    search_skills(&state.db, &query.unwrap_or_default())
}

#[tauri::command]
pub async fn skills_get_detail(
    state: State<'_, Arc<AppState>>,
    skill_id: String,
) -> Result<SkillDetailResponse, String> {
    get_skill_detail(&state.db, &skill_id)
}

#[tauri::command]
pub async fn skills_get_files(
    state: State<'_, Arc<AppState>>,
    skill_id: String,
) -> Result<Vec<super::models::SkillFileRecord>, String> {
    get_skill_files(&state.db, &skill_id)
}

#[tauri::command]
pub async fn skills_preview_file(
    state: State<'_, Arc<AppState>>,
    skill_id: String,
    relative_path: String,
) -> Result<SkillFilePreview, String> {
    read_skill_file_preview(&state.db, &skill_id, &relative_path)
}

#[tauri::command]
pub async fn skills_import_local_folder(
    state: State<'_, Arc<AppState>>,
    folder_path: String,
) -> Result<SkillRecord, String> {
    import_local_folder(&state.db, &folder_path)
}

#[tauri::command]
pub async fn skills_import_zip(
    state: State<'_, Arc<AppState>>,
    zip_path: String,
) -> Result<SkillRecord, String> {
    import_zip_archive(&state.db, &zip_path)
}

#[tauri::command]
pub async fn skills_export_zip(
    state: State<'_, Arc<AppState>>,
    skill_id: String,
) -> Result<String, String> {
    export_skill_to_zip(&state.db, &skill_id)
}

#[tauri::command]
pub async fn skills_discover_github(
    url: String,
) -> Result<(String, String, Vec<DiscoveredRepoSkill>), String> {
    discover_github_skills(&url).await
}

#[tauri::command]
pub async fn skills_import_github(
    state: State<'_, Arc<AppState>>,
    url: String,
    subpath: Option<String>,
) -> Result<SkillRecord, String> {
    import_github_skill(&state.db, &url, subpath).await
}

#[tauri::command]
pub async fn skills_delete(
    state: State<'_, Arc<AppState>>,
    skill_id: String,
) -> Result<(), String> {
    delete_skill(&state.db, &skill_id)
}

#[tauri::command]
pub async fn skills_toggle_favorite(
    state: State<'_, Arc<AppState>>,
    skill_id: String,
) -> Result<bool, String> {
    toggle_favorite(&state.db, &skill_id)
}

#[tauri::command]
pub async fn skills_list_sources(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SkillSourceRecord>, String> {
    ensure_default_sources_and_catalog(&state.db)?;
    list_sources(&state.db)
}

#[tauri::command]
pub async fn skills_add_source(
    state: State<'_, Arc<AppState>>,
    name: String,
    provider_type: String,
    source_url: String,
) -> Result<SkillSourceRecord, String> {
    let now = now_unix_secs();
    let src = SkillSourceRecord {
        id: format!("src_{}", uuid::Uuid::new_v4().simple()),
        name: name.trim().to_string(),
        provider_type,
        source_url: source_url.trim().to_string(),
        enabled: true,
        last_synced_at: None,
        last_error: None,
        created_at: now,
        updated_at: now,
    };
    upsert_source(&state.db, &src)?;
    Ok(src)
}

const RAYBOT_SKILLS_KEY: &str = "raybot_enabled_skills_json";

pub fn load_raybot_enabled_skill_ids(db: &crate::db::storage::Database) -> Vec<String> {
    db.get_setting(RAYBOT_SKILLS_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn skills_get_raybot_enabled(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<String>, String> {
    Ok(load_raybot_enabled_skill_ids(&state.db))
}

#[tauri::command]
pub async fn skills_toggle_raybot(
    state: State<'_, Arc<AppState>>,
    skill_id: String,
    enabled: bool,
) -> Result<Vec<String>, String> {
    let mut ids = load_raybot_enabled_skill_ids(&state.db);
    ids.retain(|id| id != &skill_id);
    if enabled {
        ids.push(skill_id);
    }
    let raw = serde_json::to_string(&ids).map_err(|e| e.to_string())?;
    state.db.set_setting(RAYBOT_SKILLS_KEY, &raw)?;
    Ok(ids)
}

#[tauri::command]
pub async fn skills_toggle_source(
    state: State<'_, Arc<AppState>>,
    source_id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut sources = list_sources(&state.db)?;
    if let Some(mut s) = sources.drain(..).find(|x| x.id == source_id) {
        s.enabled = enabled;
        s.updated_at = now_unix_secs();
        upsert_source(&state.db, &s)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn skills_refresh_source(
    state: State<'_, Arc<AppState>>,
    source_id: String,
) -> Result<usize, String> {
    refresh_source(&state.db, &source_id).await
}

#[tauri::command]
pub async fn skills_remove_source(
    state: State<'_, Arc<AppState>>,
    source_id: String,
) -> Result<(), String> {
    remove_source(&state.db, &source_id)
}

#[tauri::command]
pub async fn skills_detect_agents(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<AgentRecord>, String> {
    detect_all_agents(&state.db)
}

#[tauri::command]
pub async fn skills_get_agents(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<AgentRecord>, String> {
    let existing = list_agents(&state.db)?;
    if existing.is_empty() {
        detect_all_agents(&state.db)
    } else {
        Ok(existing)
    }
}

#[tauri::command]
pub async fn skills_configure_agent_path(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    manual_path: Option<String>,
) -> Result<AgentRecord, String> {
    configure_agent_manual_path(&state.db, &agent_id, manual_path)
}

#[tauri::command]
pub async fn skills_validate_agent_path(path: String) -> Result<String, String> {
    validate_agent_directory(&path)
}

#[tauri::command]
pub async fn skills_get_compatibility(
    state: State<'_, Arc<AppState>>,
    skill_id: String,
) -> Result<Vec<SkillCompatibilityEntry>, String> {
    let skill = get_skill_by_id(&state.db, &skill_id)?
        .ok_or_else(|| format!("Skill not found: {}", skill_id))?;
    let mut agents = list_agents(&state.db)?;
    if agents.is_empty() {
        agents = detect_all_agents(&state.db)?;
    }
    Ok(resolve_skill_compatibility(&skill, &agents))
}

#[tauri::command]
pub async fn skills_preview_install(
    state: State<'_, Arc<AppState>>,
    skill_id: String,
    agent_ids: Vec<String>,
    scope: String,
    project_path: Option<String>,
) -> Result<SkillInstallPreviewResponse, String> {
    preview_install(
        &state.db,
        &skill_id,
        &agent_ids,
        &scope,
        project_path.as_deref(),
    )
}

#[tauri::command]
pub async fn skills_install(
    state: State<'_, Arc<AppState>>,
    skill_id: String,
    agent_ids: Vec<String>,
    scope: String,
    project_path: Option<String>,
    conflict_policy: String,
    confirm_high_risk: bool,
) -> Result<Vec<SkillInstallTargetResult>, String> {
    install_skill_to_agents(
        &state.db,
        &skill_id,
        &agent_ids,
        &scope,
        project_path.as_deref(),
        &conflict_policy,
        confirm_high_risk,
    )
}

#[tauri::command]
pub async fn skills_list_installed(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SkillInstallationRecord>, String> {
    reconcile_installations(&state.db)
}

#[tauri::command]
pub async fn skills_scan_project_folder(
    state: State<'_, Arc<AppState>>,
    project_path: String,
) -> Result<Vec<SkillInstallationRecord>, String> {
    add_scanned_project(&state.db, &project_path)?;
    reconcile_installations(&state.db)
}

#[tauri::command]
pub async fn skills_list_scanned_projects(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<String>, String> {
    Ok(get_scanned_projects(&state.db))
}

#[tauri::command]
pub async fn skills_remove_scanned_project(
    state: State<'_, Arc<AppState>>,
    project_path: String,
) -> Result<Vec<String>, String> {
    remove_scanned_project(&state.db, &project_path)
}

#[tauri::command]
pub async fn skills_uninstall(
    state: State<'_, Arc<AppState>>,
    installation_id: String,
    force_modified: bool,
) -> Result<Vec<String>, String> {
    uninstall_skill(&state.db, &installation_id, force_modified)
}

#[tauri::command]
pub async fn skills_update_check(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SkillUpdateCheckItem>, String> {
    check_updates_for_installed(&state.db)
}

#[tauri::command]
pub async fn skills_rollback(
    state: State<'_, Arc<AppState>>,
    history_id: String,
) -> Result<String, String> {
    rollback_installation(&state.db, &history_id)
}

#[tauri::command]
pub async fn skills_get_install_history(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SkillInstallHistoryRecord>, String> {
    list_install_history(&state.db)
}

#[tauri::command]
pub async fn skills_list_presets(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SkillPresetRecord>, String> {
    ensure_builtin_presets(&state.db)
}

#[tauri::command]
pub async fn skills_save_preset(
    state: State<'_, Arc<AppState>>,
    id: Option<String>,
    name: String,
    description: String,
    role_tag: String,
    skill_ids: Vec<String>,
) -> Result<SkillPresetRecord, String> {
    let now = now_unix_secs();
    let preset = SkillPresetRecord {
        id: id.unwrap_or_else(|| format!("preset_{}", uuid::Uuid::new_v4().simple())),
        name: name.trim().to_string(),
        description: description.trim().to_string(),
        role_tag,
        is_builtin: false,
        skill_ids,
        created_at: now,
        updated_at: now,
    };
    upsert_preset(&state.db, &preset)?;
    Ok(preset)
}

#[tauri::command]
pub async fn skills_delete_preset(
    state: State<'_, Arc<AppState>>,
    preset_id: String,
) -> Result<(), String> {
    delete_preset(&state.db, &preset_id)
}

#[tauri::command]
pub async fn skills_get_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<SkillsSettings, String> {
    get_skills_settings(&state.db)
}

#[tauri::command]
pub async fn skills_update_settings(
    state: State<'_, Arc<AppState>>,
    settings: SkillsSettings,
) -> Result<SkillsSettings, String> {
    save_skills_settings(&state.db, &settings)
}
