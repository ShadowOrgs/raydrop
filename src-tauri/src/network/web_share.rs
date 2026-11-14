use crate::network::transport::format_bytes_human;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{AppHandle, Emitter};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tracing::{info, warn};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebShareItem {
    pub id: usize,
    pub name: String,
    pub size_bytes: u64,
    pub size_human: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebShareInfo {
    pub url: String, // Public relay URL by default (works over public internet)
    pub local_url: String, // Local LAN Wi-Fi URL (superfast direct transfer)
    pub share_id: String,
    pub local_ip: String,
    pub port: u16,
    pub total_files: usize,
    pub total_bytes: u64,
    pub total_bytes_human: String,
    pub files: Vec<WebShareItem>,
}

#[derive(Debug, Clone)]
pub struct SharedFileEntry {
    pub id: usize,
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

static ACTIVE_SHARE: std::sync::RwLock<Option<(String, Vec<SharedFileEntry>)>> =
    std::sync::RwLock::new(None);

pub async fn start_web_share_server(
    app: AppHandle,
    state: Arc<AppState>,
    raw_paths: Vec<String>,
) -> Result<WebShareInfo, String> {
    // 1. Collect files & folders
    let mut files: Vec<SharedFileEntry> = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut current_id: usize = 0;

    for raw in raw_paths {
        let p = PathBuf::from(raw);
        if !p.exists() {
            continue;
        }

        if p.is_file() {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string());
            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            total_bytes += size;
            files.push(SharedFileEntry {
                id: current_id,
                name,
                path: p,
                size_bytes: size,
            });
            current_id += 1;
        } else if p.is_dir() {
            for entry in WalkDir::new(&p).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    let ep = entry.path().to_path_buf();
                    let name = entry.file_name().to_string_lossy().to_string();
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    total_bytes += size;
                    files.push(SharedFileEntry {
                        id: current_id,
                        name,
                        path: ep,
                        size_bytes: size,
                    });
                    current_id += 1;
                }
            }
        }
    }

    if files.is_empty() {
        return Err("No valid files or folders selected for Web Share".to_string());
    }

    // 2. Resolve local LAN IP
    let local_ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());

    // 3. Bind TCP port (try 8420 first, then OS dynamic port 0)
    let listener = match TcpListener::bind("0.0.0.0:8420").await {
        Ok(l) => l,
        Err(_) => TcpListener::bind("0.0.0.0:0")
            .await
            .map_err(|e| format!("Failed to bind Web Share server port: {e}"))?,
    };

    let bound_addr = listener
        .local_addr()
        .map_err(|e| format!("Failed to read socket addr: {e}"))?;
    let port = bound_addr.port();
    let local_share_url = format!("http://{local_ip}:{port}");

    // 4. Resolve signaling domain and generate clean Share ID for Over-Internet public access
    let signaling_domain = {
        let s_addr = state.settings.lock().unwrap().stun_addr.clone();
        s_addr
            .trim_start_matches("wss://")
            .trim_start_matches("ws://")
            .split(':')
            .next()
            .unwrap_or("stun.raydrop.web.id")
            .to_string()
    };
    let share_id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
    let public_share_url = format!("https://{signaling_domain}/share/{share_id}");

    // Save active files in memory for streaming
    *ACTIVE_SHARE.write().unwrap() = Some((share_id.clone(), files.clone()));

    // 5. Cancel any previous web share session
    let stop_signal = Arc::new(AtomicBool::new(false));
    {
        let mut active_lock = state.active_web_share.lock().unwrap();
        if let Some(ref prev) = *active_lock {
            prev.store(true, Ordering::Relaxed);
        }
        *active_lock = Some(stop_signal.clone());
    }

    // 6. Spawn background HTTP server task for Local LAN
    let sender_alias = state.settings.lock().unwrap().alias_name.clone();
    let sender_alias_task = sender_alias.clone();
    let files_arc = Arc::new(files.clone());
    let stop_signal_task = stop_signal.clone();
    let app_task = app.clone();

    let log_url = local_share_url.clone();
    tokio::spawn(async move {
        info!("Web Share server active on {log_url}");
        while !stop_signal_task.load(Ordering::Relaxed) {
            let accept_fut = listener.accept();
            tokio::select! {
                res = accept_fut => {
                    match res {
                        Ok((mut socket, peer_addr)) => {
                            let files_ref = files_arc.clone();
                            let sender_name = sender_alias_task.clone();
                            let app_sub = app_task.clone();
                            tokio::spawn(async move {
                                let _ = handle_http_client(&mut socket, peer_addr, &files_ref, &sender_name, &app_sub).await;
                            });
                        }
                        Err(e) => {
                            warn!("Web Share listener accept error: {e}");
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                    if stop_signal_task.load(Ordering::Relaxed) {
                        break;
                    }
                }
            }
        }
        info!("Web Share server on port {port} stopped.");
    });

    let items_for_ui: Vec<WebShareItem> = files
        .iter()
        .map(|f| WebShareItem {
            id: f.id,
            name: f.name.clone(),
            size_bytes: f.size_bytes,
            size_human: format_bytes_human(f.size_bytes),
        })
        .collect();

    // 7. Register Public Web Share Session with Signaling Server (Over-Internet access)
    let reg_msg = serde_json::json!({
        "type": "REGISTER_WEB_SHARE",
        "share_id": share_id.clone(),
        "sender_device_id": state.identity.device_id,
        "sender_alias": sender_alias,
        "files": items_for_ui,
        "total_bytes": total_bytes
    })
    .to_string();

    if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let _ = tx.send(reg_msg);
    }

    Ok(WebShareInfo {
        url: public_share_url,
        local_url: local_share_url,
        share_id,
        local_ip,
        port,
        total_files: files.len(),
        total_bytes,
        total_bytes_human: format_bytes_human(total_bytes),
        files: items_for_ui,
    })
}

pub fn stop_web_share_server(state: &AppState) {
    let mut lock = state.active_web_share.lock().unwrap();
    if let Some(ref s) = *lock {
        s.store(true, Ordering::Relaxed);
    }
    *lock = None;

    if let Some((old_share_id, _)) = ACTIVE_SHARE.write().unwrap().take() {
        let unreg_msg = serde_json::json!({
            "type": "UNREGISTER_WEB_SHARE",
            "share_id": old_share_id,
            "sender_device_id": state.identity.device_id
        })
        .to_string();
        if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
            let _ = tx.send(unreg_msg);
        }
    }
}

pub async fn handle_relay_download_request(
    app: AppHandle,
    state: Arc<AppState>,
    download_id: String,
    share_id: String,
    file_id: usize,
) {
    let active_opt = ACTIVE_SHARE.read().unwrap().clone();
    let (_cur_share_id, files) = match active_opt {
        Some(pair) if pair.0 == share_id => pair,
        _ => {
            send_relay_error(&state, &download_id, "Web Share session not active").await;
            return;
        }
    };

    let target_file = match files.iter().find(|f| f.id == file_id) {
        Some(f) => f.clone(),
        None => {
            send_relay_error(&state, &download_id, "File not found").await;
            return;
        }
    };

    let mut f = match tokio::fs::File::open(&target_file.path).await {
        Ok(file) => file,
        Err(e) => {
            send_relay_error(&state, &download_id, &format!("Cannot open file: {e}")).await;
            return;
        }
    };

    let _ = app.emit(
        "web_share_download_started",
        serde_json::json!({
            "peer_ip": "🌐 Over Internet (Public Relay)",
            "file_id": file_id,
            "file_name": target_file.name,
            "file_size": target_file.size_bytes,
        }),
    );

    use base64::Engine;
    let mut buf = vec![0u8; 64 * 1024]; // 64 KB chunk
    loop {
        match f.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
                let chunk_msg = serde_json::json!({
                    "type": "WEB_SHARE_CHUNK",
                    "download_id": download_id.clone(),
                    "data_base64": b64
                })
                .to_string();

                if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
                    if tx.send(chunk_msg).is_err() {
                        break;
                    }
                } else {
                    break;
                }
            }
            Err(e) => {
                send_relay_error(&state, &download_id, &e.to_string()).await;
                return;
            }
        }
    }

    // Send EOF
    let eof_msg = serde_json::json!({
        "type": "WEB_SHARE_EOF",
        "download_id": download_id
    })
    .to_string();

    if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let _ = tx.send(eof_msg);
    }

    let _ = app.emit(
        "web_share_download_completed",
        serde_json::json!({
            "peer_ip": "🌐 Over Internet (Public Relay)",
            "file_id": file_id,
            "file_name": target_file.name,
        }),
    );
}

async fn send_relay_error(state: &AppState, download_id: &str, error: &str) {
    let err_msg = serde_json::json!({
        "type": "WEB_SHARE_ERROR",
        "download_id": download_id,
        "error": error
    })
    .to_string();
    if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let _ = tx.send(err_msg);
    }
}

async fn handle_http_client(
    socket: &mut tokio::net::TcpStream,
    peer_addr: SocketAddr,
    files: &[SharedFileEntry],
    sender_name: &str,
    app: &AppHandle,
) -> Result<(), String> {
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

    let method = parts[0];
    let path = parts[1];

    if method != "GET" && method != "HEAD" {
        let resp = "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = writer_half.write_all(resp.as_bytes()).await;
        return Ok(());
    }

    // Route 1: Homepage (HTML View)
    if path == "/" || path == "/index.html" {
        let html = generate_web_receiver_html(files, sender_name);
        let resp = format!(
            "HTTP/1.1 200 OK\r\n\
            Content-Type: text/html; charset=utf-8\r\n\
            Content-Length: {}\r\n\
            Connection: close\r\n\
            \r\n\
            {}",
            html.len(),
            html
        );
        let _ = writer_half.write_all(resp.as_bytes()).await;
        return Ok(());
    }

    // Route 2: Download File (/download/0, /download/1)
    if let Some(id_str) = path.strip_prefix("/download/") {
        if let Ok(id) = id_str.parse::<usize>() {
            if let Some(target_file) = files.iter().find(|f| f.id == id) {
                if let Ok(mut f) = File::open(&target_file.path).await {
                    let file_size = target_file.size_bytes;
                    let filename = &target_file.name;

                    // Clean percent-encoded filename for standard UTF-8 headers
                    let safe_filename = filename.replace('"', "\\\"");

                    let header = format!(
                        "HTTP/1.1 200 OK\r\n\
                        Content-Type: application/octet-stream\r\n\
                        Content-Length: {}\r\n\
                        Content-Disposition: attachment; filename=\"{}\"\r\n\
                        Access-Control-Allow-Origin: *\r\n\
                        Connection: close\r\n\
                        \r\n",
                        file_size, safe_filename
                    );

                    let _ = writer_half.write_all(header.as_bytes()).await;

                    if method == "GET" {
                        let _ = app.emit("web_share_download_started", serde_json::json!({
                            "peer_ip": peer_addr.ip().to_string(),
                            "file_id": id,
                            "file_name": filename,
                            "file_size": file_size,
                        }));

                        let mut buf = vec![0u8; 64 * 1024];
                        loop {
                            match f.read(&mut buf).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    if writer_half.write_all(&buf[..n]).await.is_err() {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }

                        let _ = app.emit("web_share_download_completed", serde_json::json!({
                            "peer_ip": peer_addr.ip().to_string(),
                            "file_id": id,
                            "file_name": filename,
                        }));
                    }
                    return Ok(());
                }
            }
        }
    }

    // Route 3: Not Found
    let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 13\r\nConnection: close\r\n\r\n404 Not Found";
    let _ = writer_half.write_all(not_found.as_bytes()).await;
    Ok(())
}

fn generate_web_receiver_html(files: &[SharedFileEntry], sender_name: &str) -> String {
    let mut files_html = String::new();
    let total_bytes: u64 = files.iter().map(|f| f.size_bytes).sum();
    let total_human = format_bytes_human(total_bytes);

    for f in files {
        let size_str = format_bytes_human(f.size_bytes);
        files_html.push_str(&format!(
            r#"<div class="file-card">
                <div class="file-info">
                  <div class="file-icon">📄</div>
                  <div class="file-meta">
                    <div class="file-name">{}</div>
                    <div class="file-size">{}</div>
                  </div>
                </div>
                <a href="/download/{}" download class="btn-dl">
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
                    <polyline points="7 10 12 15 17 10"></polyline>
                    <line x1="12" y1="15" x2="12" y2="3"></line>
                  </svg>
                  <span>Download</span>
                </a>
              </div>"#,
            f.name, size_str, f.id
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
  <title>Raydrop - Download Files</title>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; }}
    body {{ background: #070b13; color: #f1f5f9; min-height: 100vh; display: flex; flex-direction: column; align-items: center; padding: 24px 16px; }}
    .container {{ width: 100%; max-width: 520px; display: flex; flex-direction: column; gap: 20px; }}
    .header {{ text-align: center; margin-top: 12px; }}
    .logo-badge {{ display: inline-flex; align-items: center; gap: 8px; background: rgba(56, 189, 248, 0.12); border: 1px solid rgba(56, 189, 248, 0.3); padding: 6px 14px; border-radius: 999px; color: #38bdf8; font-size: 13px; font-weight: 700; letter-spacing: 0.03em; margin-bottom: 12px; }}
    .pulse {{ width: 8px; height: 8px; border-radius: 999px; background: #38bdf8; box-shadow: 0 0 12px #38bdf8; }}
    h1 {{ font-size: 24px; font-weight: 800; color: #ffffff; letter-spacing: -0.02em; }}
    .subtitle {{ font-size: 14px; color: #94a3b8; margin-top: 6px; }}
    .sender-box {{ background: rgba(30, 41, 59, 0.7); border: 1px solid rgba(255, 255, 255, 0.08); border-radius: 16px; padding: 16px; display: flex; justify-content: space-between; align-items: center; backdrop-filter: blur(12px); }}
    .sender-title {{ font-size: 11px; text-transform: uppercase; color: #64748b; font-weight: 700; letter-spacing: 0.05em; }}
    .sender-name {{ font-size: 16px; font-weight: 700; color: #38bdf8; margin-top: 2px; }}
    .total-badge {{ background: #0f172a; border: 1px solid #334155; padding: 6px 12px; border-radius: 10px; font-size: 13px; font-weight: 600; color: #cbd5e1; }}
    .files-list {{ display: flex; flex-direction: column; gap: 10px; }}
    .file-card {{ background: rgba(17, 24, 39, 0.9); border: 1px solid rgba(255, 255, 255, 0.07); border-radius: 16px; padding: 14px 16px; display: flex; align-items: center; justify-content: space-between; gap: 12px; box-shadow: 0 4px 20px rgba(0,0,0,0.3); transition: transform 0.2s; }}
    .file-card:active {{ transform: scale(0.98); }}
    .file-info {{ display: flex; align-items: center; gap: 12px; min-width: 0; }}
    .file-icon {{ font-size: 24px; }}
    .file-meta {{ min-width: 0; }}
    .file-name {{ font-size: 14px; font-weight: 600; color: #f8fafc; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 200px; }}
    .file-size {{ font-size: 12px; color: #94a3b8; margin-top: 2px; }}
    .btn-dl {{ display: inline-flex; align-items: center; gap: 6px; background: #0284c7; color: #ffffff; text-decoration: none; padding: 9px 15px; border-radius: 12px; font-size: 13px; font-weight: 700; shrink-0; box-shadow: 0 4px 14px rgba(2, 132, 199, 0.4); }}
    .btn-dl:hover {{ background: #0369a1; }}
    .btn-dl-all {{ display: flex; align-items: center; justify-content: center; gap: 8px; width: 100%; background: linear-gradient(135deg, #0284c7, #6366f1); color: #fff; text-decoration: none; padding: 14px; border-radius: 16px; font-size: 15px; font-weight: 800; border: none; cursor: pointer; box-shadow: 0 10px 25px rgba(2, 132, 199, 0.35); }}
    .btn-dl-all:active {{ transform: scale(0.98); }}
    .footer-note {{ text-align: center; font-size: 12px; color: #64748b; line-height: 1.6; margin-top: 10px; }}
    .secure-pill {{ display: inline-flex; align-items: center; gap: 4px; color: #10b981; font-weight: 600; }}
  </style>
</head>
<body>
  <div class="container">
    <div class="header">
      <div class="logo-badge"><span class="pulse"></span> Raydrop Web Share</div>
      <h1>Download Files</h1>
      <p class="subtitle">Direct Wi-Fi transfer &bull; No app needed</p>
    </div>

    <div class="sender-box">
      <div>
        <div class="sender-title">Sender PC</div>
        <div class="sender-name">{}</div>
      </div>
      <div class="total-badge">{} files &bull; {}</div>
    </div>

    <button onclick="downloadAll()" class="btn-dl-all">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
        <polyline points="7 10 12 15 17 10"></polyline>
        <line x1="12" y1="15" x2="12" y2="3"></line>
      </svg>
      <span>Download All Files</span>
    </button>

    <div class="files-list">
      {}
    </div>

    <div class="footer-note">
      <span class="secure-pill">🔒 Direct Local Wi-Fi Connection</span><br>
      Files are streamed peer-to-peer directly from the sender's PC. Zero data is uploaded to third-party cloud servers.
    </div>
  </div>

  <script>
    async function downloadAll() {{
      const links = document.querySelectorAll('.btn-dl');
      for (let i = 0; i < links.length; i++) {{
        const link = links[i];
        const a = document.createElement('a');
        a.href = link.href;
        a.download = '';
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        await new Promise(r => setTimeout(r, 600));
      }}
    }}
  </script>
</body>
</html>"#,
        sender_name,
        files.len(),
        total_human,
        files_html
    )
}
