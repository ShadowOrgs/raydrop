use super::models::SkillPresetRecord;
use super::storage::{list_presets, upsert_preset};
use crate::db::storage::{now_unix_secs, Database};

pub fn ensure_builtin_presets(db: &Database) -> Result<Vec<SkillPresetRecord>, String> {
    let existing = list_presets(db)?;
    let now = now_unix_secs();

    let builtins = [
        (
            "preset_fullstack",
            "Fullstack Developer",
            "Frontend UI design, React/Svelte patterns, E2E webapp testing, and API tooling.",
            "fullstack",
        ),
        (
            "preset_backend",
            "Backend Developer",
            "Service architecture, MCP/API integrations, database migrations, and reliability.",
            "backend",
        ),
        (
            "preset_devops",
            "DevOps & Automation",
            "CI/CD workflows, containerization, scripting hygiene, and release automation.",
            "devops",
        ),
        (
            "preset_ai_engineer",
            "AI Engineer",
            "Skill authoring, MCP server design, prompt evaluation, and agent workflows.",
            "ai_engineer",
        ),
        (
            "preset_erp",
            "ERP Developer",
            "Enterprise document processing, PDF automation, reporting, and business workflows.",
            "erp",
        ),
    ];

    for (id, name, desc, role) in builtins {
        if !existing.iter().any(|p| p.id == id) {
            upsert_preset(
                db,
                &SkillPresetRecord {
                    id: id.to_string(),
                    name: name.to_string(),
                    description: desc.to_string(),
                    role_tag: role.to_string(),
                    is_builtin: true,
                    skill_ids: Vec::new(),
                    created_at: now,
                    updated_at: now,
                },
            )?;
        }
    }

    list_presets(db)
}
