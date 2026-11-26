# Raydrop AI Skills Library — Implementation Plan

> **Project:** Raydrop (`ShadowOrgs/raydrop`)  
> **Feature:** AI Skills Library / Universal Skills Manager  
> **Platform:** Existing Raydrop desktop app (Tauri v2 + Rust + Svelte 5 + SQLite)  
> **Status:** Proposed implementation plan  
> **Primary goal:** Discover, import, inspect, install, update, and manage coding-agent skills across supported agents from a single local-first desktop UI.

---

## 1. Goals

Build an integrated AI Skills Library in Raydrop that allows users to:

- Discover skills from multiple sources.
- Search, filter, sort, favorite, and preview skills.
- Import skills from GitHub URLs, local folders, and ZIP archives.
- Detect installed coding agents and configure their skill paths.
- Install one skill into one or multiple agents.
- Install globally or into a selected project.
- Manage installed skills: view, update, uninstall, back up, and roll back.
- Track source, version/commit, files, hashes, compatibility, and install history.
- Review security warnings before installation.
- Keep the feature local-first without requiring a cloud service.

## 2. Non-goals for MVP

Do **not** implement these in the MVP:

- Running skill scripts or commands.
- Installing skill dependencies automatically.
- Running an AI agent as part of the installer.
- Cloud sync or user accounts.
- Paid marketplace or community upload backend.
- AI-generated skills.
- Automatically executing setup instructions or hooks.
- Full marketplace crawler at scale.
- Automatic modification of unrelated agent configuration files.

These may be considered later after the basic import/install lifecycle is stable.

## 3. Repository integration requirements

Before coding, inspect the existing Raydrop repository and identify the project’s conventions for:

- Svelte navigation and view routing.
- Component styling, theme, reusable UI primitives, icons, and layout.
- State/store management.
- Tauri command registration and IPC wrappers.
- SQLite schema, migration strategy, database initialization, and queries.
- Existing HTTP/download utilities.
- Filesystem path handling and OS-specific utilities.
- Logging, error handling, and notifications.
- Tests and CI/release configuration.

Use the existing patterns wherever practical. Avoid broad refactors and unrelated changes. Keep Skills Library modular and isolated.

Current project architecture (verify against checked-out code; do not assume these details remain unchanged):

- Frontend: Svelte 5.
- Desktop/backend: Tauri v2 + Rust.
- Persistence: SQLite.
- UI components: `src/lib/components/`.
- Native backend: `src-tauri/src/`.
- Existing IPC utility: `src/lib/utils/tauri-ipc.ts`.

## 4. Proposed navigation and UX

Add an AI Skills Library entry to Raydrop’s existing navigation. Reuse the current design system and navigation behavior.

### Main sections

1. **Discover**
   - Unified search.
   - Category and tag filters.
   - Source filter.
   - Sort: relevance, popularity (when source provides it), recently updated, alphabetical.
   - Curated/recommended section.
   - Favorites and recently viewed.
   - Pagination or incremental loading.

2. **Installed**
   - Installed skill list.
   - Filter by agent, scope, project, source, and update status.
   - View install location and installed version/commit.
   - Update, reinstall, uninstall, backup, and rollback actions.
   - Detect missing/orphaned installations and reconcile with filesystem.

3. **Agents**
   - Detected agents and status.
   - Agent skill compatibility.
   - Global and project skill paths.
   - Manual path configuration.
   - Re-detection and validation.
   - Show detection confidence/reason; do not infer an agent is absent solely because its executable is not in PATH.

4. **Sources**
   - Enable/disable sources.
   - Add GitHub repository or direct skill URL.
   - Add supported catalog/registry source.
   - Import local folder or ZIP.
   - View source details and last sync.
   - Source errors and retry.

### Skill detail page / panel

Display:

- Name, description, author, tags, categories.
- Source and repository URL.
- License when available; otherwise clearly show “Unknown”.
- Last updated, stars/download counts only when the source provides them.
- Version/branch/commit SHA.
- Compatibility information and caveats.
- Skill file tree and file preview.
- Security scan summary and warnings.
- Install action.

### Install dialog

Allow users to:

- Select one or multiple detected/configured agents.
- Select Global or Project scope.
- Select project directory using native folder picker.
- Preview exact destination paths.
- Resolve duplicate skill names.
- Choose install, update, overwrite, or keep existing where applicable.
- See warnings and confirm risky content.
- View progress, success, and failure per target.

## 5. Architecture

Use a local-first modular architecture:

```text
Svelte UI
   |
Tauri IPC commands
   |
Skills Service / Application Layer
   |-- Catalog / Search
   |-- Source Providers
   |-- Skill Package Validation
   |-- Security Scanner (static heuristics)
   |-- Agent Detection
   |-- Compatibility Resolver
   |-- Installer / Backup / Rollback
   |-- Update Checker
   |-- SQLite Repository
   |
Local filesystem + external sources (GitHub, catalogs, local folders, ZIP)
```

### Backend module suggestion

Adjust to existing repository conventions after auditing. Suggested structure:

```text
src-tauri/src/skills/
├── mod.rs
├── commands.rs
├── service.rs
├── models.rs
├── repository.rs
├── catalog.rs
├── installer.rs
├── detector.rs
├── compatibility.rs
├── security.rs
├── updates.rs
├── backup.rs
└── sources/
    ├── mod.rs
    ├── github.rs
    ├── skills_sh.rs
    ├── aitmpl.rs
    ├── registry.rs
    ├── local.rs
    └── zip.rs
```

Suggested frontend structure (adapt to current organization):

```text
src/lib/components/
├── SkillsLibraryView.svelte
├── SkillsDiscoverView.svelte
├── SkillsInstalledView.svelte
├── SkillsDetailView.svelte
├── SkillsInstallModal.svelte
├── SkillsAgentManager.svelte
├── SkillsSourceManager.svelte
├── SkillsSecurityPanel.svelte
└── skills/
    ├── SkillCard.svelte
    ├── SkillFilters.svelte
    ├── SkillFileTree.svelte
    ├── SkillPreview.svelte
    ├── SkillCompatibility.svelte
    ├── SkillInstallTarget.svelte
    ├── SkillUpdateDiff.svelte
    └── SkillSecurityReport.svelte
```

Do not create every suggested file if existing components or patterns provide a better organization. Avoid giant monolithic Svelte components.

## 6. Domain models

Define backend models and matching IPC DTOs. Keep source-specific data separate from normalized fields when needed.

### Skill metadata

Suggested fields:

- `id` — stable internal identifier.
- `source_id`.
- `name`.
- `description`.
- `author` (optional).
- `license` (optional).
- `repository_url` (optional).
- `source_url`.
- `source_path` (optional).
- `version` (optional).
- `commit_sha` (optional).
- `branch` (optional).
- `tags`.
- `categories`.
- `compatibility`.
- `last_updated` (optional).
- `popularity` (optional; only if provided by source).
- `security_status`.
- `created_at`, `updated_at`.

### Skill package

A skill package consists of:

- Metadata.
- Validated relative file paths.
- File contents (text and binary classification).
- Per-file size and hash.
- Source provenance and pinned commit where available.
- Validation and security report.

### Compatibility levels

- `Native`: official/native skill support and expected directory/format.
- `Compatible`: compatible with the standard skill format and agent’s documented conventions.
- `Adapted`: metadata/path transformation required and supported by the adapter.
- `Unsupported`: do not allow installation unless a future explicit, safe adapter capability exists.

Do not label compatibility based only on marketing claims. Validate agent paths and skill conventions against current official documentation. Make compatibility data configurable and testable.

## 7. Source providers

Implement source-provider abstraction so adding a source does not require rewriting the catalog or installer.

Suggested interface (language-neutral concept):

```rust
trait SkillSourceProvider {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    // Search/catalog support is optional per provider.
    async fn search(&self, query: &str, filters: &SearchFilters)
        -> Result<Vec<SkillSearchResult>>;
    async fn get_skill(&self, locator: &SkillLocator)
        -> Result<SkillPackage>;
    async fn list_files(&self, locator: &SkillLocator)
        -> Result<Vec<SkillFileMetadata>>;
}
```

Use capability flags because not every provider supports search, popularity, versions, or catalog synchronization.

### Provider implementation order

#### P0 — GitHub direct import

Support:

- Repository URL.
- Repository tree/folder URL.
- Skill folder URL.
- GitHub API or another reliable repository-tree method.
- Discover `SKILL.md`.
- Fetch full skill folder and relevant nested files.
- Detect branch and resolve to immutable commit SHA.
- Pin imported package to that SHA.
- Fetch license and metadata where possible.
- Respect API rate limits and show actionable errors.
- Support public repositories for MVP. Private repository support can be added later through secure user-authorized authentication; never ask users to paste tokens into logs or store plaintext credentials.

URL examples to support and test:

```text
https://github.com/owner/repo
https://github.com/owner/repo/tree/main/path/to/skill
https://github.com/owner/repo/tree/develop/skills/example
```

Do not assume `main` exists. Resolve default branch via repository metadata.

#### P0 — Local folder import

- User selects folder via native picker.
- Find `SKILL.md` at root or allow selection of a skill subfolder.
- Validate metadata and file tree.
- Copy package into Raydrop-managed local library (not just reference the original path) so it remains available if original files move.
- Record local source provenance.

#### P0 — ZIP import

- Native file picker.
- Safe extraction into staging.
- Detect root folder and `SKILL.md`.
- Prevent Zip Slip/path traversal, absolute paths, drive paths, UNC paths, and paths escaping staging directory.
- Enforce file count, total extracted size, and per-file size limits.
- Reject or safely handle symlinks; MVP should reject symlinks unless a strict safe policy is implemented.
- Do not execute scripts or install dependencies.

#### P1 — skills.sh discovery/import

- Research current supported public interfaces/API/CLI behavior first.
- Prefer documented endpoints or reliable, license-compliant metadata access.
- Treat skills.sh as discovery when it links to upstream; fetch actual files from original repository and pin to commit.
- Do not scrape aggressively or rely on undocumented API without a fallback and clear provider error.

#### P1 — AITMPL provider

- Audit available data/API and terms before implementation.
- Use AITMPL as discovery/template source where possible.
- For linked upstream skill content, fetch from upstream repository and preserve original source attribution.
- Do not assume all AITMPL components are skills; classify templates, agents, commands, hooks, MCP, settings, and skills separately.
- MVP Skills Library should import only skill packages; other component types may be future modules.

#### P2 — Curated/custom registry

- Define a versioned registry JSON format.
- Allow custom registry URL.
- Validate schema and URL policy.
- Cache registry data.
- Display source attribution.
- Support registry refresh and error states.

## 8. Agent adapter system

Create an adapter/registry system. Avoid hardcoding agent-specific behavior throughout UI or installer code.

### Adapter capabilities

Each agent adapter should provide:

- Stable agent ID and display name.
- Detection methods and confidence/reasons.
- Skill format/conventions.
- Supported scopes.
- Default skill paths.
- Optional manual path overrides.
- Compatibility validation.
- Install destination resolution.
- Installed-skill discovery.
- Install/update/uninstall.
- Optional metadata transformation only when required.
- Capability flags.

### Initial agent support

MVP:

1. Codex.
2. Claude Code.

Add next:

3. Cursor.
4. Gemini CLI.
5. OpenCode.
6. GitHub Copilot.
7. Windsurf.
8. Cline.
9. Roo Code.
10. Continue.

This list is a roadmap, not a promise that all agents use identical formats or paths. Verify each agent against current official documentation and test on supported OSes.

### Agent detection

Detection should use multiple signals:

- Executable found in PATH (when appropriate).
- Known configuration directory.
- Known agent settings files.
- Existing skills directory.
- User-configured paths.

Statuses:

- `Detected`.
- `NotDetected`.
- `ManualConfiguration`.
- `PathInvalid`.
- `UnsupportedPlatform`.

Include a “Configure path” option and validate user-selected directories.

## 9. Installer and filesystem safety

The installer must be transactional as far as practical and never execute package content.

### Install pipeline

1. Resolve selected skill snapshot/source and commit.
2. Validate metadata and file list.
3. Validate all relative paths.
4. Apply file count/size limits.
5. Run static security scanner.
6. Resolve agent compatibility.
7. Resolve exact target path and scope.
8. Check for existing skill and conflict.
9. Show install preview, warnings, and overwrite policy.
10. Stage package in temporary directory adjacent to target when possible.
11. Copy validated files.
12. Create backup of existing installation when replacing.
13. Replace using safe rename strategy where filesystem permits.
14. Verify installed files and hashes.
15. Persist installation record and history.
16. Clean staging directory.
17. Report per-agent success/failure.

### Path validation

- Only allow validated relative paths within package.
- Reject absolute paths, drive-qualified paths, UNC paths, parent traversal, and invalid path components.
- Canonicalize destination and check containment.
- Account for symlink behavior; reject symlink package entries in MVP.
- Never let package metadata control arbitrary destination paths.
- Agent paths come from trusted adapter definitions or explicit user configuration.
- Avoid writing outside the selected agent skill directory.
- Use platform-safe filesystem APIs.

### Conflict handling

When the target skill exists:

- Compare source and installed hashes/version.
- Offer `Update`, `Backup & Replace`, `Keep Existing`, or `Cancel` as appropriate.
- Never silently overwrite.
- Back up before replacement.
- Keep rollback information.
- Avoid deleting files not owned by Raydrop unless user explicitly confirms a safe removal.

### Uninstall

- Track files installed by Raydrop and their hashes.
- Verify current file hashes before removal.
- If files changed after install, warn and offer to preserve modified files.
- Remove only files/directories owned by the recorded installation.
- Do not delete shared parent directories containing unrelated files.

## 10. Security scanning and trust

Skill content is untrusted instructions and may contain malicious prompt injection or scripts.

### Scanner requirements

Static heuristics, no execution:

- Shell scripts: `.sh`, `.bash`, `.zsh`.
- PowerShell: `.ps1`.
- Windows scripts: `.bat`, `.cmd`.
- Python and other executable scripts.
- Executable permission bits.
- Obfuscated/base64 payload patterns.
- Download-and-execute patterns.
- Credential/token harvesting patterns.
- Attempts to access common secrets locations.
- Destructive commands.
- Network exfiltration indicators.
- Prompt injection patterns instructing the agent to ignore rules, reveal secrets, or transmit data.
- Symlinks.
- Hidden files and unexpected file types.
- Large files and suspicious binary content.

### Security UI

Show:

- Scan status.
- Severity (`Info`, `Low`, `Medium`, `High`, `Critical`).
- Finding title and explanation.
- File path and line/offset when available.
- Why it matters.
- User action/recommendation.

Do not claim scanner proves a skill is safe. Use wording such as “No known suspicious patterns detected” instead of “Safe”.

### Install policy

- Never execute skill scripts.
- Never run package managers or setup commands.
- No post-install hooks.
- No automatic dependencies.
- Require confirmation for High/Critical findings; allow configurable policy but never silently bypass.
- Preserve source and commit provenance.
- Make the skill’s instruction content visible before installation.

## 11. SQLite persistence

Use existing Raydrop SQLite setup and migration conventions. Avoid a second database unless existing architecture requires it.

Suggested logical tables (adapt naming and schema style):

### `skill_sources`

- `id`
- `name`
- `provider_type`
- `source_url`
- `enabled`
- `last_synced_at`
- `last_error`
- `created_at`
- `updated_at`

### `skills_catalog`

- `id`
- `source_id`
- `name`
- `description`
- `author`
- `license`
- `repository_url`
- `source_url`
- `source_path`
- `version`
- `branch`
- `commit_sha`
- `categories_json`
- `tags_json`
- `compatibility_json`
- `security_status`
- `last_updated`
- `created_at`
- `updated_at`

### `skill_files`

- `id`
- `skill_id`
- `relative_path`
- `content_hash`
- `size`
- `file_type`
- `content_storage_ref` or managed-library path

Avoid storing arbitrary binary file contents directly in SQLite unless existing storage patterns justify it. Use a managed local package directory and store metadata/hashes in DB.

### `agents`

- `id`
- `agent_type`
- `display_name`
- `detected_status`
- `configured_path`
- `global_skill_path`
- `project_skill_path`
- `compatibility_level`
- `last_detected_at`

### `skill_installations`

- `id`
- `skill_id`
- `agent_id`
- `scope`
- `project_path`
- `install_path`
- `installed_version`
- `installed_commit_sha`
- `installed_manifest_json`
- `installed_at`
- `updated_at`
- `status`

### `skill_install_history`

- `id`
- `installation_id`
- `action`
- `previous_version`
- `new_version`
- `backup_path`
- `timestamp`
- `result`
- `error_message`

### `skill_favorites`

- `skill_id`
- `created_at`

### `skill_presets` and `skill_preset_items` (P2)

Create these in a later migration only when preset functionality is implemented.

## 12. IPC commands and events

Follow Raydrop’s existing Tauri command naming and error conventions. These are conceptual operations; adapt signatures and DTOs.

### Catalog / import

- `skills_search`
- `skills_get_detail`
- `skills_get_files`
- `skills_import_github`
- `skills_import_local_folder`
- `skills_import_zip`
- `skills_refresh_source`
- `skills_list_sources`
- `skills_add_source`
- `skills_update_source`
- `skills_remove_source`

### Agents

- `skills_detect_agents`
- `skills_get_agents`
- `skills_configure_agent_path`
- `skills_validate_agent_path`
- `skills_get_compatibility`

### Install management

- `skills_preview_install`
- `skills_install`
- `skills_install_batch`
- `skills_list_installed`
- `skills_uninstall`
- `skills_update_check`
- `skills_update`
- `skills_rollback`
- `skills_get_install_history`

### Favorites / settings

- `skills_toggle_favorite`
- `skills_list_favorites`
- `skills_get_settings`
- `skills_update_settings`

### Events

Use events for long-running operations if consistent with Raydrop:

- `skills-import-progress`
- `skills-install-progress`
- `skills-update-progress`
- `skills-source-sync-progress`

Do not emit secrets, full environment variables, or sensitive file contents in events/logs.

## 13. Frontend state and UI implementation

Use existing Svelte stores/state patterns. Avoid duplicating server/backend state in multiple places.

Suggested states:

- `catalogSearch`.
- `catalogFilters`.
- `selectedSkill`.
- `selectedAgents`.
- `installScope`.
- `projectDirectory`.
- `installedSkills`.
- `agentStatus`.
- `sourceStatus`.
- `activeOperation`.
- `operationProgress`.
- `securityReport`.

UX details:

- Loading, empty, offline, rate-limited, and error states.
- Search debounce.
- Cancelable requests where practical.
- Keyboard navigation/accessibility.
- Native file/folder dialogs.
- Confirmation dialogs for destructive actions.
- Per-target status for batch installs.
- Clear errors with retry guidance.
- Keep UI responsive during downloads/scans/installations.
- Reuse Raydrop styling, theme, icons, spacing, modal and notification conventions.
- Do not introduce a new UI framework.

## 14. Updates, versions, and rollback

- Imported GitHub skills pin to commit SHA.
- Track source version and installed version separately.
- Update check compares upstream commit/file hashes with installed snapshot.
- Show changed file list and diff before update.
- Allow update one skill or batch update.
- Back up current installation before update.
- Rollback restores backup and installation metadata.
- Preserve local user modifications; warn when update would overwrite modified files.
- Keep backup retention configurable.
- Do not auto-update by default in MVP.

## 15. Implementation phases and task checklist

### Phase 0 — Repository audit and design

- [ ] Inspect current Raydrop navigation and view architecture.
- [ ] Inspect SQLite initialization and migration strategy.
- [ ] Inspect Tauri command registration and IPC error patterns.
- [ ] Inspect filesystem utilities and native file/folder pickers.
- [ ] Inspect HTTP client/downloader and cache conventions.
- [ ] Inspect logging, notifications, and operation progress patterns.
- [ ] Inspect current testing and CI setup.
- [ ] Confirm OS-specific paths and app permissions/capabilities.
- [ ] Produce a brief audit note and final module integration design.
- [ ] Avoid unrelated refactors.

**Exit criteria:** design is compatible with the existing project patterns and all required capabilities/permissions are understood.

### Phase 1 — Foundation and managed library

- [ ] Add skills module registration.
- [ ] Add DB migrations.
- [ ] Implement models and repository layer.
- [ ] Implement `SKILL.md` parser and metadata validation.
- [ ] Implement managed local library storage.
- [ ] Implement local-folder import.
- [ ] Implement safe ZIP import.
- [ ] Implement file hashes and package manifests.
- [ ] Add basic static security scan.
- [ ] Add source provenance tracking.
- [ ] Add frontend navigation entry and Skills Library shell.
- [ ] Add Discover/Library empty and loading states.
- [ ] Add detail/preview panel.
- [ ] Add import UX and error handling.

**Exit criteria:** local folder/ZIP can be imported, validated, stored, browsed, and previewed.

### Phase 2 — Agent detection and first installers

- [ ] Implement adapter registry and capability model.
- [ ] Implement Codex adapter using verified current paths.
- [ ] Implement Claude Code adapter using verified current paths.
- [ ] Implement agent detection with multiple signals.
- [ ] Add manually configurable agent path.
- [ ] Validate configured path and permissions.
- [ ] Implement global scope.
- [ ] Implement project scope and native project folder picker.
- [ ] Implement install preview.
- [ ] Implement conflict policy.
- [ ] Implement staging, backup, atomic replacement where possible.
- [ ] Track installed files and hashes.
- [ ] Implement installed list.
- [ ] Implement safe uninstall.
- [ ] Add per-agent install results and operation progress.

**Exit criteria:** import a skill, preview it, install to Codex and Claude globally or to a project, view installation, and uninstall without harming unrelated files.

### Phase 3 — GitHub provider

- [ ] Implement GitHub URL parsing and validation.
- [ ] Resolve repository metadata/default branch.
- [ ] Browse tree and identify skill directories.
- [ ] Support repo root and skill subfolder URLs.
- [ ] Discover and parse `SKILL.md`.
- [ ] Fetch package files with size limits.
- [ ] Resolve/pin commit SHA.
- [ ] Capture license and source metadata where available.
- [ ] Add GitHub search/discovery only if supported through reliable documented means.
- [ ] Handle rate limiting and retry/backoff.
- [ ] Add cache.
- [ ] Add GitHub import UX.
- [ ] Test URL variants and repository errors.

**Exit criteria:** user can paste a public GitHub repository/folder URL, browse/select a skill, inspect package contents, and install a pinned snapshot.

### Phase 4 — Multi-agent support

- [ ] Cursor adapter.
- [ ] Gemini CLI adapter.
- [ ] OpenCode adapter.
- [ ] GitHub Copilot adapter.
- [ ] Windsurf adapter.
- [ ] Cline adapter.
- [ ] Roo Code adapter.
- [ ] Continue adapter.
- [ ] Verify each adapter’s current official documentation.
- [ ] Add per-agent compatibility validation.
- [ ] Add batch/multi-agent installation.
- [ ] Add manual path overrides.
- [ ] Add agent capability and detection UI.
- [ ] Add tests per adapter and OS.

**Exit criteria:** batch install works only for validated supported targets; unsupported combinations are explained and blocked.

### Phase 5 — Catalog providers

- [ ] Research skills.sh discovery and import options.
- [ ] Implement skills.sh provider based on documented/reliable interface.
- [ ] Research AITMPL API/data availability and terms.
- [ ] Implement AITMPL discovery where practical.
- [ ] Resolve upstream content and preserve attribution.
- [ ] Add source manager UI.
- [ ] Add unified search.
- [ ] Add filters, categories, favorites, and sorting.
- [ ] Add cache and source sync status.
- [ ] Add curated catalog.
- [ ] Add custom registry schema and validation.

**Exit criteria:** users can discover/import skills from multiple sources in a unified catalog, with upstream source retained.

### Phase 6 — Security, update, and lifecycle hardening

- [ ] Expand scanner heuristics.
- [ ] Add findings severity and evidence.
- [ ] Add file-by-file preview.
- [ ] Add source trust/provenance display.
- [ ] Add update checker.
- [ ] Add version/commit pinning and comparisons.
- [ ] Add update diff.
- [ ] Add backup and rollback UI.
- [ ] Detect modified installed files.
- [ ] Prevent unsafe uninstall of modified files.
- [ ] Add audit history.
- [ ] Add cleanup and orphan reconciliation.
- [ ] Add limits and resource protection.
- [ ] Conduct security review.

**Exit criteria:** updates are reviewable, rollback works, modifications are preserved/warned, and risky packages cannot be installed silently.

### Phase 7 — Presets and advanced features

- [ ] Preset model.
- [ ] Fullstack Developer preset.
- [ ] Backend Developer preset.
- [ ] DevOps preset.
- [ ] AI Engineer preset.
- [ ] ERP Developer preset.
- [ ] Custom bundles.
- [ ] Export/import library and presets.
- [ ] Team/private registry support.
- [ ] Optional AI skill recommendations.
- [ ] Optional AI-assisted compatibility/security review (clearly not a security guarantee).

**Exit criteria:** users can manage repeatable skill bundles and move configurations between environments.

## 16. MVP acceptance checklist

MVP is complete when:

- [ ] Skills Library integrates into existing Raydrop navigation and UI patterns.
- [ ] User can import a skill from local folder, ZIP, and public GitHub.
- [ ] Skill metadata and files are validated.
- [ ] Preview displays metadata, file tree, and file contents.
- [ ] Basic static security report is shown.
- [ ] Codex and Claude Code can be detected/configured.
- [ ] User can choose global or project install.
- [ ] User can install one skill to multiple supported agents.
- [ ] Install destination is previewed.
- [ ] Existing installations are never silently overwritten.
- [ ] Installation is staged and verified.
- [ ] Install records and file hashes are stored.
- [ ] User can list, update/reinstall, and uninstall managed skills.
- [ ] Uninstall preserves modified files or requests explicit confirmation.
- [ ] ZIP/path traversal and symlink attacks are handled safely.
- [ ] No skill scripts or dependencies are executed.
- [ ] Failures are actionable and do not leave partial installs.
- [ ] UI handles loading, empty, errors, offline, and rate limits.
- [ ] Existing Raydrop functionality remains unaffected.
- [ ] Tests cover parsing, path validation, import, adapters, install conflicts, rollback, and uninstall.

## 17. Testing strategy

### Unit tests

- SKILL.md frontmatter parser.
- Metadata validation.
- Stable ID generation.
- URL parser.
- ZIP path validation.
- File size/count limits.
- Hash generation.
- Security heuristics.
- Compatibility resolver.
- Install path resolution.
- Manifest ownership.
- Conflict detection.
- Update comparison.
- Uninstall modified-file handling.

### Integration tests

- SQLite migration and repository CRUD.
- Local folder import.
- ZIP import with valid and malicious paths.
- GitHub provider mocked responses.
- Agent detection with mocked filesystem/PATH.
- Install to temporary target directory.
- Batch install partial failures.
- Backup and rollback.
- Uninstall while preserving unrelated files.
- Reconciliation after external file changes.

### UI tests / manual QA

- Navigation.
- Search and filters.
- Import dialog.
- Skill preview.
- Agent selection.
- Global/project scope.
- Folder picker.
- Install progress and errors.
- Conflict dialog.
- Installed list.
- Update diff.
- Security warnings.
- Uninstall confirmation.
- Dark/light theme.
- Windows/macOS/Linux path behavior as supported by Raydrop.

## 18. Operational and quality requirements

- No blocking UI during network, scanning, or filesystem operations.
- Use async tasks and existing progress/event conventions.
- Cancel operations where feasible.
- Network requests have timeout, size limits, and retry policy.
- Catalog requests use cache.
- Respect provider rate limits.
- Avoid logging sensitive skill contents or credentials.
- Errors are typed/actionable; avoid raw panic/unwrap in production paths.
- Validate all IPC input in Rust.
- Keep frontend validation for UX but never rely on it for security.
- Avoid excessive permissions in Tauri capabilities.
- Add only necessary filesystem/network permissions.
- Cross-platform behavior must be tested.
- Do not introduce unrelated dependencies without justification.
- Maintain existing CI/release workflow.
- Update user documentation and feature notes after implementation.

## 19. Suggested implementation workflow for coding agents

Work in small, reviewable steps:

1. Audit existing repository architecture and report findings.
2. Propose exact files to add/change and migration design.
3. Implement Phase 1 only.
4. Run formatting, linting, type checks, tests, and build.
5. Review diff for regressions, unsafe filesystem behavior, and scope creep.
6. Report completed tasks, tests, limitations, and next phase.
7. Proceed to the next phase only after the current phase is stable.

For agent execution:

- Do not attempt all phases in one coding session.
- Treat this plan as the source of truth and maintain a progress checklist.
- Do not mark tasks complete without verification evidence.
- Never fabricate test/build results.
- If external APIs or agent conventions are uncertain, research/verify before implementing.
- Ask for clarification only when a blocking product decision cannot be safely inferred.
- Preserve existing user changes; do not reset or overwrite unrelated work.
- Avoid unnecessary rewrites or architectural changes.

## 20. First task prompt

Use this as the first task for a coding agent:

> Read `raydrop-skills-library-implementation-plan.md` completely. First audit the current Raydrop repository architecture relevant to implementing AI Skills Library: navigation, Svelte component conventions, Tauri IPC registration, SQLite migration/persistence, filesystem and picker utilities, HTTP/download utilities, logging/progress, permissions/capabilities, tests, and CI. Do not modify code yet. Verify assumptions against the current checkout. Produce a concise audit report, identify integration points and any risks/unknowns, propose the exact Phase 1 file changes, and confirm a phased execution plan. Do not refactor unrelated code.

---

## 21. Definition of done for each phase

For each phase, the agent must provide:

- Summary of changes.
- Files added/changed.
- Tasks completed in the plan.
- Commands executed.
- Test/build results with actual output or concise evidence.
- Security considerations addressed.
- Known limitations.
- Next recommended phase.

Only check boxes after implementation and verification are complete.
