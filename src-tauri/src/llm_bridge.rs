use crate::db::storage::Database;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tracing::{info, warn};

const SETTINGS_KEY: &str = "llm_bridge_providers";

fn default_execution_mode() -> String {
    "yolo".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBridgeProvider {
    pub id: String,
    pub name: String,
    pub cli_command: String,
    pub enabled: bool,
    pub port: u16,
    pub api_key: String,
    pub default_model: String,
    pub available_models: Vec<String>,
    pub supports_image_gen: bool,
    #[serde(default)]
    pub is_public: bool,
    #[serde(default = "default_execution_mode")]
    pub execution_mode: String,
    #[serde(default)]
    pub extra_args: String,
    pub cli_installed: bool,
    pub running: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmBridgeLogEvent {
    pub provider_id: String,
    pub provider_name: String,
    pub method: String,
    pub path: String,
    pub model: String,
    pub status: u16,
    pub latency_ms: u128,
    pub message: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmPeerProxyInfo {
    pub id: String,
    pub peer_device_id: String,
    pub peer_alias: String,
    pub provider_id: String,
    pub local_port: u16,
    pub remote_api_key: String,
    pub running: bool,
}

pub struct LlmBridgeManager {
    providers: Mutex<Vec<LlmBridgeProvider>>,
    servers: Mutex<HashMap<String, oneshot::Sender<()>>>,
    peer_proxies: Mutex<Vec<LlmPeerProxyInfo>>,
    peer_proxy_stops: Mutex<HashMap<String, oneshot::Sender<()>>>,
    pending_responses: Mutex<HashMap<String, oneshot::Sender<(u16, Value)>>>,
}

impl Default for LlmBridgeManager {
    fn default() -> Self {
        Self {
            providers: Mutex::new(default_providers()),
            servers: Mutex::new(HashMap::new()),
            peer_proxies: Mutex::new(Vec::new()),
            peer_proxy_stops: Mutex::new(HashMap::new()),
            pending_responses: Mutex::new(HashMap::new()),
        }
    }
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn generate_api_key(prefix: &str) -> String {
    let r1: u64 = rand::random();
    let r2: u64 = rand::random();
    format!("sk-{}-{:08x}{:08x}", prefix, r1 as u32, r2 as u32)
}

fn default_providers() -> Vec<LlmBridgeProvider> {
    vec![
        LlmBridgeProvider {
            id: "agy".to_string(),
            name: "Antigravity (AGY)".to_string(),
            cli_command: "agy".to_string(),
            enabled: false,
            port: 48811,
            api_key: generate_api_key("agy"),
            default_model: "gemini-3.8-flash-medium".to_string(),
            available_models: vec![
                "gemini-4-hydrogen-medium".to_string(),
                "gemini-4-hydrogen-high".to_string(),
                "gemini-4-hydrogen-low".to_string(),
                "gemini-4-hydrogen-xhigh".to_string(),
                "gemini-3.8-flash-medium".to_string(),
                "gemini-3.8-flash-high".to_string(),
                "gemini-3.8-flash-low".to_string(),
                "claude-sonnet-4-6".to_string(),
                "claude-opus-4-6-thinking".to_string(),
                "gpt-oss-120b-medium".to_string(),
                "agy-default".to_string(),
            ],
            supports_image_gen: true,
            is_public: false,
            execution_mode: default_execution_mode(),
            extra_args: String::new(),
            cli_installed: false,
            running: false,
            last_error: None,
        },
        LlmBridgeProvider {
            id: "claude".to_string(),
            name: "Claude Code CLI".to_string(),
            cli_command: "claude".to_string(),
            enabled: false,
            port: 48812,
            api_key: generate_api_key("claude"),
            default_model: "claude-sonnet-4-5".to_string(),
            available_models: vec![
                "claude-sonnet-4-5".to_string(),
                "claude-opus-4-1".to_string(),
                "claude-haiku-3-5".to_string(),
                "sonnet".to_string(),
                "opus".to_string(),
            ],
            supports_image_gen: false,
            is_public: false,
            execution_mode: default_execution_mode(),
            extra_args: String::new(),
            cli_installed: false,
            running: false,
            last_error: None,
        },
        LlmBridgeProvider {
            id: "codex".to_string(),
            name: "OpenAI Codex CLI".to_string(),
            cli_command: "codex".to_string(),
            enabled: false,
            port: 48813,
            api_key: generate_api_key("codex"),
            default_model: "gpt-5-codex".to_string(),
            available_models: vec![
                "gpt-5-codex".to_string(),
                "gpt-5".to_string(),
                "o4-mini".to_string(),
                "o3".to_string(),
            ],
            supports_image_gen: false,
            is_public: false,
            execution_mode: default_execution_mode(),
            extra_args: String::new(),
            cli_installed: false,
            running: false,
            last_error: None,
        },
        LlmBridgeProvider {
            id: "gemini".to_string(),
            name: "Gemini CLI".to_string(),
            cli_command: "gemini".to_string(),
            enabled: false,
            port: 48814,
            api_key: generate_api_key("gemini"),
            default_model: "gemini-2.5-pro".to_string(),
            available_models: vec![
                "gemini-2.5-pro".to_string(),
                "gemini-2.5-flash".to_string(),
                "gemini-2.0-flash".to_string(),
            ],
            supports_image_gen: false,
            is_public: false,
            execution_mode: default_execution_mode(),
            extra_args: String::new(),
            cli_installed: false,
            running: false,
            last_error: None,
        },
        LlmBridgeProvider {
            id: "opencode".to_string(),
            name: "OpenCode CLI".to_string(),
            cli_command: "opencode".to_string(),
            enabled: false,
            port: 48815,
            api_key: generate_api_key("opencode"),
            default_model: "default".to_string(),
            available_models: vec!["default".to_string()],
            supports_image_gen: false,
            is_public: false,
            execution_mode: default_execution_mode(),
            extra_args: String::new(),
            cli_installed: false,
            running: false,
            last_error: None,
        },
    ]
}

#[cfg(target_os = "windows")]
fn resolve_windows_cli_path(program: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("where")
        .arg(program.trim())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let candidates: Vec<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    // Prefer native .exe over .cmd/.bat so multi-line prompts & JSON never pass through cmd.exe
    candidates
        .iter()
        .find(|p| p.to_lowercase().ends_with(".exe"))
        .cloned()
        .or_else(|| candidates.into_iter().next())
}

fn build_hidden_command(program: &str, args: &[&str]) -> std::process::Command {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let resolved = resolve_windows_cli_path(program).unwrap_or_else(|| program.to_string());
        let lower = resolved.to_lowercase();
        if lower.ends_with(".cmd") || lower.ends_with(".bat") {
            let mut cmd = std::process::Command::new("cmd");
            cmd.arg("/C").arg(&resolved);
            for a in args {
                cmd.arg(a);
            }
            cmd.stdin(std::process::Stdio::null());
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd
        } else {
            let mut cmd = std::process::Command::new(&resolved);
            for a in args {
                cmd.arg(a);
            }
            cmd.stdin(std::process::Stdio::null());
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = std::process::Command::new(program);
        for a in args {
            cmd.arg(a);
        }
        cmd.stdin(std::process::Stdio::null());
        cmd
    }
}

fn check_cli_installed(cli_command: &str) -> bool {
    let trimmed = cli_command.trim();
    if trimmed.is_empty() {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("where")
            .arg(trimmed)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("which")
            .arg(trimmed)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

async fn bind_dynamic_tcp_listener(host: &str, preferred_port: u16) -> Option<(TcpListener, u16)> {
    if preferred_port >= 1024 {
        for offset in 0..=30u16 {
            let try_port = preferred_port.saturating_add(offset);
            if try_port < 1024 {
                break;
            }
            if let Ok(l) = TcpListener::bind(format!("{}:{}", host, try_port)).await {
                return Some((l, try_port));
            }
        }
    }
    if let Ok(l) = TcpListener::bind(format!("{}:0", host)).await {
        if let Ok(local_addr) = l.local_addr() {
            return Some((l, local_addr.port()));
        }
    }
    None
}

impl LlmBridgeManager {
    pub fn init_from_db(&self, app: &AppHandle, db: &Arc<Database>) {
        let mut list = default_providers();
        if let Ok(Some(raw)) = db.get_setting(SETTINGS_KEY) {
            if let Ok(saved) = serde_json::from_str::<Vec<LlmBridgeProvider>>(&raw) {
                for s in saved {
                    if let Some(p) = list.iter_mut().find(|item| item.id == s.id) {
                        p.enabled = s.enabled;
                        p.port = s.port;
                        if !s.api_key.trim().is_empty() {
                            p.api_key = s.api_key;
                        }
                        if !s.default_model.trim().is_empty() {
                            p.default_model = s.default_model;
                        }
                        if !s.cli_command.trim().is_empty() {
                            p.cli_command = s.cli_command;
                        }
                        if !s.available_models.is_empty() {
                            p.available_models = s.available_models;
                        }
                        p.supports_image_gen = s.supports_image_gen;
                        p.is_public = s.is_public;
                        if !s.execution_mode.trim().is_empty() {
                            p.execution_mode = s.execution_mode;
                        }
                        p.extra_args = s.extra_args;
                    }
                }
            }
        }

        for p in list.iter_mut() {
            p.cli_installed = check_cli_installed(&p.cli_command);
            p.running = false;
            p.last_error = None;
        }

        {
            let mut guard = self.providers.lock().unwrap();
            *guard = list.clone();
        }
        self.persist_to_db(db);

        for p in list {
            if p.enabled {
                self.start_provider_server(app.clone(), p.id);
            }
        }
    }

    fn persist_to_db(&self, db: &Arc<Database>) {
        let snapshot = self.providers.lock().unwrap().clone();
        if let Ok(json_str) = serde_json::to_string(&snapshot) {
            let _ = db.set_setting(SETTINGS_KEY, &json_str);
        }
    }

    pub fn list_providers(&self) -> Vec<LlmBridgeProvider> {
        let mut guard = self.providers.lock().unwrap();
        let running_ids: Vec<String> = self.servers.lock().unwrap().keys().cloned().collect();
        for p in guard.iter_mut() {
            p.cli_installed = check_cli_installed(&p.cli_command);
            p.running = running_ids.contains(&p.id);
        }
        guard.clone()
    }

    pub fn stop_provider_server(&self, provider_id: &str) {
        if let Some(tx) = self.servers.lock().unwrap().remove(provider_id) {
            let _ = tx.send(());
        }
        if let Ok(mut guard) = self.providers.lock() {
            if let Some(p) = guard.iter_mut().find(|x| x.id == provider_id) {
                p.running = false;
            }
        }
    }

    pub fn start_provider_server(&self, app: AppHandle, provider_id: String) {
        self.stop_provider_server(&provider_id);

        let provider_opt = self
            .providers
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.id == provider_id)
            .cloned();

        let Some(provider) = provider_opt else {
            return;
        };

        let (tx, mut rx) = oneshot::channel::<()>();
        self.servers
            .lock()
            .unwrap()
            .insert(provider_id.clone(), tx);

        let port = provider.port;
        let state_mgr = app.clone();

        tauri::async_runtime::spawn(async move {
            let bound = bind_dynamic_tcp_listener("0.0.0.0", port).await;

            let (listener, active_port) = match bound {
                Some((l, p)) => {
                    info!("LLM Bridge [{}] listening on http://0.0.0.0:{}", provider_id, p);
                    if let Some(st) = state_mgr.try_state::<Arc<crate::state::AppState>>() {
                        if let Ok(mut guard) = st.llm_bridge.providers.lock() {
                            if let Some(item) = guard.iter_mut().find(|x| x.id == provider_id) {
                                item.port = p;
                                item.running = true;
                                item.last_error = None;
                            }
                        }
                        st.llm_bridge.persist_to_db(&st.db);
                        let _ = state_mgr.emit("llm_bridge_status_changed", st.llm_bridge.list_providers());
                    }
                    (l, p)
                }
                None => {
                    let err_msg = format!("Failed to bind dynamic port near {}", port);
                    warn!("LLM Bridge [{}]: {}", provider_id, err_msg);
                    if let Some(st) = state_mgr.try_state::<Arc<crate::state::AppState>>() {
                        st.llm_bridge.servers.lock().unwrap().remove(&provider_id);
                        if let Ok(mut guard) = st.llm_bridge.providers.lock() {
                            if let Some(p) = guard.iter_mut().find(|x| x.id == provider_id) {
                                p.running = false;
                                p.last_error = Some(err_msg);
                            }
                        }
                        let _ = state_mgr.emit("llm_bridge_status_changed", st.llm_bridge.list_providers());
                    }
                    return;
                }
            };

            loop {
                tokio::select! {
                    _ = &mut rx => {
                        info!("LLM Bridge [{}] stopped on port {}", provider_id, active_port);
                        break;
                    }
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((mut socket, _)) => {
                                let app_c = state_mgr.clone();
                                let pid = provider_id.clone();
                                tauri::async_runtime::spawn(async move {
                                    let _ = handle_openai_client(&mut socket, app_c, pid).await;
                                });
                            }
                            Err(e) => {
                                warn!("LLM Bridge [{}] accept error: {}", provider_id, e);
                                break;
                            }
                        }
                    }
                }
            }
        });
    }
}

fn emit_bridge_log(
    app: &AppHandle,
    provider_id: &str,
    provider_name: &str,
    method: &str,
    path: &str,
    model: &str,
    status: u16,
    latency_ms: u128,
    message: &str,
) {
    let _ = app.emit(
        "llm_bridge_log",
        LlmBridgeLogEvent {
            provider_id: provider_id.to_string(),
            provider_name: provider_name.to_string(),
            method: method.to_string(),
            path: path.to_string(),
            model: model.to_string(),
            status,
            latency_ms,
            message: message.to_string(),
            timestamp: now_unix_secs(),
        },
    );
}

async fn send_http_json<W: AsyncWriteExt + Unpin>(writer: &mut W, status: &str, body: &Value) {
    let s = body.to_string();
    let resp = format!(
        "HTTP/1.1 {}\r\n\
Content-Type: application/json\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
        status,
        s.len(),
        s
    );
    let _ = writer.write_all(resp.as_bytes()).await;
}

fn openai_error(message: &str, err_type: &str, code: &str) -> Value {
    json!({
        "error": {
            "message": message,
            "type": err_type,
            "code": code
        }
    })
}

async fn handle_openai_client(
    socket: &mut tokio::net::TcpStream,
    app: AppHandle,
    provider_id: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let started = std::time::Instant::now();
    let (reader_half, mut writer_half) = socket.split();
    let mut buf_reader = BufReader::new(reader_half);

    let mut request_line = String::new();
    if buf_reader.read_line(&mut request_line).await.is_err() || request_line.is_empty() {
        return Ok(());
    }
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }
    let method = parts[0].to_string();
    let path = parts[1].split('?').next().unwrap_or(parts[1]).to_string();

    let mut content_length: usize = 0;
    let mut auth_header = String::new();
    loop {
        let mut header_line = String::new();
        if buf_reader.read_line(&mut header_line).await.is_err() {
            break;
        }
        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break;
        }
        let lower = trimmed.to_lowercase();
        if let Some(val) = lower.strip_prefix("content-length:") {
            if let Ok(cl) = val.trim().parse::<usize>() {
                content_length = cl;
            }
        } else if lower.starts_with("authorization:") {
            auth_header = trimmed[14..].trim().to_string();
        }
    }

    if method == "OPTIONS" {
        let resp = "HTTP/1.1 200 OK\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
Content-Length: 0\r\n\
Connection: close\r\n\r\n";
        let _ = writer_half.write_all(resp.as_bytes()).await;
        return Ok(());
    }

    let provider = {
        let Some(st) = app.try_state::<Arc<crate::state::AppState>>() else {
            return Ok(());
        };
        let guard = st.llm_bridge.providers.lock().unwrap();
        guard.iter().find(|p| p.id == provider_id).cloned()
    };

    let Some(provider) = provider else {
        send_http_json(
            &mut writer_half,
            "404 Not Found",
            &openai_error("Provider not configured", "invalid_request_error", "provider_not_found"),
        )
        .await;
        return Ok(());
    };

    // Public health endpoint
    if (path == "/" || path == "/health") && method == "GET" {
        let key = &provider.api_key;
        let masked = if key.len() > 11 {
            format!("{}...{}", &key[..7], &key[key.len() - 4..])
        } else {
            "[REDACTED]".to_string()
        };
        send_http_json(
            &mut writer_half,
            "200 OK",
            &json!({
                "status": "online",
                "provider": provider.id,
                "name": provider.name,
                "default_model": provider.default_model,
                "supports_image_gen": provider.supports_image_gen,
                "api_key": masked,
                "openai_base": format!("http://127.0.0.1:{}/v1", provider.port)
            }),
        )
        .await;
        return Ok(());
    }

    // Strict Bearer API Key authentication for all /v1/* endpoints
    let bearer_token = auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "))
        .map(|s| s.trim())
        .unwrap_or("");

    if bearer_token.is_empty() || bearer_token != provider.api_key {
        emit_bridge_log(
            &app,
            &provider.id,
            &provider.name,
            &method,
            &path,
            "-",
            401,
            started.elapsed().as_millis(),
            "Unauthorized: Invalid or missing Bearer API key",
        );
        send_http_json(
            &mut writer_half,
            "401 Unauthorized",
            &openai_error(
                "Invalid or missing API Key in Authorization header (expected Bearer <api_key>)",
                "authentication_error",
                "invalid_api_key",
            ),
        )
        .await;
        return Ok(());
    }

    // GET /v1/models
    if path == "/v1/models" && method == "GET" {
        let mut models_list = provider.available_models.clone();
        if !models_list.contains(&provider.default_model) && !provider.default_model.is_empty() {
            models_list.insert(0, provider.default_model.clone());
        }
        let data: Vec<Value> = models_list
            .iter()
            .map(|m| {
                json!({
                    "id": m,
                    "object": "model",
                    "created": 1700000000,
                    "owned_by": provider.cli_command
                })
            })
            .collect();
        emit_bridge_log(
            &app,
            &provider.id,
            &provider.name,
            &method,
            &path,
            "-",
            200,
            started.elapsed().as_millis(),
            &format!("Listed {} models", data.len()),
        );
        send_http_json(&mut writer_half, "200 OK", &json!({ "object": "list", "data": data })).await;
        return Ok(());
    }

    // Read body for POST requests (up to 16 MB)
    if content_length > 16 * 1024 * 1024 {
        send_http_json(
            &mut writer_half,
            "413 Payload Too Large",
            &openai_error("Request body exceeds 16 MB limit", "invalid_request_error", "payload_too_large"),
        )
        .await;
        return Ok(());
    }
    let mut body_bytes = vec![0u8; content_length];
    if content_length > 0 {
        buf_reader.read_exact(&mut body_bytes).await?;
    }
    let req_json: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            send_http_json(
                &mut writer_half,
                "400 Bad Request",
                &openai_error(&format!("Invalid JSON body: {e}"), "invalid_request_error", "invalid_json"),
            )
            .await;
            return Ok(());
        }
    };

    // POST /v1/images/generations
    if path == "/v1/images/generations" && method == "POST" {
        let prompt = req_json
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if prompt.is_empty() {
            send_http_json(
                &mut writer_half,
                "400 Bad Request",
                &openai_error("Missing 'prompt' field", "invalid_request_error", "missing_prompt"),
            )
            .await;
            return Ok(());
        }

        if !provider.supports_image_gen && !is_comfyui_running().await {
            let msg = format!(
                "{} does not support image generation (and local ComfyUI at 127.0.0.1:8188 is not running). Use AGY bridge or start ComfyUI.",
                provider.name
            );
            emit_bridge_log(
                &app,
                &provider.id,
                &provider.name,
                &method,
                &path,
                &provider.default_model,
                400,
                started.elapsed().as_millis(),
                &msg,
            );
            send_http_json(
                &mut writer_half,
                "400 Bad Request",
                &openai_error(&msg, "invalid_request_error", "unsupported_image_gen"),
            )
            .await;
            return Ok(());
        }

        let model = req_json
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or(&provider.default_model)
            .to_string();
        let prov_clone = provider.clone();
        let prompt_clone = prompt.clone();
        let model_clone = model.clone();

        let img_res = if is_comfyui_running().await {
            run_comfyui_image_generation(&prompt_clone).await
        } else {
            tokio::task::spawn_blocking(move || {
                run_cli_image_generation(&prov_clone, &prompt_clone, &model_clone)
            })
            .await
            .map_err(|e| format!("Image task failed: {e}"))?
        };

        match img_res {
            Ok(b64_png) => {
                emit_bridge_log(
                    &app,
                    &provider.id,
                    &provider.name,
                    &method,
                    &path,
                    &model,
                    200,
                    started.elapsed().as_millis(),
                    "Image generated successfully",
                );
                send_http_json(
                    &mut writer_half,
                    "200 OK",
                    &json!({
                        "created": now_unix_secs(),
                        "data": [{ "b64_json": b64_png, "revised_prompt": prompt }]
                    }),
                )
                .await;
            }
            Err(err_msg) => {
                emit_bridge_log(
                    &app,
                    &provider.id,
                    &provider.name,
                    &method,
                    &path,
                    &model,
                    502,
                    started.elapsed().as_millis(),
                    &err_msg,
                );
                send_http_json(
                    &mut writer_half,
                    "502 Bad Gateway",
                    &openai_error(&err_msg, "server_error", "image_generation_failed"),
                )
                .await;
            }
        }
        return Ok(());
    }

    // POST /v1/chat/completions
    if path == "/v1/chat/completions" && method == "POST" {
        let req_model = req_json
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let target_model = if req_model.is_empty()
            || matches!(
                req_model.as_str(),
                "agy-default" | "default" | "gpt-4o" | "gpt-4o-mini"
            ) {
            provider.default_model.clone()
        } else {
            req_model.clone()
        };

        let messages = req_json
            .get("messages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if messages.is_empty() {
            send_http_json(
                &mut writer_half,
                "400 Bad Request",
                &openai_error("Missing or empty 'messages' array", "invalid_request_error", "missing_messages"),
            )
            .await;
            return Ok(());
        }

        let tools = req_json.get("tools").and_then(|v| v.as_array()).cloned();
        let tool_choice_none = req_json
            .get("tool_choice")
            .and_then(|v| v.as_str())
            .map(|s| s == "none")
            .unwrap_or(false);
        let has_tools = tools.as_ref().map(|t| !t.is_empty()).unwrap_or(false) && !tool_choice_none;
        let stream = req_json.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

        let full_prompt = build_cli_prompt(&messages, tools.as_deref(), has_tools);
        let p_tokens = (full_prompt.len() / 4).max(1);
        let prov_clone = provider.clone();
        let model_for_cli = target_model.clone();
        let prompt_for_cli = full_prompt.clone();

        if stream {
            let hdr = "HTTP/1.1 200 OK\r\n\
Content-Type: text/event-stream\r\n\
Cache-Control: no-cache\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
Connection: close\r\n\r\n: keep-alive\n\n";
            let _ = writer_half.write_all(hdr.as_bytes()).await;

            let mut handle = tokio::task::spawn_blocking(move || {
                run_cli_prompt(&prov_clone, &prompt_for_cli, &model_for_cli)
            });

            let cli_result = loop {
                tokio::select! {
                    res = &mut handle => {
                        break res.map_err(|e| format!("CLI join error: {e}")).and_then(|r| r);
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                        if writer_half.write_all(b": keep-alive\n\n").await.is_err() {
                            return Ok(());
                        }
                    }
                }
            };

            match cli_result {
                Ok(resp_text) => {
                    let tool_calls = if has_tools {
                        parse_tool_calls_from_response(&resp_text)
                    } else {
                        None
                    };
                    let comp_id = format!("chatcmpl-{:012x}", rand::random::<u64>() & 0xffff_ffff_ffff);
                    let (delta, finish_reason) = if let Some(tc) = tool_calls {
                        (json!({ "role": "assistant", "content": Value::Null, "tool_calls": tc }), "tool_calls")
                    } else {
                        (json!({ "role": "assistant", "content": resp_text }), "stop")
                    };
                    let chunk = json!({
                        "id": comp_id,
                        "object": "chat.completion.chunk",
                        "created": now_unix_secs(),
                        "model": target_model,
                        "choices": [{ "index": 0, "delta": delta, "finish_reason": finish_reason }]
                    });
                    let sse = format!("data: {}\n\ndata: [DONE]\n\n", chunk);
                    let _ = writer_half.write_all(sse.as_bytes()).await;
                    emit_bridge_log(
                        &app,
                        &provider.id,
                        &provider.name,
                        &method,
                        &path,
                        &target_model,
                        200,
                        started.elapsed().as_millis(),
                        &format!("Streamed {} ({})", finish_reason, resp_text.len()),
                    );
                }
                Err(err_msg) => {
                    let err_chunk = json!({
                        "error": { "message": err_msg, "type": "server_error", "code": "cli_error" }
                    });
                    let sse = format!("data: {}\n\ndata: [DONE]\n\n", err_chunk);
                    let _ = writer_half.write_all(sse.as_bytes()).await;
                    emit_bridge_log(
                        &app,
                        &provider.id,
                        &provider.name,
                        &method,
                        &path,
                        &target_model,
                        502,
                        started.elapsed().as_millis(),
                        &err_msg,
                    );
                }
            }
            return Ok(());
        }

        // Non-streaming
        let cli_result = tokio::task::spawn_blocking(move || {
            run_cli_prompt(&prov_clone, &prompt_for_cli, &model_for_cli)
        })
        .await
        .map_err(|e| format!("CLI join error: {e}"))?;

        match cli_result {
            Ok(resp_text) => {
                let c_tokens = (resp_text.len() / 4).max(1);
                let tool_calls = if has_tools {
                    parse_tool_calls_from_response(&resp_text)
                } else {
                    None
                };
                let comp_id = format!("chatcmpl-{:012x}", rand::random::<u64>() & 0xffff_ffff_ffff);
                let (message, finish_reason) = if let Some(tc) = tool_calls {
                    (json!({ "role": "assistant", "content": Value::Null, "tool_calls": tc }), "tool_calls")
                } else {
                    (json!({ "role": "assistant", "content": resp_text }), "stop")
                };

                emit_bridge_log(
                    &app,
                    &provider.id,
                    &provider.name,
                    &method,
                    &path,
                    &target_model,
                    200,
                    started.elapsed().as_millis(),
                    &format!("Completed ({})", finish_reason),
                );
                send_http_json(
                    &mut writer_half,
                    "200 OK",
                    &json!({
                        "id": comp_id,
                        "object": "chat.completion",
                        "created": now_unix_secs(),
                        "model": target_model,
                        "choices": [{
                            "index": 0,
                            "message": message,
                            "finish_reason": finish_reason
                        }],
                        "usage": {
                            "prompt_tokens": p_tokens,
                            "completion_tokens": c_tokens,
                            "total_tokens": p_tokens + c_tokens
                        }
                    }),
                )
                .await;
            }
            Err(err_msg) => {
                emit_bridge_log(
                    &app,
                    &provider.id,
                    &provider.name,
                    &method,
                    &path,
                    &target_model,
                    502,
                    started.elapsed().as_millis(),
                    &err_msg,
                );
                send_http_json(
                    &mut writer_half,
                    "502 Bad Gateway",
                    &openai_error(&err_msg, "server_error", "cli_execution_failed"),
                )
                .await;
            }
        }
        return Ok(());
    }

    send_http_json(
        &mut writer_half,
        "404 Not Found",
        &openai_error(&format!("Unknown endpoint: {} {}", method, path), "invalid_request_error", "not_found"),
    )
    .await;
    Ok(())
}

fn save_vision_image_to_temp(raw_url: &str) -> Option<String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return None;
    }
    let dir = std::env::temp_dir().join("raydrop_llm_images");
    let _ = std::fs::create_dir_all(&dir);

    if let Some(rest) = trimmed.strip_prefix("data:image/") {
        let mut parts = rest.splitn(2, ";base64,");
        let ext_raw = parts.next().unwrap_or("png").trim();
        let b64_data = parts.next()?.trim();
        let ext = match ext_raw {
            "jpeg" | "jpg" => "jpg",
            "webp" => "webp",
            "gif" => "gif",
            _ => "png",
        };
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64_data).ok()?;
        let path = dir.join(format!("vision_{}.{}", uuid::Uuid::new_v4().simple(), ext));
        std::fs::write(&path, &bytes).ok()?;
        return Some(path.to_string_lossy().to_string());
    }
    None
}

async fn is_comfyui_running() -> bool {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    client
        .get("http://127.0.0.1:8188/system_stats")
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

async fn run_comfyui_image_generation(prompt: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())?;

    let obj_info: Value = client
        .get("http://127.0.0.1:8188/object_info/CheckpointLoaderSimple")
        .send()
        .await
        .map_err(|e| format!("ComfyUI object_info failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("ComfyUI object_info parse failed: {e}"))?;

    let ckpt = obj_info
        .pointer("/CheckpointLoaderSimple/input/required/ckpt_name/0/0")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "No checkpoints found in ComfyUI".to_string())?
        .to_string();

    let seed: u64 = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        % 2_147_483_647) as u64;

    let workflow = json!({
        "3": {
            "class_type": "KSampler",
            "inputs": {
                "seed": seed,
                "steps": 20,
                "cfg": 7.0,
                "sampler_name": "euler_ancestral",
                "scheduler": "normal",
                "denoise": 1.0,
                "model": ["4", 0],
                "positive": ["6", 0],
                "negative": ["7", 0],
                "latent_image": ["5", 0]
            }
        },
        "4": {
            "class_type": "CheckpointLoaderSimple",
            "inputs": { "ckpt_name": ckpt }
        },
        "5": {
            "class_type": "EmptyLatentImage",
            "inputs": { "width": 768, "height": 768, "batch_size": 1 }
        },
        "6": {
            "class_type": "CLIPTextEncode",
            "inputs": { "text": prompt, "clip": ["4", 1] }
        },
        "7": {
            "class_type": "CLIPTextEncode",
            "inputs": { "text": "lowres, blurry, bad anatomy, worst quality", "clip": ["4", 1] }
        },
        "8": {
            "class_type": "VAEDecode",
            "inputs": { "samples": ["3", 0], "vae": ["4", 2] }
        },
        "9": {
            "class_type": "SaveImage",
            "inputs": { "filename_prefix": "raydrop", "images": ["8", 0] }
        }
    });

    let queue_resp: Value = client
        .post("http://127.0.0.1:8188/prompt")
        .json(&json!({ "prompt": workflow }))
        .send()
        .await
        .map_err(|e| format!("ComfyUI prompt failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("ComfyUI prompt parse failed: {e}"))?;

    let prompt_id = queue_resp
        .get("prompt_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("ComfyUI did not return prompt_id: {}", queue_resp))?
        .to_string();

    for _ in 0..90 {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let Ok(hist_resp) = client
            .get(format!("http://127.0.0.1:8188/history/{}", prompt_id))
            .send()
            .await
        else {
            continue;
        };
        let Ok(hist_json) = hist_resp.json::<Value>().await else {
            continue;
        };
        if let Some(img) = hist_json.pointer(&format!("/{}/outputs/9/images/0", prompt_id)) {
            let filename = img.get("filename").and_then(|v| v.as_str()).unwrap_or("");
            let subfolder = img.get("subfolder").and_then(|v| v.as_str()).unwrap_or("");
            let img_type = img.get("type").and_then(|v| v.as_str()).unwrap_or("output");
            if !filename.is_empty() {
                let bytes = client
                    .get("http://127.0.0.1:8188/view")
                    .query(&[("filename", filename), ("subfolder", subfolder), ("type", img_type)])
                    .send()
                    .await
                    .map_err(|e| format!("ComfyUI view failed: {e}"))?
                    .bytes()
                    .await
                    .map_err(|e| format!("ComfyUI view bytes failed: {e}"))?;
                return Ok(base64::engine::general_purpose::STANDARD.encode(&bytes));
            }
        }
    }

    Err("ComfyUI image generation timed out".to_string())
}

pub async fn generate_image_via_bridge(
    state: &Arc<crate::state::AppState>,
    prompt: &str,
) -> Result<String, String> {
    if is_comfyui_running().await {
        return run_comfyui_image_generation(prompt).await;
    }

    let prov_opt = {
        let guard = state.llm_bridge.providers.lock().map_err(|e| e.to_string())?;
        guard
            .iter()
            .find(|p| p.supports_image_gen && p.cli_installed && p.enabled)
            .cloned()
            .or_else(|| {
                guard
                    .iter()
                    .find(|p| p.supports_image_gen && p.cli_installed)
                    .cloned()
            })
    };

    let Some(prov) = prov_opt else {
        return Err("No image generator available. Start ComfyUI at 127.0.0.1:8188 or install/enable AGY CLI Bridge.".to_string());
    };

    let prompt_c = prompt.to_string();
    let model_c = prov.default_model.clone();
    tokio::task::spawn_blocking(move || run_cli_image_generation(&prov, &prompt_c, &model_c))
        .await
        .map_err(|e| format!("Image generation task failed: {e}"))?
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAiServiceInfo {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub running: bool,
    pub models: Vec<String>,
}

#[tauri::command]
pub async fn detect_local_ai_services() -> Result<Vec<LocalAiServiceInfo>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(1200))
        .build()
        .map_err(|e| e.to_string())?;

    let candidates = [
        ("ollama", "Ollama", "http://127.0.0.1:11434/v1", ""),
        ("lmstudio", "LM Studio", "http://127.0.0.1:1234/v1", "lm-studio"),
        ("koboldcpp", "KoboldCpp (Uncensored / Local)", "http://127.0.0.1:5001/v1", "kobold"),
        ("llamacpp", "llama.cpp / LocalAI", "http://127.0.0.1:8080/v1", ""),
    ];

    let mut out = Vec::new();
    for (id, name, base_url, api_key) in candidates {
        let mut running = false;
        let mut models = Vec::new();
        if let Ok(resp) = client.get(format!("{}/models", base_url)).send().await {
            if resp.status().is_success() {
                running = true;
                if let Ok(val) = resp.json::<Value>().await {
                    if let Some(arr) = val.get("data").and_then(|d| d.as_array()) {
                        for m in arr {
                            if let Some(mid) = m.get("id").and_then(|v| v.as_str()) {
                                models.push(mid.to_string());
                            }
                        }
                    }
                }
            }
        }
        out.push(LocalAiServiceInfo {
            id: id.to_string(),
            name: name.to_string(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            running,
            models,
        });
    }

    let comfy_running = is_comfyui_running().await;
    out.push(LocalAiServiceInfo {
        id: "comfyui".to_string(),
        name: "ComfyUI (Image Generation)".to_string(),
        base_url: "http://127.0.0.1:8188".to_string(),
        api_key: String::new(),
        running: comfy_running,
        models: Vec::new(),
    });

    Ok(out)
}

fn build_cli_prompt(messages: &[Value], tools: Option<&[Value]>, has_tools: bool) -> String {
    let mut prompt = String::new();
    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");
        let content_val = msg.get("content");
        let content_str = match content_val {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Array(parts)) => parts
                .iter()
                .filter_map(|p| {
                    let ptype = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if ptype == "text" {
                        p.get("text").and_then(|v| v.as_str()).map(|s| s.to_string())
                    } else if ptype == "image_url" {
                        let raw_url = p
                            .pointer("/image_url/url")
                            .and_then(|v| v.as_str())
                            .or_else(|| p.get("image_url").and_then(|v| v.as_str()))
                            .unwrap_or("");
                        if let Some(saved_path) = save_vision_image_to_temp(raw_url) {
                            Some(format!(
                                "[Attached Image File: {} — read/inspect this local image file directly to answer the user]",
                                saved_path
                            ))
                        } else {
                            Some("[Attached Image]".to_string())
                        }
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        };

        if role == "tool" {
            let tc_id = msg.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or("");
            prompt.push_str(&format!("TOOL_RESULT ({}): {}\n", tc_id, content_str));
        } else if let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) {
            prompt.push_str(&format!(
                "ASSISTANT_TOOL_CALLS: {}\n",
                serde_json::to_string(tcs).unwrap_or_default()
            ));
        } else {
            prompt.push_str(&format!("{}: {}\n", role.to_uppercase(), content_str));
        }
    }

    if has_tools {
        if let Some(tools_slice) = tools {
            let tools_desc = serde_json::to_string_pretty(tools_slice).unwrap_or_default();
            prompt.push_str(
                "\n━━━━━━━━━━━━━━━━━━━━━━\n\
⚠️ TOOL CALLING MODE ACTIVE\n\
━━━━━━━━━━━━━━━━━━━━━━\n\
You have access to the following function tools:\n",
            );
            prompt.push_str(&tools_desc);
            prompt.push_str(
                "\n\nRULES:\n\
1. If calling a tool is needed, output ONLY a valid JSON object (no markdown prose) using:\n\
{\"tool_calls\": [{\"type\": \"function\", \"function\": {\"name\": \"<tool_name>\", \"arguments\": \"{\\\"arg\\\": \\\"value\\\"}\"}}]}\n\
OR direct format:\n\
{\"name\": \"<tool_name>\", \"arguments\": {\"arg\": \"value\"}}\n\
2. If no tool is needed, answer the user normally.\n",
            );
        }
    }

    prompt
}

fn strip_code_fences(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    let mut lines: Vec<&str> = trimmed.lines().collect();
    if lines.len() >= 2 {
        if lines[0].starts_with("```") {
            lines.remove(0);
        }
        if lines.last().map(|l| l.starts_with("```")).unwrap_or(false) {
            lines.pop();
        }
        return lines.join("\n").trim().to_string();
    }
    trimmed.to_string()
}

fn parse_tool_calls_from_response(response_text: &str) -> Option<Vec<Value>> {
    let cleaned = strip_code_fences(response_text);
    if !(cleaned.starts_with('{') && cleaned.ends_with('}')) {
        return None;
    }
    let data: Value = serde_json::from_str(&cleaned).ok()?;
    let obj = data.as_object()?;

    if let Some(arr) = obj.get("tool_calls").and_then(|v| v.as_array()) {
        let mut out = Vec::new();
        for tc in arr {
            let tc_id = tc
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("call_{:08x}", rand::random::<u32>()));
            let fn_obj = tc.get("function");
            let fn_name = fn_obj
                .and_then(|f| f.get("name"))
                .or_else(|| tc.get("name"))
                .and_then(|v| v.as_str());
            let args_val = fn_obj
                .and_then(|f| f.get("arguments"))
                .or_else(|| tc.get("arguments"));
            let fn_args = match args_val {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => "{}".to_string(),
            };
            if let Some(name) = fn_name {
                out.push(json!({
                    "id": tc_id,
                    "type": "function",
                    "function": { "name": name, "arguments": fn_args }
                }));
            }
        }
        return if out.is_empty() { None } else { Some(out) };
    }

    if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
        let args_str = if let Some(a) = obj.get("arguments") {
            match a {
                Value::String(s) => s.clone(),
                v => v.to_string(),
            }
        } else {
            let mut rest = obj.clone();
            rest.remove("name");
            Value::Object(rest).to_string()
        };
        return Some(vec![json!({
            "id": format!("call_{:08x}", rand::random::<u32>()),
            "type": "function",
            "function": { "name": name, "arguments": args_str }
        })]);
    }

    None
}

fn split_extra_args(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in raw.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn run_cli_prompt(
    provider: &LlmBridgeProvider,
    prompt: &str,
    model: &str,
) -> Result<String, String> {
    if !check_cli_installed(&provider.cli_command) {
        return Err(format!(
            "CLI '{}' not found in PATH. Please install it or update the CLI command.",
            provider.cli_command
        ));
    }

    let mode = provider.execution_mode.trim().to_lowercase();
    let mode = match mode.as_str() {
        "auto" | "safe" | "yolo" => mode.as_str(),
        _ => "yolo",
    };

    let effective_prompt = if mode == "safe" {
        format!(
            "[SAFE / READ-ONLY MODE: Do not modify files or execute mutating shell commands. Answer, analyze, or provide code/diff suggestions in text only.]\n\n{}",
            prompt
        )
    } else {
        prompt.to_string()
    };

    let mut args: Vec<String> = Vec::new();
    match provider.id.as_str() {
        "agy" => {
            args.push("-p".to_string());
            args.push(effective_prompt.clone());
            args.push("--disable-slash-commands".to_string());
            if mode == "yolo" {
                args.push("--dangerously-skip-permissions".to_string());
            }
            if !model.is_empty() {
                args.push("--model".to_string());
                args.push(model.to_string());
            }
        }
        "claude" => {
            args.push("-p".to_string());
            args.push(effective_prompt.clone());
            args.push("--output-format".to_string());
            args.push("text".to_string());
            match mode {
                "yolo" => args.push("--dangerously-skip-permissions".to_string()),
                "auto" => {
                    args.push("--permission-mode".to_string());
                    args.push("acceptEdits".to_string());
                }
                "safe" => {
                    args.push("--permission-mode".to_string());
                    args.push("plan".to_string());
                }
                _ => {}
            }
            if !model.is_empty() {
                args.push("--model".to_string());
                args.push(model.to_string());
            }
        }
        "codex" => {
            args.push("exec".to_string());
            match mode {
                "yolo" => args.push("--dangerously-bypass-approvals-and-sandbox".to_string()),
                "auto" => args.push("--full-auto".to_string()),
                "safe" => {
                    args.push("--sandbox".to_string());
                    args.push("read-only".to_string());
                }
                _ => {}
            }
            if !model.is_empty() {
                args.push("--model".to_string());
                args.push(model.to_string());
            }
            args.push(effective_prompt.clone());
        }
        "gemini" => {
            args.push("-p".to_string());
            args.push(effective_prompt.clone());
            match mode {
                "yolo" => args.push("--yolo".to_string()),
                "auto" => {
                    args.push("--approval-mode".to_string());
                    args.push("auto_edit".to_string());
                }
                _ => {}
            }
            if !model.is_empty() {
                args.push("-m".to_string());
                args.push(model.to_string());
            }
        }
        _ => {
            args.push("-p".to_string());
            args.push(effective_prompt.clone());
            if !model.is_empty() && model != "default" {
                args.push("--model".to_string());
                args.push(model.to_string());
            }
        }
    }

    for extra in split_extra_args(&provider.extra_args) {
        args.push(extra);
    }

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = build_hidden_command(&provider.cli_command, &arg_refs)
        .output()
        .map_err(|e| format!("Failed to spawn '{}': {}", provider.cli_command, e))?;

    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let cleaned = strip_code_fences(&stdout);

    if !out.status.success() && cleaned.is_empty() {
        return Err(if stderr.is_empty() {
            format!("CLI exited with status {}", out.status)
        } else {
            stderr
        });
    }
    if cleaned.is_empty() {
        return Err(if stderr.is_empty() {
            "CLI returned empty output".to_string()
        } else {
            stderr
        });
    }
    Ok(cleaned)
}

fn run_cli_image_generation(
    provider: &LlmBridgeProvider,
    prompt: &str,
    model: &str,
) -> Result<String, String> {
    if provider.execution_mode.trim().eq_ignore_ascii_case("safe") {
        return Err(
            "Image generation requires YOLO or Auto-Edit execution mode (provider is currently in Safe/Read-Only mode)."
                .to_string(),
        );
    }
    let tmp_dir: PathBuf = std::env::temp_dir().join(format!(
        "raydrop_img_{}_{}",
        provider.id,
        rand::random::<u32>()
    ));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let out_file = tmp_dir.join("generated.png");

    let gen_prompt = format!(
        "Generate an image for this prompt and save the final PNG image directly to '{}':\n{}",
        out_file.to_string_lossy(),
        prompt
    );

    let res = run_cli_prompt(provider, &gen_prompt, model);

    // Check if generated.png or any image file was written in tmp_dir
    let mut found_bytes: Option<Vec<u8>> = None;
    if out_file.exists() {
        found_bytes = std::fs::read(&out_file).ok();
    } else if let Ok(entries) = std::fs::read_dir(&tmp_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp") {
                    found_bytes = std::fs::read(&p).ok();
                    break;
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&tmp_dir);

    if let Some(bytes) = found_bytes {
        return Ok(base64::engine::general_purpose::STANDARD.encode(bytes));
    }

    match res {
        Ok(text) => {
            if let Some(idx) = text.find("data:image/") {
                if let Some(b64_start) = text[idx..].find("base64,") {
                    let b64_part = &text[idx + b64_start + 7..];
                    let clean: String = b64_part
                        .chars()
                        .take_while(|c| !c.is_whitespace() && *c != '"' && *c != ')')
                        .collect();
                    if !clean.is_empty() {
                        return Ok(clean);
                    }
                }
            }
            Err(format!(
                "No image file was produced by CLI. Output: {}",
                text.chars().take(240).collect::<String>()
            ))
        }
        Err(e) => Err(e),
    }
}

fn discover_cli_models(provider: &LlmBridgeProvider) -> Vec<String> {
    let mut models = provider.available_models.clone();
    if !check_cli_installed(&provider.cli_command) {
        return models;
    }

    let probe_args: &[&str] = match provider.id.as_str() {
        "agy" => &["models"],
        _ => &["--help"],
    };

    if let Ok(out) = build_hidden_command(&provider.cli_command, probe_args).output() {
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        for token in text.split(|c: char| c.is_whitespace() || c == ',' || c == '"' || c == '\'' || c == '`') {
            let t = token.trim_matches(|c: char| c == '.' || c == ':' || c == ';' || c == '(' || c == ')');
            let looks_like_model = (t.starts_with("gemini-")
                || t.starts_with("claude-")
                || t.starts_with("gpt-")
                || t.starts_with("o3")
                || t.starts_with("o4-"))
                && t.len() >= 4
                && t.len() <= 48
                && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
            if looks_like_model && !models.iter().any(|m| m == t) {
                models.push(t.to_string());
            }
        }
    }
    models
}

// ============================================================================
// Tauri Commands for LLM Bridge Page
// ============================================================================

#[tauri::command]
pub async fn get_llm_bridge_status(
    state: State<'_, Arc<crate::state::AppState>>,
) -> Result<Vec<LlmBridgeProvider>, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || st.llm_bridge.list_providers())
        .await
        .map_err(|e| format!("Task join error: {e}"))
}

#[derive(Debug, Deserialize)]
pub struct UpdateLlmBridgeProviderInput {
    pub id: String,
    pub enabled: bool,
    pub port: u16,
    pub api_key: String,
    pub default_model: String,
    pub cli_command: String,
    #[serde(default)]
    pub is_public: bool,
    #[serde(default)]
    pub execution_mode: Option<String>,
    #[serde(default)]
    pub extra_args: Option<String>,
}

#[tauri::command]
pub async fn update_llm_bridge_provider(
    app: AppHandle,
    state: State<'_, Arc<crate::state::AppState>>,
    input: UpdateLlmBridgeProviderInput,
) -> Result<Vec<LlmBridgeProvider>, String> {
    if input.port != 0 && input.port < 1024 {
        return Err("Port must be 0 (Auto) or >= 1024".to_string());
    }
    if input.api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let should_start = input.enabled;
    let pid = input.id.clone();

    {
        let mut guard = state.llm_bridge.providers.lock().unwrap();
        let target = guard
            .iter_mut()
            .find(|p| p.id == input.id)
            .ok_or_else(|| format!("Unknown provider '{}'", input.id))?;
        target.enabled = input.enabled;
        target.port = input.port;
        target.api_key = input.api_key.trim().to_string();
        target.default_model = input.default_model.trim().to_string();
        target.cli_command = input.cli_command.trim().to_string();
        target.is_public = input.is_public;
        if let Some(mode) = input.execution_mode {
            let norm = mode.trim().to_lowercase();
            target.execution_mode = match norm.as_str() {
                "yolo" | "auto" | "safe" => norm,
                _ => "yolo".to_string(),
            };
        }
        if let Some(extra) = input.extra_args {
            target.extra_args = extra.trim().to_string();
        }
        target.last_error = None;
        if !target.default_model.is_empty()
            && !target.available_models.contains(&target.default_model)
        {
            target.available_models.insert(0, target.default_model.clone());
        }
    }

    state.llm_bridge.persist_to_db(&state.db);
    if should_start {
        state.llm_bridge.start_provider_server(app, pid);
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    } else {
        state.llm_bridge.stop_provider_server(&pid);
    }

    Ok(state.llm_bridge.list_providers())
}

#[tauri::command]
pub async fn refresh_llm_bridge_models(
    state: State<'_, Arc<crate::state::AppState>>,
    provider_id: String,
) -> Result<Vec<LlmBridgeProvider>, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let prov = {
            let guard = st.llm_bridge.providers.lock().unwrap();
            guard
                .iter()
                .find(|p| p.id == provider_id)
                .cloned()
                .ok_or_else(|| format!("Unknown provider '{}'", provider_id))?
        };
        let discovered = discover_cli_models(&prov);
        {
            let mut guard = st.llm_bridge.providers.lock().unwrap();
            if let Some(p) = guard.iter_mut().find(|x| x.id == provider_id) {
                p.available_models = discovered;
                p.cli_installed = check_cli_installed(&p.cli_command);
            }
        }
        st.llm_bridge.persist_to_db(&st.db);
        Ok(st.llm_bridge.list_providers())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn test_llm_bridge_provider(
    state: State<'_, Arc<crate::state::AppState>>,
    provider_id: String,
) -> Result<Value, String> {
    let st = state.inner().clone();
    let prov = {
        let guard = st.llm_bridge.providers.lock().unwrap();
        guard
            .iter()
            .find(|p| p.id == provider_id)
            .cloned()
            .ok_or_else(|| format!("Unknown provider '{}'", provider_id))?
    };

    let started = std::time::Instant::now();
    let model = prov.default_model.clone();
    let res = tokio::task::spawn_blocking(move || {
        run_cli_prompt(&prov, "Reply with exactly: OK", &model)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?;

    match res {
        Ok(reply) => Ok(json!({
            "ok": true,
            "latency_ms": started.elapsed().as_millis(),
            "reply": reply
        })),
        Err(err) => Ok(json!({
            "ok": false,
            "latency_ms": started.elapsed().as_millis(),
            "error": err
        })),
    }
}

// ============================================================================
// Tailscale Detection + Raydrop WAN/LAN Peer Bridge Relay (Without Tailscale)
// ============================================================================

pub fn detect_tailscale_ip() -> Option<String> {
    let ifas = local_ip_address::list_afinet_netifas().ok()?;
    for (_name, ip) in ifas {
        if let std::net::IpAddr::V4(v4) = ip {
            let oct = v4.octets();
            // Tailscale CGNAT range: 100.64.0.0/10
            if oct[0] == 100 && (64..=127).contains(&oct[1]) {
                return Some(v4.to_string());
            }
        }
    }
    None
}

#[tauri::command]
pub async fn get_llm_bridge_network_info(
    state: State<'_, Arc<crate::state::AppState>>,
) -> Result<Value, String> {
    let local_ip = state.local_ip.clone();
    let device_id = state.identity.device_id.clone();
    let sig_url = state
        .settings
        .lock()
        .map(|s| s.signaling_url.clone())
        .unwrap_or_default();
    let wan_http_base = {
        let trimmed = sig_url.trim().trim_end_matches('/');
        let no_ws = trimmed.strip_suffix("/ws").unwrap_or(trimmed);
        if let Some(rest) = no_ws.strip_prefix("wss://") {
            format!("https://{}", rest)
        } else if let Some(rest) = no_ws.strip_prefix("ws://") {
            format!("http://{}", rest)
        } else if no_ws.starts_with("http://") || no_ws.starts_with("https://") {
            no_ws.to_string()
        } else {
            "https://stun.raydrop.web.id".to_string()
        }
    };
    let tailscale_ip = tokio::task::spawn_blocking(detect_tailscale_ip)
        .await
        .ok()
        .flatten();
    Ok(json!({
        "local_ip": local_ip,
        "tailscale_ip": tailscale_ip,
        "device_id": device_id,
        "wan_http_base": wan_http_base
    }))
}

async fn send_llm_relay_packet(
    state: &Arc<crate::state::AppState>,
    target_device_id: &str,
    session_id: &str,
    payload: &Value,
) -> Result<(), String> {
    let hex_payload = hex::encode(payload.to_string().as_bytes());
    let sent_wan = if let Some(sig_tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let packet = json!({
            "type": "RELAY_CONTROL",
            "session_id": session_id,
            "from_device_id": state.identity.device_id,
            "target_device_id": target_device_id,
            "payload_json": hex_payload
        })
        .to_string();
        sig_tx.send(packet).is_ok()
    } else {
        false
    };

    let mut sent_lan = false;
    if let Ok(Some(contact)) = state.db.get_contact(target_device_id) {
        if let (Some(ip), Some(port)) = (contact.last_known_ip, contact.last_known_port) {
            let addr = format!("{}:{}", ip, port);
            if let Ok(Ok(mut stream)) = tokio::time::timeout(
                std::time::Duration::from_millis(1200),
                tokio::net::TcpStream::connect(&addr),
            )
            .await
            {
                let packet_val = json!({
                    "type": "MEETING_SIGNAL",
                    "from_device_id": state.identity.device_id,
                    "session_id": session_id,
                    "payload": payload
                });
                if let Ok(data) = serde_json::to_vec(&packet_val) {
                    let len_prefix = (data.len() as u32).to_be_bytes();
                    if stream.write_all(&len_prefix).await.is_ok()
                        && stream.write_all(&data).await.is_ok()
                    {
                        sent_lan = true;
                    }
                }
            }
        }
    }

    if sent_wan || sent_lan {
        Ok(())
    } else {
        Err("Target peer unreachable on both Raydrop WAN signaling and LAN".to_string())
    }
}

pub fn handle_llm_bridge_relay_signal(
    app: &AppHandle,
    state: &Arc<crate::state::AppState>,
    from_device_id: &str,
    session_id: &str,
    payload: &Value,
) -> bool {
    if !session_id.starts_with("llm_bridge:") {
        return false;
    }

    let msg_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if msg_type == "llm_resp" {
        if let Some(req_id) = payload.get("req_id").and_then(|v| v.as_str()) {
            let status = payload.get("status").and_then(|v| v.as_u64()).unwrap_or(502) as u16;
            let body = payload.get("body").cloned().unwrap_or_else(|| json!({}));
            if let Some(tx) = state.llm_bridge.pending_responses.lock().unwrap().remove(req_id) {
                let _ = tx.send((status, body));
            }
        }
        return true;
    }

    if msg_type == "llm_discover" {
        let req_id = payload
            .get("req_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let st = state.clone();
        let from_id = from_device_id.to_string();
        let sid = session_id.to_string();

        tauri::async_runtime::spawn(async move {
            let contact_opt = st.db.get_contact(&from_id).ok().flatten();
            let is_own = contact_opt.as_ref().map(|c| c.is_own_device).unwrap_or(false);
            let is_trusted_private = contact_opt
                .as_ref()
                .map(|c| c.is_own_device || c.trust_level == "trusted_auto_accept")
                .unwrap_or(false);
            let is_any_contact = contact_opt.is_some();

            let list: Vec<Value> = if !is_any_contact {
                Vec::new()
            } else {
                st.llm_bridge
                    .list_providers()
                    .into_iter()
                    .filter(|p| p.enabled && (is_trusted_private || p.is_public))
                    .map(|p| {
                        json!({
                            "id": p.id,
                            "name": p.name,
                            "port": p.port,
                            "default_model": p.default_model,
                            "available_models": p.available_models,
                            "supports_image_gen": p.supports_image_gen,
                            "is_public": p.is_public,
                            "cli_installed": p.cli_installed,
                            "api_key": if is_own { Value::String(p.api_key) } else { Value::Null }
                        })
                    })
                    .collect()
            };

            let ack = json!({
                "type": "llm_resp",
                "req_id": req_id,
                "status": 200,
                "body": { "providers": list }
            });
            let _ = send_llm_relay_packet(&st, &from_id, &sid, &ack).await;
        });
        return true;
    }

    if msg_type == "llm_http_req" {
        let req_id = payload
            .get("req_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let provider_id = payload
            .get("provider_id")
            .and_then(|v| v.as_str())
            .unwrap_or("agy")
            .to_string();
        let method = payload
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("POST")
            .to_string();
        let path = payload
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("/v1/chat/completions")
            .to_string();
        let bearer_token = payload
            .get("bearer_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let req_body = payload.get("body").cloned().unwrap_or(Value::Null);

        let st = state.clone();
        let app_c = app.clone();
        let sid = session_id.to_string();

        tauri::async_runtime::spawn(async move {
            let prov_opt = st
                .llm_bridge
                .providers
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.id == provider_id)
                .cloned();

            let (status, resp_body) = match prov_opt {
                None => (
                    404u16,
                    openai_error("LLM provider not found on host", "invalid_request_error", "not_found"),
                ),
                Some(p) if !p.enabled => (
                    503,
                    openai_error(
                        &format!("LLM Bridge '{}' is currently OFF on host", p.name),
                        "server_error",
                        "bridge_disabled",
                    ),
                ),
                Some(p) if !p.api_key.trim().is_empty() && bearer_token.trim() != p.api_key.trim() => (
                    401,
                    openai_error("Invalid API key for WAN Relay", "authentication_error", "invalid_api_key"),
                ),
                Some(p) => {
                    let url = format!("http://127.0.0.1:{}{}", p.port, path);
                    let client = reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(180))
                        .build()
                        .unwrap_or_else(|_| reqwest::Client::new());
                    let builder = if method == "GET" {
                        client.get(&url)
                    } else {
                        let mut clean_body = req_body.clone();
                        if let Some(obj) = clean_body.as_object_mut() {
                            obj.insert("stream".to_string(), json!(false));
                        }
                        client.post(&url).json(&clean_body)
                    };
                    match builder
                        .header("Authorization", format!("Bearer {}", p.api_key))
                        .send()
                        .await
                    {
                        Ok(r) => {
                            let code = r.status().as_u16();
                            let val = r.json::<Value>().await.unwrap_or_else(|e| {
                                openai_error(&format!("Invalid host response: {e}"), "server_error", "bad_gateway")
                            });
                            (code, val)
                        }
                        Err(e) => (
                            502,
                            openai_error(
                                &format!("Host bridge forward error: {e}"),
                                "server_error",
                                "bad_gateway",
                            ),
                        ),
                    }
                }
            };

            emit_bridge_log(
                &app_c,
                &provider_id,
                "WAN HTTP Relay",
                &method,
                &path,
                "-",
                status,
                0,
                "External WAN HTTP request handled",
            );

            let ack = json!({
                "type": "llm_resp",
                "req_id": req_id,
                "status": status,
                "body": resp_body
            });
            let _ = send_llm_relay_packet(&st, "__signal_http__", &sid, &ack).await;
        });
        return true;
    }

    if msg_type == "llm_req" {
        let req_id = payload
            .get("req_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let provider_id = payload
            .get("provider_id")
            .and_then(|v| v.as_str())
            .unwrap_or("agy")
            .to_string();
        let method = payload
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("POST")
            .to_string();
        let path = payload
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("/v1/chat/completions")
            .to_string();
        let auth_header = payload
            .get("auth_header")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let req_body = payload.get("body").cloned().unwrap_or(Value::Null);

        let st = state.clone();
        let app_c = app.clone();
        let from_id = from_device_id.to_string();
        let sid = session_id.to_string();

        tauri::async_runtime::spawn(async move {
            let prov_opt = st
                .llm_bridge
                .providers
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.id == provider_id)
                .cloned();

            let contact_opt = st.db.get_contact(&from_id).ok().flatten();
            let is_trusted_private = contact_opt
                .as_ref()
                .map(|c| c.is_own_device || c.trust_level == "trusted_auto_accept")
                .unwrap_or(false);
            let is_any_contact = contact_opt.is_some();

            let (status, resp_body) = match prov_opt {
                None => (
                    404u16,
                    openai_error("LLM provider not found on host", "invalid_request_error", "not_found"),
                ),
                Some(p) if !p.enabled => (
                    503,
                    openai_error(
                        &format!("LLM Bridge '{}' is currently OFF on host", p.name),
                        "server_error",
                        "bridge_disabled",
                    ),
                ),
                Some(p) if !(is_trusted_private || (p.is_public && is_any_contact)) => (
                    403,
                    openai_error(
                        "Peer not authorized: bridge is Private (requires same user or trusted_auto_accept) or sender not in contacts",
                        "permission_error",
                        "peer_not_authorized",
                    ),
                ),
                Some(p) => {
                    let url = format!("http://127.0.0.1:{}{}", p.port, path);
                    let client = reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(180))
                        .build()
                        .unwrap_or_else(|_| reqwest::Client::new());
                    let auth_val = if auth_header.is_empty() {
                        format!("Bearer {}", p.api_key)
                    } else {
                        auth_header.clone()
                    };
                    let builder = if method == "GET" {
                        client.get(&url)
                    } else {
                        let mut clean_body = req_body.clone();
                        if let Some(obj) = clean_body.as_object_mut() {
                            obj.insert("stream".to_string(), json!(false));
                        }
                        client.post(&url).json(&clean_body)
                    };
                    match builder.header("Authorization", auth_val).send().await {
                        Ok(r) => {
                            let code = r.status().as_u16();
                            let val = r.json::<Value>().await.unwrap_or_else(|e| {
                                openai_error(&format!("Invalid host response: {e}"), "server_error", "bad_gateway")
                            });
                            (code, val)
                        }
                        Err(e) => (
                            502,
                            openai_error(
                                &format!("Host bridge forward error: {e}"),
                                "server_error",
                                "bad_gateway",
                            ),
                        ),
                    }
                }
            };

            emit_bridge_log(
                &app_c,
                &provider_id,
                &format!("WAN Relay ({})", from_id),
                &method,
                &path,
                "-",
                status,
                0,
                "Peer relay request handled",
            );

            let ack = json!({
                "type": "llm_resp",
                "req_id": req_id,
                "status": status,
                "body": resp_body
            });
            let _ = send_llm_relay_packet(&st, &from_id, &sid, &ack).await;
        });

        return true;
    }

    false
}

#[tauri::command]
pub async fn discover_peer_llm_bridges(
    state: State<'_, Arc<crate::state::AppState>>,
    peer_device_id: String,
) -> Result<Value, String> {
    let req_id = format!("disc_{:012x}", rand::random::<u64>() & 0xffff_ffff_ffff);
    let (tx, rx) = oneshot::channel::<(u16, Value)>();
    state
        .llm_bridge
        .pending_responses
        .lock()
        .unwrap()
        .insert(req_id.clone(), tx);

    let payload = json!({
        "type": "llm_discover",
        "req_id": req_id
    });
    let sid = format!("llm_bridge:{}", req_id);
    if let Err(e) = send_llm_relay_packet(state.inner(), &peer_device_id, &sid, &payload).await {
        state
            .llm_bridge
            .pending_responses
            .lock()
            .unwrap()
            .remove(&req_id);
        return Err(e);
    }

    match tokio::time::timeout(std::time::Duration::from_secs(6), rx).await {
        Ok(Ok((_, body))) => Ok(body
            .get("providers")
            .cloned()
            .unwrap_or_else(|| json!([]))),
        _ => {
            state
                .llm_bridge
                .pending_responses
                .lock()
                .unwrap()
                .remove(&req_id);
            Err("Timed out discovering bridges on peer device".to_string())
        }
    }
}

#[tauri::command]
pub async fn get_peer_llm_proxies(
    state: State<'_, Arc<crate::state::AppState>>,
) -> Result<Vec<LlmPeerProxyInfo>, String> {
    let guard = state.llm_bridge.peer_proxies.lock().unwrap();
    Ok(guard.clone())
}

#[tauri::command]
pub async fn stop_peer_llm_proxy(
    state: State<'_, Arc<crate::state::AppState>>,
    proxy_id: String,
) -> Result<Vec<LlmPeerProxyInfo>, String> {
    if let Some(tx) = state.llm_bridge.peer_proxy_stops.lock().unwrap().remove(&proxy_id) {
        let _ = tx.send(());
    }
    state
        .llm_bridge
        .peer_proxies
        .lock()
        .unwrap()
        .retain(|p| p.id != proxy_id);
    Ok(state.llm_bridge.peer_proxies.lock().unwrap().clone())
}

#[tauri::command]
pub async fn start_peer_llm_proxy(
    app: AppHandle,
    state: State<'_, Arc<crate::state::AppState>>,
    peer_device_id: String,
    peer_alias: String,
    provider_id: String,
    local_port: u16,
    remote_api_key: String,
) -> Result<Vec<LlmPeerProxyInfo>, String> {
    if local_port != 0 && local_port < 1024 {
        return Err("Local port must be 0 (Auto) or >= 1024".to_string());
    }
    let proxy_id = format!("{}:{}", peer_device_id, provider_id);
    if let Some(tx) = state.llm_bridge.peer_proxy_stops.lock().unwrap().remove(&proxy_id) {
        let _ = tx.send(());
    }
    state
        .llm_bridge
        .peer_proxies
        .lock()
        .unwrap()
        .retain(|p| p.id != proxy_id);

    let (listener, active_local_port) = bind_dynamic_tcp_listener("127.0.0.1", local_port)
        .await
        .ok_or_else(|| format!("Failed to bind local peer proxy near port {}", local_port))?;

    let info = LlmPeerProxyInfo {
        id: proxy_id.clone(),
        peer_device_id: peer_device_id.clone(),
        peer_alias,
        provider_id: provider_id.clone(),
        local_port: active_local_port,
        remote_api_key: remote_api_key.trim().to_string(),
        running: true,
    };

    let (tx, mut rx) = oneshot::channel::<()>();
    state
        .llm_bridge
        .peer_proxy_stops
        .lock()
        .unwrap()
        .insert(proxy_id.clone(), tx);
    state.llm_bridge.peer_proxies.lock().unwrap().push(info);

    let app_c = app.clone();
    let st_c = state.inner().clone();
    let _pid_c = proxy_id.clone();

    tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut rx => break,
                res = listener.accept() => {
                    if let Ok((mut socket, _)) = res {
                        let st_req = st_c.clone();
                        let app_req = app_c.clone();
                        let target_dev = peer_device_id.clone();
                        let target_prov = provider_id.clone();
                        let fallback_key = remote_api_key.trim().to_string();
                        tauri::async_runtime::spawn(async move {
                            let _ = handle_peer_proxy_client(
                                &mut socket,
                                &app_req,
                                &st_req,
                                &target_dev,
                                &target_prov,
                                &fallback_key,
                            ).await;
                        });
                    }
                }
            }
        }
    });

    Ok(state.llm_bridge.peer_proxies.lock().unwrap().clone())
}

async fn handle_peer_proxy_client(
    socket: &mut tokio::net::TcpStream,
    _app: &AppHandle,
    state: &Arc<crate::state::AppState>,
    target_device_id: &str,
    provider_id: &str,
    fallback_key: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader_half, mut writer_half) = socket.split();
    let mut buf_reader = BufReader::new(reader_half);

    let mut request_line = String::new();
    if buf_reader.read_line(&mut request_line).await.is_err() || request_line.is_empty() {
        return Ok(());
    }
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }
    let method = parts[0].to_string();
    let path = parts[1].split('?').next().unwrap_or(parts[1]).to_string();

    let mut content_length: usize = 0;
    let mut auth_header = String::new();
    loop {
        let mut line = String::new();
        if buf_reader.read_line(&mut line).await.is_err() {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        let lower = trimmed.to_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            if let Ok(cl) = v.trim().parse::<usize>() {
                content_length = cl;
            }
        } else if lower.starts_with("authorization:") {
            auth_header = trimmed[14..].trim().to_string();
        }
    }

    if method == "OPTIONS" {
        let resp = "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = writer_half.write_all(resp.as_bytes()).await;
        return Ok(());
    }

    let mut body_bytes = vec![0u8; content_length.min(4 * 1024 * 1024)];
    if !body_bytes.is_empty() {
        buf_reader.read_exact(&mut body_bytes).await?;
    }
    let req_body: Value = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(Value::Null)
    };
    let wants_stream = req_body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    let req_id = format!("llm_{:012x}", rand::random::<u64>() & 0xffff_ffff_ffff);
    let (tx, rx) = oneshot::channel::<(u16, Value)>();
    state.llm_bridge.pending_responses.lock().unwrap().insert(req_id.clone(), tx);

    let effective_auth = if !fallback_key.is_empty() {
        format!("Bearer {}", fallback_key)
    } else {
        auth_header
    };

    let relay_payload = json!({
        "type": "llm_req",
        "req_id": req_id,
        "provider_id": provider_id,
        "method": method,
        "path": path,
        "auth_header": effective_auth,
        "body": req_body
    });

    let sid = format!("llm_bridge:{}", req_id);
    if let Err(e) = send_llm_relay_packet(state, target_device_id, &sid, &relay_payload).await {
        state.llm_bridge.pending_responses.lock().unwrap().remove(&req_id);
        send_http_json(
            &mut writer_half,
            "503 Service Unavailable",
            &openai_error(&e, "server_error", "peer_unreachable"),
        )
        .await;
        return Ok(());
    }

    match tokio::time::timeout(std::time::Duration::from_secs(180), rx).await {
        Ok(Ok((status_code, body))) => {
            if wants_stream && status_code == 200 {
                let hdr = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n";
                let _ = writer_half.write_all(hdr.as_bytes()).await;
                let content = body
                    .pointer("/choices/0/message/content")
                    .cloned()
                    .unwrap_or(Value::Null);
                let tool_calls = body.pointer("/choices/0/message/tool_calls").cloned();
                let finish = body
                    .pointer("/choices/0/finish_reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("stop");
                let delta = if let Some(tc) = tool_calls {
                    json!({ "role": "assistant", "content": Value::Null, "tool_calls": tc })
                } else {
                    json!({ "role": "assistant", "content": content })
                };
                let chunk = json!({
                    "id": body.get("id").cloned().unwrap_or_else(|| json!("chatcmpl-relay")),
                    "object": "chat.completion.chunk",
                    "created": now_unix_secs(),
                    "model": body.get("model").cloned().unwrap_or_else(|| json!(provider_id)),
                    "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }]
                });
                let sse = format!("data: {}\n\ndata: [DONE]\n\n", chunk);
                let _ = writer_half.write_all(sse.as_bytes()).await;
            } else {
                let status_line = match status_code {
                    200 => "200 OK",
                    400 => "400 Bad Request",
                    401 => "401 Unauthorized",
                    403 => "403 Forbidden",
                    404 => "404 Not Found",
                    503 => "503 Service Unavailable",
                    _ => "502 Bad Gateway",
                };
                send_http_json(&mut writer_half, status_line, &body).await;
            }
        }
        _ => {
            state.llm_bridge.pending_responses.lock().unwrap().remove(&req_id);
            send_http_json(
                &mut writer_half,
                "504 Gateway Timeout",
                &openai_error("Timed out waiting for peer LLM Bridge response", "server_error", "timeout"),
            )
            .await;
        }
    }
    Ok(())
}
