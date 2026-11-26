use crate::db::storage::{now_unix_secs, Database};
use crate::skills::models::{
    SecurityReport, SkillRecord, SkillSourceRecord, SkillsSearchQuery,
};
use crate::skills::parser::{slugify_skill_name, stable_skill_id};
use crate::skills::storage::{list_sources, search_skills, upsert_skill_with_files, upsert_source};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct RegistryJson {
    pub schema_version: u32,
    pub name: Option<String>,
    pub skills: Vec<RegistrySkillEntry>,
}

#[derive(Debug, Deserialize)]
pub struct RegistrySkillEntry {
    pub name: String,
    pub description: String,
    pub author: Option<String>,
    pub license: Option<String>,
    pub repository_url: Option<String>,
    pub source_url: String,
    pub source_path: Option<String>,
    pub categories: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub popularity: Option<u64>,
}

struct CuratedSeed {
    name: &'static str,
    description: &'static str,
    author: &'static str,
    license: &'static str,
    repository_url: &'static str,
    source_url: &'static str,
    source_path: &'static str,
    categories: &'static [&'static str],
    tags: &'static [&'static str],
    popularity: u64,
}

const CURATED_SEEDS: &[CuratedSeed] = &[
    CuratedSeed {
        name: "Frontend Design",
        description: "Create distinctive, production-grade frontend interfaces with high design quality.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/frontend-design",
        source_path: "skills/frontend-design",
        categories: &["Frontend", "Design"],
        tags: &["ui", "css", "svelte", "react", "design"],
        popularity: 950,
    },
    CuratedSeed {
        name: "Skill Creator",
        description: "Guide for creating effective, modular AI coding-agent skills with SKILL.md.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/skill-creator",
        source_path: "skills/skill-creator",
        categories: &["AI Engineering", "Tooling"],
        tags: &["skill", "agent", "prompt", "authoring"],
        popularity: 920,
    },
    CuratedSeed {
        name: "MCP Builder",
        description: "Guide for building high-quality Model Context Protocol (MCP) servers and tools.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/mcp-builder",
        source_path: "skills/mcp-builder",
        categories: &["AI Engineering", "Backend"],
        tags: &["mcp", "tools", "protocol", "typescript", "python"],
        popularity: 890,
    },
    CuratedSeed {
        name: "Webapp Testing",
        description: "Toolkit for interacting with and testing local web applications using Playwright.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/webapp-testing",
        source_path: "skills/webapp-testing",
        categories: &["Testing", "Frontend"],
        tags: &["playwright", "e2e", "browser", "qa"],
        popularity: 860,
    },
    CuratedSeed {
        name: "PDF Processing",
        description: "Comprehensive PDF extraction, creation, merging, splitting, and form handling.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/pdf",
        source_path: "skills/pdf",
        categories: &["Documents", "Automation"],
        tags: &["pdf", "documents", "python"],
        popularity: 840,
    },
    CuratedSeed {
        name: "React Best Practices",
        description: "Performance optimization, rendering patterns, and bundle hygiene for React & Next.js.",
        author: "Vercel Labs",
        license: "MIT",
        repository_url: "https://github.com/vercel-labs/agent-skills",
        source_url: "https://github.com/vercel-labs/agent-skills/tree/main/skills/react-best-practices",
        source_path: "skills/react-best-practices",
        categories: &["Frontend", "Fullstack"],
        tags: &["react", "nextjs", "performance", "typescript"],
        popularity: 910,
    },
];

/// Ensures default sources exist and seeds the curated catalog if empty.
pub fn ensure_default_sources_and_catalog(db: &Database) -> Result<(), String> {
    let now = now_unix_secs();
    let sources = list_sources(db)?;
    let defaults = [
        ("src_curated", "Raydrop Curated Catalog", "curated", "builtin://curated", true),
        ("src_github", "GitHub Repositories", "github", "https://github.com", true),
        ("src_skills_sh", "skills.sh Directory", "skills_sh", "https://skills.sh", true),
        ("src_aitmpl", "AITMPL Templates", "aitmpl", "https://www.aitmpl.com", true),
        ("src_local", "Local Folder Imports", "local", "local://folders", true),
        ("src_zip", "ZIP Archive Imports", "zip", "zip://archives", true),
    ];

    for (id, name, ptype, url, enabled) in defaults {
        if !sources.iter().any(|s| s.id == id) {
            upsert_source(
                db,
                &SkillSourceRecord {
                    id: id.to_string(),
                    name: name.to_string(),
                    provider_type: ptype.to_string(),
                    source_url: url.to_string(),
                    enabled,
                    last_synced_at: if id == "src_curated" { Some(now) } else { None },
                    last_error: None,
                    created_at: now,
                    updated_at: now,
                },
            )?;
        }
    }

    let existing_curated = search_skills(
        db,
        &SkillsSearchQuery {
            source_id: Some("src_curated".to_string()),
            ..Default::default()
        },
    )?;

    if existing_curated.is_empty() {
        for seed in CURATED_SEEDS {
            let slug = slugify_skill_name(seed.name);
            let id = stable_skill_id("src_curated", &slug);
            let rec = SkillRecord {
                id,
                source_id: "src_curated".to_string(),
                name: seed.name.to_string(),
                slug,
                description: seed.description.to_string(),
                author: Some(seed.author.to_string()),
                license: Some(seed.license.to_string()),
                repository_url: Some(seed.repository_url.to_string()),
                source_url: seed.source_url.to_string(),
                source_path: Some(seed.source_path.to_string()),
                version: Some("catalog-ref".to_string()),
                branch: Some("main".to_string()),
                commit_sha: None,
                categories: seed.categories.iter().map(|s| s.to_string()).collect(),
                tags: seed.tags.iter().map(|s| s.to_string()).collect(),
                compatibility: Vec::new(),
                security_status: "unscanned".to_string(),
                security_report: SecurityReport::default(),
                popularity: Some(seed.popularity),
                local_package_path: None,
                last_updated: Some(now),
                created_at: now,
                updated_at: now,
                is_favorite: false,
                installed_agents: Vec::new(),
            };
            upsert_skill_with_files(db, &rec, &[])?;
        }
    }

    Ok(())
}

/// Refreshes a source (curated, custom JSON registry, or remote catalog).
pub async fn refresh_source(db: &Database, source_id: &str) -> Result<usize, String> {
    let sources = list_sources(db)?;
    let mut src = sources
        .into_iter()
        .find(|s| s.id == source_id)
        .ok_or_else(|| format!("Source not found: {}", source_id))?;

    let now = now_unix_secs();
    let res: Result<usize, String> = match src.provider_type.as_str() {
        "curated" => {
            ensure_default_sources_and_catalog(db)?;
            Ok(CURATED_SEEDS.len())
        }
        "custom_registry" => sync_custom_registry_url(db, &src).await,
        "skills_sh" | "aitmpl" => {
            // Respectful discovery check: verify endpoint reachability without aggressive scraping
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(12))
                .user_agent("Raydrop-Skills-Library/1.0")
                .build()
                .map_err(|e| e.to_string())?;
            let resp = client
                .get(&src.source_url)
                .send()
                .await
                .map_err(|e| format!("Failed reaching {}: {}", src.name, e))?;
            if !resp.status().is_success() {
                return Err(format!("{} returned HTTP {}", src.name, resp.status()));
            }
            Ok(0)
        }
        _ => Ok(0),
    };

    match res {
        Ok(count) => {
            src.last_synced_at = Some(now);
            src.last_error = None;
            src.updated_at = now;
            upsert_source(db, &src)?;
            Ok(count)
        }
        Err(err) => {
            src.last_error = Some(err.clone());
            src.updated_at = now;
            let _ = upsert_source(db, &src);
            Err(err)
        }
    }
}

async fn sync_custom_registry_url(db: &Database, src: &SkillSourceRecord) -> Result<usize, String> {
    if !src.source_url.starts_with("https://") && !src.source_url.starts_with("http://localhost") {
        return Err("Custom registry URL must use HTTPS (or http://localhost)".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Raydrop-Skills-Library/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&src.source_url)
        .send()
        .await
        .map_err(|e| format!("Failed fetching registry: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Registry returned HTTP {}", resp.status()));
    }
    let reg: RegistryJson = resp
        .json()
        .await
        .map_err(|e| format!("Invalid registry JSON schema: {}", e))?;
    if reg.schema_version != 1 {
        return Err(format!("Unsupported registry schema_version: {}", reg.schema_version));
    }

    let now = now_unix_secs();
    let count = reg.skills.len();
    for entry in reg.skills {
        let slug = slugify_skill_name(&entry.name);
        let id = stable_skill_id(&src.id, &slug);
        let rec = SkillRecord {
            id,
            source_id: src.id.clone(),
            name: entry.name,
            slug,
            description: entry.description,
            author: entry.author,
            license: entry.license,
            repository_url: entry.repository_url,
            source_url: entry.source_url,
            source_path: entry.source_path,
            version: Some("registry-ref".to_string()),
            branch: None,
            commit_sha: None,
            categories: entry.categories.unwrap_or_else(|| vec!["General".to_string()]),
            tags: entry.tags.unwrap_or_default(),
            compatibility: Vec::new(),
            security_status: "unscanned".to_string(),
            security_report: SecurityReport::default(),
            popularity: entry.popularity,
            local_package_path: None,
            last_updated: Some(now),
            created_at: now,
            updated_at: now,
            is_favorite: false,
            installed_agents: Vec::new(),
        };
        upsert_skill_with_files(db, &rec, &[])?;
    }
    Ok(count)
}
