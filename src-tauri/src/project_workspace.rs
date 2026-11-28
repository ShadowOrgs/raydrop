use crate::db::storage::{now_unix_secs, Database};
use crate::state::AppState;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tauri::State;

pub const PROJECTS_ROOT_SETTING_KEY: &str = "raybot_projects_root_dir";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub name: String,
    pub path: String,
    pub tech_stack: String,
    pub summary: String,
    pub pending_tasks: Vec<String>,
    pub is_active: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRelation {
    pub source_project: String,
    pub target_project: String,
    pub relation_type: String,
    pub description: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectsOverview {
    pub root_dir: String,
    pub active_project: Option<String>,
    pub projects: Vec<ProjectEntry>,
    pub relations: Vec<ProjectRelation>,
}

pub fn default_projects_root() -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn get_projects_root(db: &Database) -> PathBuf {
    if let Ok(Some(val)) = db.get_setting(PROJECTS_ROOT_SETTING_KEY) {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    default_projects_root()
}

fn expand_tilde(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        return default_projects_root();
    }
    if let Some(rest) = trimmed.strip_prefix("~/").or_else(|| trimmed.strip_prefix("~\\")) {
        return default_projects_root().join(rest);
    }
    PathBuf::from(trimmed)
}

pub fn set_projects_root(db: &Database, path_str: &str) -> Result<ProjectsOverview, String> {
    let expanded = expand_tilde(path_str);
    if !expanded.exists() {
        std::fs::create_dir_all(&expanded)
            .map_err(|e| format!("Failed to create projects root '{}': {}", expanded.display(), e))?;
    }
    if !expanded.is_dir() {
        return Err(format!("'{}' is not a directory", expanded.display()));
    }
    let canonical = dunce::canonicalize(&expanded).unwrap_or(expanded);
    let canonical_str = canonical.to_string_lossy().to_string();
    db.set_setting(PROJECTS_ROOT_SETTING_KEY, &canonical_str)
        .map_err(|e| format!("Failed to save projects root: {}", e))?;
    scan_projects_in_root(db)
}

fn detect_tech_stack(dir: &Path) -> String {
    let mut stacks: Vec<&'static str> = Vec::new();
    if dir.join("Cargo.toml").exists() {
        stacks.push("Rust");
    }
    if dir.join("src-tauri").join("tauri.conf.json").exists() || dir.join("tauri.conf.json").exists() {
        stacks.push("Tauri");
    }
    if dir.join("package.json").exists() {
        stacks.push("Node/JS/TS");
    }
    if dir.join("pyproject.toml").exists() || dir.join("requirements.txt").exists() {
        stacks.push("Python");
    }
    if dir.join("go.mod").exists() {
        stacks.push("Go");
    }
    if dir.join("pom.xml").exists() || dir.join("build.gradle").exists() || dir.join("build.gradle.kts").exists() {
        stacks.push("Java/Kotlin");
    }
    if dir.join("docker-compose.yml").exists() || dir.join("Dockerfile").exists() {
        stacks.push("Docker");
    }
    if stacks.is_empty() && dir.join(".git").exists() {
        stacks.push("Git Repo");
    }
    stacks.join(", ")
}

fn is_project_dir(dir: &Path) -> bool {
    let markers = [
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "requirements.txt",
        "go.mod",
        ".git",
        "tauri.conf.json",
        "src-tauri",
        "pom.xml",
        "build.gradle",
        "Makefile",
        "docker-compose.yml",
    ];
    markers.iter().any(|m| dir.join(m).exists())
}

fn upsert_scanned_project(db: &Database, name: &str, path: &Path, tech_stack: &str) -> Result<(), String> {
    let path_str = dunce::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();
    let now = now_unix_secs();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO raybot_projects (name, path, tech_stack, summary, pending_tasks_json, is_active, updated_at)
         VALUES (?1, ?2, ?3, '', '[]', 0, ?4)
         ON CONFLICT(name) DO UPDATE SET
           path = excluded.path,
           tech_stack = CASE WHEN excluded.tech_stack != '' THEN excluded.tech_stack ELSE raybot_projects.tech_stack END,
           updated_at = excluded.updated_at",
        params![name, path_str, tech_stack, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn scan_projects_in_root(db: &Database) -> Result<ProjectsOverview, String> {
    let root = get_projects_root(db);
    if !root.exists() {
        return Err(format!("Projects root '{}' does not exist", root.display()));
    }

    if is_project_dir(&root) {
        let name = root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("root-project")
            .to_string();
        let stack = detect_tech_stack(&root);
        upsert_scanned_project(db, &name, &root, &stack)?;
    }

    let entries = std::fs::read_dir(&root)
        .map_err(|e| format!("Failed to read directory '{}': {}", root.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.starts_with('.') || name == "node_modules" || name == "target" || name == "__pycache__" {
            continue;
        }
        if is_project_dir(&path) {
            let stack = detect_tech_stack(&path);
            upsert_scanned_project(db, name, &path, &stack)?;
        }
    }

    get_projects_overview(db)
}

pub fn list_projects(db: &Database) -> Result<Vec<ProjectEntry>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT name, path, tech_stack, summary, pending_tasks_json, is_active, updated_at
             FROM raybot_projects
             ORDER BY is_active DESC, updated_at DESC, name ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let tasks_raw: String = row.get(4)?;
            let pending_tasks: Vec<String> = serde_json::from_str(&tasks_raw).unwrap_or_default();
            Ok(ProjectEntry {
                name: row.get(0)?,
                path: row.get(1)?,
                tech_stack: row.get(2)?,
                summary: row.get(3)?,
                pending_tasks,
                is_active: row.get::<_, i64>(5)? != 0,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

pub fn list_relations(db: &Database) -> Result<Vec<ProjectRelation>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT source_project, target_project, relation_type, description, updated_at
             FROM raybot_project_relations
             ORDER BY updated_at DESC, source_project ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ProjectRelation {
                source_project: row.get(0)?,
                target_project: row.get(1)?,
                relation_type: row.get(2)?,
                description: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

pub fn get_projects_overview(db: &Database) -> Result<ProjectsOverview, String> {
    let root_dir = get_projects_root(db).to_string_lossy().to_string();
    let projects = list_projects(db)?;
    let active_project = projects.iter().find(|p| p.is_active).map(|p| p.name.clone());
    let relations = list_relations(db)?;
    Ok(ProjectsOverview {
        root_dir,
        active_project,
        projects,
        relations,
    })
}

pub fn get_active_project(db: &Database) -> Result<Option<ProjectEntry>, String> {
    let projects = list_projects(db)?;
    Ok(projects.into_iter().find(|p| p.is_active))
}

fn find_project_by_name(db: &Database, name: &str) -> Result<Option<ProjectEntry>, String> {
    let target = name.trim().to_lowercase();
    let projects = list_projects(db)?;
    Ok(projects.into_iter().find(|p| p.name.to_lowercase() == target))
}

pub fn select_active_project(db: &Database, name_or_path: &str) -> Result<ProjectEntry, String> {
    let trimmed = name_or_path.trim();
    if trimmed.is_empty() {
        return Err("Project name cannot be empty".to_string());
    }

    let matched = if let Some(found) = find_project_by_name(db, trimmed)? {
        found
    } else {
        let p = expand_tilde(trimmed);
        if p.is_dir() {
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("project")
                .to_string();
            let stack = detect_tech_stack(&p);
            upsert_scanned_project(db, &name, &p, &stack)?;
            find_project_by_name(db, &name)?
                .ok_or_else(|| "Failed to load registered project".to_string())?
        } else {
            return Err(format!(
                "Project '{}' not found in registry. Run `project_scan` or pass a valid directory path.",
                trimmed
            ));
        }
    };

    let now = now_unix_secs();
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("UPDATE raybot_projects SET is_active = 0", [])
            .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE raybot_projects SET is_active = 1, updated_at = ?1 WHERE name = ?2",
            params![now, matched.name],
        )
        .map_err(|e| e.to_string())?;
    }

    find_project_by_name(db, &matched.name)?
        .ok_or_else(|| "Project not found after activation".to_string())
}

pub fn add_project_relation(
    db: &Database,
    source_project: &str,
    target_project: &str,
    relation_type: &str,
    description: &str,
) -> Result<ProjectRelation, String> {
    let src = source_project.trim();
    let dst = target_project.trim();
    let rel = relation_type.trim();
    if src.is_empty() || dst.is_empty() || rel.is_empty() {
        return Err("source_project, target_project, and relation_type are required".to_string());
    }
    let now = now_unix_secs();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO raybot_project_relations (source_project, target_project, relation_type, description, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(source_project, target_project, relation_type) DO UPDATE SET
           description = excluded.description,
           updated_at = excluded.updated_at",
        params![src, dst, rel, description.trim(), now],
    )
    .map_err(|e| e.to_string())?;
    Ok(ProjectRelation {
        source_project: src.to_string(),
        target_project: dst.to_string(),
        relation_type: rel.to_string(),
        description: description.trim().to_string(),
        updated_at: now as i64,
    })
}

pub fn save_project_memory(
    db: &Database,
    project_name: Option<&str>,
    summary: Option<&str>,
    pending_tasks: Option<Vec<String>>,
) -> Result<ProjectEntry, String> {
    let target = match project_name.map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => find_project_by_name(db, n)?
            .ok_or_else(|| format!("Project '{}' not found", n))?,
        None => get_active_project(db)?
            .ok_or_else(|| "No active project selected. Call `project_select` first.".to_string())?,
    };

    let new_summary = summary.map(|s| s.trim().to_string()).unwrap_or(target.summary);
    let new_tasks = pending_tasks.unwrap_or(target.pending_tasks);
    let tasks_json = serde_json::to_string(&new_tasks).unwrap_or_else(|_| "[]".to_string());
    let now = now_unix_secs();

    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE raybot_projects SET summary = ?1, pending_tasks_json = ?2, updated_at = ?3 WHERE name = ?4",
            params![new_summary, tasks_json, now, target.name],
        )
        .map_err(|e| e.to_string())?;
    }

    find_project_by_name(db, &target.name)?
        .ok_or_else(|| "Project not found after update".to_string())
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn resolve_project_dir(db: &Database, project_name: Option<&str>) -> Result<ProjectEntry, String> {
    match project_name.map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => find_project_by_name(db, n)?
            .ok_or_else(|| format!("Project '{}' not found. Call `project_list` or `project_scan`.", n)),
        None => get_active_project(db)?.ok_or_else(|| {
            "No active project selected. Call `project_scan` and `project_select` first.".to_string()
        }),
    }
}

pub fn resolve_project_path(
    db: &Database,
    project_name: Option<&str>,
    rel_or_abs_path: Option<&str>,
) -> Result<(ProjectEntry, PathBuf), String> {
    let proj = resolve_project_dir(db, project_name)?;
    let proj_root = dunce::canonicalize(&proj.path).unwrap_or_else(|_| PathBuf::from(&proj.path));

    let raw = rel_or_abs_path.map(str::trim).unwrap_or("");
    if raw.is_empty() || raw == "." {
        return Ok((proj, proj_root));
    }

    let candidate = Path::new(raw);
    let joined = if candidate.is_absolute() {
        normalize_lexical(candidate)
    } else {
        normalize_lexical(&proj_root.join(candidate))
    };

    let resolved = if joined.exists() {
        dunce::canonicalize(&joined).unwrap_or(joined)
    } else {
        // Canonicalize the nearest existing ancestor so symlinks/8.3 short paths match proj_root
        let mut cur = joined.as_path();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        while !cur.exists() {
            if let Some(name) = cur.file_name() {
                tail.push(name.to_os_string());
            }
            match cur.parent() {
                Some(p) => cur = p,
                None => break,
            }
        }
        let mut base = if cur.exists() {
            dunce::canonicalize(cur).unwrap_or_else(|_| cur.to_path_buf())
        } else {
            joined.clone()
        };
        for seg in tail.into_iter().rev() {
            base.push(seg);
        }
        base
    };

    if !resolved.starts_with(&proj_root) {
        return Err(format!(
            "Access denied: '{}' is outside project '{}' ({})",
            resolved.display(),
            proj.name,
            proj_root.display()
        ));
    }

    Ok((proj, resolved))
}

pub fn build_projects_prompt_context(db: &Database) -> String {
    let root = get_projects_root(db);
    let projects = list_projects(db).unwrap_or_default();
    let relations = list_relations(db).unwrap_or_default();

    let mut out = String::new();
    out.push_str("\n\nCoding Workspace & Persistent Project Memory:\n");
    out.push_str(&format!("- Projects Root Directory: `{}`\n", root.display()));

    if let Some(active) = projects.iter().find(|p| p.is_active) {
        out.push_str(&format!(
            "- Active Project: **{}** (`{}`{})",
            active.name,
            active.path,
            if active.tech_stack.is_empty() {
                String::new()
            } else {
                format!(", stack: {}", active.tech_stack)
            }
        ));
        if !active.summary.is_empty() {
            out.push_str(&format!(" — {}", active.summary));
        }
        out.push('\n');
        if !active.pending_tasks.is_empty() {
            out.push_str(&format!(
                "  Pending tasks: {}\n",
                active.pending_tasks.join("; ")
            ));
        }
    } else {
        out.push_str("- Active Project: None selected yet (call `project_scan` and `project_select` when coding).\n");
    }

    if !projects.is_empty() {
        out.push_str("- Known Projects:\n");
        for p in projects.iter().take(25) {
            let marker = if p.is_active { " [ACTIVE]" } else { "" };
            let stack = if p.tech_stack.is_empty() {
                String::new()
            } else {
                format!(" [{}]", p.tech_stack)
            };
            let summary_preview = if p.summary.is_empty() {
                String::new()
            } else {
                let s: String = p.summary.chars().take(120).collect();
                format!(" — {}", s)
            };
            out.push_str(&format!(
                "  * {}{}{} (`{}`){}\n",
                p.name, marker, stack, p.path, summary_preview
            ));
        }
    }

    if !relations.is_empty() {
        out.push_str("- Cross-Project Relations:\n");
        for r in relations.iter().take(30) {
            let desc = if r.description.is_empty() {
                String::new()
            } else {
                format!(" ({})", r.description)
            };
            out.push_str(&format!(
                "  * {} --[{}]--> {}{}\n",
                r.source_project, r.relation_type, r.target_project, desc
            ));
        }
    }

    out.push_str("Use `project_*` (`project_set_root`, `project_scan`, `project_select`, `project_add_relation`, `project_save_memory`) and `code_*` (`code_list_files`, `code_search`, `code_read_file`, `code_write_file`, `code_edit_file`, `code_run_command`) tools for coding tasks and to keep project summaries/relations updated.");
    out
}

#[tauri::command]
pub fn raybot_get_projects_overview(state: State<'_, Arc<AppState>>) -> Result<ProjectsOverview, String> {
    get_projects_overview(&state.db)
}

#[tauri::command]
pub fn raybot_set_projects_root(
    state: State<'_, Arc<AppState>>,
    root_dir: String,
) -> Result<ProjectsOverview, String> {
    set_projects_root(&state.db, &root_dir)
}

#[tauri::command]
pub fn raybot_scan_projects(state: State<'_, Arc<AppState>>) -> Result<ProjectsOverview, String> {
    scan_projects_in_root(&state.db)
}

#[tauri::command]
pub fn raybot_select_project(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> Result<ProjectsOverview, String> {
    select_active_project(&state.db, &name)?;
    get_projects_overview(&state.db)
}

#[tauri::command]
pub fn raybot_add_project_relation(
    state: State<'_, Arc<AppState>>,
    source_project: String,
    target_project: String,
    relation_type: String,
    description: String,
) -> Result<ProjectsOverview, String> {
    add_project_relation(
        &state.db,
        &source_project,
        &target_project,
        &relation_type,
        &description,
    )?;
    get_projects_overview(&state.db)
}
