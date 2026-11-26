use crate::db::storage::NoteItem;
use crate::state::AppState;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{info, warn};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpServerRuntime {
    pub running: bool,
    pub port: u16,
    pub url: String,
    pub last_error: Option<String>,
}

impl Default for McpServerRuntime {
    fn default() -> Self {
        Self {
            running: false,
            port: 48765,
            url: "http://127.0.0.1:48765/mcp".to_string(),
            last_error: None,
        }
    }
}

pub struct McpServerManager {
    stop_flag: Mutex<Option<Arc<AtomicBool>>>,
    pub runtime: Mutex<McpServerRuntime>,
}

impl Default for McpServerManager {
    fn default() -> Self {
        Self {
            stop_flag: Mutex::new(None),
            runtime: Mutex::new(McpServerRuntime::default()),
        }
    }
}

pub fn get_mcp_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "media_search_ytmusic",
            "description": "Search songs or music videos on YouTube Music.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search keywords (song title, artist, or video)" },
                    "filter": { "type": "string", "enum": ["all", "songs", "videos"], "description": "Filter results by songs or videos" }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "media_play_ytmusic",
            "description": "Search YouTube Music and immediately play the top matching song or music video in RayAmp.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Song or video query to search and play" },
                    "video_id": { "type": "string", "description": "Optional exact YouTube videoId if already known" },
                    "prefer_video": { "type": "boolean", "description": "True to open video mode and play music video" }
                }
            }
        }),
        json!({
            "name": "media_download_ytmusic",
            "description": "Search YouTube Music and download the top matching song or video directly as MP3 (audio) or MP4 (video) to RayDrop's downloads folder.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Song or video search query" },
                    "video_id": { "type": "string", "description": "Optional exact YouTube videoId if already known" },
                    "format": { "type": "string", "enum": ["mp3", "mp4"], "description": "Download format: mp3 (audio) or mp4 (video)" }
                },
                "required": ["format"]
            }
        }),
        json!({
            "name": "media_control",
            "description": "Control RayAmp playback: toggle, pause, resume, stop, next, prev, set_volume, or set_video_mode.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["toggle", "pause", "resume", "stop", "next", "prev", "set_volume", "set_video_mode"]
                    },
                    "volume": { "type": "number", "description": "Volume from 0.0 to 1.0 (used with set_volume)" },
                    "video_mode": { "type": "boolean", "description": "Enable/disable video view (used with set_video_mode)" }
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "media_get_status",
            "description": "Get current RayAmp playback status, active track, queue length, volume, and video mode.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "playlist_list",
            "description": "List all saved RayAmp media playlists.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "playlist_create",
            "description": "Create a new RayAmp media playlist.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Playlist name" }
                },
                "required": ["name"]
            }
        }),
        json!({
            "name": "playlist_add_ytmusic",
            "description": "Search YouTube Music and add the top matching track/video to a saved playlist.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "playlist_id": { "type": "string", "description": "Target playlist ID" },
                    "query": { "type": "string", "description": "Song or video search query" },
                    "prefer_video": { "type": "boolean" }
                },
                "required": ["playlist_id", "query"]
            }
        }),
        json!({
            "name": "playlist_play",
            "description": "Play a saved playlist in RayAmp by playlist_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "playlist_id": { "type": "string", "description": "Playlist ID to play" }
                },
                "required": ["playlist_id"]
            }
        }),
        json!({
            "name": "devices_list",
            "description": "List known RayDrop devices/contacts with online status, OS, device_id, and MAC address.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "device_wake_on_lan",
            "description": "Send a Wake-on-LAN Magic Packet to wake up a PC on LAN or WAN.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mac_address": { "type": "string", "description": "Target MAC address (AA:BB:CC:DD:EE:FF)" },
                    "wan_host": { "type": "string", "description": "Optional DDNS hostname or Public IP for WAN WoL" },
                    "port": { "type": "integer", "description": "Optional UDP port (default 9)" }
                },
                "required": ["mac_address"]
            }
        }),
        json!({
            "name": "system_telemetry",
            "description": "Get live system telemetry of this host PC (CPU, RAM, GPU, Disks, Network, Uptime).",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "clipboard_get_history",
            "description": "Read recent clipboard history entries stored by RayDrop.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "Max items to return (default 10)" }
                }
            }
        }),
        json!({
            "name": "notes_list",
            "description": "List saved RayDrop notes and checklists.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "include_archived": { "type": "boolean" }
                }
            }
        }),
        json!({
            "name": "notes_create",
            "description": "Create a new note in RayDrop.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["title", "content"]
            }
        }),
        json!({
            "name": "shared_folders_list",
            "description": "List local folders shared on this device for remote browsing.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
    ]
}

pub fn to_openai_tools() -> Vec<Value> {
    get_mcp_tool_definitions()
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

pub async fn execute_mcp_tool(
    app: &AppHandle,
    state: &Arc<AppState>,
    name: &str,
    args: &Value,
) -> Result<String, String> {
    match name {
        "media_search_ytmusic" => {
            let query = args["query"].as_str().unwrap_or("").trim();
            let filter = args["filter"].as_str().unwrap_or("all");
            let items = crate::media_player::search_ytmusic(query, filter).await?;
            serde_json::to_string_pretty(&items).map_err(|e| e.to_string())
        }
        "media_play_ytmusic" => {
            let prefer_video = args["prefer_video"].as_bool().unwrap_or(false);
            let mut video_id = args["video_id"].as_str().unwrap_or("").trim().to_string();
            let mut title = "YouTube Music Track".to_string();
            let mut artist = "YouTube Music".to_string();
            let mut duration_secs = 0u64;
            let mut thumbnail_url = None;
            let mut is_video = prefer_video;

            if video_id.is_empty() {
                let query = args["query"].as_str().unwrap_or("").trim();
                if query.is_empty() {
                    return Err("Provide either `query` or `video_id`".to_string());
                }
                let filter = if prefer_video { "videos" } else { "all" };
                let results = crate::media_player::search_ytmusic(query, filter).await?;
                let first = results
                    .into_iter()
                    .next()
                    .ok_or_else(|| format!("No YouTube Music results found for '{}'", query))?;
                video_id = first.video_id;
                title = first.title;
                artist = first.artist;
                duration_secs = first.duration_secs;
                thumbnail_url = first.thumbnail_url;
                is_video = prefer_video || first.is_video;
            }

            let track = crate::media_player::MediaTrack {
                id: format!("yt-{}", video_id),
                source_type: "ytmusic".to_string(),
                title: title.clone(),
                artist: artist.clone(),
                duration_secs,
                uri_or_id: video_id.clone(),
                thumbnail_url,
                is_video,
                stream_url: None,
            };

            let _ = app.emit(
                "media_player_remote_action",
                json!({
                    "action": "play_track",
                    "payload": { "track": track }
                }),
            );

            Ok(json!({
                "status": "playing_requested",
                "title": title,
                "artist": artist,
                "video_id": video_id,
                "is_video": is_video
            })
            .to_string())
        }
        "media_download_ytmusic" => {
            let fmt = args["format"].as_str().unwrap_or("mp3");
            let mut video_id = args["video_id"].as_str().unwrap_or("").trim().to_string();
            let mut title_hint = None;
            if video_id.is_empty() {
                let query = args["query"].as_str().unwrap_or("").trim();
                if query.is_empty() {
                    return Err("Provide either `query` or `video_id`".to_string());
                }
                let filter = if fmt == "mp4" { "videos" } else { "all" };
                let results = crate::media_player::search_ytmusic(query, filter).await?;
                let first = results
                    .into_iter()
                    .next()
                    .ok_or_else(|| format!("No YouTube Music results found for '{}'", query))?;
                video_id = first.video_id;
                title_hint = Some(first.title);
            }
            let dir = state
                .settings
                .lock()
                .map(|s| s.download_dir.clone())
                .map_err(|e| e.to_string())?;
            let res = crate::media_player::download_ytmusic_media(
                Some(app),
                &video_id,
                title_hint.as_deref(),
                fmt,
                &dir,
            )
            .await?;
            serde_json::to_string_pretty(&res).map_err(|e| e.to_string())
        }
        "media_control" => {
            let action = args["action"].as_str().unwrap_or("toggle");
            let _ = app.emit(
                "media_player_remote_action",
                json!({
                    "action": action,
                    "payload": args
                }),
            );
            Ok(json!({ "status": "ok", "action": action }).to_string())
        }
        "media_get_status" => {
            let st = state
                .media_player
                .lock()
                .map(|g| g.clone())
                .map_err(|e| e.to_string())?;
            serde_json::to_string_pretty(&st).map_err(|e| e.to_string())
        }
        "playlist_list" => {
            let list = state.db.list_media_playlists()?;
            serde_json::to_string_pretty(&list).map_err(|e| e.to_string())
        }
        "playlist_create" => {
            let pname = args["name"].as_str().unwrap_or("New Playlist");
            let pl = state.db.create_media_playlist(pname)?;
            serde_json::to_string_pretty(&pl).map_err(|e| e.to_string())
        }
        "playlist_add_ytmusic" => {
            let pid = args["playlist_id"].as_str().unwrap_or("").trim();
            let query = args["query"].as_str().unwrap_or("").trim();
            let prefer_video = args["prefer_video"].as_bool().unwrap_or(false);
            if pid.is_empty() || query.is_empty() {
                return Err("`playlist_id` and `query` are required".to_string());
            }
            let filter = if prefer_video { "videos" } else { "all" };
            let results = crate::media_player::search_ytmusic(query, filter).await?;
            let first = results
                .into_iter()
                .next()
                .ok_or_else(|| format!("No results for '{}'", query))?;
            let item = state.db.add_media_playlist_item(
                pid,
                "ytmusic",
                &first.title,
                &first.artist,
                first.duration_secs,
                &first.video_id,
                first.thumbnail_url.as_deref(),
                prefer_video || first.is_video,
            )?;
            serde_json::to_string_pretty(&item).map_err(|e| e.to_string())
        }
        "playlist_play" => {
            let pid = args["playlist_id"].as_str().unwrap_or("").trim();
            if pid.is_empty() {
                return Err("`playlist_id` is required".to_string());
            }
            let _ = app.emit(
                "media_player_remote_action",
                json!({
                    "action": "play_playlist",
                    "payload": { "playlist_id": pid }
                }),
            );
            Ok(json!({ "status": "ok", "playlist_id": pid }).to_string())
        }
        "devices_list" => {
            let contacts = state.db.list_contacts()?;
            let summary: Vec<Value> = contacts
                .into_iter()
                .map(|c| {
                    json!({
                        "device_id": c.device_id,
                        "alias_name": c.alias_name,
                        "custom_name": c.custom_name,
                        "os_type": c.os_type,
                        "is_own_device": c.is_own_device,
                        "is_online": c.is_online,
                        "mac_address": c.mac_address,
                        "network_type": c.network_type
                    })
                })
                .collect();
            serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
        }
        "device_wake_on_lan" => {
            let mac = args["mac_address"].as_str().unwrap_or("").to_string();
            let wan = args["wan_host"].as_str().map(|s| s.to_string());
            let port = args["port"].as_u64().map(|p| p as u16);
            let target_id = args["target_device_id"].as_str().map(|s| s.to_string());
            let msg = crate::commands::send_wake_on_lan_auto_inner(
                state,
                target_id,
                mac.clone(),
                wan.clone(),
                port,
            )
            .await?;
            Ok(json!({
                "status": "magic_packet_sent",
                "message": msg,
                "mac_address": mac,
                "wan_host": wan,
                "port": port.unwrap_or(9)
            })
            .to_string())
        }
        "system_telemetry" => {
            let t = tokio::task::spawn_blocking(crate::system_monitor::collect_system_telemetry)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_string_pretty(&t).map_err(|e| e.to_string())
        }
        "clipboard_get_history" => {
            let limit = args["limit"].as_u64().unwrap_or(10) as usize;
            let items = state.db.list_clipboard_history(limit.min(30))?;
            serde_json::to_string_pretty(&items).map_err(|e| e.to_string())
        }
        "notes_list" => {
            let inc = args["include_archived"].as_bool().unwrap_or(false);
            let notes = state.db.list_notes(inc)?;
            serde_json::to_string_pretty(&notes).map_err(|e| e.to_string())
        }
        "notes_create" => {
            let title = args["title"].as_str().unwrap_or("AI Note").to_string();
            let content = args["content"].as_str().unwrap_or("").to_string();
            let note = NoteItem {
                id: String::new(),
                title,
                content,
                note_type: "note".to_string(),
                checklist: Vec::new(),
                images: Vec::new(),
                color: "default".to_string(),
                labels: vec!["ai".to_string()],
                pinned: false,
                archived: false,
                created_at: 0,
                updated_at: 0,
            };
            let saved = state.db.upsert_note(note)?;
            serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())
        }
        "shared_folders_list" => {
            let folders = state.db.list_shared_folders()?;
            serde_json::to_string_pretty(&folders).map_err(|e| e.to_string())
        }
        _ => Err(format!("Unknown MCP tool: {}", name)),
    }
}

pub async fn handle_mcp_jsonrpc(
    app: &AppHandle,
    state: &Arc<AppState>,
    req: Value,
) -> Option<Value> {
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

    // Notifications (no id) do not require a response
    if id.is_none() || method.starts_with("notifications/") {
        return None;
    }

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": { "listChanged": false }
            },
            "serverInfo": {
                "name": "raydrop-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": get_mcp_tool_definitions()
        })),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or(json!({}));
            let tool_name = params["name"].as_str().unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match execute_mcp_tool(app, state, tool_name, &args).await {
                Ok(text) => Ok(json!({
                    "content": [{ "type": "text", "text": text }],
                    "isError": false
                })),
                Err(err) => Ok(json!({
                    "content": [{ "type": "text", "text": err }],
                    "isError": true
                })),
            }
        }
        _ => Err((-32601, format!("Method not found: {}", method))),
    };

    match result {
        Ok(res) => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": res
        })),
        Err((code, msg)) => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": msg }
        })),
    }
}

pub fn restart_mcp_server(
    app: AppHandle,
    state: Arc<AppState>,
    enabled: bool,
    port: u16,
    auth_token: String,
) {
    if let Ok(mut guard) = state.mcp_server.stop_flag.lock() {
        if let Some(old_flag) = guard.take() {
            old_flag.store(true, Ordering::Relaxed);
        }
        if !enabled {
            if let Ok(mut rt) = state.mcp_server.runtime.lock() {
                rt.running = false;
                rt.port = port;
                rt.url = format!("http://127.0.0.1:{}/mcp", port);
                rt.last_error = None;
            }
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        *guard = Some(stop.clone());

        let state_bg = state.clone();
        tauri::async_runtime::spawn(async move {
            run_mcp_http_loop(app, state_bg, port, auth_token, stop).await;
        });
    }
}

async fn run_mcp_http_loop(
    app: AppHandle,
    state: Arc<AppState>,
    port: u16,
    auth_token: String,
    stop: Arc<AtomicBool>,
) {
    // Try configured port first, then automatically fall back to nearby free ports if occupied
    let mut listener_opt: Option<(TcpListener, u16)> = None;
    let mut last_bind_err = String::new();
    for offset in 0..=20u16 {
        let candidate_port = port.saturating_add(offset);
        if candidate_port == 0 {
            continue;
        }
        let addr = format!("127.0.0.1:{}", candidate_port);
        match TcpListener::bind(&addr).await {
            Ok(l) => {
                listener_opt = Some((l, candidate_port));
                break;
            }
            Err(e) => {
                last_bind_err = e.to_string();
            }
        }
    }

    if listener_opt.is_none() {
        if let Ok(l) = TcpListener::bind("127.0.0.1:0").await {
            if let Ok(local) = l.local_addr() {
                listener_opt = Some((l, local.port()));
            }
        }
    }

    let (listener, actual_port) = match listener_opt {
        Some(pair) => pair,
        None => {
            warn!("Failed to bind MCP HTTP server near port {}: {}", port, last_bind_err);
            if let Ok(mut rt) = state.mcp_server.runtime.lock() {
                rt.running = false;
                rt.port = port;
                rt.last_error = Some(format!("Port {} bind error: {}", port, last_bind_err));
            }
            return;
        }
    };

    let bind_addr = format!("127.0.0.1:{}", actual_port);
    info!("RayDrop MCP HTTP server listening on http://{}/mcp", bind_addr);
    if let Ok(mut rt) = state.mcp_server.runtime.lock() {
        rt.running = true;
        rt.port = actual_port;
        rt.url = format!("http://{}/mcp", bind_addr);
        rt.last_error = None;
    }

    while !stop.load(Ordering::Relaxed) {
        let accept_fut = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            listener.accept(),
        )
        .await;

        let Ok(Ok((mut socket, _))) = accept_fut else {
            continue;
        };

        let app_c = app.clone();
        let state_c = state.clone();
        let token_c = auth_token.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            let Ok(n) = socket.read(&mut buf).await else {
                return;
            };
            if n == 0 {
                return;
            }
            let raw = String::from_utf8_lossy(&buf[..n]);
            let Some((head, body)) = raw.split_once("\r\n\r\n") else {
                return;
            };

            let first_line = head.lines().next().unwrap_or("");
            if first_line.starts_with("OPTIONS ") {
                let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, GET, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nConnection: close\r\n\r\n";
                let _ = socket.write_all(resp.as_bytes()).await;
                return;
            }
            if first_line.starts_with("GET /health") {
                let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"ok\":true,\"server\":\"raydrop-mcp\"}";
                let _ = socket.write_all(resp.as_bytes()).await;
                return;
            }

            if !token_c.trim().is_empty() {
                let expected = format!("authorization: bearer {}", token_c.trim().to_lowercase());
                let authorized = head
                    .lines()
                    .any(|l| l.trim().to_lowercase() == expected);
                if !authorized {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Unauthorized\"}";
                    let _ = socket.write_all(resp.as_bytes()).await;
                    return;
                }
            }

            let req_json: Value = match serde_json::from_str(body.trim()) {
                Ok(v) => v,
                Err(e) => {
                    let err_body = json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                    })
                    .to_string();
                    let resp = format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        err_body.len(),
                        err_body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            match handle_mcp_jsonrpc(&app_c, &state_c, req_json).await {
                Some(reply) => {
                    let reply_str = reply.to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        reply_str.len(),
                        reply_str
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                }
                None => {
                    let resp = "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = socket.write_all(resp.as_bytes()).await;
                }
            }
        });
    }
}
