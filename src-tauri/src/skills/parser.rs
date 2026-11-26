use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSkillFrontmatter {
    pub name: String,
    pub description: String,
    pub author: Option<String>,
    pub license: Option<String>,
    pub version: Option<String>,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    pub compatibility_hints: Vec<String>,
    pub body: String,
}

/// Computes lowercase hex SHA-256 of arbitrary bytes.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Generates a deterministic slug suitable for directory names and stable IDs.
pub fn slugify_skill_name(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "untitled-skill".to_string()
    } else {
        trimmed
    }
}

/// Generates a stable internal skill ID from source provenance and skill slug.
pub fn stable_skill_id(source_key: &str, slug: &str) -> String {
    let input = format!("{}::{}", source_key.trim(), slug.trim());
    let digest = sha256_bytes(input.as_bytes());
    format!("sk_{}", &digest[..16])
}

/// Validates a relative path from an untrusted source (ZIP, GitHub tree, local folder).
/// Rejects absolute paths, Windows drive letters, UNC prefixes, `..` traversal, empty paths, and NUL bytes.
pub fn validate_relative_path(raw: &str) -> Result<PathBuf, String> {
    let normalized = raw.replace('\\', "/");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err("Empty relative path is not allowed".to_string());
    }
    if trimmed.contains('\0') {
        return Err("NUL byte in path is not allowed".to_string());
    }
    if trimmed.starts_with('/') || trimmed.starts_with("//") {
        return Err(format!("Absolute or UNC path rejected: {}", raw));
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(format!("Drive-qualified path rejected: {}", raw));
    }

    let mut clean = PathBuf::new();
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(format!("Parent traversal ('..') rejected: {}", raw));
        }
        // Reject Windows reserved device names or control chars
        if part.chars().any(|c| c.is_control()) {
            return Err(format!("Control character in path rejected: {}", raw));
        }
        clean.push(part);
    }

    if clean.as_os_str().is_empty() {
        return Err(format!("Path resolved to empty: {}", raw));
    }

    for comp in clean.components() {
        match comp {
            Component::Normal(_) => {}
            _ => return Err(format!("Unsafe path component rejected: {}", raw)),
        }
    }

    Ok(clean)
}

/// Parses SKILL.md YAML-like frontmatter (`--- ... ---`) and extracts metadata + markdown body.
/// Falls back gracefully to heading/first paragraph if frontmatter is minimal or absent.
pub fn parse_skill_md(content: &str, fallback_name: &str) -> Result<ParsedSkillFrontmatter, String> {
    let normalized = content.replace("\r\n", "\n");
    let trimmed_start = normalized.trim_start_matches('\u{feff}');

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut author: Option<String> = None;
    let mut license: Option<String> = None;
    let mut version: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut categories: Vec<String> = Vec::new();
    let mut compatibility_hints: Vec<String> = Vec::new();
    let mut body = normalized.clone();

    if let Some(rest) = trimmed_start.strip_prefix("---\n") {
        if let Some(end_idx) = rest.find("\n---") {
            let fm_block = &rest[..end_idx];
            let after = &rest[end_idx + 4..];
            body = after.trim_start_matches('\n').to_string();

            let mut active_list_key: Option<String> = None;
            let mut multiline_key: Option<String> = None;
            let mut multiline_buf: Vec<String> = Vec::new();

            let flush_multiline = |key: &mut Option<String>,
                                   buf: &mut Vec<String>,
                                   desc_out: &mut Option<String>| {
                if let Some(k) = key.take() {
                    let joined = buf.join(" ").trim().to_string();
                    buf.clear();
                    if k == "description" && !joined.is_empty() {
                        *desc_out = Some(joined);
                    }
                }
            };

            for raw_line in fm_block.lines() {
                let trimmed = raw_line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }

                // Handle multiline block scalar continuation
                if let Some(_) = multiline_key {
                    if raw_line.starts_with(' ') || raw_line.starts_with('\t') {
                        multiline_buf.push(trimmed.to_string());
                        continue;
                    } else {
                        flush_multiline(&mut multiline_key, &mut multiline_buf, &mut description);
                    }
                }

                // Handle YAML list item (`- value`)
                if let Some(item_val) = trimmed.strip_prefix("- ") {
                    let cleaned = unquote_yaml_value(item_val);
                    if !cleaned.is_empty() {
                        match active_list_key.as_deref() {
                            Some("tags") => tags.push(cleaned),
                            Some("categories") | Some("category") => categories.push(cleaned),
                            Some("compatibility") | Some("agents") => {
                                compatibility_hints.push(cleaned)
                            }
                            _ => {}
                        }
                    }
                    continue;
                }

                active_list_key = None;

                if let Some((k_raw, v_raw)) = trimmed.split_once(':') {
                    let key = k_raw.trim().to_ascii_lowercase();
                    let val_raw = v_raw.trim();

                    if val_raw == "|" || val_raw == ">" || val_raw == "|-" || val_raw == ">-" {
                        multiline_key = Some(key);
                        multiline_buf.clear();
                        continue;
                    }

                    if val_raw.is_empty() {
                        active_list_key = Some(key);
                        continue;
                    }

                    if val_raw.starts_with('[') && val_raw.ends_with(']') {
                        let inner = &val_raw[1..val_raw.len() - 1];
                        let parsed_items: Vec<String> = inner
                            .split(',')
                            .map(unquote_yaml_value)
                            .filter(|s| !s.is_empty())
                            .collect();
                        match key.as_str() {
                            "tags" => tags.extend(parsed_items),
                            "categories" | "category" => categories.extend(parsed_items),
                            "compatibility" | "agents" => {
                                compatibility_hints.extend(parsed_items)
                            }
                            _ => {}
                        }
                        continue;
                    }

                    let val = unquote_yaml_value(val_raw);
                    if val.is_empty() {
                        continue;
                    }
                    match key.as_str() {
                        "name" | "title" => name = Some(val),
                        "description" | "summary" => description = Some(val),
                        "author" | "creator" => author = Some(val),
                        "license" => license = Some(val),
                        "version" => version = Some(val),
                        "tags" => {
                            tags.extend(
                                val.split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty()),
                            );
                        }
                        "category" | "categories" => {
                            categories.extend(
                                val.split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty()),
                            );
                        }
                        "compatibility" | "agents" => {
                            compatibility_hints.extend(
                                val.split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty()),
                            );
                        }
                        _ => {}
                    }
                }
            }

            flush_multiline(&mut multiline_key, &mut multiline_buf, &mut description);
        }
    }

    // Fallback: extract heading as name if frontmatter didn't specify one
    if name.is_none() {
        for line in body.lines() {
            let t = line.trim();
            if let Some(heading) = t.strip_prefix("# ") {
                let h = heading.trim();
                if !h.is_empty() {
                    name = Some(h.to_string());
                    break;
                }
            }
        }
    }

    // Fallback: extract first non-heading paragraph as description
    if description.is_none() {
        for line in body.lines() {
            let t = line.trim();
            if !t.is_empty() && !t.starts_with('#') && !t.starts_with("```") {
                let clipped: String = t.chars().take(240).collect();
                description = Some(clipped);
                break;
            }
        }
    }

    let final_name = name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| fallback_name.trim().to_string());
    if final_name.is_empty() {
        return Err("Skill name could not be determined from SKILL.md".to_string());
    }

    if categories.is_empty() {
        categories.push("General".to_string());
    }

    Ok(ParsedSkillFrontmatter {
        name: final_name,
        description: description.unwrap_or_else(|| "No description provided.".to_string()),
        author,
        license,
        version,
        tags,
        categories,
        compatibility_hints,
        body,
    })
}

fn unquote_yaml_value(raw: &str) -> String {
    let t = raw.trim();
    if (t.starts_with('"') && t.ends_with('"') && t.len() >= 2)
        || (t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2)
    {
        t[1..t.len() - 1].trim().to_string()
    } else {
        t.to_string()
    }
}

pub fn classify_file_type(rel_path: &Path, bytes: &[u8]) -> String {
    let ext = rel_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if matches!(
        ext.as_str(),
        "sh" | "bash" | "zsh" | "ps1" | "bat" | "cmd" | "py" | "rb" | "pl"
    ) {
        return "script".to_string();
    }
    if matches!(ext.as_str(), "md" | "mdx" | "markdown") {
        return "markdown".to_string();
    }

    // Binary check: NUL byte in first 4KB
    let sample = &bytes[..bytes.len().min(4096)];
    if sample.contains(&0) || std::str::from_utf8(sample).is_err() {
        return "binary".to_string();
    }

    "text".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_md_frontmatter() {
        let md = r#"---
name: rust-architect
description: Helps design modular Rust services
author: Raydrop
license: MIT
version: 1.0.0
tags: [rust, architecture, backend]
categories:
  - Backend
  - Architecture
---

# Rust Architect
Use clean modules.
"#;
        let parsed = parse_skill_md(md, "fallback").unwrap();
        assert_eq!(parsed.name, "rust-architect");
        assert_eq!(parsed.description, "Helps design modular Rust services");
        assert_eq!(parsed.author.as_deref(), Some("Raydrop"));
        assert_eq!(parsed.license.as_deref(), Some("MIT"));
        assert_eq!(parsed.tags, vec!["rust", "architecture", "backend"]);
        assert_eq!(parsed.categories, vec!["Backend", "Architecture"]);
        assert!(parsed.body.contains("# Rust Architect"));
    }

    #[test]
    fn test_validate_relative_path_rejects_traversal() {
        assert!(validate_relative_path("../evil.sh").is_err());
        assert!(validate_relative_path("a/../../evil.sh").is_err());
        assert!(validate_relative_path("/etc/passwd").is_err());
        assert!(validate_relative_path("C:\\Windows\\System32").is_err());
        assert!(validate_relative_path("\\\\server\\share\\file").is_err());
        assert!(validate_relative_path("scripts/helper.py").is_ok());
        assert!(validate_relative_path("./SKILL.md").is_ok());
    }
}
