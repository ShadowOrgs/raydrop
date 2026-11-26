use crate::db::storage::Database;
use crate::mcp_server::{execute_mcp_tool, get_mcp_tool_definitions, to_openai_tools};
use crate::state::AppState;
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
            mcp_port: 8765,
            mcp_auth_token: String::new(),
            llm_provider: "openai".to_string(),
            llm_base_url: "https://api.openai.com/v1".to_string(),
            llm_api_key: String::new(),
            llm_model: "gpt-4o-mini".to_string(),
            llm_temperature: 0.7,
            llm_max_tokens: 2048,
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
    } else {
        chat_openai_compatible_loop(app, state, cfg, history, user_message).await
    }
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
    let tools = to_openai_tools();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    for _step in 0..6 {
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
                "tools": tools,
                "temperature": cfg.llm_temperature,
                "max_tokens": cfg.llm_max_tokens
            }))
            .send()
            .await
            .map_err(|e| format!("LLM request failed: {}", e))?;

        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse LLM JSON: {}", e))?;

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

            let tool_out = match execute_mcp_tool(app, state, fn_name, &args_val).await {
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
    let anthropic_tools: Vec<Value> = get_mcp_tool_definitions()
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
                    let out = match execute_mcp_tool(app, state, name, &input).await {
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
