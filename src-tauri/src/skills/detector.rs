use super::models::{AgentDetectedStatus, AgentRecord, CompatibilityLevel};
use crate::db::storage::{now_unix_secs, Database};
use crate::skills::storage::{list_agents, upsert_agent};
use directories::BaseDirs;
use std::path::{Path, PathBuf};

pub struct AgentDefinition {
    pub id: &'static str,
    pub agent_type: &'static str,
    pub display_name: &'static str,
    pub executables: &'static [&'static str],
    pub home_config_dirs: &'static [&'static str],
    pub default_global_rel: &'static str,
    pub default_project_rel: &'static str,
    pub compatibility: CompatibilityLevel,
}

pub const AGENT_DEFINITIONS: &[AgentDefinition] = &[
    AgentDefinition {
        id: "antigravity",
        agent_type: "antigravity",
        display_name: "Antigravity IDE",
        executables: &["agy", "agy.exe", "agy.cmd"],
        home_config_dirs: &[".gemini/antigravity", ".gemini/antigravity-ide", ".gemini/config"],
        default_global_rel: ".gemini/config/skills",
        default_project_rel: ".agent/skills",
        compatibility: CompatibilityLevel::Native,
    },
    AgentDefinition {
        id: "codex",
        agent_type: "codex",
        display_name: "OpenAI Codex",
        executables: &["codex", "codex.exe", "codex.cmd"],
        home_config_dirs: &[".codex", ".agents"],
        default_global_rel: ".codex/skills",
        default_project_rel: ".agents/skills",
        compatibility: CompatibilityLevel::Native,
    },
    AgentDefinition {
        id: "claude_code",
        agent_type: "claude_code",
        display_name: "Claude Code",
        executables: &["claude", "claude.exe", "claude.cmd"],
        home_config_dirs: &[".claude"],
        default_global_rel: ".claude/skills",
        default_project_rel: ".claude/skills",
        compatibility: CompatibilityLevel::Native,
    },
    AgentDefinition {
        id: "cursor",
        agent_type: "cursor",
        display_name: "Cursor",
        executables: &["cursor", "cursor.exe", "cursor.cmd"],
        home_config_dirs: &[".cursor"],
        default_global_rel: ".cursor/skills",
        default_project_rel: ".cursor/skills",
        compatibility: CompatibilityLevel::Native,
    },
    AgentDefinition {
        id: "gemini_cli",
        agent_type: "gemini_cli",
        display_name: "Gemini CLI",
        executables: &["gemini", "gemini.exe", "gemini.cmd"],
        home_config_dirs: &[".gemini"],
        default_global_rel: ".gemini/skills",
        default_project_rel: ".gemini/skills",
        compatibility: CompatibilityLevel::Native,
    },
    AgentDefinition {
        id: "opencode",
        agent_type: "opencode",
        display_name: "OpenCode",
        executables: &["opencode", "opencode.exe"],
        home_config_dirs: &[".config/opencode", ".opencode"],
        default_global_rel: ".config/opencode/skills",
        default_project_rel: ".opencode/skills",
        compatibility: CompatibilityLevel::Native,
    },
    AgentDefinition {
        id: "github_copilot",
        agent_type: "github_copilot",
        display_name: "GitHub Copilot",
        executables: &["code", "code.exe", "code.cmd"],
        home_config_dirs: &[".copilot"],
        default_global_rel: ".copilot/skills",
        default_project_rel: ".github/skills",
        compatibility: CompatibilityLevel::Compatible,
    },
    AgentDefinition {
        id: "windsurf",
        agent_type: "windsurf",
        display_name: "Windsurf",
        executables: &["windsurf", "windsurf.exe", "windsurf.cmd"],
        home_config_dirs: &[".codeium/windsurf", ".windsurf"],
        default_global_rel: ".codeium/windsurf/skills",
        default_project_rel: ".windsurf/skills",
        compatibility: CompatibilityLevel::Compatible,
    },
    AgentDefinition {
        id: "cline",
        agent_type: "cline",
        display_name: "Cline",
        executables: &[],
        home_config_dirs: &[".cline"],
        default_global_rel: ".cline/skills",
        default_project_rel: ".cline/skills",
        compatibility: CompatibilityLevel::Compatible,
    },
    AgentDefinition {
        id: "roo_code",
        agent_type: "roo_code",
        display_name: "Roo Code",
        executables: &[],
        home_config_dirs: &[".roo"],
        default_global_rel: ".roo/skills",
        default_project_rel: ".roo/skills",
        compatibility: CompatibilityLevel::Compatible,
    },
    AgentDefinition {
        id: "continue",
        agent_type: "continue",
        display_name: "Continue",
        executables: &["cn", "cn.exe"],
        home_config_dirs: &[".continue"],
        default_global_rel: ".continue/skills",
        default_project_rel: ".continue/skills",
        compatibility: CompatibilityLevel::Compatible,
    },
];

fn home_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|b| b.home_dir().to_path_buf())
}

fn find_executable_in_path(names: &[&str]) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in names {
            let cand = dir.join(name);
            if cand.is_file() {
                return Some(cand.to_string_lossy().to_string());
            }
        }
    }
    None
}

pub fn detect_all_agents(db: &Database) -> Result<Vec<AgentRecord>, String> {
    let existing = list_agents(db).unwrap_or_default();
    let home = home_dir();
    let now = now_unix_secs();
    let mut results = Vec::new();

    for def in AGENT_DEFINITIONS {
        let prev_configured = existing
            .iter()
            .find(|a| a.id == def.id)
            .and_then(|a| a.configured_path.clone());

        let default_global = home
            .as_ref()
            .map(|h| h.join(def.default_global_rel))
            .unwrap_or_else(|| PathBuf::from(def.default_global_rel));
        let default_global_str = default_global.to_string_lossy().to_string();

        let (status, reason) = if let Some(ref manual_path) = prev_configured {
            let p = Path::new(manual_path);
            if p.exists() && p.is_dir() {
                (
                    AgentDetectedStatus::ManualConfiguration,
                    format!("Manual path configured: {}", manual_path),
                )
            } else {
                (
                    AgentDetectedStatus::PathInvalid,
                    format!("Configured path does not exist: {}", manual_path),
                )
            }
        } else {
            let mut reasons = Vec::new();
            if let Some(exe_path) = find_executable_in_path(def.executables) {
                reasons.push(format!("Executable in PATH ({})", exe_path));
            }
            if let Some(ref h) = home {
                for rel in def.home_config_dirs {
                    let cfg_dir = h.join(rel);
                    if cfg_dir.is_dir() {
                        reasons.push(format!("Config directory found (~/{})", rel));
                        break;
                    }
                }
                if default_global.is_dir() {
                    reasons.push(format!("Skills directory exists ({})", default_global_str));
                }
            }
            if reasons.is_empty() {
                (
                    AgentDetectedStatus::NotDetected,
                    "No executable, config directory, or skills directory found. You can still configure a manual path.".to_string(),
                )
            } else {
                (AgentDetectedStatus::Detected, reasons.join(" • "))
            }
        };

        let record = AgentRecord {
            id: def.id.to_string(),
            agent_type: def.agent_type.to_string(),
            display_name: def.display_name.to_string(),
            detected_status: status,
            detection_reason: reason,
            configured_path: prev_configured,
            global_skill_path: default_global_str,
            project_skill_path: def.default_project_rel.to_string(),
            compatibility_level: def.compatibility.clone(),
            supports_global: true,
            supports_project: true,
            last_detected_at: now,
        };
        upsert_agent(db, &record)?;
        results.push(record);
    }

    Ok(results)
}

pub fn configure_agent_manual_path(
    db: &Database,
    agent_id: &str,
    manual_path: Option<String>,
) -> Result<AgentRecord, String> {
    let cleaned = manual_path
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(ref p) = cleaned {
        validate_agent_directory(p)?;
    }

    let mut agents = list_agents(db)?;
    if agents.is_empty() {
        agents = detect_all_agents(db)?;
    }
    let mut target = agents
        .into_iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| format!("Unknown agent ID: {}", agent_id))?;

    target.configured_path = cleaned;
    upsert_agent(db, &target)?;
    // Re-run detection to refresh status and reason
    let refreshed = detect_all_agents(db)?;
    refreshed
        .into_iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| "Agent disappeared after update".to_string())
}

pub fn validate_agent_directory(path_str: &str) -> Result<String, String> {
    let p = Path::new(path_str.trim());
    if p.as_os_str().is_empty() {
        return Err("Agent path cannot be empty".to_string());
    }
    if !p.is_absolute() {
        return Err("Agent path must be an absolute directory path".to_string());
    }
    if p.exists() && !p.is_dir() {
        return Err("Selected path exists but is not a directory".to_string());
    }
    if !p.exists() {
        std::fs::create_dir_all(p)
            .map_err(|e| format!("Directory does not exist and could not be created: {}", e))?;
    }
    let canon = dunce::canonicalize(p).map_err(|e| format!("Invalid directory path: {}", e))?;
    Ok(canon.to_string_lossy().to_string())
}
