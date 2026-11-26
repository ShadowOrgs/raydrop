use crate::ai_agent::{chat_with_mcp_tools, load_ai_mcp_config};
use crate::mcp_server::execute_mcp_tool;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::AppHandle;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramBotStatus {
    pub running: bool,
    pub bot_username: Option<String>,
    pub last_error: Option<String>,
}

pub struct TelegramBotManager {
    stop_flag: Mutex<Option<Arc<AtomicBool>>>,
    pub status: Mutex<TelegramBotStatus>,
}

impl Default for TelegramBotManager {
    fn default() -> Self {
        Self {
            stop_flag: Mutex::new(None),
            status: Mutex::new(TelegramBotStatus::default()),
        }
    }
}

pub async fn verify_telegram_token(token: &str) -> Result<String, String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err("Telegram Bot Token is empty".to_string());
    }
    let url = format!("https://api.telegram.org/bot{}/getMe", trimmed);
    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(12))
        .send()
        .await
        .map_err(|e| format!("Telegram getMe failed: {}", e))?;

    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !body["ok"].as_bool().unwrap_or(false) {
        return Err(body["description"]
            .as_str()
            .unwrap_or("Invalid Telegram Bot Token")
            .to_string());
    }
    let username = body
        .pointer("/result/username")
        .and_then(|u| u.as_str())
        .unwrap_or("unknown_bot");
    Ok(format!("@{}", username))
}

pub fn restart_telegram_bot(app: AppHandle, state: Arc<AppState>) {
    let cfg = load_ai_mcp_config(&state.db);
    if let Ok(mut guard) = state.telegram_bot.stop_flag.lock() {
        if let Some(old) = guard.take() {
            old.store(true, Ordering::Relaxed);
        }
        if !cfg.telegram_enabled || cfg.telegram_bot_token.trim().is_empty() {
            if let Ok(mut st) = state.telegram_bot.status.lock() {
                st.running = false;
                st.last_error = None;
            }
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        *guard = Some(stop.clone());
        let state_bg = state.clone();
        tokio::spawn(async move {
            run_telegram_polling_loop(app, state_bg, stop).await;
        });
    }
}

async fn send_telegram_message(client: &reqwest::Client, token: &str, chat_id: i64, text: &str) {
    let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
    // Telegram max message length is 4096 chars
    let chunks: Vec<&str> = if text.len() > 3900 {
        text.as_bytes()
            .chunks(3900)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect()
    } else {
        vec![text]
    };
    for chunk in chunks {
        if chunk.is_empty() {
            continue;
        }
        let _ = client
            .post(&url)
            .json(&json!({
                "chat_id": chat_id,
                "text": chunk
            }))
            .send()
            .await;
    }
}

async fn run_telegram_polling_loop(
    app: AppHandle,
    state: Arc<AppState>,
    stop: Arc<AtomicBool>,
) {
    let initial_cfg = load_ai_mcp_config(&state.db);
    let token = initial_cfg.telegram_bot_token.trim().to_string();
    if token.is_empty() {
        return;
    }

    match verify_telegram_token(&token).await {
        Ok(uname) => {
            info!("Telegram AI Bot started as {}", uname);
            if let Ok(mut st) = state.telegram_bot.status.lock() {
                st.running = true;
                st.bot_username = Some(uname);
                st.last_error = None;
            }
        }
        Err(e) => {
            warn!("Telegram Bot failed verification: {}", e);
            if let Ok(mut st) = state.telegram_bot.status.lock() {
                st.running = false;
                st.last_error = Some(e);
            }
            return;
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(35))
        .build()
        .unwrap_or_default();

    let mut offset: i64 = 0;
    let mut chat_histories: HashMap<i64, Vec<Value>> = HashMap::new();

    while !stop.load(Ordering::Relaxed) {
        let url = format!(
            "https://api.telegram.org/bot{}/getUpdates?timeout=25&offset={}",
            token, offset
        );
        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                if let Ok(mut st) = state.telegram_bot.status.lock() {
                    st.last_error = Some(format!("Polling error: {}", e));
                }
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                continue;
            }
        };

        let Ok(body) = resp.json::<Value>().await else {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        };

        if !body["ok"].as_bool().unwrap_or(false) {
            let desc = body["description"]
                .as_str()
                .unwrap_or("Telegram API error")
                .to_string();
            if let Ok(mut st) = state.telegram_bot.status.lock() {
                st.last_error = Some(desc);
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }

        let updates = body["result"].as_array().cloned().unwrap_or_default();
        for upd in updates {
            if let Some(uid) = upd["update_id"].as_i64() {
                offset = uid + 1;
            }

            let Some(msg) = upd.get("message") else {
                continue;
            };
            let chat_id = msg.pointer("/chat/id").and_then(|c| c.as_i64()).unwrap_or(0);
            let from_id = msg.pointer("/from/id").and_then(|f| f.as_i64()).unwrap_or(0);
            let text = msg.get("text").and_then(|t| t.as_str()).unwrap_or("").trim();

            if chat_id == 0 || text.is_empty() {
                continue;
            }

            // Reload latest config dynamically so whitelist / persona / model updates take effect immediately
            let cfg = load_ai_mcp_config(&state.db);
            let allowed = cfg
                .telegram_allowed_users
                .iter()
                .any(|u| u.user_id == from_id);

            if !allowed {
                send_telegram_message(
                    &client,
                    &token,
                    chat_id,
                    &format!(
                        "⛔ Access denied. Your Telegram User ID (`{}`) is not whitelisted in RayDrop Settings.",
                        from_id
                    ),
                )
                .await;
                continue;
            }

            // Handle quick slash commands without consuming LLM tokens
            if text == "/clear" {
                chat_histories.remove(&chat_id);
                send_telegram_message(&client, &token, chat_id, "🧹 AI conversation history cleared.")
                    .await;
                continue;
            }
            if text == "/start" || text == "/help" {
                let bot_name = if cfg.persona_name.trim().is_empty() {
                    "RayBot"
                } else {
                    cfg.persona_name.trim()
                };
                send_telegram_message(
                    &client,
                    &token,
                    chat_id,
                    &format!(
                        "👋 Hi! I'm {} connected to RayDrop MCP.\n\nQuick commands:\n/status — Host telemetry & RayAmp status\n/pause — Pause/Resume RayAmp\n/next — Next track\n/clear — Reset AI memory\n\nOr just chat naturally (e.g. \"Play lofi music\", \"Wake my office PC\", \"List my notes\").",
                        bot_name
                    ),
                )
                .await;
                continue;
            }
            if text == "/pause" {
                let _ = execute_mcp_tool(
                    &app,
                    &state,
                    "media_control",
                    &json!({ "action": "toggle" }),
                )
                .await;
                send_telegram_message(&client, &token, chat_id, "⏯️ Toggled RayAmp play/pause.").await;
                continue;
            }
            if text == "/next" {
                let _ = execute_mcp_tool(
                    &app,
                    &state,
                    "media_control",
                    &json!({ "action": "next" }),
                )
                .await;
                send_telegram_message(&client, &token, chat_id, "⏭️ Skipped to next track.").await;
                continue;
            }

            let history = chat_histories.entry(chat_id).or_default();
            match chat_with_mcp_tools(&app, &state, &cfg, history, text).await {
                Ok(reply) => {
                    send_telegram_message(&client, &token, chat_id, &reply).await;
                }
                Err(err) => {
                    send_telegram_message(
                        &client,
                        &token,
                        chat_id,
                        &format!("⚠️ Error: {}", err),
                    )
                    .await;
                }
            }
        }
    }

    if let Ok(mut st) = state.telegram_bot.status.lock() {
        st.running = false;
    }
}
