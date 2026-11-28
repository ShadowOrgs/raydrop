use crate::db::storage::{now_unix_secs, Database};
use crate::state::AppState;
use async_trait::async_trait;
use base64::Engine as _;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshGroup {
    pub id: String,
    pub name: String,
    pub color: String,
    pub sort_order: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshServer {
    pub id: String,
    pub group_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub password: String,
    pub private_key_path: String,
    pub notes: String,
    pub last_connected_at: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshWorkspace {
    pub groups: Vec<SshGroup>,
    pub servers: Vec<SshServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshServerHealth {
    pub server_id: String,
    pub kernel: String,
    pub uptime: String,
    pub cpu_cores: u32,
    pub load_1m: f64,
    pub load_5m: f64,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub disk_used_mb: u64,
    pub disk_total_mb: u64,
    pub raw_summary: String,
    pub checked_at: u64,
}

pub enum SshCmd {
    Input(Vec<u8>),
    Resize(u32, u32),
    Close,
}

#[derive(Default)]
pub struct SshManager {
    pub sessions: Mutex<HashMap<String, mpsc::UnboundedSender<SshCmd>>>,
}

pub struct ClientHandler;

#[async_trait]
impl russh::client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub async fn open_authenticated_session(
    server: &SshServer,
) -> Result<russh::client::Handle<ClientHandler>, String> {
    let config = russh::client::Config {
        keepalive_interval: Some(std::time::Duration::from_secs(12)),
        keepalive_max: 6,
        inactivity_timeout: None,
        ..Default::default()
    };
    let addr = (server.host.trim(), server.port);
    let mut session = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        russh::client::connect(Arc::new(config), addr, ClientHandler),
    )
    .await
    .map_err(|_| format!("SSH connection to {}:{} timed out", server.host, server.port))?
    .map_err(|e| format!("SSH connect failed: {}", e))?;

    let authed = if server.auth_type == "key" && !server.private_key_path.trim().is_empty() {
        let pass = if server.password.is_empty() {
            None
        } else {
            Some(server.password.as_str())
        };
        let key = russh_keys::load_secret_key(server.private_key_path.trim(), pass)
            .map_err(|e| format!("Failed to load SSH key '{}': {}", server.private_key_path, e))?;
        session
            .authenticate_publickey(server.username.trim(), Arc::new(key))
            .await
            .map_err(|e| format!("SSH key auth error: {}", e))?
    } else {
        session
            .authenticate_password(server.username.trim(), &server.password)
            .await
            .map_err(|e| format!("SSH password auth error: {}", e))?
    };

    if !authed {
        return Err("SSH authentication failed (check username, password, or key).".to_string());
    }
    Ok(session)
}

pub fn get_server_by_id(db: &Database, id: &str) -> Result<SshServer, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, group_id, name, host, port, username, auth_type, password, private_key_path, notes, last_connected_at, created_at
         FROM ssh_servers WHERE id = ?1",
        params![id],
        |r| {
            Ok(SshServer {
                id: r.get(0)?,
                group_id: r.get(1)?,
                name: r.get(2)?,
                host: r.get(3)?,
                port: r.get::<_, i64>(4)? as u16,
                username: r.get(5)?,
                auth_type: r.get(6)?,
                password: r.get(7)?,
                private_key_path: r.get(8)?,
                notes: r.get(9)?,
                last_connected_at: r.get(10)?,
                created_at: r.get(11)?,
            })
        },
    )
    .map_err(|e| format!("SSH server '{}' not found: {}", id, e))
}

#[tauri::command]
pub fn ssh_list_workspace(state: State<'_, Arc<AppState>>) -> Result<SshWorkspace, String> {
    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    let mut g_stmt = conn
        .prepare("SELECT id, name, color, sort_order, created_at FROM ssh_groups ORDER BY sort_order ASC, name ASC")
        .map_err(|e| e.to_string())?;
    let groups = g_stmt
        .query_map([], |r| {
            Ok(SshGroup {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                sort_order: r.get(3)?,
                created_at: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let mut s_stmt = conn
        .prepare(
            "SELECT id, group_id, name, host, port, username, auth_type, password, private_key_path, notes, last_connected_at, created_at
             FROM ssh_servers ORDER BY name ASC",
        )
        .map_err(|e| e.to_string())?;
    let servers = s_stmt
        .query_map([], |r| {
            Ok(SshServer {
                id: r.get(0)?,
                group_id: r.get(1)?,
                name: r.get(2)?,
                host: r.get(3)?,
                port: r.get::<_, i64>(4)? as u16,
                username: r.get(5)?,
                auth_type: r.get(6)?,
                password: r.get(7)?,
                private_key_path: r.get(8)?,
                notes: r.get(9)?,
                last_connected_at: r.get(10)?,
                created_at: r.get(11)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(SshWorkspace { groups, servers })
}

#[tauri::command]
pub fn ssh_save_group(
    state: State<'_, Arc<AppState>>,
    id: Option<String>,
    name: String,
    color: Option<String>,
) -> Result<SshGroup, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Group name cannot be empty".to_string());
    }
    let gid = id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("sshg_{}", uuid::Uuid::new_v4().simple()));
    let col = color.unwrap_or_else(|| "#06b6d4".to_string());
    let now = now_unix_secs() as i64;
    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO ssh_groups (id, name, color, sort_order, created_at)
         VALUES (?1, ?2, ?3, 0, ?4)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, color = excluded.color",
        params![gid, trimmed, col, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(SshGroup {
        id: gid,
        name: trimmed.to_string(),
        color: col,
        sort_order: 0,
        created_at: now,
    })
}

#[tauri::command]
pub fn ssh_delete_group(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE ssh_servers SET group_id = '' WHERE group_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM ssh_groups WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct SaveSshServerInput {
    pub id: Option<String>,
    pub group_id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
    pub auth_type: Option<String>,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub notes: Option<String>,
}

#[tauri::command]
pub fn ssh_save_server(
    state: State<'_, Arc<AppState>>,
    input: SaveSshServerInput,
) -> Result<SshServer, String> {
    if input.name.trim().is_empty() || input.host.trim().is_empty() || input.username.trim().is_empty() {
        return Err("Name, host, and username are required".to_string());
    }
    let sid = input
        .id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("sshs_{}", uuid::Uuid::new_v4().simple()));
    let now = now_unix_secs() as i64;
    let group_id = input.group_id.unwrap_or_default();
    let port = input.port.unwrap_or(22);
    let auth_type = input.auth_type.unwrap_or_else(|| "password".to_string());
    let password = input.password.unwrap_or_default();
    let key_path = input.private_key_path.unwrap_or_default();
    let notes = input.notes.unwrap_or_default();

    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO ssh_servers (id, group_id, name, host, port, username, auth_type, password, private_key_path, notes, last_connected_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11)
         ON CONFLICT(id) DO UPDATE SET
           group_id = excluded.group_id,
           name = excluded.name,
           host = excluded.host,
           port = excluded.port,
           username = excluded.username,
           auth_type = excluded.auth_type,
           password = excluded.password,
           private_key_path = excluded.private_key_path,
           notes = excluded.notes",
        params![
            sid,
            group_id,
            input.name.trim(),
            input.host.trim(),
            port as i64,
            input.username.trim(),
            auth_type,
            password,
            key_path,
            notes,
            now
        ],
    )
    .map_err(|e| e.to_string())?;

    drop(conn);
    get_server_by_id(&state.db, &sid)
}

#[tauri::command]
pub fn ssh_delete_server(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM ssh_servers WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn ssh_connect(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    server_id: String,
    cols: Option<u32>,
    rows: Option<u32>,
) -> Result<String, String> {
    let server = get_server_by_id(&state.db, &server_id)?;
    let session = open_authenticated_session(&server).await?;

    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("Failed to open SSH channel: {}", e))?;
    let c = cols.unwrap_or(100).max(20);
    let r = rows.unwrap_or(30).max(5);
    channel
        .request_pty(false, "xterm-256color", c, r, 0, 0, &[])
        .await
        .map_err(|e| format!("Failed to allocate PTY: {}", e))?;
    channel
        .request_shell(false)
        .await
        .map_err(|e| format!("Failed to start shell: {}", e))?;

    {
        if let Ok(conn) = state.db.conn.lock() {
            let _ = conn.execute(
                "UPDATE ssh_servers SET last_connected_at = ?1 WHERE id = ?2",
                params![now_unix_secs() as i64, server.id],
            );
        }
    }

    let session_id = format!("sshsess_{}", uuid::Uuid::new_v4().simple());
    let (tx, mut rx) = mpsc::unbounded_channel::<SshCmd>();
    {
        let mut map = state.ssh_manager.sessions.lock().map_err(|e| e.to_string())?;
        map.insert(session_id.clone(), tx);
    }

    let state_bg = state.inner().clone();
    let app_bg = app.clone();
    let sid_bg = session_id.clone();

    tauri::async_runtime::spawn(async move {
        let _keep_session = session;
        let _ = app_bg.emit(
            "ssh_status",
            serde_json::json!({
                "session_id": sid_bg,
                "status": "connected",
                "message": "Connected (Keepalive 12s)"
            }),
        );

        loop {
            tokio::select! {
                cmd_opt = rx.recv() => {
                    match cmd_opt {
                        Some(SshCmd::Input(bytes)) => {
                            if channel.data(&bytes[..]).await.is_err() {
                                break;
                            }
                        }
                        Some(SshCmd::Resize(nc, nr)) => {
                            let _ = channel.window_change(nc.max(20), nr.max(5), 0, 0).await;
                        }
                        Some(SshCmd::Close) | None => {
                            let _ = channel.eof().await;
                            let _ = channel.close().await;
                            break;
                        }
                    }
                }
                msg_opt = channel.wait() => {
                    match msg_opt {
                        Some(russh::ChannelMsg::Data { ref data }) => {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(data.as_ref());
                            let _ = app_bg.emit("ssh_output", serde_json::json!({
                                "session_id": sid_bg,
                                "data_b64": b64,
                            }));
                        }
                        Some(russh::ChannelMsg::ExtendedData { ref data, .. }) => {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(data.as_ref());
                            let _ = app_bg.emit("ssh_output", serde_json::json!({
                                "session_id": sid_bg,
                                "data_b64": b64,
                            }));
                        }
                        Some(russh::ChannelMsg::Eof) | Some(russh::ChannelMsg::Close) | None => {
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Ok(mut map) = state_bg.ssh_manager.sessions.lock() {
            map.remove(&sid_bg);
        }
        let _ = app_bg.emit(
            "ssh_status",
            serde_json::json!({
                "session_id": sid_bg,
                "status": "disconnected",
                "message": "SSH session closed"
            }),
        );
    });

    Ok(session_id)
}

#[tauri::command]
pub fn ssh_send_input(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    data_b64: String,
) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64.as_bytes())
        .map_err(|e| format!("Invalid base64: {}", e))?;
    let map = state.ssh_manager.sessions.lock().map_err(|e| e.to_string())?;
    if let Some(tx) = map.get(&session_id) {
        tx.send(SshCmd::Input(bytes))
            .map_err(|_| "SSH session closed".to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn ssh_resize(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    let map = state.ssh_manager.sessions.lock().map_err(|e| e.to_string())?;
    if let Some(tx) = map.get(&session_id) {
        let _ = tx.send(SshCmd::Resize(cols, rows));
    }
    Ok(())
}

#[tauri::command]
pub fn ssh_disconnect(state: State<'_, Arc<AppState>>, session_id: String) -> Result<(), String> {
    let tx_opt = {
        let mut map = state.ssh_manager.sessions.lock().map_err(|e| e.to_string())?;
        map.remove(&session_id)
    };
    if let Some(tx) = tx_opt {
        let _ = tx.send(SshCmd::Close);
    }
    Ok(())
}

#[tauri::command]
pub async fn ssh_check_server_health(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<SshServerHealth, String> {
    let server = get_server_by_id(&state.db, &server_id)?;
    let session = open_authenticated_session(&server).await?;
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("Failed to open exec channel: {}", e))?;

    let probe_cmd = "echo KERNEL=$(uname -srm); echo UPTIME=$(uptime -p 2>/dev/null || uptime); echo CORES=$(nproc 2>/dev/null || echo 1); echo LOAD=$(cat /proc/loadavg 2>/dev/null | awk '{print $1,$2}'); echo MEM=$(free -m 2>/dev/null | awk '/Mem:/ {print $3,$2}'); echo DISK=$(df -Pm / 2>/dev/null | tail -n 1 | awk '{print $3,$2}')";
    channel
        .exec(true, probe_cmd)
        .await
        .map_err(|e| format!("Failed to execute health probe: {}", e))?;

    let mut output = Vec::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { ref data } => output.extend_from_slice(data.as_ref()),
            russh::ChannelMsg::Eof | russh::ChannelMsg::Close => break,
            _ => {}
        }
    }
    let text = String::from_utf8_lossy(&output).to_string();

    let mut kernel = String::new();
    let mut uptime = String::new();
    let mut cpu_cores = 1u32;
    let mut load_1m = 0.0f64;
    let mut load_5m = 0.0f64;
    let mut ram_used_mb = 0u64;
    let mut ram_total_mb = 0u64;
    let mut disk_used_mb = 0u64;
    let mut disk_total_mb = 0u64;

    for line in text.lines() {
        if let Some(v) = line.strip_prefix("KERNEL=") {
            kernel = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("UPTIME=") {
            uptime = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("CORES=") {
            cpu_cores = v.trim().parse().unwrap_or(1);
        } else if let Some(v) = line.strip_prefix("LOAD=") {
            let parts: Vec<&str> = v.split_whitespace().collect();
            if let Some(a) = parts.first() {
                load_1m = a.parse().unwrap_or(0.0);
            }
            if let Some(b) = parts.get(1) {
                load_5m = b.parse().unwrap_or(0.0);
            }
        } else if let Some(v) = line.strip_prefix("MEM=") {
            let parts: Vec<&str> = v.split_whitespace().collect();
            if let Some(a) = parts.first() {
                ram_used_mb = a.parse().unwrap_or(0);
            }
            if let Some(b) = parts.get(1) {
                ram_total_mb = b.parse().unwrap_or(0);
            }
        } else if let Some(v) = line.strip_prefix("DISK=") {
            let parts: Vec<&str> = v.split_whitespace().collect();
            if let Some(a) = parts.first() {
                disk_used_mb = a.parse().unwrap_or(0);
            }
            if let Some(b) = parts.get(1) {
                disk_total_mb = b.parse().unwrap_or(0);
            }
        }
    }

    Ok(SshServerHealth {
        server_id,
        kernel,
        uptime,
        cpu_cores,
        load_1m,
        load_5m,
        ram_used_mb,
        ram_total_mb,
        disk_used_mb,
        disk_total_mb,
        raw_summary: text.trim().to_string(),
        checked_at: now_unix_secs(),
    })
}
