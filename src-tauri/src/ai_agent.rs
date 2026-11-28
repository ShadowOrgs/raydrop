use crate::db::storage::{now_unix_secs, Database};
use crate::mcp_library::{call_external_mcp_tool, get_enabled_bot_mcp_tools};
use crate::mcp_server::{execute_mcp_tool, get_mcp_tool_definitions};
use crate::state::AppState;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::AppHandle;

const CONFIG_SETTING_KEY: &str = "ai_mcp_config_json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramAllowedUser {
    pub user_id: i64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiMcpConfig {
    // MCP Server
    pub mcp_enabled: bool,
    pub mcp_port: u16,
    pub mcp_auth_token: String,

    // LLM Provider
    pub llm_provider: String, // "openai" | "gemini" | "anthropic" | "deepseek" | "groq" | "openrouter" | "ollama" | "custom"
    pub llm_base_url: String,
    pub llm_api_key: String,
    pub llm_model: String,
    pub llm_temperature: f32,
    pub llm_max_tokens: u32,
    #[serde(default)]
    pub llm_prompt_tool_mode: bool,

    // Bot Persona
    pub persona_name: String,
    pub persona_preset: String,   // "assistant" | "friendly" | "concise" | "custom"
    pub persona_language: String, // "auto" | "id" | "en"
    pub persona_system_prompt: String,

    // Telegram Bot
    pub telegram_enabled: bool,
    pub telegram_bot_token: String,
    pub telegram_allowed_users: Vec<TelegramAllowedUser>,
}

impl Default for AiMcpConfig {
    fn default() -> Self {
        Self {
            mcp_enabled: true,
            mcp_port: 48765,
            mcp_auth_token: String::new(),
            llm_provider: "openai".to_string(),
            llm_base_url: "https://api.openai.com/v1".to_string(),
            llm_api_key: String::new(),
            llm_model: "gpt-4o-mini".to_string(),
            llm_temperature: 0.7,
            llm_max_tokens: 2048,
            llm_prompt_tool_mode: false,
            persona_name: "RayBot".to_string(),
            persona_preset: "assistant".to_string(),
            persona_language: "auto".to_string(),
            persona_system_prompt:
                "You are RayBot, a helpful AI assistant controlling the user's RayDrop PC, RayAmp media player, Wake-on-LAN, notes, clipboard, and system telemetry via MCP tools. Always confirm actions clearly and concisely."
                    .to_string(),
            telegram_enabled: false,
            telegram_bot_token: String::new(),
            telegram_allowed_users: Vec::new(),
        }
    }
}

pub fn load_ai_mcp_config(db: &Database) -> AiMcpConfig {
    match db.get_setting(CONFIG_SETTING_KEY) {
        Ok(Some(raw)) => serde_json::from_str(&raw).unwrap_or_default(),
        _ => AiMcpConfig::default(),
    }
}

pub fn save_ai_mcp_config(db: &Database, cfg: &AiMcpConfig) -> Result<(), String> {
    let raw = serde_json::to_string(cfg).map_err(|e| e.to_string())?;
    db.set_setting(CONFIG_SETTING_KEY, &raw)
}

pub fn build_persona_system_prompt(cfg: &AiMcpConfig) -> String {
    let bot_name = if cfg.persona_name.trim().is_empty() {
        "RayBot"
    } else {
        cfg.persona_name.trim()
    };

    let style_hint = match cfg.persona_preset.as_str() {
        "friendly" => "Use a warm, friendly, and casual conversational tone.",
        "concise" => "Be ultra-concise, direct, and technical. Avoid filler.",
        "custom" => "",
        _ => "Be polite, clear, and structured like a reliable personal assistant.",
    };

    let lang_hint = match cfg.persona_language.as_str() {
        "id" => "Always respond in Indonesian (Bahasa Indonesia).",
        "en" => "Always respond in English.",
        _ => "Match the language used by the user.",
    };

    format!(
        "Your name is {}. {}\n{}\n\nCustom Persona Instructions:\n{}",
        bot_name,
        style_hint,
        lang_hint,
        cfg.persona_system_prompt.trim()
    )
}

pub async fn test_llm_connection(cfg: &AiMcpConfig) -> Result<String, String> {
    let model = cfg.llm_model.trim();
    if model.is_empty() {
        return Err("Model name cannot be empty".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .build()
        .map_err(|e| e.to_string())?;

    if cfg.llm_provider == "anthropic" {
        let base = cfg.llm_base_url.trim().trim_end_matches('/');
        let url = format!("{}/messages", base);
        let resp = client
            .post(&url)
            .header("x-api-key", cfg.llm_api_key.trim())
            .header("anthropic-version", "2023-06-01")
            .json(&json!({
                "model": model,
                "max_tokens": 32,
                "messages": [{ "role": "user", "content": "Reply with OK only." }]
            }))
            .send()
            .await
            .map_err(|e| format!("Request error: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("Anthropic HTTP {}: {}", status, text));
        }
        return Ok(format!("Connected to Anthropic ({})", model));
    }

    let base = cfg.llm_base_url.trim().trim_end_matches('/');
    let url = format!("{}/chat/completions", base);
    let mut req = client.post(&url);
    if !cfg.llm_api_key.trim().is_empty() {
        req = req.bearer_auth(cfg.llm_api_key.trim());
    }
    let resp = req
        .json(&json!({
            "model": model,
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "Reply with OK only." }]
        }))
        .send()
        .await
        .map_err(|e| format!("Request error: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status, text));
    }
    Ok(format!("Connected to {} ({})", base, model))
}

/// Run an AI conversation turn with full RayDrop MCP tool-calling support.
pub async fn chat_with_mcp_tools(
    app: &AppHandle,
    state: &Arc<AppState>,
    cfg: &AiMcpConfig,
    history: &mut Vec<Value>,
    user_message: &str,
) -> Result<String, String> {
    if cfg.llm_provider == "anthropic" {
        chat_anthropic_loop(app, state, cfg, history, user_message).await
    } else if cfg.llm_prompt_tool_mode {
        chat_prompt_tool_loop(app, state, cfg, history, user_message).await
    } else {
        match chat_openai_compatible_loop(app, state, cfg, history, user_message).await {
            Ok(res) => Ok(res),
            Err(err)
                if err.contains("timed out")
                    || err.contains("tools")
                    || err.contains("HTTP 400")
                    || err.contains("HTTP 422") =>
            {
                // Auto-fallback to Prompt-Based Tool Calling if proxy doesn't support OpenAI `tools`
                chat_prompt_tool_loop(app, state, cfg, history, user_message).await
            }
            Err(err) => Err(err),
        }
    }
}

fn sanitize_llm_tool_part(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn make_bot_mcp_fn_name(mcp_id: &str, tool_name: &str) -> String {
    let full = format!("extmcp__{}__{}", mcp_id, sanitize_llm_tool_part(tool_name));
    if full.len() <= 64 {
        full
    } else {
        full.chars().take(64).collect()
    }
}

fn get_all_bot_tool_definitions(state: &Arc<AppState>) -> Vec<Value> {
    let mut defs = get_mcp_tool_definitions();
    for (mcp_id, mcp_name, tool) in get_enabled_bot_mcp_tools(&state.db) {
        defs.push(json!({
            "name": make_bot_mcp_fn_name(&mcp_id, &tool.name),
            "description": format!("[{}] {}", mcp_name, tool.description),
            "inputSchema": tool.input_schema
        }));
    }
    defs
}

fn to_all_openai_tools(state: &Arc<AppState>) -> Vec<Value> {
    get_all_bot_tool_definitions(state)
        .into_iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t["name"],
                    "description": t["description"],
                    "parameters": t["inputSchema"]
                }
            })
        })
        .collect()
}

async fn execute_bot_tool(
    app: &AppHandle,
    state: &Arc<AppState>,
    name: &str,
    args: &Value,
) -> Result<String, String> {
    if name.starts_with("extmcp__") {
        for (mcp_id, _, tool) in get_enabled_bot_mcp_tools(&state.db) {
            if make_bot_mcp_fn_name(&mcp_id, &tool.name) == name {
                return call_external_mcp_tool(&state.db, &mcp_id, &tool.name, args).await;
            }
        }
    }
    execute_mcp_tool(app, state, name, args).await
}

fn extract_prompt_tool_call(text: &str) -> Option<(String, Value)> {
    let start = text.find("<tool_call>")?;
    let after = &text[start + "<tool_call>".len()..];
    let end = after.find("</tool_call>")?;
    let json_str = after[..end].trim();
    let val: Value = serde_json::from_str(json_str).ok()?;
    let name = val.get("name")?.as_str()?.to_string();
    let args = val.get("arguments").cloned().unwrap_or(json!({}));
    Some((name, args))
}

async fn chat_prompt_tool_loop(
    app: &AppHandle,
    state: &Arc<AppState>,
    cfg: &AiMcpConfig,
    history: &mut Vec<Value>,
    user_message: &str,
) -> Result<String, String> {
    let base = cfg.llm_base_url.trim().trim_end_matches('/');
    if base.is_empty() || cfg.llm_model.trim().is_empty() {
        return Err("LLM Base URL and Model must be configured in Settings".to_string());
    }
    let url = format!("{}/chat/completions", base);

    history.push(json!({ "role": "user", "content": user_message }));
    if history.len() > 20 {
        let drain = history.len() - 20;
        history.drain(0..drain);
    }

    let tool_specs: Vec<String> = get_all_bot_tool_definitions(state)
        .into_iter()
        .map(|t| {
            format!(
                "- {}: {} | args: {}",
                t["name"].as_str().unwrap_or(""),
                t["description"].as_str().unwrap_or(""),
                t["inputSchema"]
            )
        })
        .collect();

    let system_prompt = format!(
        "{}\n\nYou have access to RayDrop MCP tools:\n{}\n\nWhen you need to call a tool, respond ONLY with:\n<tool_call>{{\"name\":\"tool_name\",\"arguments\":{{...}}}}</tool_call>\nAfter receiving the tool result, answer the user clearly.",
        build_persona_system_prompt(cfg),
        tool_specs.join("\n")
    );

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(90))
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|e| e.to_string())?;

    for _step in 0..5 {
        let mut messages = vec![json!({ "role": "system", "content": system_prompt })];
        messages.extend(history.clone());

        let mut req = client.post(&url);
        if !cfg.llm_api_key.trim().is_empty() {
            req = req.bearer_auth(cfg.llm_api_key.trim());
        }

        let resp = req
            .json(&json!({
                "model": cfg.llm_model.trim(),
                "messages": messages,
                "temperature": cfg.llm_temperature,
                "max_tokens": cfg.llm_max_tokens
            }))
            .send()
            .await
            .map_err(|e| format!("LLM request failed: {}", format_reqwest_error(&e)))?;

        let status = resp.status();
        let raw_text = resp
            .text()
            .await
            .map_err(|e| format!("Failed reading LLM response: {}", format_reqwest_error(&e)))?;
        let body: Value = serde_json::from_str(&raw_text)
            .map_err(|e| format!("Failed to parse LLM JSON ({}): {}", e, raw_text))?;

        if !status.is_success() {
            return Err(format!("LLM HTTP {}: {}", status, body));
        }

        let reply_text = body
            .pointer("/choices/0/message/content")
            .and_then(|c| c.as_str())
            .unwrap_or("(No response)")
            .to_string();

        history.push(json!({ "role": "assistant", "content": reply_text }));

        if let Some((fn_name, args_val)) = extract_prompt_tool_call(&reply_text) {
            let tool_out = match execute_bot_tool(app, state, &fn_name, &args_val).await {
                Ok(out) => out,
                Err(e) => format!("Tool error: {}", e),
            };
            history.push(json!({
                "role": "user",
                "content": format!("[Tool Result for {}]:\n{}", fn_name, tool_out)
            }));
            continue;
        }

        return Ok(reply_text);
    }

    Err("AI reached maximum tool-calling steps".to_string())
}

fn format_reqwest_error(e: &reqwest::Error) -> String {
    use std::error::Error;
    let mut parts = vec![e.to_string()];
    let mut cur = e.source();
    while let Some(src) = cur {
        parts.push(src.to_string());
        cur = src.source();
    }
    parts.join(" -> ")
}

async fn chat_openai_compatible_loop(
    app: &AppHandle,
    state: &Arc<AppState>,
    cfg: &AiMcpConfig,
    history: &mut Vec<Value>,
    user_message: &str,
) -> Result<String, String> {
    let base = cfg.llm_base_url.trim().trim_end_matches('/');
    if base.is_empty() || cfg.llm_model.trim().is_empty() {
        return Err("LLM Base URL and Model must be configured in Settings".to_string());
    }
    let url = format!("{}/chat/completions", base);

    history.push(json!({ "role": "user", "content": user_message }));
    if history.len() > 24 {
        let drain = history.len() - 24;
        history.drain(0..drain);
    }

    let system_prompt = build_persona_system_prompt(cfg);
    let tools = to_all_openai_tools(state);
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(120))
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|e| e.to_string())?;

    for _step in 0..6 {
        let mut messages = vec![json!({ "role": "system", "content": system_prompt })];
        messages.extend(history.clone());

        let payload = json!({
            "model": cfg.llm_model.trim(),
            "messages": messages,
            "tools": tools,
            "temperature": cfg.llm_temperature,
            "max_tokens": cfg.llm_max_tokens
        });

        let mut resp_opt = None;
        let mut last_send_err = String::new();
        for attempt in 0..2 {
            let mut req = client.post(&url);
            if !cfg.llm_api_key.trim().is_empty() {
                req = req.bearer_auth(cfg.llm_api_key.trim());
            }
            match req.json(&payload).send().await {
                Ok(r) => {
                    resp_opt = Some(r);
                    break;
                }
                Err(e) => {
                    last_send_err = format_reqwest_error(&e);
                    if attempt == 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    }
                }
            }
        }
        let resp = resp_opt.ok_or_else(|| format!("LLM request failed: {}", last_send_err))?;

        let status = resp.status();
        let raw_text = resp
            .text()
            .await
            .map_err(|e| format!("Failed reading LLM response: {}", format_reqwest_error(&e)))?;
        let body: Value = serde_json::from_str(&raw_text)
            .map_err(|e| format!("Failed to parse LLM JSON ({}): {}", e, raw_text))?;

        if !status.is_success() {
            return Err(format!("LLM HTTP {}: {}", status, body));
        }

        let msg = body
            .pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| "Missing choices[0].message in LLM response".to_string())?;

        history.push(msg.clone());

        let tool_calls = msg
            .get("tool_calls")
            .and_then(|tc| tc.as_array())
            .cloned()
            .unwrap_or_default();

        if tool_calls.is_empty() {
            let reply = msg
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("(No response)")
                .to_string();
            return Ok(reply);
        }

        for tc in tool_calls {
            let tc_id = tc["id"].as_str().unwrap_or("call_0").to_string();
            let fn_name = tc
                .pointer("/function/name")
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let args_raw = tc
                .pointer("/function/arguments")
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            let args_val: Value = serde_json::from_str(args_raw).unwrap_or(json!({}));

            let tool_out = match execute_bot_tool(app, state, fn_name, &args_val).await {
                Ok(out) => out,
                Err(e) => format!("Tool error: {}", e),
            };

            history.push(json!({
                "role": "tool",
                "tool_call_id": tc_id,
                "content": tool_out
            }));
        }
    }

    Err("AI reached maximum tool-calling steps".to_string())
}

async fn chat_anthropic_loop(
    app: &AppHandle,
    state: &Arc<AppState>,
    cfg: &AiMcpConfig,
    history: &mut Vec<Value>,
    user_message: &str,
) -> Result<String, String> {
    let base = cfg.llm_base_url.trim().trim_end_matches('/');
    let url = format!("{}/messages", base);

    history.push(json!({ "role": "user", "content": user_message }));
    if history.len() > 20 {
        let drain = history.len() - 20;
        history.drain(0..drain);
    }

    let system_prompt = build_persona_system_prompt(cfg);
    let anthropic_tools: Vec<Value> = get_all_bot_tool_definitions(state)
        .into_iter()
        .map(|t| {
            json!({
                "name": t["name"],
                "description": t["description"],
                "input_schema": t["inputSchema"]
            })
        })
        .collect();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    for _step in 0..6 {
        let resp = client
            .post(&url)
            .header("x-api-key", cfg.llm_api_key.trim())
            .header("anthropic-version", "2023-06-01")
            .json(&json!({
                "model": cfg.llm_model.trim(),
                "system": system_prompt,
                "messages": history,
                "tools": anthropic_tools,
                "max_tokens": cfg.llm_max_tokens,
                "temperature": cfg.llm_temperature
            }))
            .send()
            .await
            .map_err(|e| format!("Anthropic request failed: {}", e))?;

        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Anthropic JSON: {}", e))?;

        if !status.is_success() {
            return Err(format!("Anthropic HTTP {}: {}", status, body));
        }

        let content_blocks = body
            .get("content")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        history.push(json!({ "role": "assistant", "content": content_blocks }));

        let mut tool_results = Vec::new();
        let mut text_parts = Vec::new();

        for block in &content_blocks {
            match block.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                "text" => {
                    if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                        text_parts.push(t.to_string());
                    }
                }
                "tool_use" => {
                    let id = block["id"].as_str().unwrap_or("tool_0");
                    let name = block["name"].as_str().unwrap_or("");
                    let input = block.get("input").cloned().unwrap_or(json!({}));
                    let out = match execute_bot_tool(app, state, name, &input).await {
                        Ok(s) => s,
                        Err(e) => format!("Tool error: {}", e),
                    };
                    tool_results.push(json!({
                        "type": "tool_result",
                        "tool_use_id": id,
                        "content": out
                    }));
                }
                _ => {}
            }
        }

        if tool_results.is_empty() {
            return Ok(text_parts.join("\n"));
        }

        history.push(json!({
            "role": "user",
            "content": tool_results
        }));
    }

    Err("Anthropic AI reached maximum tool-calling steps".to_string())
}

// ============================================================================
// Persistent RayBot Chat (Sessions, Messages, Attachments: File / Image / Voice)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaybotAttachment {
    pub kind: String, // "image" | "file" | "voice"
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub mime: String,
    #[serde(default)]
    pub data_url: String,
    #[serde(default)]
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaybotSession {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub last_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaybotMessage {
    pub id: String,
    pub session_id: String,
    pub role: String, // "user" | "assistant"
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<RaybotAttachment>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaybotSendResult {
    pub user_message: RaybotMessage,
    pub assistant_message: RaybotMessage,
    pub session: RaybotSession,
}

pub fn list_raybot_sessions(db: &Database) -> Result<Vec<RaybotSession>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.title, s.created_at, s.updated_at,
                    COALESCE((SELECT m.content FROM raybot_messages m WHERE m.session_id = s.id ORDER BY m.created_at DESC LIMIT 1), '')
             FROM raybot_sessions s
             ORDER BY s.updated_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(RaybotSession {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get::<_, i64>(2)? as u64,
                updated_at: row.get::<_, i64>(3)? as u64,
                last_message: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

pub fn create_raybot_session(db: &Database, title: Option<&str>) -> Result<RaybotSession, String> {
    let now = now_unix_secs();
    let id = format!("rb_{}", uuid::Uuid::new_v4().simple());
    let clean_title = title
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("New Chat")
        .to_string();

    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO raybot_sessions (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, clean_title, now as i64, now as i64],
    )
    .map_err(|e| e.to_string())?;

    Ok(RaybotSession {
        id,
        title: clean_title,
        created_at: now,
        updated_at: now,
        last_message: String::new(),
    })
}

pub fn list_raybot_messages(db: &Database, session_id: &str) -> Result<Vec<RaybotMessage>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, role, content, attachments_json, created_at
             FROM raybot_messages
             WHERE session_id = ?1
             ORDER BY created_at ASC, rowid ASC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![session_id], |row| {
            let att_json: String = row.get(4)?;
            Ok(RaybotMessage {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                attachments: serde_json::from_str(&att_json).unwrap_or_default(),
                created_at: row.get::<_, i64>(5)? as u64,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

fn insert_raybot_message(db: &Database, msg: &RaybotMessage) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let att_json = serde_json::to_string(&msg.attachments).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO raybot_messages (id, session_id, role, content, attachments_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            msg.id,
            msg.session_id,
            msg.role,
            msg.content,
            att_json,
            msg.created_at as i64
        ],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE raybot_sessions SET updated_at = ?1 WHERE id = ?2",
        params![msg.created_at as i64, msg.session_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn build_attachment_context(attachments: &[RaybotAttachment]) -> (String, Vec<String>) {
    let mut notes = Vec::new();
    let mut image_urls = Vec::new();

    for att in attachments {
        match att.kind.as_str() {
            "image" => {
                let label = if att.name.is_empty() { "pasted-image.png" } else { &att.name };
                notes.push(format!("[Attached Image: {}]", label));
                if !att.data_url.trim().is_empty() {
                    image_urls.push(att.data_url.clone());
                }
            }
            "voice" => {
                notes.push(format!("[Attached Voice Note: {} ({} bytes)]", att.name, att.size_bytes));
            }
            _ => {
                let path = att.path.trim();
                let mut file_note = format!("[Attached File: {} | path={} | {} bytes]", att.name, path, att.size_bytes);
                if !path.is_empty() {
                    if let Ok(bytes) = std::fs::read(path) {
                        if bytes.len() <= 24 * 1024 {
                            if let Ok(text) = std::str::from_utf8(&bytes) {
                                file_note.push_str(&format!("\n```\n{}\n```", text));
                            }
                        }
                    }
                }
                notes.push(file_note);
            }
        }
    }

    (notes.join("\n\n"), image_urls)
}

#[tauri::command]
pub async fn raybot_list_sessions(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<RaybotSession>, String> {
    list_raybot_sessions(&state.db)
}

#[tauri::command]
pub async fn raybot_create_session(
    state: tauri::State<'_, Arc<AppState>>,
    title: Option<String>,
) -> Result<RaybotSession, String> {
    create_raybot_session(&state.db, title.as_deref())
}

#[tauri::command]
pub async fn raybot_rename_session(
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
    title: String,
) -> Result<(), String> {
    let clean = title.trim();
    if clean.is_empty() {
        return Err("Title cannot be empty".to_string());
    }
    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE raybot_sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
        params![clean, now_unix_secs() as i64, session_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn raybot_delete_session(
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<(), String> {
    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM raybot_messages WHERE session_id = ?1", params![session_id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM raybot_sessions WHERE id = ?1", params![session_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn raybot_get_messages(
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<Vec<RaybotMessage>, String> {
    list_raybot_messages(&state.db, &session_id)
}

#[tauri::command]
pub async fn raybot_send_message(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
    text: String,
    attachments: Vec<RaybotAttachment>,
) -> Result<RaybotSendResult, String> {
    let cfg = load_ai_mcp_config(&state.db);
    if cfg.llm_base_url.trim().is_empty() || cfg.llm_model.trim().is_empty() {
        return Err("Configure LLM Base URL & Model in Settings -> AI & MCP first.".to_string());
    }

    let now = now_unix_secs();
    let user_msg = RaybotMessage {
        id: format!("rbm_{}", uuid::Uuid::new_v4().simple()),
        session_id: session_id.clone(),
        role: "user".to_string(),
        content: text.clone(),
        attachments: attachments.clone(),
        created_at: now,
    };

    // Load prior messages for context before inserting current user message
    let prior = list_raybot_messages(&state.db, &session_id)?;
    let mut history: Vec<Value> = prior
        .into_iter()
        .rev()
        .take(18)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect();

    insert_raybot_message(&state.db, &user_msg)?;

    // Auto-title session on first message
    {
        let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
        let current_title: String = conn
            .query_row(
                "SELECT title FROM raybot_sessions WHERE id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "New Chat".to_string());
        if current_title == "New Chat" {
            let auto_title_src = if !text.trim().is_empty() {
                text.trim().to_string()
            } else if let Some(first_att) = attachments.first() {
                first_att.name.clone()
            } else {
                "RayBot Chat".to_string()
            };
            let short: String = auto_title_src.chars().take(42).collect();
            let _ = conn.execute(
                "UPDATE raybot_sessions SET title = ?1 WHERE id = ?2",
                params![short, session_id],
            );
        }
    }

    let (att_ctx, image_urls) = build_attachment_context(&attachments);
    let combined_prompt = match (text.trim().is_empty(), att_ctx.is_empty()) {
        (false, true) => text.clone(),
        (true, false) => att_ctx,
        (false, false) => format!("{}\n\n{}", text.trim(), att_ctx),
        (true, true) => "Hello".to_string(),
    };

    // If images are attached and using OpenAI-compatible vision mode, try multimodal first
    let reply = if !image_urls.is_empty() && cfg.llm_provider != "anthropic" && !cfg.llm_prompt_tool_mode {
        let mut parts = vec![json!({ "type": "text", "text": combined_prompt })];
        for img in &image_urls {
            parts.push(json!({ "type": "image_url", "image_url": { "url": img } }));
        }
        let mut vision_history = history.clone();
        vision_history.push(json!({ "role": "user", "content": parts }));

        let base = cfg.llm_base_url.trim().trim_end_matches('/');
        let url = format!("{}/chat/completions", base);
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(20))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| e.to_string())?;
        let mut req = client.post(&url);
        if !cfg.llm_api_key.trim().is_empty() {
            req = req.bearer_auth(cfg.llm_api_key.trim());
        }
        let mut msgs = vec![json!({ "role": "system", "content": build_persona_system_prompt(&cfg) })];
        msgs.extend(vision_history);
        let vision_res = req
            .json(&json!({
                "model": cfg.llm_model.trim(),
                "messages": msgs,
                "tools": to_all_openai_tools(state.inner()),
                "temperature": cfg.llm_temperature,
                "max_tokens": cfg.llm_max_tokens
            }))
            .send()
            .await;

        let mut handled_vision = None;
        if let Ok(resp) = vision_res {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<Value>().await {
                    if let Some(msg) = body.pointer("/choices/0/message") {
                        let tool_calls = msg.get("tool_calls").and_then(|t| t.as_array()).cloned().unwrap_or_default();
                        if tool_calls.is_empty() {
                            if let Some(c) = msg.get("content").and_then(|c| c.as_str()) {
                                handled_vision = Some(c.to_string());
                            }
                        }
                    }
                }
            }
        }
        match handled_vision {
            Some(r) => r,
            None => chat_with_mcp_tools(&app, state.inner(), &cfg, &mut history, &combined_prompt).await?,
        }
    } else {
        chat_with_mcp_tools(&app, state.inner(), &cfg, &mut history, &combined_prompt).await?
    };

    let assistant_msg = RaybotMessage {
        id: format!("rbm_{}", uuid::Uuid::new_v4().simple()),
        session_id: session_id.clone(),
        role: "assistant".to_string(),
        content: reply,
        attachments: Vec::new(),
        created_at: now_unix_secs(),
    };
    insert_raybot_message(&state.db, &assistant_msg)?;

    let session = list_raybot_sessions(&state.db)?
        .into_iter()
        .find(|s| s.id == session_id)
        .unwrap_or(RaybotSession {
            id: session_id,
            title: "RayBot Chat".to_string(),
            created_at: now,
            updated_at: assistant_msg.created_at,
            last_message: assistant_msg.content.clone(),
        });

    Ok(RaybotSendResult {
        user_message: user_msg,
        assistant_message: assistant_msg,
        session,
    })
}
