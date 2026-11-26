use super::models::{AgentRecord, CompatibilityLevel, SkillCompatibilityEntry, SkillRecord};
use std::path::{Path, PathBuf};

/// Computes compatibility entries for a skill across all known agents.
pub fn resolve_skill_compatibility(
    skill: &SkillRecord,
    agents: &[AgentRecord],
) -> Vec<SkillCompatibilityEntry> {
    agents
        .iter()
        .map(|agent| {
            let level = if agent.compatibility_level == CompatibilityLevel::Unsupported {
                CompatibilityLevel::Unsupported
            } else {
                agent.compatibility_level.clone()
            };

            let note = match level {
                CompatibilityLevel::Native => format!(
                    "{} natively supports SKILL.md skill folders.",
                    agent.display_name
                ),
                CompatibilityLevel::Compatible => format!(
                    "{} supports standard SKILL.md conventions at {}.",
                    agent.display_name, agent.project_skill_path
                ),
                CompatibilityLevel::Adapted => format!(
                    "Requires path/metadata adaptation for {}.",
                    agent.display_name
                ),
                CompatibilityLevel::Unsupported => format!(
                    "{} is not supported for skill '{}'.",
                    agent.display_name, skill.name
                ),
            };

            SkillCompatibilityEntry {
                agent_id: agent.id.clone(),
                agent_name: agent.display_name.clone(),
                level,
                note,
            }
        })
        .collect()
}

/// Resolves the exact installation destination directory for `(agent, scope, project_path, skill_slug)`.
pub fn resolve_install_destination(
    agent: &AgentRecord,
    scope: &str,
    project_path: Option<&str>,
    skill_slug: &str,
) -> Result<PathBuf, String> {
    if skill_slug.trim().is_empty()
        || skill_slug.contains('/')
        || skill_slug.contains('\\')
        || skill_slug.contains("..")
    {
        return Err(format!("Unsafe skill slug rejected: {}", skill_slug));
    }

    let base_skills_dir = match scope {
        "global" => {
            if let Some(ref manual) = agent.configured_path {
                PathBuf::from(manual)
            } else {
                PathBuf::from(&agent.global_skill_path)
            }
        }
        "project" => {
            let proj = project_path
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "Project directory is required for project-scope install".to_string())?;
            let proj_path = Path::new(proj);
            if !proj_path.exists() || !proj_path.is_dir() {
                return Err(format!("Selected project directory does not exist: {}", proj));
            }
            let canon_proj = dunce::canonicalize(proj_path)
                .map_err(|e| format!("Failed to resolve project path: {}", e))?;
            canon_proj.join(&agent.project_skill_path)
        }
        other => return Err(format!("Invalid install scope: {}", other)),
    };

    Ok(base_skills_dir.join(skill_slug))
}
