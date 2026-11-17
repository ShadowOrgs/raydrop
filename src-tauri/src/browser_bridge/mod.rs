use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tracing::{error, info, warn};

pub const BROWSER_BRIDGE_PORT: u16 = 45321;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtensionDownloadRequest {
    pub url: String,
    pub filename: Option<String>,
    pub referrer: Option<String>,
    pub connections: Option<u32>,
}

pub fn start_browser_bridge_daemon(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let addr = format!("127.0.0.1:{}", BROWSER_BRIDGE_PORT);
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => {
                info!("Raydrop Browser Extension Bridge listening on http://{}", addr);
                l
            }
            Err(e) => {
                warn!("Could not bind Browser Extension Bridge on {}: {}", addr, e);
                return;
            }
        };

        loop {
            match listener.accept().await {
                Ok((mut socket, _)) => {
                    let app_c = app.clone();
                    let state_c = state.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = handle_extension_client(&mut socket, app_c, state_c).await;
                    });
                }
                Err(e) => {
                    warn!("Browser Extension Bridge accept error: {}", e);
                    break;
                }
            }
        }
    });
}

async fn handle_extension_client(
    socket: &mut tokio::net::TcpStream,
    app: AppHandle,
    state: Arc<AppState>,
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

    let method = parts[0];
    let path = parts[1];

    // Read HTTP headers
    let mut content_length: usize = 0;
    loop {
        let mut header_line = String::new();
        if buf_reader.read_line(&mut header_line).await.is_err() {
            break;
        }
        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(pos) = trimmed.to_lowercase().find("content-length:") {
            let val = trimmed[pos + 15..].trim();
            if let Ok(cl) = val.parse::<usize>() {
                content_length = cl;
            }
        }
    }

    // Handle CORS Preflight
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

    // Route 1: Ping / Health Check
    if path == "/api/ping" {
        let json_body = serde_json::json!({
            "status": "ok",
            "app": "raydrop",
            "version": "1.2.14",
            "capabilities": ["http_segmented_accelerator", "torrent_magnet"]
        })
        .to_string();

        let resp = format!(
            "HTTP/1.1 200 OK\r\n\
Content-Type: application/json\r\n\
Access-Control-Allow-Origin: *\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
            json_body.len(),
            json_body
        );
        let _ = writer_half.write_all(resp.as_bytes()).await;
        return Ok(());
    }

    // Route 2: Add Download
    if path == "/api/download" && method == "POST" {
        if content_length > 64 * 1024 {
            let resp = "HTTP/1.1 413 Payload Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = writer_half.write_all(resp.as_bytes()).await;
            return Ok(());
        }

        let mut body_bytes = vec![0u8; content_length];
        if content_length > 0 {
            let _ = buf_reader.read_exact(&mut body_bytes).await;
        }

        let req: ExtensionDownloadRequest = match serde_json::from_slice(&body_bytes) {
            Ok(r) => r,
            Err(e) => {
                let err_json = serde_json::json!({ "error": format!("Invalid JSON payload: {e}") }).to_string();
                let resp = format!(
                    "HTTP/1.1 400 Bad Request\r\n\
Content-Type: application/json\r\n\
Access-Control-Allow-Origin: *\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
                    err_json.len(),
                    err_json
                );
                let _ = writer_half.write_all(resp.as_bytes()).await;
                return Ok(());
            }
        };

        let trimmed_url = req.url.trim().to_string();

        // Bring Raydrop window into focus
        crate::network::transport::popup_and_focus_main_window(&app);

        if trimmed_url.starts_with("magnet:?") {
            info!("Received magnet link from browser extension: {}", trimmed_url);
            if let Ok(mut pending) = state.pending_magnet_link.lock() {
                *pending = Some(trimmed_url.clone());
            }
            let _ = app.emit("open_magnet_link", &trimmed_url);

            let ok_json = serde_json::json!({
                "status": "ok",
                "type": "magnet",
                "message": "Magnet link dispatched to Raydrop"
            })
            .to_string();

            let resp = format!(
                "HTTP/1.1 200 OK\r\n\
Content-Type: application/json\r\n\
Access-Control-Allow-Origin: *\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
                ok_json.len(),
                ok_json
            );
            let _ = writer_half.write_all(resp.as_bytes()).await;
            return Ok(());
        }

        // HTTP/HTTPS URL - extract clean filename if provided and pass as custom_filename (not save_dir!)
        let clean_filename = req.filename.as_ref().and_then(|f| {
            let p = std::path::Path::new(f);
            p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
        });

        // IDM-Style Feature: Check if an existing task for this exact file is paused or errored
        // If so, seamlessly refresh its download address with the new session link instead of duplicating
        let mut refreshed_task = None;
        if let Some(ref fname) = clean_filename {
            if let Some(matching_id) = state.http_downloader.find_matching_refresh_task(fname).await {
                info!("Found existing matching task {} for {}; refreshing download address with new URL", matching_id, fname);
                if let Ok(updated_task) = state.http_downloader.update_download_url(&matching_id, &trimmed_url).await {
                    let _ = app.emit("http_download_added", &updated_task);
                    refreshed_task = Some(updated_task);
                }
            }
        }

        if let Some(task_info) = refreshed_task {
            let ok_json = serde_json::json!({
                "status": "ok",
                "type": "http",
                "id": task_info.id,
                "filename": task_info.filename,
                "message": "Download address refreshed and resumed seamlessly"
            })
            .to_string();

            let resp = format!(
                "HTTP/1.1 200 OK\r\n\
Access-Control-Allow-Origin: *\r\n\
Content-Type: application/json\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
                ok_json.len(),
                ok_json
            );
            let _ = writer_half.write_all(resp.as_bytes()).await;
            return Ok(());
        }

        match state.http_downloader.add_download(&trimmed_url, None, clean_filename.clone()).await {
            Ok(task_info) => {
                info!("Browser extension download started: {} (id: {})", task_info.url, task_info.id);
                let _ = app.emit("http_download_added", &task_info);

                let ok_json = serde_json::json!({
                    "status": "ok",
                    "type": "http",
                    "id": task_info.id,
                    "filename": task_info.filename,
                    "message": "Accelerated download started in Raydrop"
                })
                .to_string();

                let resp = format!(
                    "HTTP/1.1 200 OK\r\n\
Content-Type: application/json\r\n\
Access-Control-Allow-Origin: *\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
                    ok_json.len(),
                    ok_json
                );
                let _ = writer_half.write_all(resp.as_bytes()).await;
            }
            Err(e) => {
                error!("Download initiation error: {e}");

                // Dispatch native OS notification to alert the user immediately
                use tauri_plugin_notification::NotificationExt;
                let notif_title = if e.contains("Insufficient disk space") {
                    "Raydrop: Insufficient Disk Space"
                } else {
                    "Raydrop: Download Error"
                };
                let display_fname = clean_filename.unwrap_or_else(|| "Requested file".to_string());
                let _ = app
                    .notification()
                    .builder()
                    .title(notif_title)
                    .body(format!("{display_fname}: {e}"))
                    .show();

                let err_json = serde_json::json!({ "error": format!("Failed to initiate download: {e}") }).to_string();
                let resp = format!(
                    "HTTP/1.1 500 Internal Server Error\r\n\
Content-Type: application/json\r\n\
Access-Control-Allow-Origin: *\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
                    err_json.len(),
                    err_json
                );
                let _ = writer_half.write_all(resp.as_bytes()).await;
            }
        }

        return Ok(());
    }

    // Default 404
    let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let _ = writer_half.write_all(not_found.as_bytes()).await;
    Ok(())
}
