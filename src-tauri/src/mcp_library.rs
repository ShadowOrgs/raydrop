use crate::ai_agent::AiMcpConfig;
use crate::db::storage::{now_unix_secs, Database};
use crate::state::AppState;
use directories::BaseDirs;
use futures_util::StreamExt;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpAuthField {
    pub key: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_auth_target")]
    pub target: String, // "header" | "env"
    #[serde(default)]
    pub header_prefix: String, // e.g. "Bearer " or ""
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub placeholder: String,
}

fn default_auth_target() -> String {
    "header".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpAgentInstallInfo {
    pub agent_id: String,
    pub agent_name: String,
    pub config_path: String,
    pub installed: bool,
    pub installed_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpLibraryItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source_url: String,
    pub transport: String, // "http" | "sse" | "stdio"
    pub url: String,
    pub command: String,
    pub args: Vec<String>,
    pub headers: HashMap<String, String>,
    pub env: HashMap<String, String>,
    pub auth_fields: Vec<McpAuthField>,
    pub enabled_for_bot: bool,
    pub cached_tools: Vec<McpToolInfo>,
    pub last_test_status: String,
    pub last_test_error: String,
    #[serde(default)]
    pub installations: Vec<McpAgentInstallInfo>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInspectResult {
    pub item: McpLibraryItem,
    pub detected_via: String, // "llm" | "probe" | "json" | "command"
    pub probe_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTestResult {
    pub ok: bool,
    pub message: String,
    pub tools: Vec<McpToolInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSupportedAgent {
    pub id: String,
    pub display_name: String,
    pub config_path: String,
    pub detected: bool,
}

fn home_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|b| b.home_dir().to_path_buf())
}

pub fn get_supported_mcp_agents() -> Vec<McpSupportedAgent> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };

    let defs: &[(&str, &str, &str, &str)] = &[
        ("antigravity", "Antigravity IDE", ".gemini/antigravity/mcp_config.json", ".gemini"),
        ("claude_code", "Claude Code", ".claude.json", ".claude"),
        ("cursor", "Cursor", ".cursor/mcp.json", ".cursor"),
        ("gemini_cli", "Gemini CLI", ".gemini/settings.json", ".gemini"),
        ("codex", "OpenAI Codex", ".codex/config.toml", ".codex"),
        ("windsurf", "Windsurf", ".codeium/windsurf/mcp_config.json", ".codeium"),
        ("opencode", "OpenCode", ".config/opencode/opencode.json", ".config/opencode"),
        ("cline", "Cline", ".cline/mcp_settings.json", ".cline"),
    ];

    defs.iter()
        .map(|(id, name, rel_cfg, detect_dir)| {
            let cfg_path = home.join(rel_cfg);
            let detected = cfg_path.exists() || home.join(detect_dir).exists();
            McpSupportedAgent {
                id: (*id).to_string(),
                display_name: (*name).to_string(),
                config_path: cfg_path.to_string_lossy().to_string(),
                detected,
            }
        })
        .collect()
}

fn sanitize_mcp_id(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        format!("mcp-{}", now_unix_secs() % 100000)
    } else {
        trimmed
    }
}

fn slug_from_url(url_str: &str) -> (String, String) {
    let cleaned = url_str
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = cleaned.split('/').next().unwrap_or("mcp-server");
    let host_no_port = host.split(':').next().unwrap_or(host);
    let parts: Vec<&str> = host_no_port
        .split('.')
        .filter(|p| !p.is_empty() && *p != "www" && *p != "mcp" && *p != "api" && *p != "com" && *p != "io" && *p != "dev" && *p != "org" && *p != "net")
        .collect();
    let base = if let Some(first) = parts.first() {
        (*first).to_string()
    } else {
        host_no_port.replace('.', "-")
    };
    let id = sanitize_mcp_id(&base);
    let title = id
        .split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    (id, if title.is_empty() { "MCP Server".to_string() } else { format!("{} MCP", title) })
}

// ============================================================================
// SQLite CRUD
// ============================================================================

pub fn list_mcp_library_items(db: &Database) -> Result<Vec<McpLibraryItem>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, description, source_url, transport, url, command, args_json,
                    headers_json, env_json, auth_fields_json, enabled_for_bot,
                    cached_tools_json, last_test_status, last_test_error, created_at, updated_at
             FROM mcp_library_items
             ORDER BY updated_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            let args_json: String = row.get(7)?;
            let headers_json: String = row.get(8)?;
            let env_json: String = row.get(9)?;
            let auth_json: String = row.get(10)?;
            let tools_json: String = row.get(12)?;
            Ok(McpLibraryItem {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                source_url: row.get(3)?,
                transport: row.get(4)?,
                url: row.get(5)?,
                command: row.get(6)?,
                args: serde_json::from_str(&args_json).unwrap_or_default(),
                headers: serde_json::from_str(&headers_json).unwrap_or_default(),
                env: serde_json::from_str(&env_json).unwrap_or_default(),
                auth_fields: serde_json::from_str(&auth_json).unwrap_or_default(),
                enabled_for_bot: row.get::<_, i64>(11)? != 0,
                cached_tools: serde_json::from_str(&tools_json).unwrap_or_default(),
                last_test_status: row.get(13)?,
                last_test_error: row.get(14)?,
                installations: Vec::new(),
                created_at: row.get::<_, i64>(15)? as u64,
                updated_at: row.get::<_, i64>(16)? as u64,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut items = Vec::new();
    for r in rows {
        items.push(r.map_err(|e| e.to_string())?);
    }

    let mut install_map: HashMap<String, Vec<(String, String, u64)>> = HashMap::new();
    if let Ok(mut istmt) = conn.prepare(
        "SELECT mcp_id, agent_id, config_path, installed_at FROM mcp_agent_installations",
    ) {
        if let Ok(irows) = istmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? as u64,
            ))
        }) {
            for ir in irows.flatten() {
                install_map.entry(ir.0).or_default().push((ir.1, ir.2, ir.3));
            }
        }
    }
    drop(stmt);
    drop(conn);

    let supported = get_supported_mcp_agents();
    for item in &mut items {
        let installed_entries = install_map.get(&item.id);
        item.installations = supported
            .iter()
            .map(|ag| {
                let found = installed_entries
                    .and_then(|list| list.iter().find(|(aid, _, _)| aid == &ag.id));
                McpAgentInstallInfo {
                    agent_id: ag.id.clone(),
                    agent_name: ag.display_name.clone(),
                    config_path: found
                        .map(|(_, p, _)| p.clone())
                        .unwrap_or_else(|| ag.config_path.clone()),
                    installed: found.is_some(),
                    installed_at: found.map(|(_, _, ts)| *ts),
                }
            })
            .collect();
    }

    Ok(items)
}

pub fn get_mcp_library_item(db: &Database, id: &str) -> Result<Option<McpLibraryItem>, String> {
    let all = list_mcp_library_items(db)?;
    Ok(all.into_iter().find(|i| i.id == id))
}

pub fn upsert_mcp_library_item(db: &Database, item: &McpLibraryItem) -> Result<McpLibraryItem, String> {
    let mut saved = item.clone();
    saved.id = sanitize_mcp_id(&saved.id);
    let now = now_unix_secs();
    if saved.created_at == 0 {
        saved.created_at = now;
    }
    saved.updated_at = now;

    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO mcp_library_items (
            id, name, description, source_url, transport, url, command, args_json,
            headers_json, env_json, auth_fields_json, enabled_for_bot,
            cached_tools_json, last_test_status, last_test_error, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            description = excluded.description,
            source_url = excluded.source_url,
            transport = excluded.transport,
            url = excluded.url,
            command = excluded.command,
            args_json = excluded.args_json,
            headers_json = excluded.headers_json,
            env_json = excluded.env_json,
            auth_fields_json = excluded.auth_fields_json,
            enabled_for_bot = excluded.enabled_for_bot,
            cached_tools_json = excluded.cached_tools_json,
            last_test_status = excluded.last_test_status,
            last_test_error = excluded.last_test_error,
            updated_at = excluded.updated_at",
        params![
            saved.id,
            saved.name,
            saved.description,
            saved.source_url,
            saved.transport,
            saved.url,
            saved.command,
            serde_json::to_string(&saved.args).unwrap_or_else(|_| "[]".to_string()),
            serde_json::to_string(&saved.headers).unwrap_or_else(|_| "{}".to_string()),
            serde_json::to_string(&saved.env).unwrap_or_else(|_| "{}".to_string()),
            serde_json::to_string(&saved.auth_fields).unwrap_or_else(|_| "[]".to_string()),
            if saved.enabled_for_bot { 1i64 } else { 0i64 },
            serde_json::to_string(&saved.cached_tools).unwrap_or_else(|_| "[]".to_string()),
            saved.last_test_status,
            saved.last_test_error,
            saved.created_at as i64,
            saved.updated_at as i64,
        ],
    )
    .map_err(|e| e.to_string())?;
    drop(conn);

    Ok(get_mcp_library_item(db, &saved.id)?.unwrap_or(saved))
}

pub fn delete_mcp_library_item(db: &Database, id: &str) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM mcp_agent_installations WHERE mcp_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM mcp_library_items WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ============================================================================
// MCP Client (Streamable HTTP, SSE, Stdio)
// ============================================================================

fn parse_sse_json_messages(raw: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(data) = trimmed.strip_prefix("data:") {
            let payload = data.trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }
            if let Ok(val) = serde_json::from_str::<Value>(payload) {
                out.push(val);
            }
        }
    }
    out
}

fn parse_jsonrpc_body(raw: &str, target_id: i64) -> Result<Value, String> {
    if let Ok(val) = serde_json::from_str::<Value>(raw.trim()) {
        if val.is_object() {
            return Ok(val);
        }
        if let Some(arr) = val.as_array() {
            if let Some(found) = arr.iter().find(|v| v.get("id").and_then(|i| i.as_i64()) == Some(target_id)) {
                return Ok(found.clone());
            }
            if let Some(first) = arr.first() {
                return Ok(first.clone());
            }
        }
    }

    let msgs = parse_sse_json_messages(raw);
    if let Some(found) = msgs
        .iter()
        .find(|v| v.get("id").and_then(|i| i.as_i64()) == Some(target_id))
    {
        return Ok(found.clone());
    }
    if let Some(last) = msgs.last() {
        return Ok(last.clone());
    }

    Err(format!(
        "Invalid MCP response: {}",
        raw.chars().take(280).collect::<String>()
    ))
}

fn apply_headers(
    mut req: reqwest::RequestBuilder,
    headers: &HashMap<String, String>,
) -> reqwest::RequestBuilder {
    for (k, v) in headers {
        if !k.trim().is_empty() && !v.trim().is_empty() {
            req = req.header(k.trim(), v.trim());
        }
    }
    req
}

async fn mcp_http_rpc(
    client: &reqwest::Client,
    url: &str,
    headers: &HashMap<String, String>,
    session_id: Option<&str>,
    req_id: i64,
    method: &str,
    params: Value,
) -> Result<(Value, Option<String>), String> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": req_id,
        "method": method,
        "params": params
    });

    let mut req = client
        .post(url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json");
    if let Some(sid) = session_id {
        req = req.header("Mcp-Session-Id", sid);
    }
    req = apply_headers(req, headers);

    let resp = req
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("MCP HTTP request failed: {}", e))?;

    let status = resp.status();
    let new_sid = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "HTTP {}: {}",
            status,
            text.chars().take(300).collect::<String>()
        ));
    }

    let val = parse_jsonrpc_body(&text, req_id)?;
    if let Some(err) = val.get("error") {
        return Err(format!("MCP error: {}", err));
    }
    Ok((val, new_sid))
}

async fn mcp_http_list_tools(item: &McpLibraryItem) -> Result<Vec<McpToolInfo>, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(12))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let init_params = json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": { "name": "raydrop", "version": "1.2.95" }
    });

    let (_, sid) = mcp_http_rpc(&client, &item.url, &item.headers, None, 1, "initialize", init_params).await?;

    // Best-effort notifications/initialized
    let mut notif = client
        .post(&item.url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json");
    if let Some(ref s) = sid {
        notif = notif.header("Mcp-Session-Id", s);
    }
    notif = apply_headers(notif, &item.headers);
    let _ = notif
        .json(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .send()
        .await;

    let (tools_resp, _) =
        mcp_http_rpc(&client, &item.url, &item.headers, sid.as_deref(), 2, "tools/list", json!({})).await?;

    extract_tools_from_response(&tools_resp)
}

async fn mcp_http_call_tool(
    item: &McpLibraryItem,
    tool_name: &str,
    arguments: &Value,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(12))
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;

    let init_params = json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": { "name": "raydrop", "version": "1.2.95" }
    });
    let (_, sid) = mcp_http_rpc(&client, &item.url, &item.headers, None, 1, "initialize", init_params).await?;

    // Best-effort notifications/initialized required by strict MCP servers
    let mut notif = client
        .post(&item.url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json");
    if let Some(ref s) = sid {
        notif = notif.header("Mcp-Session-Id", s);
    }
    notif = apply_headers(notif, &item.headers);
    let _ = notif
        .json(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .send()
        .await;

    let (resp, _) = mcp_http_rpc(
        &client,
        &item.url,
        &item.headers,
        sid.as_deref(),
        2,
        "tools/call",
        json!({ "name": tool_name, "arguments": arguments }),
    )
    .await?;

    format_tool_call_result(&resp)
}

fn resolve_sse_post_url(base_url: &str, endpoint_path: &str) -> String {
    if endpoint_path.starts_with("http://") || endpoint_path.starts_with("https://") {
        return endpoint_path.to_string();
    }
    let trimmed_base = base_url.trim_end_matches('/');
    if let Some(scheme_idx) = trimmed_base.find("://") {
        let after_scheme = &trimmed_base[scheme_idx + 3..];
        let origin_end = after_scheme
            .find('/')
            .map(|i| scheme_idx + 3 + i)
            .unwrap_or(trimmed_base.len());
        let origin = &trimmed_base[..origin_end];
        if endpoint_path.starts_with('/') {
            return format!("{}{}", origin, endpoint_path);
        }
    }
    format!("{}/{}", trimmed_base, endpoint_path.trim_start_matches('/'))
}

async fn mcp_sse_session_call(
    item: &McpLibraryItem,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(12))
        .timeout(std::time::Duration::from_secs(35))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.get(&item.url).header("Accept", "text/event-stream");
    req = apply_headers(req, &item.headers);
    let resp = req
        .send()
        .await
        .map_err(|e| format!("SSE connect failed: {}", e))?;

    if !resp.status().is_success() {
        let st = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "SSE HTTP {}: {}",
            st,
            body.chars().take(250).collect::<String>()
        ));
    }

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut post_url: Option<String> = None;
    let mut current_event = String::new();
    let mut sent_init = false;
    let mut sent_req = false;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(25);

    loop {
        let chunk_res = tokio::time::timeout_at(deadline, stream.next()).await;
        let chunk = match chunk_res {
            Ok(Some(Ok(b))) => b,
            Ok(Some(Err(e))) => return Err(format!("SSE stream error: {}", e)),
            Ok(None) => return Err("SSE stream closed before response".to_string()),
            Err(_) => return Err("Timed out waiting for MCP SSE response".to_string()),
        };

        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].trim_end_matches('\r').to_string();
            buf.drain(..=nl);

            if line.is_empty() {
                current_event.clear();
                continue;
            }
            if let Some(ev) = line.strip_prefix("event:") {
                current_event = ev.trim().to_string();
            } else if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if current_event == "endpoint" || post_url.is_none() && data.starts_with('/') {
                    post_url = Some(resolve_sse_post_url(&item.url, data));
                    if !sent_init {
                        sent_init = true;
                        let target = post_url.as_ref().unwrap().clone();
                        let mut p = client.post(&target).header("Content-Type", "application/json");
                        p = apply_headers(p, &item.headers);
                        let _ = p
                            .json(&json!({
                                "jsonrpc": "2.0",
                                "id": 1,
                                "method": "initialize",
                                "params": {
                                    "protocolVersion": "2024-11-05",
                                    "capabilities": {},
                                    "clientInfo": { "name": "raydrop", "version": "1.2.95" }
                                }
                            }))
                            .send()
                            .await;
                    }
                } else if let Ok(val) = serde_json::from_str::<Value>(data) {
                    let rid = val.get("id").and_then(|v| v.as_i64());
                    if rid == Some(1) && !sent_req {
                        sent_req = true;
                        if let Some(ref target) = post_url {
                            let mut n = client.post(target).header("Content-Type", "application/json");
                            n = apply_headers(n, &item.headers);
                            let _ = n
                                .json(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
                                .send()
                                .await;

                            let mut p = client.post(target).header("Content-Type", "application/json");
                            p = apply_headers(p, &item.headers);
                            let _ = p
                                .json(&json!({
                                    "jsonrpc": "2.0",
                                    "id": 2,
                                    "method": method,
                                    "params": params
                                }))
                                .send()
                                .await;
                        }
                    } else if rid == Some(2) {
                        if let Some(err) = val.get("error") {
                            return Err(format!("MCP error: {}", err));
                        }
                        return Ok(val);
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn build_augmented_windows_path() -> String {
    let mut dirs: Vec<String> = Vec::new();
    if let Ok(p) = std::env::var("PATH") {
        for part in p.split(';').filter(|s| !s.trim().is_empty()) {
            dirs.push(part.trim().to_string());
        }
    }
    let mut extra: Vec<String> = vec![
        r"C:\Program Files\nodejs".to_string(),
        r"C:\Program Files (x86)\nodejs".to_string(),
        r"C:\ProgramData\chocolatey\bin".to_string(),
    ];
    if let Ok(appdata) = std::env::var("APPDATA") {
        extra.push(format!(r"{}\npm", appdata));
        extra.push(format!(r"{}\uv\bin", appdata));
        let py_roaming = std::path::Path::new(&appdata).join("Python");
        if let Ok(entries) = std::fs::read_dir(py_roaming) {
            for entry in entries.flatten() {
                let scripts = entry.path().join("Scripts");
                if scripts.is_dir() {
                    extra.push(scripts.to_string_lossy().to_string());
                }
            }
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        extra.push(format!(r"{}\Programs\nodejs", local));
        extra.push(format!(r"{}\Volta\bin", local));
        extra.push(format!(r"{}\pnpm", local));
        extra.push(format!(r"{}\uv\bin", local));
        extra.push(format!(r"{}\Microsoft\WinGet\Links", local));
        let py_local = std::path::Path::new(&local).join("Programs").join("Python");
        if let Ok(entries) = std::fs::read_dir(py_local) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    extra.push(p.to_string_lossy().to_string());
                    let scripts = p.join("Scripts");
                    if scripts.is_dir() {
                        extra.push(scripts.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        extra.push(format!(r"{}\.local\bin", profile));
        extra.push(format!(r"{}\.cargo\bin", profile));
        extra.push(format!(r"{}\.bun\bin", profile));
        extra.push(format!(r"{}\scoop\shims", profile));
    }
    if let Ok(nvm) = std::env::var("NVM_SYMLINK") {
        extra.push(nvm);
    }
    for d in extra {
        if !d.is_empty() && std::path::Path::new(&d).is_dir() && !dirs.iter().any(|x| x.eq_ignore_ascii_case(&d)) {
            dirs.push(d);
        }
    }
    dirs.join(";")
}

#[cfg(target_os = "windows")]
fn resolve_windows_command(raw_cmd: &str, path_env: &str) -> (String, bool) {
    let trimmed = raw_cmd.trim();
    let lower = trimmed.to_lowercase();
    let p = std::path::Path::new(trimmed);

    // Direct path given
    if p.components().count() > 1 {
        let is_batch = lower.ends_with(".cmd") || lower.ends_with(".bat");
        return (trimmed.to_string(), is_batch);
    }

    let exts: &[&str] = if lower.ends_with(".exe") || lower.ends_with(".cmd") || lower.ends_with(".bat") {
        &[""]
    } else {
        &[".exe", ".cmd", ".bat", ""]
    };

    for dir in path_env.split(';').filter(|s| !s.trim().is_empty()) {
        for ext in exts {
            let candidate = std::path::Path::new(dir.trim()).join(format!("{}{}", trimmed, ext));
            if candidate.is_file() {
                let s = candidate.to_string_lossy().to_string();
                let sl = s.to_lowercase();
                let is_batch = sl.ends_with(".cmd") || sl.ends_with(".bat");
                return (s, is_batch);
            }
        }
    }

    let is_known_shim = matches!(lower.as_str(), "npx" | "npm" | "pnpm" | "pnpx" | "yarn" | "corepack");
    (trimmed.to_string(), is_known_shim)
}

async fn mcp_stdio_call(
    item: &McpLibraryItem,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    if item.command.trim().is_empty() {
        return Err("MCP stdio command is empty".to_string());
    }

    #[cfg(target_os = "windows")]
    let mut cmd = {
        let aug_path = build_augmented_windows_path();
        let (resolved, needs_cmd_shell) = resolve_windows_command(item.command.trim(), &aug_path);
        let mut c = if needs_cmd_shell {
            let mut shell = Command::new("cmd.exe");
            shell.arg("/C").arg(&resolved).args(&item.args);
            shell
        } else {
            let mut direct = Command::new(&resolved);
            direct.args(&item.args);
            direct
        };
        c.env("PATH", aug_path);
        c.creation_flags(0x08000000);
        c
    };

    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = Command::new(item.command.trim());
        c.args(&item.args);
        c
    };

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in &item.env {
        if !k.trim().is_empty() {
            cmd.env(k.trim(), v);
        }
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn '{}': {}", item.command, e))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Failed to open stdio stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to open stdio stdout".to_string())?;
    let mut reader = BufReader::new(stdout).lines();

    let init_line = format!(
        "{}\n",
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "raydrop", "version": "1.2.95" }
            }
        })
    );
    stdin
        .write_all(init_line.as_bytes())
        .await
        .map_err(|e| format!("Failed writing initialize: {}", e))?;
    stdin.flush().await.map_err(|e| e.to_string())?;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(25);
    let mut initialized = false;
    let mut sent_req = false;

    loop {
        let next_line = tokio::time::timeout_at(deadline, reader.next_line()).await;
        let line_opt = match next_line {
            Ok(Ok(l)) => l,
            Ok(Err(e)) => {
                let _ = child.kill().await;
                return Err(format!("Stdio read error: {}", e));
            }
            Err(_) => {
                let _ = child.kill().await;
                return Err("Timed out waiting for MCP stdio response".to_string());
            }
        };

        let Some(line) = line_opt else {
            let _ = child.kill().await;
            return Err("MCP stdio process exited before responding".to_string());
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };

        let rid = val.get("id").and_then(|i| i.as_i64());
        if rid == Some(1) && !initialized {
            initialized = true;
            let notif = format!("{}\n", json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
            let _ = stdin.write_all(notif.as_bytes()).await;
            if !sent_req {
                sent_req = true;
                let req = format!(
                    "{}\n",
                    json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": method,
                        "params": params
                    })
                );
                stdin
                    .write_all(req.as_bytes())
                    .await
                    .map_err(|e| format!("Failed writing {}: {}", method, e))?;
                stdin.flush().await.map_err(|e| e.to_string())?;
            }
        } else if rid == Some(2) {
            let _ = child.kill().await;
            if let Some(err) = val.get("error") {
                return Err(format!("MCP error: {}", err));
            }
            return Ok(val);
        }
    }
}

fn extract_tools_from_response(resp: &Value) -> Result<Vec<McpToolInfo>, String> {
    let arr = resp
        .pointer("/result/tools")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(arr
        .into_iter()
        .filter_map(|t| {
            let name = t.get("name")?.as_str()?.to_string();
            let description = t
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            let input_schema = t
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
            Some(McpToolInfo {
                name,
                description,
                input_schema,
            })
        })
        .collect())
}

fn format_tool_call_result(resp: &Value) -> Result<String, String> {
    if let Some(content) = resp.pointer("/result/content").and_then(|c| c.as_array()) {
        let mut parts = Vec::new();
        for block in content {
            if let Some(txt) = block.get("text").and_then(|t| t.as_str()) {
                parts.push(txt.to_string());
            } else {
                parts.push(block.to_string());
            }
        }
        return Ok(parts.join("\n"));
    }
    if let Some(res) = resp.get("result") {
        return Ok(serde_json::to_string_pretty(res).unwrap_or_else(|_| res.to_string()));
    }
    Ok(resp.to_string())
}

pub async fn test_mcp_item_connection(item: &McpLibraryItem) -> Result<Vec<McpToolInfo>, String> {
    match item.transport.as_str() {
        "sse" => {
            let resp = mcp_sse_session_call(item, "tools/list", json!({})).await?;
            extract_tools_from_response(&resp)
        }
        "stdio" => {
            let resp = mcp_stdio_call(item, "tools/list", json!({})).await?;
            extract_tools_from_response(&resp)
        }
        _ => mcp_http_list_tools(item).await,
    }
}

pub async fn call_external_mcp_tool(
    db: &Database,
    mcp_id: &str,
    tool_name: &str,
    arguments: &Value,
) -> Result<String, String> {
    let item = get_mcp_library_item(db, mcp_id)?
        .ok_or_else(|| format!("MCP server '{}' not found", mcp_id))?;
    if !item.enabled_for_bot {
        return Err(format!("MCP server '{}' is not enabled for Raydrop Bot", mcp_id));
    }

    match item.transport.as_str() {
        "sse" => {
            let resp = mcp_sse_session_call(
                &item,
                "tools/call",
                json!({ "name": tool_name, "arguments": arguments }),
            )
            .await?;
            format_tool_call_result(&resp)
        }
        "stdio" => {
            let resp = mcp_stdio_call(
                &item,
                "tools/call",
                json!({ "name": tool_name, "arguments": arguments }),
            )
            .await?;
            format_tool_call_result(&resp)
        }
        _ => mcp_http_call_tool(&item, tool_name, arguments).await,
    }
}

pub fn get_enabled_bot_mcp_tools(db: &Database) -> Vec<(String, String, McpToolInfo)> {
    let Ok(items) = list_mcp_library_items(db) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        if item.enabled_for_bot {
            for tool in item.cached_tools {
                out.push((item.id.clone(), item.name.clone(), tool));
            }
        }
    }
    out
}

// ============================================================================
// AI URL / GitHub / JSON Inspector
// ============================================================================

fn parse_github_repo(url: &str) -> Option<(String, String)> {
    let cleaned = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let rest = cleaned.strip_prefix("github.com/")?;
    let mut parts = rest.split('/').filter(|s| !s.is_empty());
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.trim_end_matches(".git").to_string();
    Some((owner, repo))
}

async fn fetch_text_limited(client: &reqwest::Client, url: &str, max_chars: usize) -> Option<String> {
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let txt = resp.text().await.ok()?;
    Some(txt.chars().take(max_chars).collect())
}

fn load_ai_config(db: &Database) -> Option<AiMcpConfig> {
    let raw = db.get_setting("ai_mcp_config_json").ok().flatten()?;
    let cfg: AiMcpConfig = serde_json::from_str(&raw).ok()?;
    if cfg.llm_base_url.trim().is_empty() || cfg.llm_model.trim().is_empty() {
        return None;
    }
    Some(cfg)
}

async fn analyze_with_llm(
    cfg: &AiMcpConfig,
    user_input: &str,
    evidence: &str,
) -> Result<McpLibraryItem, String> {
    let base = cfg.llm_base_url.trim().trim_end_matches('/');
    let url = if cfg.llm_provider == "anthropic" {
        format!("{}/messages", base)
    } else {
        format!("{}/chat/completions", base)
    };

    let prompt = format!(
        "You are an MCP (Model Context Protocol) configuration expert.\n\
        Given the user input and fetched probe/documentation evidence, return ONLY a valid JSON object (no markdown fences) with this exact schema:\n\
        {{\n\
          \"id\": \"lowercase-slug\",\n\
          \"name\": \"Human Readable MCP Name\",\n\
          \"description\": \"Short 1-sentence description\",\n\
          \"transport\": \"http\" | \"sse\" | \"stdio\",\n\
          \"url\": \"https://... (for http/sse, else empty string)\",\n\
          \"command\": \"npx or uvx or node (for stdio, else empty string)\",\n\
          \"args\": [\"-y\", \"...\"],\n\
          \"headers\": {{}},\n\
          \"env\": {{}},\n\
          \"auth_fields\": [\n\
            {{\n\
              \"key\": \"Authorization\" or \"X-API-Key\" or \"ENV_VAR_NAME\",\n\
              \"label\": \"API Key\",\n\
              \"target\": \"header\" | \"env\",\n\
              \"header_prefix\": \"Bearer \" or \"\",\n\
              \"required\": true,\n\
              \"placeholder\": \"sk-...\"\n\
            }}\n\
          ]\n\
        }}\n\n\
        User Input:\n{}\n\n\
        Evidence:\n{}",
        user_input, evidence
    );

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;

    let raw_reply = if cfg.llm_provider == "anthropic" {
        let resp = client
            .post(&url)
            .header("x-api-key", cfg.llm_api_key.trim())
            .header("anthropic-version", "2023-06-01")
            .json(&json!({
                "model": cfg.llm_model.trim(),
                "messages": [{ "role": "user", "content": prompt }],
                "max_tokens": 1200,
                "temperature": 0.1
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body: Value = resp.json().await.map_err(|e| e.to_string())?;
        body.pointer("/content/0/text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        let mut req = client.post(&url);
        if !cfg.llm_api_key.trim().is_empty() {
            req = req.bearer_auth(cfg.llm_api_key.trim());
        }
        let resp = req
            .json(&json!({
                "model": cfg.llm_model.trim(),
                "messages": [{ "role": "user", "content": prompt }],
                "temperature": 0.1,
                "max_tokens": 1200
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body: Value = resp.json().await.map_err(|e| e.to_string())?;
        body.pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let cleaned = raw_reply
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let start = cleaned.find('{').ok_or_else(|| "LLM did not return JSON".to_string())?;
    let end = cleaned.rfind('}').ok_or_else(|| "LLM JSON incomplete".to_string())?;
    let val: Value = serde_json::from_str(&cleaned[start..=end]).map_err(|e| e.to_string())?;

    let id = sanitize_mcp_id(val.get("id").and_then(|v| v.as_str()).unwrap_or("mcp-server"));
    let transport = match val.get("transport").and_then(|v| v.as_str()).unwrap_or("http") {
        "sse" => "sse",
        "stdio" => "stdio",
        _ => "http",
    }
    .to_string();

    Ok(McpLibraryItem {
        id,
        name: val
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("MCP Server")
            .to_string(),
        description: val
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        source_url: user_input.trim().to_string(),
        transport,
        url: val.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        command: val
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        args: val
            .get("args")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        headers: val
            .get("headers")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        env: val
            .get("env")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        auth_fields: val
            .get("auth_fields")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        enabled_for_bot: false,
        cached_tools: Vec::new(),
        last_test_status: String::new(),
        last_test_error: String::new(),
        installations: Vec::new(),
        created_at: 0,
        updated_at: 0,
    })
}

pub async fn inspect_mcp_source(db: &Database, raw_input: &str) -> Result<McpInspectResult, String> {
    let trimmed = raw_input.trim();
    if trimmed.is_empty() {
        return Err("Please enter an MCP URL, GitHub repo, command, or JSON config".to_string());
    }

    // 1. Direct JSON snippet (`{ "mcpServers": { ... } }` or single server object)
    if trimmed.starts_with('{') {
        if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
            let (name_key, obj) = if let Some(servers) = val.get("mcpServers").and_then(|v| v.as_object()) {
                let (k, v) = servers
                    .iter()
                    .next()
                    .ok_or_else(|| "Empty mcpServers JSON".to_string())?;
                (k.clone(), v.clone())
            } else {
                ("custom-mcp".to_string(), val)
            };

            let url = obj
                .get("url")
                .or_else(|| obj.get("serverUrl"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let command = obj
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let transport = if !command.is_empty() {
                "stdio".to_string()
            } else if obj.get("type").and_then(|v| v.as_str()) == Some("sse") || url.ends_with("/sse") {
                "sse".to_string()
            } else {
                "http".to_string()
            };

            return Ok(McpInspectResult {
                item: McpLibraryItem {
                    id: sanitize_mcp_id(&name_key),
                    name: name_key,
                    description: "Imported from JSON config".to_string(),
                    source_url: String::new(),
                    transport,
                    url,
                    command,
                    args: obj
                        .get("args")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default(),
                    headers: obj
                        .get("headers")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default(),
                    env: obj
                        .get("env")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default(),
                    auth_fields: Vec::new(),
                    enabled_for_bot: false,
                    cached_tools: Vec::new(),
                    last_test_status: String::new(),
                    last_test_error: String::new(),
                    installations: Vec::new(),
                    created_at: 0,
                    updated_at: 0,
                },
                detected_via: "json".to_string(),
                probe_notes: vec!["Parsed directly from JSON config".to_string()],
            });
        }
    }

    // 2. Direct stdio command (`npx ...`, `uvx ...`, `node ...`)
    if trimmed.starts_with("npx ") || trimmed.starts_with("uvx ") || trimmed.starts_with("node ") || trimmed.starts_with("python ") {
        let parts: Vec<String> = trimmed.split_whitespace().map(|s| s.to_string()).collect();
        let command = parts.first().cloned().unwrap_or_default();
        let args = parts.into_iter().skip(1).collect::<Vec<_>>();
        let raw_name = args
            .iter()
            .find(|a| !a.starts_with('-'))
            .map(|s| s.as_str())
            .unwrap_or("stdio-mcp");
        let id = sanitize_mcp_id(raw_name.split('/').last().unwrap_or(raw_name));
        return Ok(McpInspectResult {
            item: McpLibraryItem {
                id: id.clone(),
                name: id,
                description: "Local stdio MCP server".to_string(),
                source_url: String::new(),
                transport: "stdio".to_string(),
                url: String::new(),
                command,
                args,
                headers: HashMap::new(),
                env: HashMap::new(),
                auth_fields: Vec::new(),
                enabled_for_bot: false,
                cached_tools: Vec::new(),
                last_test_status: String::new(),
                last_test_error: String::new(),
                installations: Vec::new(),
                created_at: 0,
                updated_at: 0,
            },
            detected_via: "command".to_string(),
            probe_notes: vec!["Parsed directly from CLI command".to_string()],
        });
    }

    // 3. URL / GitHub probe (auto-prepend https:// for bare domain inputs)
    let normalized_url = if !trimmed.starts_with("http://")
        && !trimmed.starts_with("https://")
        && !trimmed.contains(' ')
        && trimmed.contains('.')
    {
        format!("https://{}", trimmed)
    } else {
        trimmed.to_string()
    };
    let trimmed = normalized_url.as_str();

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let mut probe_notes = Vec::new();
    let mut evidence = String::new();
    let mut best_url = trimmed.to_string();
    let mut best_transport = if trimmed.ends_with("/sse") { "sse" } else { "http" }.to_string();
    let mut needs_auth = false;
    let mut suggested_header_key = "Authorization".to_string();
    let mut suggested_prefix = "Bearer ".to_string();

    if let Some((owner, repo)) = parse_github_repo(trimmed) {
        probe_notes.push(format!("Detected GitHub repository: {}/{}", owner, repo));
        for branch in ["HEAD", "main", "master"] {
            let readme_url = format!("https://raw.githubusercontent.com/{}/{}/{}/README.md", owner, repo, branch);
            if let Some(txt) = fetch_text_limited(&client, &readme_url, 6000).await {
                evidence.push_str(&format!("--- README.md ({}) ---\n{}\n", branch, txt));
                probe_notes.push(format!("Fetched README.md from branch {}", branch));
                break;
            }
        }
        let pkg_url = format!("https://raw.githubusercontent.com/{}/{}/HEAD/package.json", owner, repo);
        if let Some(pkg) = fetch_text_limited(&client, &pkg_url, 2500).await {
            evidence.push_str(&format!("--- package.json ---\n{}\n", pkg));
        }
    } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let base = trimmed.trim_end_matches('/');
        let mut candidates = vec![trimmed.to_string()];
        if !base.ends_with("/mcp") && !base.ends_with("/sse") {
            candidates.push(format!("{}/mcp", base));
            candidates.push(format!("{}/sse", base));
        }

        for cand in candidates {
            let init_payload = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "raydrop", "version": "1.2.95" }
                }
            });

            if let Ok(resp) = client
                .post(&cand)
                .header("Accept", "application/json, text/event-stream")
                .json(&init_payload)
                .send()
                .await
            {
                let status = resp.status();
                let www_auth = resp
                    .headers()
                    .get("www-authenticate")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                let ctype = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                let body = resp.text().await.unwrap_or_default();
                let snippet: String = body.chars().take(1800).collect();

                probe_notes.push(format!("POST {} -> HTTP {} ({})", cand, status.as_u16(), ctype));
                evidence.push_str(&format!(
                    "POST {} -> HTTP {}, content-type={}, www-authenticate={}, body={}\n",
                    cand, status, ctype, www_auth, snippet
                ));

                if status.as_u16() == 401 || status.as_u16() == 403 {
                    needs_auth = true;
                    best_url = cand.clone();
                    best_transport = if cand.ends_with("/sse") { "sse" } else { "http" }.to_string();
                    if snippet.to_lowercase().contains("x-api-key") {
                        suggested_header_key = "X-API-Key".to_string();
                        suggested_prefix = String::new();
                    }
                    break;
                }
                if status.is_success() && (ctype.contains("json") || ctype.contains("event-stream") || snippet.contains("jsonrpc")) {
                    best_url = cand.clone();
                    best_transport = if cand.ends_with("/sse") { "sse" } else { "http" }.to_string();
                    break;
                }
            }
        }

        // Also GET the root page for docs/HTML context if needed
        if let Some(html) = fetch_text_limited(&client, trimmed, 4000).await {
            evidence.push_str(&format!("--- GET {} ---\n{}\n", trimmed, html));
        }
    }

    // Try AI Config Analyzer if configured in Raydrop Settings
    if let Some(ai_cfg) = load_ai_config(db) {
        match analyze_with_llm(&ai_cfg, trimmed, &evidence).await {
            Ok(mut item) => {
                if item.auth_fields.is_empty() && needs_auth {
                    item.auth_fields.push(McpAuthField {
                        key: suggested_header_key,
                        label: "API Key / Token".to_string(),
                        target: "header".to_string(),
                        header_prefix: suggested_prefix,
                        required: true,
                        placeholder: "Paste your API key or token".to_string(),
                    });
                }
                probe_notes.push(format!("Analyzed with LLM ({})", ai_cfg.llm_model));
                return Ok(McpInspectResult {
                    item,
                    detected_via: "llm".to_string(),
                    probe_notes,
                });
            }
            Err(e) => {
                probe_notes.push(format!("LLM analyzer skipped ({}), using probe fallback", e));
            }
        }
    } else {
        probe_notes.push("No LLM configured in AI Settings; using smart HTTP/repo probe".to_string());
    }

    // Deterministic fallback
    let (id, name) = if let Some((owner, repo)) = parse_github_repo(trimmed) {
        (sanitize_mcp_id(&repo), format!("{}/{} MCP", owner, repo))
    } else {
        slug_from_url(trimmed)
    };

    let mut auth_fields = Vec::new();
    if needs_auth || evidence.to_lowercase().contains("api key") || evidence.to_lowercase().contains("bearer") {
        auth_fields.push(McpAuthField {
            key: suggested_header_key,
            label: "API Key / Bearer Token".to_string(),
            target: "header".to_string(),
            header_prefix: suggested_prefix,
            required: needs_auth,
            placeholder: "Paste API key or token (optional if public)".to_string(),
        });
    }

    Ok(McpInspectResult {
        item: McpLibraryItem {
            id,
            name,
            description: format!("MCP server from {}", trimmed),
            source_url: trimmed.to_string(),
            transport: best_transport,
            url: if trimmed.starts_with("http") && parse_github_repo(trimmed).is_none() {
                best_url
            } else {
                String::new()
            },
            command: String::new(),
            args: Vec::new(),
            headers: HashMap::new(),
            env: HashMap::new(),
            auth_fields,
            enabled_for_bot: false,
            cached_tools: Vec::new(),
            last_test_status: String::new(),
            last_test_error: String::new(),
            installations: Vec::new(),
            created_at: 0,
            updated_at: 0,
        },
        detected_via: "probe".to_string(),
        probe_notes,
    })
}

// ============================================================================
// Coding Agent Config Installer
// ============================================================================

fn build_agent_mcp_entry(agent_id: &str, item: &McpLibraryItem) -> Value {
    if item.transport == "stdio" {
        let mut obj = json!({
            "command": item.command,
            "args": item.args,
        });
        if !item.env.is_empty() {
            obj["env"] = json!(item.env);
        }
        return obj;
    }

    // Remote HTTP / SSE
    let mut obj = match agent_id {
        "antigravity" | "windsurf" => {
            json!({
                "serverUrl": item.url,
            })
        }
        "opencode" => {
            json!({
                "type": "remote",
                "url": item.url,
                "enabled": true,
            })
        }
        _ => {
            json!({
                "type": if item.transport == "sse" { "sse" } else { "http" },
                "url": item.url,
            })
        }
    };

    if !item.headers.is_empty() {
        obj["headers"] = json!(item.headers);
    }
    obj
}

fn upsert_codex_toml_section(existing: &str, mcp_id: &str, item: &McpLibraryItem) -> String {
    let section_header = format!("[mcp_servers.{}]", mcp_id);
    let mut lines_out: Vec<String> = Vec::new();
    let mut skipping = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == section_header || trimmed.starts_with(&format!("[mcp_servers.{}.", mcp_id)) {
                skipping = true;
                continue;
            } else {
                skipping = false;
            }
        }
        if !skipping {
            lines_out.push(line.to_string());
        }
    }

    while lines_out.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        lines_out.pop();
    }

    if !lines_out.is_empty() {
        lines_out.push(String::new());
    }
    lines_out.push(section_header);
    if item.transport == "stdio" {
        lines_out.push(format!("command = {:?}", item.command));
        let args_toml = item
            .args
            .iter()
            .map(|a| format!("{:?}", a))
            .collect::<Vec<_>>()
            .join(", ");
        lines_out.push(format!("args = [{}]", args_toml));
        if !item.env.is_empty() {
            lines_out.push(format!("[mcp_servers.{}.env]", mcp_id));
            for (k, v) in &item.env {
                lines_out.push(format!("{} = {:?}", k, v));
            }
        }
    } else {
        lines_out.push(format!("url = {:?}", item.url));
        if !item.headers.is_empty() {
            let hdrs = item
                .headers
                .iter()
                .map(|(k, v)| format!("{:?} = {:?}", k, v))
                .collect::<Vec<_>>()
                .join(", ");
            lines_out.push(format!("http_headers = {{ {} }}", hdrs));
        }
    }
    lines_out.push(String::new());
    lines_out.join("\n")
}

fn remove_codex_toml_section(existing: &str, mcp_id: &str) -> String {
    let section_header = format!("[mcp_servers.{}]", mcp_id);
    let mut lines_out: Vec<String> = Vec::new();
    let mut skipping = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == section_header || trimmed.starts_with(&format!("[mcp_servers.{}.", mcp_id)) {
                skipping = true;
                continue;
            } else {
                skipping = false;
            }
        }
        if !skipping {
            lines_out.push(line.to_string());
        }
    }
    lines_out.join("\n")
}

pub fn install_mcp_to_agent(
    db: &Database,
    mcp_id: &str,
    agent_id: &str,
) -> Result<McpAgentInstallInfo, String> {
    let item = get_mcp_library_item(db, mcp_id)?
        .ok_or_else(|| format!("MCP '{}' not found", mcp_id))?;
    let agent = get_supported_mcp_agents()
        .into_iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| format!("Unsupported agent '{}'", agent_id))?;

    let path = PathBuf::from(&agent.config_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let existing = if path.exists() {
        std::fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };

    if agent_id == "codex" {
        let updated = upsert_codex_toml_section(&existing, &item.id, &item);
        std::fs::write(&path, updated).map_err(|e| e.to_string())?;
    } else {
        let root_key = if agent_id == "opencode" { "mcp" } else { "mcpServers" };
        let mut root: Value = if existing.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&existing).map_err(|e| {
                format!("Existing config at {} is not valid JSON: {}", agent.config_path, e)
            })?
        };
        if !root.is_object() {
            root = json!({});
        }
        let servers = root
            .as_object_mut()
            .unwrap()
            .entry(root_key)
            .or_insert_with(|| json!({}));
        if !servers.is_object() {
            *servers = json!({});
        }
        servers
            .as_object_mut()
            .unwrap()
            .insert(item.id.clone(), build_agent_mcp_entry(agent_id, &item));

        let pretty = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
        std::fs::write(&path, pretty).map_err(|e| e.to_string())?;
    }

    let now = now_unix_secs();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO mcp_agent_installations (mcp_id, agent_id, config_path, installed_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(mcp_id, agent_id) DO UPDATE SET
            config_path = excluded.config_path,
            installed_at = excluded.installed_at",
        params![item.id, agent_id, agent.config_path, now as i64],
    )
    .map_err(|e| e.to_string())?;

    Ok(McpAgentInstallInfo {
        agent_id: agent.id,
        agent_name: agent.display_name,
        config_path: agent.config_path,
        installed: true,
        installed_at: Some(now),
    })
}

pub fn uninstall_mcp_from_agent(
    db: &Database,
    mcp_id: &str,
    agent_id: &str,
) -> Result<(), String> {
    let agent = get_supported_mcp_agents()
        .into_iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| format!("Unsupported agent '{}'", agent_id))?;

    let path = PathBuf::from(&agent.config_path);
    if path.exists() {
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        if agent_id == "codex" {
            let updated = remove_codex_toml_section(&existing, mcp_id);
            let _ = std::fs::write(&path, updated);
        } else if let Ok(mut root) = serde_json::from_str::<Value>(&existing) {
            let root_key = if agent_id == "opencode" { "mcp" } else { "mcpServers" };
            if let Some(servers) = root.get_mut(root_key).and_then(|v| v.as_object_mut()) {
                servers.remove(mcp_id);
                if let Ok(pretty) = serde_json::to_string_pretty(&root) {
                    let _ = std::fs::write(&path, pretty);
                }
            }
        }
    }

    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM mcp_agent_installations WHERE mcp_id = ?1 AND agent_id = ?2",
        params![mcp_id, agent_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[tauri::command]
pub async fn mcp_library_list(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<McpLibraryItem>, String> {
    list_mcp_library_items(&state.db)
}

#[tauri::command]
pub async fn mcp_library_supported_agents() -> Result<Vec<McpSupportedAgent>, String> {
    Ok(get_supported_mcp_agents())
}

#[tauri::command]
pub async fn mcp_library_inspect(
    state: tauri::State<'_, Arc<AppState>>,
    input: String,
) -> Result<McpInspectResult, String> {
    inspect_mcp_source(&state.db, &input).await
}

#[tauri::command]
pub async fn mcp_library_save(
    state: tauri::State<'_, Arc<AppState>>,
    item: McpLibraryItem,
) -> Result<McpLibraryItem, String> {
    upsert_mcp_library_item(&state.db, &item)
}

#[tauri::command]
pub async fn mcp_library_delete(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    let supported = get_supported_mcp_agents();
    for ag in supported {
        let _ = uninstall_mcp_from_agent(&state.db, &id, &ag.id);
    }
    delete_mcp_library_item(&state.db, &id)
}

#[tauri::command]
pub async fn mcp_library_test(
    state: tauri::State<'_, Arc<AppState>>,
    item: McpLibraryItem,
) -> Result<McpTestResult, String> {
    let exists_in_db = get_mcp_library_item(&state.db, &item.id)?.is_some();
    match test_mcp_item_connection(&item).await {
        Ok(tools) => {
            if exists_in_db {
                let mut updated = item.clone();
                updated.cached_tools = tools.clone();
                updated.last_test_status = "ok".to_string();
                updated.last_test_error = String::new();
                let _ = upsert_mcp_library_item(&state.db, &updated);
            }
            Ok(McpTestResult {
                ok: true,
                message: format!("Connected • {} tools detected", tools.len()),
                tools,
            })
        }
        Err(err) => {
            if exists_in_db {
                let mut updated = item.clone();
                updated.last_test_status = "error".to_string();
                updated.last_test_error = err.clone();
                let _ = upsert_mcp_library_item(&state.db, &updated);
            }
            Ok(McpTestResult {
                ok: false,
                message: err,
                tools: Vec::new(),
            })
        }
    }
}

#[tauri::command]
pub async fn mcp_library_toggle_bot(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    enabled: bool,
) -> Result<McpLibraryItem, String> {
    let mut item = get_mcp_library_item(&state.db, &id)?
        .ok_or_else(|| format!("MCP '{}' not found", id))?;
    item.enabled_for_bot = enabled;
    if enabled && item.cached_tools.is_empty() {
        if let Ok(tools) = test_mcp_item_connection(&item).await {
            item.cached_tools = tools;
            item.last_test_status = "ok".to_string();
            item.last_test_error = String::new();
        }
    }
    upsert_mcp_library_item(&state.db, &item)
}

#[tauri::command]
pub async fn mcp_library_install_agent(
    state: tauri::State<'_, Arc<AppState>>,
    mcp_id: String,
    agent_id: String,
) -> Result<McpAgentInstallInfo, String> {
    install_mcp_to_agent(&state.db, &mcp_id, &agent_id)
}

#[tauri::command]
pub async fn mcp_library_uninstall_agent(
    state: tauri::State<'_, Arc<AppState>>,
    mcp_id: String,
    agent_id: String,
) -> Result<(), String> {
    uninstall_mcp_from_agent(&state.db, &mcp_id, &agent_id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPopularPreset {
    pub category: String,
    pub requires_auth: bool,
    pub item: McpLibraryItem,
}

fn preset_item(
    id: &str,
    name: &str,
    description: &str,
    source_url: &str,
    transport: &str,
    url: &str,
    command: &str,
    args: &[&str],
    auth_fields: Vec<McpAuthField>,
) -> McpLibraryItem {
    McpLibraryItem {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        source_url: source_url.to_string(),
        transport: transport.to_string(),
        url: url.to_string(),
        command: command.to_string(),
        args: args.iter().map(|s| (*s).to_string()).collect(),
        headers: HashMap::new(),
        env: HashMap::new(),
        auth_fields,
        enabled_for_bot: false,
        cached_tools: Vec::new(),
        last_test_status: String::new(),
        last_test_error: String::new(),
        installations: Vec::new(),
        created_at: 0,
        updated_at: 0,
    }
}

fn env_field(key: &str, label: &str, placeholder: &str, required: bool) -> McpAuthField {
    McpAuthField {
        key: key.to_string(),
        label: label.to_string(),
        target: "env".to_string(),
        header_prefix: String::new(),
        required,
        placeholder: placeholder.to_string(),
    }
}

#[tauri::command]
pub async fn mcp_library_popular_presets() -> Result<Vec<McpPopularPreset>, String> {
    Ok(vec![
        McpPopularPreset {
            category: "Coding & Web".to_string(),
            requires_auth: false,
            item: preset_item(
                "context7",
                "Context7 (Upstash)",
                "Fetch up-to-date library & framework documentation directly into your AI agent.",
                "https://mcp.context7.com/mcp",
                "http",
                "https://mcp.context7.com/mcp",
                "",
                &[],
                vec![],
            ),
        },
        McpPopularPreset {
            category: "Coding & Web".to_string(),
            requires_auth: false,
            item: preset_item(
                "playwright",
                "Playwright MCP",
                "Official Microsoft browser automation, UI interaction, screenshots, and E2E testing.",
                "https://github.com/microsoft/playwright-mcp",
                "stdio",
                "",
                "npx",
                &["-y", "@playwright/mcp@latest"],
                vec![],
            ),
        },
        McpPopularPreset {
            category: "Coding & Web".to_string(),
            requires_auth: false,
            item: preset_item(
                "sequential-thinking",
                "Sequential Thinking",
                "Dynamic and reflective step-by-step problem-solving tool for complex tasks.",
                "https://github.com/modelcontextprotocol/servers",
                "stdio",
                "",
                "npx",
                &["-y", "@modelcontextprotocol/server-sequential-thinking"],
                vec![],
            ),
        },
        McpPopularPreset {
            category: "Coding & Web".to_string(),
            requires_auth: false,
            item: preset_item(
                "fetch",
                "Fetch (Web Reader)",
                "Fetch web pages and convert HTML into clean Markdown for LLM consumption.",
                "https://github.com/modelcontextprotocol/servers",
                "stdio",
                "",
                "uvx",
                &["mcp-server-fetch"],
                vec![],
            ),
        },
        McpPopularPreset {
            category: "Coding & Web".to_string(),
            requires_auth: false,
            item: preset_item(
                "memory",
                "Memory (Knowledge Graph)",
                "Persistent local knowledge-graph memory across AI sessions.",
                "https://github.com/modelcontextprotocol/servers",
                "stdio",
                "",
                "npx",
                &["-y", "@modelcontextprotocol/server-memory"],
                vec![],
            ),
        },
        McpPopularPreset {
            category: "Coding & Web".to_string(),
            requires_auth: false,
            item: preset_item(
                "filesystem",
                "Filesystem MCP",
                "Secure local filesystem read/write/search scoped to allowed directories.",
                "https://github.com/modelcontextprotocol/servers",
                "stdio",
                "",
                "npx",
                &["-y", "@modelcontextprotocol/server-filesystem", "."],
                vec![],
            ),
        },
        McpPopularPreset {
            category: "Database".to_string(),
            requires_auth: true,
            item: preset_item(
                "mysql",
                "MySQL MCP",
                "Inspect MySQL schemas, tables, and run read/write SQL queries via AI.",
                "https://github.com/benborla/mcp-server-mysql",
                "stdio",
                "",
                "npx",
                &["-y", "@benborla29/mcp-server-mysql"],
                vec![
                    env_field("MYSQL_HOST", "MySQL Host", "127.0.0.1", true),
                    env_field("MYSQL_PORT", "MySQL Port", "3306", false),
                    env_field("MYSQL_USER", "MySQL User", "root", true),
                    env_field("MYSQL_PASS", "MySQL Password", "password", false),
                    env_field("MYSQL_DB", "Database Name", "app_db", true),
                ],
            ),
        },
        McpPopularPreset {
            category: "Database".to_string(),
            requires_auth: false,
            item: preset_item(
                "postgres",
                "PostgreSQL MCP",
                "Read-only schema inspection and SQL querying for PostgreSQL databases (set connection URL in args).",
                "https://github.com/modelcontextprotocol/servers",
                "stdio",
                "",
                "npx",
                &["-y", "@modelcontextprotocol/server-postgres", "postgresql://user:pass@localhost:5432/dbname"],
                vec![],
            ),
        },
        McpPopularPreset {
            category: "Database".to_string(),
            requires_auth: false,
            item: preset_item(
                "sqlite",
                "SQLite MCP",
                "Inspect tables, run queries, and analyze local SQLite database files.",
                "https://github.com/modelcontextprotocol/servers",
                "stdio",
                "",
                "uvx",
                &["mcp-server-sqlite", "--db-path", "./data.db"],
                vec![],
            ),
        },
        McpPopularPreset {
            category: "Database".to_string(),
            requires_auth: true,
            item: preset_item(
                "supabase",
                "Supabase MCP",
                "Manage Supabase projects, Postgres tables, migrations, and Edge Functions.",
                "https://github.com/supabase-community/supabase-mcp",
                "stdio",
                "",
                "npx",
                &["-y", "@supabase/mcp-server-supabase@latest"],
                vec![env_field("SUPABASE_ACCESS_TOKEN", "Supabase Access Token", "sbp_...", true)],
            ),
        },
        McpPopularPreset {
            category: "Workspace & Search".to_string(),
            requires_auth: true,
            item: preset_item(
                "github",
                "GitHub MCP",
                "Manage GitHub repositories, issues, pull requests, branches, and code search.",
                "https://github.com/modelcontextprotocol/servers",
                "stdio",
                "",
                "npx",
                &["-y", "@modelcontextprotocol/server-github"],
                vec![env_field("GITHUB_PERSONAL_ACCESS_TOKEN", "GitHub Personal Access Token", "ghp_...", true)],
            ),
        },
        McpPopularPreset {
            category: "Workspace & Search".to_string(),
            requires_auth: true,
            item: preset_item(
                "brave-search",
                "Brave Search MCP",
                "Real-time web and local search powered by the Brave Search API.",
                "https://github.com/modelcontextprotocol/servers",
                "stdio",
                "",
                "npx",
                &["-y", "@modelcontextprotocol/server-brave-search"],
                vec![env_field("BRAVE_API_KEY", "Brave Search API Key", "BSA...", true)],
            ),
        },
        McpPopularPreset {
            category: "Workspace & Search".to_string(),
            requires_auth: true,
            item: preset_item(
                "firecrawl",
                "Firecrawl MCP",
                "Deep web scraping, crawling, and structured content extraction for AI.",
                "https://github.com/mendableai/firecrawl-mcp-server",
                "stdio",
                "",
                "npx",
                &["-y", "firecrawl-mcp"],
                vec![env_field("FIRECRAWL_API_KEY", "Firecrawl API Key", "fc-...", true)],
            ),
        },
        McpPopularPreset {
            category: "Workspace & Search".to_string(),
            requires_auth: true,
            item: preset_item(
                "notion",
                "Notion MCP",
                "Search, read, create, and update Notion pages and databases.",
                "https://github.com/makenotion/notion-mcp-server",
                "stdio",
                "",
                "npx",
                &["-y", "@notionhq/notion-mcp-server"],
                vec![env_field("NOTION_TOKEN", "Notion Integration Token", "ntn_...", true)],
            ),
        },
        McpPopularPreset {
            category: "Workspace & Search".to_string(),
            requires_auth: false,
            item: preset_item(
                "figma",
                "Figma MCP",
                "Official remote Figma MCP server for reading designs, components, and tokens.",
                "https://mcp.figma.com/mcp",
                "http",
                "https://mcp.figma.com/mcp",
                "",
                &[],
                vec![],
            ),
        },
    ])
}
