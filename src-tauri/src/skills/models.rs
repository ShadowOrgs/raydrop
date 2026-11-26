use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompatibilityLevel {
    Native,
    Compatible,
    Adapted,
    Unsupported,
}

impl Default for CompatibilityLevel {
    fn default() -> Self {
        Self::Compatible
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecuritySeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFinding {
    pub id: String,
    pub severity: SecuritySeverity,
    pub title: String,
    pub explanation: String,
    pub why_it_matters: String,
    pub recommendation: String,
    pub file_path: String,
    pub line_number: Option<usize>,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityReport {
    pub status: String, // "clean", "warnings", "high_risk"
    pub summary: String,
    pub highest_severity: Option<SecuritySeverity>,
    pub findings: Vec<SecurityFinding>,
    pub scanned_files: usize,
    pub executable_files: usize,
    pub scanned_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCompatibilityEntry {
    pub agent_id: String,
    pub agent_name: String,
    pub level: CompatibilityLevel,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub id: String,
    pub source_id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub author: Option<String>,
    pub license: Option<String>,
    pub repository_url: Option<String>,
    pub source_url: String,
    pub source_path: Option<String>,
    pub version: Option<String>,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub compatibility: Vec<SkillCompatibilityEntry>,
    pub security_status: String,
    pub security_report: SecurityReport,
    pub popularity: Option<u64>,
    pub local_package_path: Option<String>,
    pub last_updated: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
    pub is_favorite: bool,
    pub installed_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFileRecord {
    pub id: String,
    pub skill_id: String,
    pub relative_path: String,
    pub content_hash: String,
    pub size: u64,
    pub file_type: String, // "markdown", "text", "script", "binary"
    pub storage_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFilePreview {
    pub relative_path: String,
    pub file_type: String,
    pub size: u64,
    pub content_hash: String,
    pub content: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDetailResponse {
    pub skill: SkillRecord,
    pub files: Vec<SkillFileRecord>,
    pub skill_md_content: Option<String>,
    pub installations: Vec<SkillInstallationRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSourceRecord {
    pub id: String,
    pub name: String,
    pub provider_type: String, // "local", "zip", "github", "curated", "skills_sh", "aitmpl", "custom_registry"
    pub source_url: String,
    pub enabled: bool,
    pub last_synced_at: Option<u64>,
    pub last_error: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentDetectedStatus {
    Detected,
    NotDetected,
    ManualConfiguration,
    PathInvalid,
    UnsupportedPlatform,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub id: String,
    pub agent_type: String,
    pub display_name: String,
    pub detected_status: AgentDetectedStatus,
    pub detection_reason: String,
    pub configured_path: Option<String>,
    pub global_skill_path: String,
    pub project_skill_path: String,
    pub compatibility_level: CompatibilityLevel,
    pub supports_global: bool,
    pub supports_project: bool,
    pub last_detected_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledManifestEntry {
    pub relative_path: String,
    pub content_hash: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallationRecord {
    pub id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub agent_id: String,
    pub scope: String, // "global" | "project"
    pub project_path: Option<String>,
    pub install_path: String,
    pub installed_version: Option<String>,
    pub installed_commit_sha: Option<String>,
    pub installed_manifest: Vec<InstalledManifestEntry>,
    pub installed_at: u64,
    pub updated_at: u64,
    pub status: String, // "installed", "modified", "missing", "outdated"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallHistoryRecord {
    pub id: String,
    pub installation_id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub agent_id: String,
    pub action: String, // "install", "update", "replace", "uninstall", "rollback"
    pub previous_version: Option<String>,
    pub new_version: Option<String>,
    pub backup_path: Option<String>,
    pub timestamp: u64,
    pub result: String, // "success", "failed"
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallPreviewTarget {
    pub agent_id: String,
    pub agent_name: String,
    pub destination_path: String,
    pub compatibility: CompatibilityLevel,
    pub conflict_exists: bool,
    pub conflict_same_hash: bool,
    pub existing_files_count: usize,
    pub allowed: bool,
    pub block_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallPreviewResponse {
    pub skill_id: String,
    pub skill_name: String,
    pub security_report: SecurityReport,
    pub targets: Vec<SkillInstallPreviewTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallTargetResult {
    pub agent_id: String,
    pub agent_name: String,
    pub destination_path: String,
    pub success: bool,
    pub action_taken: String,
    pub installation_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUpdateCheckItem {
    pub installation_id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub agent_id: String,
    pub current_commit_sha: Option<String>,
    pub latest_commit_sha: Option<String>,
    pub has_update: bool,
    pub locally_modified_files: Vec<String>,
    pub changed_files: Vec<String>,
    pub added_files: Vec<String>,
    pub removed_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPresetRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub role_tag: String,
    pub is_builtin: bool,
    pub skill_ids: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsSettings {
    pub backup_retention_count: u32,
    pub require_confirm_high_risk: bool,
    pub block_critical_by_default: bool,
    pub default_scope: String, // "global" | "project"
    pub auto_sync_curated: bool,
}

impl Default for SkillsSettings {
    fn default() -> Self {
        Self {
            backup_retention_count: 5,
            require_confirm_high_risk: true,
            block_critical_by_default: false,
            default_scope: "global".to_string(),
            auto_sync_curated: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsSearchQuery {
    pub query: Option<String>,
    pub source_id: Option<String>,
    pub category: Option<String>,
    pub tag: Option<String>,
    pub favorites_only: bool,
    pub sort_by: Option<String>, // "relevance", "popularity", "updated", "alpha"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredRepoSkill {
    pub name: String,
    pub description: String,
    pub relative_path: String,
    pub tags: Vec<String>,
}
