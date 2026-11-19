use crate::network::transport::{initiate_peer_transfer, read_json_packet, write_json_packet};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteShareEntry {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFileItem {
    pub name: String,
    pub relative_path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub modified_unix_ms: u64,
}

/// Safely resolve a subpath inside base_dir, strictly preventing path traversal.
pub fn resolve_and_verify_path(base_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let clean_rel = rel.trim_start_matches('/').trim_start_matches('\\');
    let candidate = if clean_rel.is_empty() {
        base_dir.to_path_buf()
    } else {
        base_dir.join(clean_rel)
    };

    let canonical_base = dunce::canonicalize(base_dir)
        .map_err(|e| format!("Base folder inaccessible: {e}"))?;
    
    if !candidate.exists() {
        return Err(format!("Requested path does not exist: {}", candidate.display()));
    }

    let canonical_target = dunce::canonicalize(&candidate)
        .map_err(|e| format!("Target path inaccessible: {e}"))?;

    if !canonical_target.starts_with(&canonical_base) {
        return Err("Security Violation: Path traversal outside shared folder is prohibited".to_string());
    }

    Ok(canonical_target)
}

/// Locate root folder path for a given share_id on the host machine
fn get_share_root_path(state: &AppState, share_id: &str) -> Result<PathBuf, String> {
    if share_id == "default_downloads" {
        let dl_dir = state
            .settings
            .lock()
            .map(|s| s.download_dir.clone())
            .unwrap_or_else(|_| "Downloads".to_string());
        let p = PathBuf::from(dl_dir);
        if !p.exists() {
            let _ = std::fs::create_dir_all(&p);
        }
        return Ok(p);
    }

    if let Ok(Some(shared)) = state.db.get_shared_folder(share_id) {
        let p = PathBuf::from(&shared.local_path);
        if p.exists() && p.is_dir() {
            return Ok(p);
        } else {
            return Err("Shared folder path does not exist on disk".to_string());
        }
    }

    Err("Shared folder not found".to_string())
}

/// Process an incoming FS_REQUEST on the host peer (direct TCP or WAN relay)
pub async fn handle_host_fs_request(
    app: &AppHandle,
    state: &Arc<AppState>,
    req: Value,
) -> Value {
    let req_id = req.get("request_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let sender_id = req.get("sender_device_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let action = req.get("action").and_then(|v| v.as_str()).unwrap_or("");

    // Privacy & Security Check
    if !state.is_discoverable() {
        return json!({
            "request_id": req_id,
            "success": false,
            "error": "Target device is currently set to hidden (Discoverability OFF)"
        });
    }

    if !sender_id.is_empty() {
        if let Ok(Some(contact)) = state.db.get_contact(&sender_id) {
            if contact.trust_level == "blocked" {
                return json!({
                    "request_id": req_id,
                    "success": false,
                    "error": "Access blocked by user policy"
                });
            }
        }
    }

    match action {
        "list_shares" => {
            let mut list = Vec::new();

            // Default Downloads folder
            list.push(RemoteShareEntry {
                id: "default_downloads".to_string(),
                name: "Raydrop Downloads".to_string(),
                is_default: true,
            });

            // Configured shared folders
            if let Ok(shares) = state.db.list_shared_folders() {
                for s in shares {
                    if s.is_active {
                        list.push(RemoteShareEntry {
                            id: s.id,
                            name: s.name,
                            is_default: false,
                        });
                    }
                }
            }

            json!({
                "request_id": req_id,
                "success": true,
                "shares": list
            })
        }

        "list_dir" => {
            let share_id = req.get("share_id").and_then(|v| v.as_str()).unwrap_or("default_downloads");
            let subpath = req.get("subpath").and_then(|v| v.as_str()).unwrap_or("");

            let base_path = match get_share_root_path(state, share_id) {
                Ok(p) => p,
                Err(e) => {
                    return json!({
                        "request_id": req_id,
                        "success": false,
                        "error": e
                    });
                }
            };

            let target_path = match resolve_and_verify_path(&base_path, subpath) {
                Ok(p) => p,
                Err(e) => {
                    return json!({
                        "request_id": req_id,
                        "success": false,
                        "error": e
                    });
                }
            };

            if !target_path.is_dir() {
                return json!({
                    "request_id": req_id,
                    "success": false,
                    "error": "Target path is a file, not a directory"
                });
            }

            let mut entries: Vec<RemoteFileItem> = Vec::new();
            if let Ok(read_dir) = std::fs::read_dir(&target_path) {
                for entry in read_dir.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    // Skip hidden/system dotfiles
                    if file_name.starts_with('.') {
                        continue;
                    }

                    let file_type = entry.file_type().ok();
                    let is_dir = file_type.map(|t| t.is_dir()).unwrap_or(false);
                    let meta = entry.metadata().ok();
                    let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    let modified_unix_ms = meta
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);

                    let entry_rel = if subpath.is_empty() {
                        file_name.clone()
                    } else {
                        format!("{}/{}", subpath.trim_matches('/'), file_name)
                    };

                    entries.push(RemoteFileItem {
                        name: file_name,
                        relative_path: entry_rel,
                        is_dir,
                        size_bytes,
                        modified_unix_ms,
                    });
                }
            }

            // Sort: directories first, then alphabetical by name
            entries.sort_by(|a, b| {
                if a.is_dir == b.is_dir {
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                } else if a.is_dir {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            });

            json!({
                "request_id": req_id,
                "success": true,
                "entries": entries
            })
        }

        "pull_file" => {
            let share_id = req.get("share_id").and_then(|v| v.as_str()).unwrap_or("default_downloads");
            let relative_path = req.get("relative_path").and_then(|v| v.as_str()).unwrap_or("");

            if relative_path.is_empty() {
                return json!({
                    "request_id": req_id,
                    "success": false,
                    "error": "Relative path is required"
                });
            }

            let base_path = match get_share_root_path(state, share_id) {
                Ok(p) => p,
                Err(e) => {
                    return json!({
                        "request_id": req_id,
                        "success": false,
                        "error": e
                    });
                }
            };

            let target_path = match resolve_and_verify_path(&base_path, relative_path) {
                Ok(p) => p,
                Err(e) => {
                    return json!({
                        "request_id": req_id,
                        "success": false,
                        "error": e
                    });
                }
            };

            let path_str = target_path.to_string_lossy().to_string();

            // Initiate Raydrop peer transfer to the requesting peer
            let app_c = app.clone();
            let state_c = state.clone();
            let sender_dev = sender_id.clone();
            let target_path_clone = path_str.clone();

            tauri::async_runtime::spawn(async move {
                info!("Remote file pull accepted for {sender_dev}: {target_path_clone}");
                let _ = initiate_peer_transfer(app_c, state_c, sender_dev, vec![target_path_clone]).await;
            });

            json!({
                "request_id": req_id,
                "success": true,
                "message": format!("Transfer initiated for {}", target_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default())
            })
        }

        _ => json!({
            "request_id": req_id,
            "success": false,
            "error": format!("Unknown remote fs action: {action}")
        }),
    }
}

/// Send a request from local device to a remote peer with LAN direct TCP + WAN relay fallback
pub async fn send_remote_fs_request(
    _app: &AppHandle,
    state: &Arc<AppState>,
    peer_device_id: &str,
    mut req: Value,
) -> Result<Value, String> {
    let req_id = uuid::Uuid::new_v4().to_string();
    if let Some(obj) = req.as_object_mut() {
        obj.insert("type".to_string(), json!("FS_REQUEST"));
        obj.insert("request_id".to_string(), json!(req_id.clone()));
        obj.insert("sender_device_id".to_string(), json!(state.identity.device_id.clone()));
    }

    // 1. Direct LAN TCP Attempt (sub-millisecond when on same WiFi/Tailscale)
    if let Ok(Some(contact)) = state.db.get_contact(peer_device_id) {
        if let (Some(ip), Some(port)) = (contact.last_known_ip, contact.last_known_port) {
            let addr = format!("{}:{}", ip, port);
            if let Ok(Ok(mut stream)) = tokio::time::timeout(
                Duration::from_millis(2000),
                TcpStream::connect(&addr),
            )
            .await
            {
                if write_json_packet(&mut stream, &req).await.is_ok() {
                    if let Ok(Ok(resp)) = tokio::time::timeout(
                        Duration::from_millis(3000),
                        read_json_packet::<_, Value>(&mut stream),
                    )
                    .await
                    {
                        return Ok(resp);
                    }
                }
            }
        }
    }

    // 2. WAN Relay Fallback
    let (tx, rx) = oneshot::channel();
    {
        let mut map = state.pending_fs_requests.lock().unwrap();
        map.insert(req_id.clone(), tx);
    }

    let payload_str = req.to_string();
    let hex_payload = hex::encode(payload_str.as_bytes());

    let sent_via_relay = if let Some(sig_tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let packet = json!({
            "type": "RELAY_CONTROL",
            "session_id": format!("fs:{}", req_id),
            "from_device_id": state.identity.device_id,
            "target_device_id": peer_device_id,
            "payload_json": hex_payload
        })
        .to_string();
        sig_tx.send(packet).is_ok()
    } else {
        false
    };

    if !sent_via_relay {
        let mut map = state.pending_fs_requests.lock().unwrap();
        map.remove(&req_id);
        return Err("Peer is unreachable (Offline or blocked by network)".to_string());
    }

    match tokio::time::timeout(Duration::from_secs(6), rx).await {
        Ok(Ok(resp)) => Ok(resp),
        _ => {
            let mut map = state.pending_fs_requests.lock().unwrap();
            map.remove(&req_id);
            Err("Remote peer request timed out".to_string())
        }
    }
}
