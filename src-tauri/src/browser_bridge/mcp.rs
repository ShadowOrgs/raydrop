use crate::state::AppState;
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

fn rpc_ok(id: Value, result: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
    .to_string()
}

fn rpc_err(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        }
    })
    .to_string()
}

fn tool_ok(data: Value) -> Value {
    let text = serde_json::to_string_pretty(&data).unwrap_or_else(|_| data.to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    })
}

fn tool_err(msg: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": msg }],
        "isError": true
    })
}

pub fn list_tools_schema() -> Value {
    json!({
        "tools": [
            {
                "name": "raydrop_get_status",
                "description": "Get this Raydrop device's identity (device_id, alias), network addresses (local IP, port, WAN addr, signaling status), and live system telemetry (CPU, RAM, disk, OS).",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
            },
            {
                "name": "raydrop_list_contacts",
                "description": "List all known Raydrop devices and contacts (own devices, LAN peers, WAN contacts) with online status, IP, trust level, and Direct Mesh status.",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
            },
            {
                "name": "raydrop_get_lan_statuses",
                "description": "Check real-time LAN connectivity (Local LAN, Tailscale Virtual LAN, or Raydrop Direct Mesh) for all contacts.",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
            },
            {
                "name": "raydrop_send_files",
                "description": "Send one or more local files or folders to another Raydrop device via LAN, Tailscale, Direct Mesh, QUIC P2P, or WSS Relay.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "target_device_id": { "type": "string", "description": "Target Raydrop device ID (e.g. RAY-XXXX-XXXX-XXXX)" },
                        "file_paths": { "type": "array", "items": { "type": "string" }, "description": "Absolute local paths of files or folders to send" }
                    },
                    "required": ["target_device_id", "file_paths"]
                }
            },
            {
                "name": "raydrop_list_transfers",
                "description": "List active and historical file transfers (direction, route, progress bytes, speed, status).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Max records to return (default 25)" }
                    }
                }
            },
            {
                "name": "raydrop_control_transfer",
                "description": "Pause, resume, or cancel an active/paused file transfer.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "description": "Transfer session ID" },
                        "action": { "type": "string", "enum": ["pause", "resume", "cancel"], "description": "Control action" }
                    },
                    "required": ["session_id", "action"]
                }
            },
            {
                "name": "raydrop_send_chat",
                "description": "Send an encrypted P2P chat message to a Raydrop contact/device.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "peer_device_id": { "type": "string", "description": "Target device ID" },
                        "content": { "type": "string", "description": "Message text to send" }
                    },
                    "required": ["peer_device_id", "content"]
                }
            },
            {
                "name": "raydrop_get_chat_history",
                "description": "Read recent P2P chat messages with a Raydrop contact/device.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "peer_device_id": { "type": "string", "description": "Peer device ID" },
                        "limit": { "type": "integer", "description": "Max messages to return (default 30)" }
                    },
                    "required": ["peer_device_id"]
                }
            },
            {
                "name": "raydrop_manage_mesh",
                "description": "Manage Raydrop Direct Mesh (Virtual LAN) connection with a WAN peer: request, approve, decline, or revoke.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "device_id": { "type": "string", "description": "Peer device ID" },
                        "action": { "type": "string", "enum": ["request", "approve", "decline", "revoke"] }
                    },
                    "required": ["device_id", "action"]
                }
            },
            {
                "name": "raydrop_wake_on_lan",
                "description": "Send a Wake-on-LAN magic packet to wake up an offline PC by device_id or mac_address.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "device_id": { "type": "string", "description": "Optional Raydrop device_id (looks up saved MAC address)" },
                        "mac_address": { "type": "string", "description": "Optional MAC address (e.g. AA:BB:CC:DD:EE:FF)" },
                        "target_host": { "type": "string", "description": "Optional broadcast/IP target" },
                        "port": { "type": "integer", "description": "Optional UDP port (default 9)" }
                    }
                }
            },
            {
                "name": "raydrop_start_download",
                "description": "Start an accelerated multi-connection HTTP/HTTPS download or BitTorrent/Magnet download in Raydrop.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "HTTP/HTTPS URL or magnet:? link" },
                        "filename": { "type": "string", "description": "Optional custom filename (for HTTP downloads)" },
                        "save_dir": { "type": "string", "description": "Optional target directory" }
                    },
                    "required": ["url"]
                }
            },
            {
                "name": "raydrop_list_downloads",
                "description": "List all HTTP accelerated downloads and BitTorrent/Magnet tasks in Raydrop.",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
            },
            {
                "name": "raydrop_clipboard",
                "description": "Read or write the system clipboard, or inspect Raydrop clipboard history.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["read", "write", "history"] },
                        "content": { "type": "string", "description": "Text to write when action is 'write'" },
                        "limit": { "type": "integer", "description": "History limit (default 20)" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "raydrop_notes",
                "description": "List notes or create/update a note in Raydrop Notes.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["list", "save"] },
                        "id": { "type": "string", "description": "Optional note ID when updating" },
                        "title": { "type": "string", "description": "Note title" },
                        "content": { "type": "string", "description": "Note markdown/text body" },
                        "pinned": { "type": "boolean" },
                        "include_archived": { "type": "boolean" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "raydrop_list_monitors",
                "description": "List connected displays/monitors and Virtual Display Driver status on this host.",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
            },
            {
                "name": "raydrop_remote_input",
                "description": "Simulate local mouse, wheel, keyboard key, or shortcut input on this PC.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["mouse_move", "mouse_click", "mouse_wheel", "key", "shortcut"] },
                        "x": { "type": "number", "description": "Normalized X (0.0 - 1.0) for mouse_move" },
                        "y": { "type": "number", "description": "Normalized Y (0.0 - 1.0) for mouse_move" },
                        "button": { "type": "string", "enum": ["left", "right", "middle"] },
                        "click_action": { "type": "string", "enum": ["down", "up", "click"] },
                        "delta_y": { "type": "integer", "description": "Scroll delta for mouse_wheel" },
                        "key": { "type": "string" },
                        "code": { "type": "string" },
                        "key_action": { "type": "string", "enum": ["down", "up"] },
                        "shortcut": { "type": "string", "description": "Shortcut name e.g. win_d, alt_tab, copy, paste" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "raydrop_convert_images",
                "description": "Convert, resize, or compress image files (png, jpg, webp, ico, pdf, bmp, tiff) using Raydrop Image Converter.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "input_paths": { "type": "array", "items": { "type": "string" } },
                        "target_format": { "type": "string", "description": "Target format: png, jpg, webp, ico, pdf, bmp, tiff" },
                        "resize_width": { "type": "integer" },
                        "resize_height": { "type": "integer" },
                        "keep_aspect": { "type": "boolean" },
                        "quality": { "type": "integer", "description": "JPEG/WebP quality 1-100" },
                        "output_dir": { "type": "string" }
                    },
                    "required": ["input_paths", "target_format"]
                }
            },
            {
                "name": "raydrop_web_debug",
                "description": "Web Debugger & Speedtest: run live HTTP latency/throughput speedtest on any URL, or read the latest Web Debugger audit report & live browser events captured by the Raydrop Chrome Extension.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["speedtest", "get_audit", "get_events", "clear"] },
                        "url": { "type": "string", "description": "Target URL for speedtest (http:// or https://)" },
                        "limit": { "type": "integer", "description": "Max events to return for get_events (default 50)" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "raydrop_convert_documents",
                "description": "Convert documents (DOCX, PDF, TXT, HTML, etc.) using Raydrop Document Converter (Word/LibreOffice).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "input_paths": { "type": "array", "items": { "type": "string" } },
                        "target_format": { "type": "string", "description": "e.g. pdf, docx, txt, html" },
                        "output_dir": { "type": "string" }
                    },
                    "required": ["input_paths", "target_format"]
                }
            },
            {
                "name": "raydrop_generate_mobile_icons",
                "description": "Generate an Android/iOS mobile app icon pack ZIP from a source image.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "input_path": { "type": "string" },
                        "platform": { "type": "string", "enum": ["android", "ios", "all"] },
                        "output_zip_path": { "type": "string" }
                    },
                    "required": ["input_path", "platform"]
                }
            },
            {
                "name": "raydrop_folder_sync",
                "description": "List, trigger, toggle auto-sync, or delete real-time Folder Sync mirroring tasks.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["list", "trigger", "toggle_auto", "delete"] },
                        "task_id": { "type": "string" },
                        "enabled": { "type": "boolean" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "raydrop_remote_files",
                "description": "Browse or pull files from a remote Raydrop peer's shared folders, or manage local shared folders.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["list_local_shares", "add_local_share", "delete_local_share", "browse_peer_shares", "browse_peer_dir", "pull_file"] },
                        "peer_device_id": { "type": "string" },
                        "share_id": { "type": "string" },
                        "subpath": { "type": "string" },
                        "relative_path": { "type": "string" },
                        "folder_path": { "type": "string" },
                        "alias": { "type": "string" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "raydrop_web_share",
                "description": "Start or stop a public local/QR Web Share HTTP server for selected files.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["start", "stop"] },
                        "paths": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "raydrop_group_chat",
                "description": "List Raydrop P2P chat groups, inspect a group, or send a group message.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["list", "get", "send"] },
                        "group_id": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "raydrop_control_download",
                "description": "Pause, resume, or delete an HTTP accelerated download or BitTorrent/Magnet task.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string", "enum": ["http", "torrent"] },
                        "id": { "type": "string" },
                        "action": { "type": "string", "enum": ["pause", "resume", "delete"] },
                        "delete_files": { "type": "boolean" }
                    },
                    "required": ["kind", "id", "action"]
                }
            },
            {
                "name": "raydrop_respond_transfer",
                "description": "Accept or reject a pending incoming file transfer request.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string" },
                        "accepted": { "type": "boolean" },
                        "target_directory": { "type": "string" }
                    },
                    "required": ["session_id", "accepted"]
                }
            },
            {
                "name": "raydrop_virtual_display",
                "description": "Check or toggle the Raydrop Virtual Display Driver (ON / OFF).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["status", "enable", "disable"] }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "raydrop_system_power",
                "description": "Execute a system power action on this host (lock, sleep, restart, shutdown).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["lock", "sleep", "restart", "shutdown"] }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "raydrop_take_screenshot",
                "description": "Capture a live PNG screenshot of the host's primary display so the AI can visually inspect the screen.",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
            },
            {
                "name": "raydrop_analytics_and_tools",
                "description": "Get transfer analytics, compute SHA-256 file hash, set transfer speed limit, or toggle discoverability.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["analytics", "hash_file", "set_speed_limit", "set_discoverable"] },
                        "path": { "type": "string" },
                        "speed_limit_mbps": { "type": "integer" },
                        "discoverable": { "type": "boolean" }
                    },
                    "required": ["action"]
                }
            }
        ]
    })
}

pub async fn handle_mcp_request(
    req: Value,
    app: &AppHandle,
    state: &Arc<AppState>,
) -> Option<String> {
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");

    // Notifications without id don't expect a JSON-RPC response
    let Some(rpc_id) = id else {
        return None;
    };

    let resp = match method {
        "initialize" => rpc_ok(
            rpc_id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "raydrop-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "ping" | "notifications/initialized" => rpc_ok(rpc_id, json!({})),
        "tools/list" => rpc_ok(rpc_id, list_tools_schema()),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let result = execute_tool(tool_name, args, app, state).await;
            rpc_ok(rpc_id, result)
        }
        _ => rpc_err(rpc_id, -32601, &format!("Method not found: {method}")),
    };

    Some(resp)
}

async fn execute_tool(
    name: &str,
    args: Value,
    app: &AppHandle,
    state: &Arc<AppState>,
) -> Value {
    match name {
        "raydrop_get_status" => {
            let settings = state.settings.lock().unwrap().clone();
            let wan = state.wan_reflexive_addr.lock().unwrap().clone();
            let telemetry = tokio::task::spawn_blocking(crate::system_monitor::collect_system_telemetry)
                .await
                .ok();
            tool_ok(json!({
                "device_id": state.identity.device_id,
                "alias_name": settings.alias_name,
                "lan_addresses": crate::network::transport::collect_all_lan_addresses(state.listen_port),
                "listen_port": state.listen_port,
                "wan_reflexive_addr": wan,
                "signaling_connected": state.is_signaling_connected(),
                "discoverable": settings.discoverable,
                "download_dir": settings.download_dir,
                "version": env!("CARGO_PKG_VERSION"),
                "telemetry": telemetry,
            }))
        }
        "raydrop_list_contacts" => match state.db.list_contacts() {
            Ok(list) => tool_ok(json!({ "contacts": list })),
            Err(e) => tool_err(&e),
        },
        "raydrop_get_lan_statuses" => {
            let statuses = crate::commands::compute_lan_connection_statuses(state);
            tool_ok(json!({ "statuses": statuses }))
        }
        "raydrop_send_files" => {
            let target = args.get("target_device_id").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let paths: Vec<String> = args
                .get("file_paths")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            if target.is_empty() || paths.is_empty() {
                return tool_err("target_device_id and non-empty file_paths are required");
            }
            match crate::network::transport::initiate_peer_transfer(app.clone(), state.clone(), target, paths).await {
                Ok(session_id) => tool_ok(json!({ "status": "started", "session_id": session_id })),
                Err(e) => tool_err(&e),
            }
        }
        "raydrop_list_transfers" => {
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(25) as usize;
            match state.db.list_transfers() {
                Ok(mut list) => {
                    list.truncate(limit);
                    tool_ok(json!({ "transfers": list }))
                }
                Err(e) => tool_err(&e),
            }
        }
        "raydrop_control_transfer" => {
            let sid = args.get("session_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
            if sid.is_empty() {
                return tool_err("session_id is required");
            }
            match action {
                "resume" => match crate::network::transport::resume_peer_transfer(app.clone(), state.clone(), sid).await {
                    Ok(s) => tool_ok(json!({ "status": "resumed", "session_id": s })),
                    Err(e) => tool_err(&e),
                },
                "pause" | "cancel" => {
                    if let Some(flag) = state.active_cancellations.lock().unwrap().get(&sid) {
                        flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    if let Ok(Some(mut rec)) = state.db.get_transfer(&sid) {
                        rec.status = if action == "pause" { "paused".to_string() } else { "cancelled".to_string() };
                        let _ = state.db.upsert_transfer(&rec);
                        let _ = app.emit("transfer_state_changed", &rec);
                        crate::commands::send_transfer_control(
                            state,
                            &sid,
                            &rec.peer_device_id,
                            if action == "pause" { "TRANSFER_PAUSE" } else { "TRANSFER_CANCEL" },
                            None,
                        );
                    }
                    tool_ok(json!({ "status": action, "session_id": sid }))
                }
                _ => tool_err("action must be pause, resume, or cancel"),
            }
        }
        "raydrop_send_chat" => {
            let peer = args.get("peer_device_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if peer.is_empty() || content.is_empty() {
                return tool_err("peer_device_id and content are required");
            }
            match crate::commands::send_chat_message_internal(app, state, peer, content, None, None, None) {
                Ok(msg) => tool_ok(json!({ "status": "sent", "message": msg })),
                Err(e) => tool_err(&e),
            }
        }
        "raydrop_get_chat_history" => {
            let peer = args.get("peer_device_id").and_then(|v| v.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(30) as u32;
            if peer.is_empty() {
                return tool_err("peer_device_id is required");
            }
            match state.db.list_chat_messages(peer, limit, 0) {
                Ok(msgs) => tool_ok(json!({ "messages": msgs })),
                Err(e) => tool_err(&e),
            }
        }
        "raydrop_manage_mesh" => {
            let device_id = args.get("device_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
            if device_id.is_empty() {
                return tool_err("device_id is required");
            }
            match action {
                "request" => {
                    let _ = state.db.set_contact_mesh_status(&device_id, "pending_out");
                    let _ = app.emit("contacts_updated", ());
                    crate::commands::send_mesh_control(state, &device_id, "MESH_REQUEST");
                    tool_ok(json!({ "status": "pending_out", "device_id": device_id }))
                }
                "approve" => {
                    let _ = state.db.set_contact_mesh_status(&device_id, "approved");
                    let _ = app.emit("contacts_updated", ());
                    crate::commands::send_mesh_control(state, &device_id, "MESH_APPROVE");
                    let st = state.clone();
                    let pid = device_id.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::network::transport::warm_direct_mesh_peer(&st, &pid).await;
                    });
                    tool_ok(json!({ "status": "approved", "device_id": device_id }))
                }
                "decline" | "revoke" => {
                    let _ = state.db.set_contact_mesh_status(&device_id, "none");
                    state.mesh_connections.lock().unwrap().remove(&device_id);
                    let _ = app.emit("contacts_updated", ());
                    crate::commands::send_mesh_control(state, &device_id, "MESH_REVOKE");
                    tool_ok(json!({ "status": "none", "device_id": device_id }))
                }
                _ => tool_err("action must be request, approve, decline, or revoke"),
            }
        }
        "raydrop_wake_on_lan" => {
            let mut mac = args.get("mac_address").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_default();
            let mut host = args.get("target_host").and_then(|v| v.as_str()).map(|s| s.to_string());
            if mac.trim().is_empty() {
                if let Some(did) = args.get("device_id").and_then(|v| v.as_str()) {
                    if let Ok(Some(c)) = state.db.get_contact(did) {
                        if let Some(m) = c.mac_address {
                            mac = m;
                        }
                        if host.is_none() {
                            host = c.last_known_ip;
                        }
                    }
                }
            }
            if mac.trim().is_empty() {
                return tool_err("mac_address or a device_id with a saved MAC address is required");
            }
            let port = args.get("port").and_then(|v| v.as_u64()).map(|p| p as u16);
            match crate::commands::send_wake_on_lan(mac.clone(), host, port).await {
                Ok(()) => tool_ok(json!({ "status": "sent", "mac_address": mac })),
                Err(e) => tool_err(&e),
            }
        }
        "raydrop_start_download" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            if url.is_empty() {
                return tool_err("url is required");
            }
            let filename = args.get("filename").and_then(|v| v.as_str()).map(|s| s.to_string());
            let save_dir = args.get("save_dir").and_then(|v| v.as_str()).map(|s| s.to_string());
            if url.starts_with("magnet:?") {
                match state.torrent_manager.add_magnet(&url, save_dir).await {
                    Ok(info) => tool_ok(json!({ "type": "torrent", "task": info })),
                    Err(e) => tool_err(&e),
                }
            } else {
                match state.http_downloader.add_download(&url, save_dir, filename).await {
                    Ok(task) => {
                        let _ = app.emit("http_download_added", &task);
                        tool_ok(json!({ "type": "http", "task": task }))
                    }
                    Err(e) => tool_err(&e),
                }
            }
        }
        "raydrop_list_downloads" => {
            let http = state.http_downloader.list_tasks().await;
            let torrents = state.torrent_manager.list_torrents().await;
            tool_ok(json!({ "http_downloads": http, "torrents": torrents }))
        }
        "raydrop_clipboard" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("read");
            match action {
                "read" => match crate::commands::read_system_clipboard() {
                    Ok(content) => tool_ok(json!({ "content": content })),
                    Err(e) => tool_err(&e),
                },
                "write" => {
                    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    match crate::commands::write_system_clipboard(content) {
                        Ok(()) => tool_ok(json!({ "status": "ok" })),
                        Err(e) => tool_err(&e),
                    }
                }
                "history" => {
                    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
                    match state.db.list_clipboard_history(limit) {
                        Ok(items) => tool_ok(json!({ "history": items })),
                        Err(e) => tool_err(&e),
                    }
                }
                _ => tool_err("action must be read, write, or history"),
            }
        }
        "raydrop_notes" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list");
            match action {
                "list" => {
                    let include_archived = args.get("include_archived").and_then(|v| v.as_bool()).unwrap_or(false);
                    match state.db.list_notes(include_archived) {
                        Ok(notes) => tool_ok(json!({ "notes": notes })),
                        Err(e) => tool_err(&e),
                    }
                }
                "save" => {
                    let now = crate::db::storage::now_unix_secs();
                    let note = crate::db::storage::NoteItem {
                        id: args.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_default(),
                        title: args.get("title").and_then(|v| v.as_str()).unwrap_or("AI Note").to_string(),
                        content: args.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        note_type: "text".to_string(),
                        checklist: vec![],
                        images: vec![],
                        color: "default".to_string(),
                        labels: vec![],
                        pinned: args.get("pinned").and_then(|v| v.as_bool()).unwrap_or(false),
                        archived: false,
                        created_at: now,
                        updated_at: now,
                    };
                    match state.db.upsert_note(note) {
                        Ok(saved) => tool_ok(json!({ "note": saved })),
                        Err(e) => tool_err(&e),
                    }
                }
                _ => tool_err("action must be list or save"),
            }
        }
        "raydrop_list_monitors" => {
            let monitors = crate::remote_input::list_monitors();
            let vdd = crate::commands::get_virtual_display_status().await.ok();
            tool_ok(json!({ "monitors": monitors, "virtual_display": vdd }))
        }
        "raydrop_remote_input" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
            let res = match action {
                "mouse_move" => {
                    let x = args.get("x").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                    let y = args.get("y").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                    crate::remote_input::simulate_mouse_move(x, y)
                }
                "mouse_click" => {
                    let btn = args.get("button").and_then(|v| v.as_str()).unwrap_or("left");
                    let ca = args.get("click_action").and_then(|v| v.as_str()).unwrap_or("click");
                    if ca == "click" {
                        crate::remote_input::simulate_mouse_click(btn, "down", None, None, None, None)
                            .and_then(|_| crate::remote_input::simulate_mouse_click(btn, "up", None, None, None, None))
                    } else {
                        crate::remote_input::simulate_mouse_click(btn, ca, None, None, None, None)
                    }
                }
                "mouse_wheel" => {
                    let dy = args.get("delta_y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    crate::remote_input::simulate_mouse_wheel(dy)
                }
                "key" => {
                    let k = args.get("key").and_then(|v| v.as_str()).unwrap_or("");
                    let c = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
                    let ka = args.get("key_action").and_then(|v| v.as_str()).unwrap_or("down");
                    crate::remote_input::simulate_keyboard_key(k, c, ka)
                }
                "shortcut" => {
                    let sc = args.get("shortcut").and_then(|v| v.as_str()).unwrap_or("");
                    crate::remote_input::simulate_shortcut(sc)
                }
                _ => Err("Invalid remote_input action".to_string()),
            };
            match res {
                Ok(()) => tool_ok(json!({ "status": "ok" })),
                Err(e) => tool_err(&e),
            }
        }
        "raydrop_convert_images" => {
            let input_paths: Vec<String> = args
                .get("input_paths")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let target_format = args.get("target_format").and_then(|v| v.as_str()).unwrap_or("png").to_string();
            let resize_width = args.get("resize_width").and_then(|v| v.as_u64()).map(|v| v as u32);
            let resize_height = args.get("resize_height").and_then(|v| v.as_u64()).map(|v| v as u32);
            let keep_aspect = args.get("keep_aspect").and_then(|v| v.as_bool()).unwrap_or(true);
            let quality = args.get("quality").and_then(|v| v.as_u64()).map(|v| v as u8);
            let output_dir = args.get("output_dir").and_then(|v| v.as_str()).map(|s| s.to_string());
            match tokio::task::spawn_blocking(move || {
                crate::image_tools::convert_and_process_images(
                    input_paths,
                    target_format,
                    resize_width,
                    resize_height,
                    keep_aspect,
                    quality,
                    output_dir,
                )
            })
            .await
            {
                Ok(Ok(results)) => tool_ok(json!({ "results": results })),
                Ok(Err(e)) => tool_err(&e),
                Err(e) => tool_err(&format!("Image conversion join error: {e}")),
            }
        }
        "raydrop_web_debug" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("get_audit");
            match action {
                "speedtest" => {
                    let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    match super::run_web_speedtest(url).await {
                        Ok(res) => tool_ok(res),
                        Err(e) => tool_err(&e),
                    }
                }
                "get_audit" => tool_ok(json!({ "audit": super::get_latest_web_audit() })),
                "get_events" => {
                    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
                    tool_ok(json!({ "events": super::get_recent_web_events(limit) }))
                }
                "clear" => {
                    super::clear_web_debug_cache();
                    tool_ok(json!({ "status": "cleared" }))
                }
                _ => tool_err("action must be speedtest, get_audit, get_events, or clear"),
            }
        }
        "raydrop_convert_documents" => {
            let input_paths: Vec<String> = args
                .get("input_paths")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let target_format = args.get("target_format").and_then(|v| v.as_str()).unwrap_or("pdf").to_string();
            let output_dir = args.get("output_dir").and_then(|v| v.as_str()).map(|s| s.to_string());
            match tokio::task::spawn_blocking(move || {
                crate::image_tools::convert_documents(input_paths, target_format, output_dir)
            })
            .await
            {
                Ok(Ok(results)) => tool_ok(json!({ "results": results })),
                Ok(Err(e)) => tool_err(&e),
                Err(e) => tool_err(&format!("Document conversion join error: {e}")),
            }
        }
        "raydrop_generate_mobile_icons" => {
            let input_path = args.get("input_path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let platform = args.get("platform").and_then(|v| v.as_str()).unwrap_or("all").to_string();
            let output_zip_path = args.get("output_zip_path").and_then(|v| v.as_str()).map(|s| s.to_string());
            match tokio::task::spawn_blocking(move || {
                crate::image_tools::generate_mobile_icons_zip(input_path, platform, output_zip_path)
            })
            .await
            {
                Ok(Ok(res)) => tool_ok(json!({ "result": res })),
                Ok(Err(e)) => tool_err(&e),
                Err(e) => tool_err(&format!("Icon generator join error: {e}")),
            }
        }
        "raydrop_folder_sync" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list");
            match action {
                "list" => match state.db.list_folder_sync_tasks() {
                    Ok(tasks) => tool_ok(json!({ "tasks": tasks })),
                    Err(e) => tool_err(&e),
                },
                "trigger" => {
                    let task_id = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                    match state.db.get_folder_sync_task(task_id) {
                        Ok(Some(task)) => {
                            match crate::network::transport::initiate_peer_transfer(
                                app.clone(),
                                state.clone(),
                                task.target_device_id,
                                vec![task.local_path],
                            )
                            .await
                            {
                                Ok(sid) => {
                                    let _ = state.db.update_folder_sync_last_time(task_id, crate::db::storage::now_unix_secs());
                                    tool_ok(json!({ "status": "triggered", "session_id": sid, "task_id": task_id }))
                                }
                                Err(e) => tool_err(&e),
                            }
                        }
                        Ok(None) => tool_err("Folder sync task not found"),
                        Err(e) => tool_err(&e),
                    }
                }
                "toggle_auto" => {
                    let task_id = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                    let enabled = args.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                    match state.db.update_folder_sync_auto_flag(task_id, enabled) {
                        Ok(()) => {
                            if enabled {
                                if let Ok(Some(task)) = state.db.get_folder_sync_task(task_id) {
                                    let _ = state.folder_sync_watcher.start_watching(
                                        task.id,
                                        task.local_path,
                                        task.target_device_id,
                                        app.clone(),
                                        state.clone(),
                                    );
                                }
                            } else {
                                state.folder_sync_watcher.stop_watching(task_id);
                            }
                            tool_ok(json!({ "status": "updated", "task_id": task_id, "enabled": enabled }))
                        }
                        Err(e) => tool_err(&e),
                    }
                }
                "delete" => {
                    let task_id = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                    state.folder_sync_watcher.stop_watching(task_id);
                    match state.db.delete_folder_sync_task(task_id) {
                        Ok(()) => tool_ok(json!({ "status": "deleted", "task_id": task_id })),
                        Err(e) => tool_err(&e),
                    }
                }
                _ => tool_err("Invalid folder_sync action"),
            }
        }
        "raydrop_remote_files" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list_local_shares");
            match action {
                "list_local_shares" => match state.db.list_shared_folders() {
                    Ok(shares) => tool_ok(json!({ "shares": shares })),
                    Err(e) => tool_err(&e),
                },
                "add_local_share" => {
                    let folder_path = args.get("folder_path").and_then(|v| v.as_str()).unwrap_or("");
                    let alias = args.get("alias").and_then(|v| v.as_str()).unwrap_or("Shared Folder");
                    let share = crate::db::storage::SharedFolder {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: alias.to_string(),
                        local_path: folder_path.to_string(),
                        is_active: true,
                        created_at: crate::db::storage::now_unix_secs(),
                    };
                    match state.db.insert_shared_folder(&share) {
                        Ok(()) => tool_ok(json!({ "share": share })),
                        Err(e) => tool_err(&e),
                    }
                }
                "delete_local_share" => {
                    let share_id = args.get("share_id").and_then(|v| v.as_str()).unwrap_or("");
                    match state.db.delete_shared_folder(share_id) {
                        Ok(()) => tool_ok(json!({ "status": "deleted" })),
                        Err(e) => tool_err(&e),
                    }
                }
                "browse_peer_shares" => {
                    let peer = args.get("peer_device_id").and_then(|v| v.as_str()).unwrap_or("");
                    let req = json!({ "action": "list_shares" });
                    match crate::remote_fs::send_remote_fs_request(app, state, peer, req).await {
                        Ok(resp) => tool_ok(resp),
                        Err(e) => tool_err(&e),
                    }
                }
                "browse_peer_dir" => {
                    let peer = args.get("peer_device_id").and_then(|v| v.as_str()).unwrap_or("");
                    let share_id = args.get("share_id").and_then(|v| v.as_str()).unwrap_or("");
                    let subpath = args.get("subpath").and_then(|v| v.as_str()).unwrap_or("");
                    let req = json!({ "action": "list_dir", "share_id": share_id, "subpath": subpath });
                    match crate::remote_fs::send_remote_fs_request(app, state, peer, req).await {
                        Ok(resp) => tool_ok(resp),
                        Err(e) => tool_err(&e),
                    }
                }
                "pull_file" => {
                    let peer = args.get("peer_device_id").and_then(|v| v.as_str()).unwrap_or("");
                    let share_id = args.get("share_id").and_then(|v| v.as_str()).unwrap_or("");
                    let rel = args.get("relative_path").and_then(|v| v.as_str()).unwrap_or("");
                    let req = json!({ "action": "pull_file", "share_id": share_id, "relative_path": rel });
                    match crate::remote_fs::send_remote_fs_request(app, state, peer, req).await {
                        Ok(resp) => tool_ok(resp),
                        Err(e) => tool_err(&e),
                    }
                }
                _ => tool_err("Invalid remote_files action"),
            }
        }
        "raydrop_web_share" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("start");
            match action {
                "start" => {
                    let paths: Vec<String> = args
                        .get("paths")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    match crate::network::web_share::start_web_share_server(app.clone(), state.clone(), paths).await {
                        Ok(info) => tool_ok(json!({ "web_share": info })),
                        Err(e) => tool_err(&e),
                    }
                }
                "stop" => {
                    crate::network::web_share::stop_web_share_server(state);
                    tool_ok(json!({ "status": "stopped" }))
                }
                _ => tool_err("action must be start or stop"),
            }
        }
        "raydrop_group_chat" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list");
            match action {
                "list" => match state.db.list_chat_groups() {
                    Ok(groups) => tool_ok(json!({ "groups": groups })),
                    Err(e) => tool_err(&e),
                },
                "get" => {
                    let gid = args.get("group_id").and_then(|v| v.as_str()).unwrap_or("");
                    match state.db.get_chat_group(gid) {
                        Ok(group) => tool_ok(json!({ "group": group })),
                        Err(e) => tool_err(&e),
                    }
                }
                "send" => {
                    let gid = args.get("group_id").and_then(|v| v.as_str()).unwrap_or("");
                    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    match state.db.get_chat_group(gid) {
                        Ok(Some(group)) => {
                            let now = crate::db::storage::now_unix_secs();
                            let msg_id = uuid::Uuid::new_v4().to_string();
                            let my_id = state.identity.device_id.clone();
                            let my_alias = state.settings.lock().unwrap().alias_name.clone();
                            let chat_msg = crate::db::storage::ChatMessage {
                                id: msg_id.clone(),
                                peer_device_id: gid.to_string(),
                                direction: "outgoing".to_string(),
                                content: content.to_string(),
                                msg_type: "text".to_string(),
                                transfer_session_id: None,
                                files_json: None,
                                is_read: true,
                                created_at: now,
                                sender_device_id: Some(my_id.clone()),
                                sender_alias: Some(my_alias.clone()),
                            };
                            if let Err(e) = state.db.insert_chat_message(&chat_msg) {
                                return tool_err(&e);
                            }
                            let payload = json!({
                                "id": msg_id,
                                "group_id": gid,
                                "content": content,
                                "msg_type": "text",
                                "from_device_id": my_id,
                                "from_alias": my_alias,
                                "created_at": now
                            })
                            .to_string();
                            let hex_payload = hex::encode(payload.as_bytes());
                            if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
                                for m in group.members {
                                    if m.device_id != my_id {
                                        let pkt = json!({
                                            "type": "RELAY_CONTROL",
                                            "session_id": format!("grp_msg:{}", gid),
                                            "from_device_id": my_id,
                                            "target_device_id": m.device_id,
                                            "payload_json": hex_payload
                                        })
                                        .to_string();
                                        let _ = tx.send(pkt);
                                    }
                                }
                            }
                            let _ = app.emit("chat_message_received", &chat_msg);
                            tool_ok(json!({ "status": "sent", "message": chat_msg }))
                        }
                        Ok(None) => tool_err("Group not found"),
                        Err(e) => tool_err(&e),
                    }
                }
                _ => tool_err("action must be list, get, or send"),
            }
        }
        "raydrop_control_download" => {
            let kind = args.get("kind").and_then(|v| v.as_str()).unwrap_or("http");
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("pause");
            let delete_files = args.get("delete_files").and_then(|v| v.as_bool()).unwrap_or(false);
            let res = match (kind, action) {
                ("http", "pause") => state.http_downloader.pause_task(id).await,
                ("http", "resume") => state.http_downloader.resume_task(id).await,
                ("http", "delete") => state.http_downloader.delete_task(id, delete_files).await,
                ("torrent", "pause") => state.torrent_manager.pause_torrent(id).await,
                ("torrent", "resume") => state.torrent_manager.resume_torrent(id).await,
                ("torrent", "delete") => state.torrent_manager.delete_torrent(id, delete_files).await,
                _ => Err("Invalid kind or action".to_string()),
            };
            match res {
                Ok(()) => tool_ok(json!({ "status": action, "kind": kind, "id": id })),
                Err(e) => tool_err(&e),
            }
        }
        "raydrop_respond_transfer" => {
            let sid = args.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
            let accepted = args.get("accepted").and_then(|v| v.as_bool()).unwrap_or(false);
            let target_directory = args
                .get("target_directory")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(sender) = state.pending_requests.lock().unwrap().remove(sid) {
                let _ = sender.send(crate::state::IncomingRequestDecision {
                    accepted,
                    target_directory,
                });
                tool_ok(json!({ "status": if accepted { "accepted" } else { "rejected" }, "session_id": sid }))
            } else {
                tool_err("Pending transfer request not found or already handled")
            }
        }
        "raydrop_virtual_display" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("status");
            match action {
                "status" => match crate::commands::get_virtual_display_status().await {
                    Ok(st) => tool_ok(json!({ "virtual_display": st })),
                    Err(e) => tool_err(&e),
                },
                "enable" | "disable" => {
                    let en = action == "enable";
                    match crate::commands::set_virtual_display_enabled(app.clone(), en).await {
                        Ok(st) => tool_ok(json!({ "virtual_display": st })),
                        Err(e) => tool_err(&e),
                    }
                }
                _ => tool_err("action must be status, enable, or disable"),
            }
        }
        "raydrop_system_power" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string();
            match crate::commands::execute_system_power_action(action.clone()) {
                Ok(()) => tool_ok(json!({ "status": "executed", "action": action })),
                Err(e) => tool_err(&e),
            }
        }
        "raydrop_take_screenshot" => {
            match tokio::task::spawn_blocking(crate::remote_capture::capture_screenshot_png_base64).await {
                Ok(Ok(b64)) => json!({
                    "content": [{
                        "type": "image",
                        "data": b64,
                        "mimeType": "image/png"
                    }],
                    "isError": false
                }),
                Ok(Err(e)) => tool_err(&e),
                Err(e) => tool_err(&format!("Screenshot task error: {e}")),
            }
        }
        "raydrop_analytics_and_tools" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("analytics");
            match action {
                "analytics" => match state.db.get_transfer_analytics() {
                    Ok(a) => tool_ok(json!({ "analytics": a })),
                    Err(e) => tool_err(&e),
                },
                "hash_file" => {
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    match crate::commands::calculate_file_hash(path).await {
                        Ok(h) => tool_ok(json!({ "sha256": h })),
                        Err(e) => tool_err(&e),
                    }
                }
                "set_speed_limit" => {
                    let mbps = args.get("speed_limit_mbps").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let _ = state.db.set_setting("speed_limit_mbps", &mbps.to_string());
                    if let Ok(mut s) = state.settings.lock() {
                        s.speed_limit_mbps = mbps;
                    }
                    tool_ok(json!({ "speed_limit_mbps": mbps }))
                }
                "set_discoverable" => {
                    let en = args.get("discoverable").and_then(|v| v.as_bool()).unwrap_or(true);
                    let _ = state.db.set_setting("discoverable", if en { "true" } else { "false" });
                    if let Ok(mut s) = state.settings.lock() {
                        s.discoverable = en;
                    }
                    let _ = app.emit("discoverability_changed", en);
                    tool_ok(json!({ "discoverable": en }))
                }
                _ => tool_err("Invalid action"),
            }
        }
        _ => tool_err(&format!("Unknown Raydrop MCP tool: {name}")),
    }
}
