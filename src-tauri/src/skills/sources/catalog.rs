use super::{ingest_skill_package, PackageIngestInput, RawSkillFile};
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
    CuratedSeed {
        name: "Web Design Guidelines",
        description: "Audit UI/UX accessibility, typography, contrast, layout spacing, and responsive design.",
        author: "Vercel Labs",
        license: "MIT",
        repository_url: "https://github.com/vercel-labs/agent-skills",
        source_url: "https://github.com/vercel-labs/agent-skills/tree/main/skills/web-design-guidelines",
        source_path: "skills/web-design-guidelines",
        categories: &["Design", "Frontend"],
        tags: &["ui", "ux", "accessibility", "audit"],
        popularity: 880,
    },
    CuratedSeed {
        name: "Composition Patterns",
        description: "Scalable component composition patterns, slots, and state isolation for modern web apps.",
        author: "Vercel Labs",
        license: "MIT",
        repository_url: "https://github.com/vercel-labs/agent-skills",
        source_url: "https://github.com/vercel-labs/agent-skills/tree/main/skills/composition-patterns",
        source_path: "skills/composition-patterns",
        categories: &["Frontend", "Architecture"],
        tags: &["components", "patterns", "react", "architecture"],
        popularity: 820,
    },
    CuratedSeed {
        name: "DOCX Document Processing",
        description: "Create, edit, format, and extract structured content from Microsoft Word (.docx) files.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/docx",
        source_path: "skills/docx",
        categories: &["Documents", "Automation"],
        tags: &["docx", "word", "documents", "python"],
        popularity: 850,
    },
    CuratedSeed {
        name: "XLSX Spreadsheet Automation",
        description: "Generate, analyze, and format Excel spreadsheets (.xlsx) with formulas and charts.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/xlsx",
        source_path: "skills/xlsx",
        categories: &["Documents", "Data"],
        tags: &["excel", "xlsx", "spreadsheets", "data"],
        popularity: 845,
    },
    CuratedSeed {
        name: "PPTX Presentation Builder",
        description: "Build clean, structured PowerPoint (.pptx) decks programmatically with layouts and visuals.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/pptx",
        source_path: "skills/pptx",
        categories: &["Documents", "Design"],
        tags: &["pptx", "slides", "presentations"],
        popularity: 810,
    },
    CuratedSeed {
        name: "Canvas Design",
        description: "Generate high-craft posters, editorial visuals, and vector/canvas artwork.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/canvas-design",
        source_path: "skills/canvas-design",
        categories: &["Design", "Creative"],
        tags: &["canvas", "design", "visual", "art"],
        popularity: 795,
    },
    CuratedSeed {
        name: "Theme Factory",
        description: "Apply cohesive color palettes, typography pairings, and visual themes to artifacts and UIs.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/theme-factory",
        source_path: "skills/theme-factory",
        categories: &["Design", "Frontend"],
        tags: &["theme", "colors", "typography", "css"],
        popularity: 770,
    },
    CuratedSeed {
        name: "Doc Coauthoring",
        description: "Structured workflow for co-authoring specs, RFCs, technical docs, and proposals.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/doc-coauthoring",
        source_path: "skills/doc-coauthoring",
        categories: &["Documents", "Productivity"],
        tags: &["docs", "rfc", "writing", "workflow"],
        popularity: 760,
    },
    CuratedSeed {
        name: "Internal Comms",
        description: "Draft crisp status updates, incident reports, newsletters, and leadership memos.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/internal-comms",
        source_path: "skills/internal-comms",
        categories: &["Productivity", "Documents"],
        tags: &["comms", "reports", "writing"],
        popularity: 730,
    },
    CuratedSeed {
        name: "Brand Guidelines",
        description: "Enforce consistent brand colors, typography, tone of voice, and visual identity.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/brand-guidelines",
        source_path: "skills/brand-guidelines",
        categories: &["Design", "Marketing"],
        tags: &["brand", "identity", "guidelines"],
        popularity: 740,
    },
    CuratedSeed {
        name: "Algorithmic Art",
        description: "Create generative p5.js and algorithmic art with deterministic seeded randomness.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/algorithmic-art",
        source_path: "skills/algorithmic-art",
        categories: &["Creative", "Frontend"],
        tags: &["p5js", "generative", "art", "canvas"],
        popularity: 755,
    },
    CuratedSeed {
        name: "Claude API Integration",
        description: "Best practices for Anthropic SDKs, tool use, streaming, structured outputs, and caching.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/claude-api",
        source_path: "skills/claude-api",
        categories: &["AI Engineering", "Backend"],
        tags: &["claude", "llm", "sdk", "streaming"],
        popularity: 875,
    },
    CuratedSeed {
        name: "React Native Skills",
        description: "Mobile performance, navigation, native modules, and animation patterns for React Native.",
        author: "Vercel Labs",
        license: "MIT",
        repository_url: "https://github.com/vercel-labs/agent-skills",
        source_url: "https://github.com/vercel-labs/agent-skills/tree/main/skills/react-native-skills",
        source_path: "skills/react-native-skills",
        categories: &["Mobile", "Frontend"],
        tags: &["react-native", "mobile", "ios", "android"],
        popularity: 805,
    },
    CuratedSeed {
        name: "Web Artifacts Builder",
        description: "Build self-contained multi-component interactive HTML/React/Tailwind web artifacts.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/web-artifacts-builder",
        source_path: "skills/web-artifacts-builder",
        categories: &["Frontend", "Tooling"],
        tags: &["html", "tailwind", "react", "artifacts"],
        popularity: 815,
    },
    CuratedSeed {
        name: "Slack GIF Creator",
        description: "Create crisp, optimized animated GIFs tailored for team chat and Slack reactions.",
        author: "Anthropic",
        license: "Apache-2.0",
        repository_url: "https://github.com/anthropics/skills",
        source_url: "https://github.com/anthropics/skills/tree/main/skills/slack-gif-creator",
        source_path: "skills/slack-gif-creator",
        categories: &["Creative", "Tooling"],
        tags: &["gif", "animation", "slack"],
        popularity: 710,
    },
];

/// Ensures default sources exist and seeds the curated catalog if empty.
pub fn ensure_default_sources_and_catalog(db: &Database) -> Result<(), String> {
    let now = now_unix_secs();
    let sources = list_sources(db)?;
    let defaults = [
        ("src_curated", "Raydrop Curated Catalog", "curated", "builtin://curated", true),
        ("src_gh_anthropics", "Anthropic Official Skills", "github_repo", "https://github.com/anthropics/skills", true),
        ("src_gh_vercel", "Vercel Labs Agent Skills", "github_repo", "https://github.com/vercel-labs/agent-skills", true),
        ("src_gh_superpowers", "Superpowers Agent Skills (obra)", "github_repo", "https://github.com/obra/superpowers", true),
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

    for seed in CURATED_SEEDS {
        let slug = slugify_skill_name(seed.name);
        let id = stable_skill_id("src_curated", &slug);
        if existing_curated.iter().any(|s| s.id == id) {
            continue;
        }
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

    ensure_builtin_context_skills(db)?;
    Ok(())
}

struct BuiltinSkillPackage {
    slug: &'static str,
    skill_md: &'static str,
}

const BUILTIN_CONTEXT_SKILLS: &[BuiltinSkillPackage] = &[
    BuiltinSkillPackage {
        slug: "codebase-context-mapper",
        skill_md: r#"---
name: Codebase Context Mapper
description: Efficiently map repository architecture, entrypoints, data flow, and conventions without wasting context window.
author: Raydrop
license: MIT
version: 1.0.0
categories:
  - Context
  - Architecture
tags: [context, architecture, codebase, navigation, efficiency]
---

# Codebase Context Mapper

Use this workflow before editing an unfamiliar or large repository so you build accurate mental models with minimal token/context waste.

## 1. Locate Before Reading
1. List top-level manifests (`package.json`, `Cargo.toml`, `pyproject.toml`, `go.mod`, `tauri.conf.json`) and directory layout first.
2. Search for exact symbol names, routes, IPC commands, or error strings before opening files.
3. Read only the relevant line ranges (50-150 lines) around call sites and definitions instead of dumping entire large files.

## 2. Map the Core Spine
Trace and record:
- **Entrypoints**: main binary/app bootstrap, router, and UI shell.
- **State & Persistence**: stores, database schemas, migrations, and cache keys.
- **Boundaries**: IPC/HTTP/RPC contracts, auth/permission checks, and external providers.
- **Build & Test Commands**: exact scripts from manifests and CI workflows.

## 3. Rules for Context Discipline
- Never guess unverified file paths, function signatures, or config keys.
- Re-read a file only after it changes.
- Batch independent searches and reads in parallel before planning edits.
"#,
    },
    BuiltinSkillPackage {
        slug: "session-context-handoff",
        skill_md: r#"---
name: Session Context & Handoff
description: Preserve coding task state, verified facts, changed files, and exact next steps across long sessions or agent handoffs.
author: Raydrop
license: MIT
version: 1.0.0
categories:
  - Context
  - Productivity
tags: [context, handoff, memory, workflow, continuity]
---

# Session Context & Handoff

Keep multi-step engineering sessions reliable when context grows large or work is handed off between coding agents.

## What to Track Continuously
1. **User Goal & Constraints**: exact requirements, non-goals, and environment constraints.
2. **Files Touched**: exact file paths and what changed in each.
3. **Verified State**: only commands, tests, and builds actually executed after the latest edit (never assume passing status from earlier runs).
4. **Open Risks & Next Steps**: concrete, ordered remaining actions with exact symbol/file targets.

## Anti-Drift Rules
- Separate **observed facts** (read from files/tool output) from **inferences**.
- Do not re-audit already completed work unless new regressions or requirements appear.
- Before finishing, re-verify the build/tests after the final edit and report exact scope and results.
"#,
    },
    BuiltinSkillPackage {
        slug: "api-context-verification",
        skill_md: r#"---
name: API & Library Context Verification
description: Verify exact library versions, types, and APIs from local manifests and source before writing integration code.
author: Raydrop
license: MIT
version: 1.0.0
categories:
  - Context
  - Tooling
tags: [context, api, versions, types, verification]
---

# API & Library Context Verification

Prevent version-drift bugs and hallucinated APIs by anchoring every library call to actual project context.

## Checklist Before Writing Integration Code
1. **Check Exact Installed Versions**: inspect `package.json` / `package-lock.json` / `Cargo.toml` / `Cargo.lock` (e.g., Svelte 4 vs Svelte 5, Tauri v1 vs v2, Reqwest 0.12 vs 0.13).
2. **Match Repo Patterns**: inspect 1-2 existing callers in the repository to match error handling, IPC wrappers, and store conventions.
3. **Verify Schema & Types**: check TypeScript interfaces and Rust `#[derive(Serialize, Deserialize)]` DTOs side-by-side so field names and optionality match 100%.
4. **Validate Edge Cases**: check platform guards (`#[cfg(target_os = ...)]`), nullability, and async error propagation.
"#,
    },
    BuiltinSkillPackage {
        slug: "performance-optimization-skill",
        skill_md: r#"---
name: Performance Optimization Skill
description: CLI performance optimization - startup time, memory usage, token savings benchmarking.
author: Raydrop
license: MIT
version: 1.0.0
categories:
  - Performance
  - Backend
tags: [performance, cli, benchmark, memory, startup, rust]
---

# Performance Optimization Skill

Systematic performance analysis and optimization for CLI tools, focusing on **startup time (<10ms)**, **memory usage (<5MB)**, and **token savings (>=20% floor)**.

## When to Use
- After filter changes, regex modifications, or dependency additions
- When performance degradation is suspected or before a release
- After any code change that could impact startup time, memory, or binary size

## Performance Analysis Workflow
1. **Establish Baseline**: capture startup time (`hyperfine`), resident memory (`/usr/bin/time`), and stripped binary size before editing.
2. **Optimize**: eliminate repeated regex compilation (`LazyLock`), avoid startup file I/O, and minimize heap allocations.
3. **Rebuild and Measure**: run `cargo build --release` and re-benchmark with `hyperfine`.
4. **Identify Regressions**: profile startup regressions (>15% or >2ms) with `cargo flamegraph` and heap allocations with `dhat`.
"#,
    },
    BuiltinSkillPackage {
        slug: "reverse-engineer-anything",
        skill_md: r#"---
name: Reverse Engineer Anything
description: Reverse engineer native, managed, Electron/JavaScript, packaged, firmware, and browser targets with REA, and build AgentEnv bundles from live instances or captures.
author: morluto
license: MIT
version: 1.0.0
categories:
  - Security
  - Analysis
tags: [reverse-engineering, binary, asar, electron, firmware, rea]
---

# Reverse Engineer Anything (REA)

Use REA when a claim depends on a shipped binary or package, decompilation, requested runtime evidence, or comparison with behavior not established by available source. Skip REA for ordinary source-repository architecture analysis.

## Quick Routing
- **ASAR / Electron / JavaScript tree**: `open_binary` -> `inspect_artifact` (`detail: "summary"`), or CLI fallback:
  `npx -y rea-agents@latest analyze-javascript-application /absolute/path/to/app --json`
- **Archives / Installers (ZIP, APK, IPA, MSIX, DMG, PyInstaller)**: `open_binary` -> `inspect_artifact`
- **Managed PE/.NET assembly**: `inspect_managed_artifact`
- **Android APK**: `inspect_android_package`
- **Firmware / ESP32**: `inspect_artifact` / `inspect_firmware_regions`

## Rules
- Never invent unverified runtime behavior; ground every claim in observed Evidence records, hashes, and tool outputs.
- Keep complete inline Evidence when retained references are unavailable.
"#,
    },
];

pub fn ensure_builtin_context_skills(db: &Database) -> Result<(), String> {
    let existing = search_skills(
        db,
        &SkillsSearchQuery {
            source_id: Some("src_curated".to_string()),
            ..Default::default()
        },
    )?;

    for pkg in BUILTIN_CONTEXT_SKILLS {
        let source_url = format!("builtin://curated/{}", pkg.slug);
        let id = stable_skill_id(&source_url, pkg.slug);
        if existing.iter().any(|s| s.id == id && s.local_package_path.is_some()) {
            continue;
        }
        let _ = ingest_skill_package(
            db,
            PackageIngestInput {
                source_id: "src_curated".to_string(),
                source_name: "Raydrop Curated Catalog".to_string(),
                provider_type: "curated".to_string(),
                source_url,
                source_path: None,
                repository_url: None,
                branch: None,
                commit_sha: None,
                license_hint: Some("MIT".to_string()),
                fallback_name: pkg.slug.to_string(),
                files: vec![RawSkillFile {
                    relative_path: "SKILL.md".to_string(),
                    bytes: pkg.skill_md.as_bytes().to_vec(),
                    is_executable: false,
                }],
            },
        )?;
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
        "github_repo" => sync_github_repo_source(db, &src).await,
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

async fn sync_github_repo_source(db: &Database, src: &SkillSourceRecord) -> Result<usize, String> {
    let parsed = super::github::parse_github_url(&src.source_url)?;
    let (branch, commit_sha, discovered) = super::github::discover_github_skills(&src.source_url).await?;
    let existing = search_skills(db, &SkillsSearchQuery::default())?;

    let now = now_unix_secs();
    let count = discovered.len();
    let repo_url = format!("https://github.com/{}/{}", parsed.owner, parsed.repo);

    for d in discovered {
        let slug = slugify_skill_name(&d.name);
        let rel = d.relative_path.trim_matches('/');
        let skill_url = if rel.is_empty() {
            repo_url.clone()
        } else {
            format!("{}/tree/{}/{}", repo_url, branch, rel)
        };
        let id = stable_skill_id(&skill_url, &slug);
        let prev = existing.iter().find(|s| s.id == id);

        let rec = SkillRecord {
            id,
            source_id: src.id.clone(),
            name: d.name,
            slug,
            description: d.description,
            author: Some(parsed.owner.clone()),
            license: prev.and_then(|p| p.license.clone()),
            repository_url: Some(repo_url.clone()),
            source_url: skill_url,
            source_path: if rel.is_empty() { None } else { Some(rel.to_string()) },
            version: Some("github-sync".to_string()),
            branch: Some(branch.clone()),
            commit_sha: Some(commit_sha.clone()),
            categories: vec!["Community".to_string()],
            tags: d.tags,
            compatibility: Vec::new(),
            security_status: prev
                .map(|p| p.security_status.clone())
                .unwrap_or_else(|| "unscanned".to_string()),
            security_report: prev
                .map(|p| p.security_report.clone())
                .unwrap_or_default(),
            popularity: prev.and_then(|p| p.popularity).or(Some(800)),
            local_package_path: prev.and_then(|p| p.local_package_path.clone()),
            last_updated: Some(now),
            created_at: prev.map(|p| p.created_at).unwrap_or(now),
            updated_at: now,
            is_favorite: prev.map(|p| p.is_favorite).unwrap_or(false),
            installed_agents: Vec::new(),
        };
        upsert_skill_with_files(db, &rec, &[])?;
    }

    Ok(count)
}
