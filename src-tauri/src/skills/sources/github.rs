use super::{
    ingest_skill_package, PackageIngestInput, RawSkillFile, MAX_PACKAGE_FILES,
    MAX_PACKAGE_TOTAL_BYTES, MAX_SINGLE_FILE_BYTES,
};
use crate::db::storage::Database;
use crate::skills::models::{DiscoveredRepoSkill, SkillRecord};
use crate::skills::parser::parse_skill_md;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGitHubUrl {
    pub owner: String,
    pub repo: String,
    pub branch: Option<String>,
    pub subpath: Option<String>,
}

pub fn parse_github_url(raw: &str) -> Result<ParsedGitHubUrl, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let without_proto = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .ok_or_else(|| "URL must start with https://github.com/".to_string())?;

    let parts: Vec<&str> = without_proto.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        return Err("GitHub URL must include owner and repository name".to_string());
    }

    let owner = parts[0].to_string();
    let repo = parts[1].trim_end_matches(".git").to_string();

    if parts.len() == 2 {
        return Ok(ParsedGitHubUrl {
            owner,
            repo,
            branch: None,
            subpath: None,
        });
    }

    if parts.len() >= 4 && (parts[2] == "tree" || parts[2] == "blob") {
        let branch = parts[3].to_string();
        let rest = if parts.len() > 4 {
            let mut sub = parts[4..].join("/");
            if parts[2] == "blob" && sub.ends_with("/SKILL.md") {
                sub = sub.trim_end_matches("/SKILL.md").to_string();
            } else if parts[2] == "blob" && sub == "SKILL.md" {
                sub = String::new();
            }
            if sub.is_empty() {
                None
            } else {
                Some(sub)
            }
        } else {
            None
        };
        return Ok(ParsedGitHubUrl {
            owner,
            repo,
            branch: Some(branch),
            subpath: rest,
        });
    }

    Ok(ParsedGitHubUrl {
        owner,
        repo,
        branch: None,
        subpath: None,
    })
}

#[derive(Deserialize)]
struct GhRepoMeta {
    default_branch: String,
    license: Option<GhLicense>,
}

#[derive(Deserialize)]
struct GhLicense {
    spdx_id: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct GhBranchRef {
    commit: GhCommitObj,
}

#[derive(Deserialize)]
struct GhCommitObj {
    sha: String,
}

#[derive(Deserialize)]
struct GhTreeResp {
    tree: Vec<GhTreeEntry>,
    truncated: Option<bool>,
}

#[derive(Deserialize)]
struct GhTreeEntry {
    path: String,
    #[serde(rename = "type")]
    entry_type: String,
    size: Option<u64>,
}

fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("Raydrop-Skills-Library/1.0")
        .build()
        .map_err(|e| format!("Failed creating HTTP client: {}", e))
}

async fn gh_get_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, String> {
    let resp = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub network request failed: {}", e))?;

    if resp.status().as_u16() == 403 || resp.status().as_u16() == 429 {
        return Err(
            "GitHub API rate limit reached. Wait a few minutes or import via ZIP/Local folder."
                .to_string(),
        );
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub API error {} for {}", resp.status(), url));
    }
    resp.json::<T>()
        .await
        .map_err(|e| format!("Failed parsing GitHub JSON response: {}", e))
}

/// Discovers all `SKILL.md` directories in a public GitHub repository or subtree.
pub async fn discover_github_skills(url: &str) -> Result<(String, String, Vec<DiscoveredRepoSkill>), String> {
    let parsed = parse_github_url(url)?;
    let client = build_client()?;

    let repo_api = format!("https://api.github.com/repos/{}/{}", parsed.owner, parsed.repo);
    let meta: GhRepoMeta = gh_get_json(&client, &repo_api).await?;
    let branch = parsed.branch.unwrap_or(meta.default_branch);

    let branch_api = format!(
        "https://api.github.com/repos/{}/{}/branches/{}",
        parsed.owner, parsed.repo, branch
    );
    let branch_info: GhBranchRef = gh_get_json(&client, &branch_api).await?;
    let commit_sha = branch_info.commit.sha;

    let tree_api = format!(
        "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
        parsed.owner, parsed.repo, commit_sha
    );
    let tree_resp: GhTreeResp = gh_get_json(&client, &tree_api).await?;
    let _ = tree_resp.truncated;

    let prefix = parsed
        .subpath
        .as_deref()
        .map(|s| format!("{}/", s.trim_matches('/')))
        .unwrap_or_default();

    let mut skill_dirs = Vec::new();
    for entry in &tree_resp.tree {
        if entry.entry_type != "blob" {
            continue;
        }
        let p = &entry.path;
        if !prefix.is_empty() && !p.starts_with(&prefix) && p != &format!("{}SKILL.md", prefix) {
            // Also allow exact subpath/SKILL.md
            if let Some(ref sub) = parsed.subpath {
                if p != &format!("{}/SKILL.md", sub.trim_matches('/')) {
                    continue;
                }
            } else {
                continue;
            }
        }
        if p == "SKILL.md" || p.ends_with("/SKILL.md") {
            let dir = if p == "SKILL.md" {
                String::new()
            } else {
                p.trim_end_matches("/SKILL.md").to_string()
            };
            skill_dirs.push((dir, p.clone()));
        }
    }

    let mut discovered = Vec::new();
    for (dir, md_path) in skill_dirs.into_iter().take(40) {
        let raw_url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            parsed.owner, parsed.repo, commit_sha, md_path
        );
        let md_text = match client.get(&raw_url).send().await {
            Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
            _ => String::new(),
        };
        let fallback = if dir.is_empty() {
            parsed.repo.clone()
        } else {
            dir.split('/').last().unwrap_or(&parsed.repo).to_string()
        };
        let parsed_md = parse_skill_md(&md_text, &fallback).ok();
        discovered.push(DiscoveredRepoSkill {
            name: parsed_md
                .as_ref()
                .map(|m| m.name.clone())
                .unwrap_or(fallback),
            description: parsed_md
                .as_ref()
                .map(|m| m.description.clone())
                .unwrap_or_else(|| "Skill package".to_string()),
            relative_path: dir,
            tags: parsed_md.map(|m| m.tags).unwrap_or_default(),
        });
    }

    Ok((branch, commit_sha, discovered))
}

/// Imports a specific skill folder (or root skill) from a public GitHub URL, pinning to commit SHA.
pub async fn import_github_skill(
    db: &Database,
    url: &str,
    selected_subpath: Option<String>,
) -> Result<SkillRecord, String> {
    let parsed = parse_github_url(url)?;
    let client = build_client()?;

    let repo_api = format!("https://api.github.com/repos/{}/{}", parsed.owner, parsed.repo);
    let meta: GhRepoMeta = gh_get_json(&client, &repo_api).await?;
    let branch = parsed.branch.clone().unwrap_or(meta.default_branch);
    let license_hint = meta
        .license
        .and_then(|l| l.spdx_id.or(l.name))
        .filter(|s| s != "NOASSERTION");

    let branch_api = format!(
        "https://api.github.com/repos/{}/{}/branches/{}",
        parsed.owner, parsed.repo, branch
    );
    let branch_info: GhBranchRef = gh_get_json(&client, &branch_api).await?;
    let commit_sha = branch_info.commit.sha;

    let tree_api = format!(
        "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
        parsed.owner, parsed.repo, commit_sha
    );
    let tree_resp: GhTreeResp = gh_get_json(&client, &tree_api).await?;

    let target_sub = selected_subpath
        .or(parsed.subpath)
        .map(|s| s.trim_matches('/').to_string())
        .filter(|s| !s.is_empty());

    let mut matching_blobs: Vec<(&GhTreeEntry, String)> = Vec::new();
    for entry in &tree_resp.tree {
        if entry.entry_type != "blob" {
            continue;
        }
        let rel = match target_sub {
            Some(ref sub) => {
                let prefix = format!("{}/", sub);
                if let Some(stripped) = entry.path.strip_prefix(&prefix) {
                    stripped.to_string()
                } else {
                    continue;
                }
            }
            None => entry.path.clone(),
        };
        if rel.starts_with(".git/") || rel.starts_with("node_modules/") {
            continue;
        }
        if entry.size.unwrap_or(0) > MAX_SINGLE_FILE_BYTES {
            return Err(format!(
                "Remote file '{}' exceeds max file size ({} bytes)",
                rel, MAX_SINGLE_FILE_BYTES
            ));
        }
        matching_blobs.push((entry, rel));
    }

    if matching_blobs.is_empty() {
        return Err("No files found at the specified GitHub skill path".to_string());
    }
    if matching_blobs.len() > MAX_PACKAGE_FILES {
        return Err("GitHub skill folder exceeds maximum file count".to_string());
    }

    let mut files = Vec::with_capacity(matching_blobs.len());
    let mut total_bytes: u64 = 0;

    for (entry, rel) in matching_blobs {
        let raw_url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            parsed.owner, parsed.repo, commit_sha, entry.path
        );
        let resp = client
            .get(&raw_url)
            .send()
            .await
            .map_err(|e| format!("Failed downloading '{}': {}", rel, e))?;
        if !resp.status().is_success() {
            return Err(format!(
                "Failed downloading '{}' (HTTP {})",
                rel,
                resp.status()
            ));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed reading bytes for '{}': {}", rel, e))?
            .to_vec();

        total_bytes = total_bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| "Size overflow".to_string())?;
        if total_bytes > MAX_PACKAGE_TOTAL_BYTES {
            return Err("GitHub skill exceeds maximum package size (25 MB)".to_string());
        }

        files.push(RawSkillFile {
            relative_path: rel,
            bytes,
            is_executable: false,
        });
    }

    let repo_url = format!("https://github.com/{}/{}", parsed.owner, parsed.repo);
    let source_url = match target_sub {
        Some(ref sub) => format!("{}/tree/{}/{}", repo_url, branch, sub),
        None => repo_url.clone(),
    };
    let fallback_name = target_sub
        .as_deref()
        .and_then(|s| s.split('/').last())
        .unwrap_or(&parsed.repo)
        .to_string();

    ingest_skill_package(
        db,
        PackageIngestInput {
            source_id: "src_github".to_string(),
            source_name: "GitHub Repositories".to_string(),
            provider_type: "github".to_string(),
            source_url,
            source_path: target_sub,
            repository_url: Some(repo_url),
            branch: Some(branch),
            commit_sha: Some(commit_sha),
            license_hint,
            fallback_name,
            files,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_urls() {
        let a = parse_github_url("https://github.com/anthropics/skills").unwrap();
        assert_eq!(a.owner, "anthropics");
        assert_eq!(a.repo, "skills");
        assert_eq!(a.branch, None);
        assert_eq!(a.subpath, None);

        let b = parse_github_url("https://github.com/owner/repo/tree/develop/skills/example")
            .unwrap();
        assert_eq!(b.owner, "owner");
        assert_eq!(b.repo, "repo");
        assert_eq!(b.branch.as_deref(), Some("develop"));
        assert_eq!(b.subpath.as_deref(), Some("skills/example"));
    }
}
