//! Raydrop Second Display Local Web Server (iPad / Tablet / Phone via QR Code)
//! Serves a zero-install, low-latency MJPEG + Touch-to-Mouse web viewer over LAN HTTP
//! so any iPad (Safari), Android tablet, or phone can act as a wireless second display.

use base64::Engine;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;

#[derive(Debug, Clone, Serialize)]
pub struct SecondScreenInfo {
    pub url: String,
    pub token: String,
    pub port: u16,
}

pub struct SecondScreenState {
    pub active_flag: Mutex<Option<Arc<AtomicBool>>>,
    pub frame_tx: Mutex<Option<watch::Sender<Vec<u8>>>>,
}

impl SecondScreenState {
    pub fn new() -> Self {
        Self {
            active_flag: Mutex::new(None),
            frame_tx: Mutex::new(None),
        }
    }

    pub fn push_frame_b64(&self, b64: &str) {
        if let Ok(guard) = self.frame_tx.lock() {
            if let Some(tx) = guard.as_ref() {
                let raw = b64.strip_prefix("data:image/jpeg;base64,").unwrap_or(b64);
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(raw) {
                    let _ = tx.send(bytes);
                }
            }
        }
    }

    pub fn stop(&self) {
        if let Ok(mut guard) = self.active_flag.lock() {
            if let Some(flag) = guard.take() {
                flag.store(false, Ordering::SeqCst);
            }
        }
        if let Ok(mut tx_guard) = self.frame_tx.lock() {
            *tx_guard = None;
        }
    }
}

pub async fn start_second_screen_server(
    second_screen: Arc<SecondScreenState>,
) -> Result<SecondScreenInfo, String> {
    // Stop any previous server
    second_screen.stop();

    let local_ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());

    let listener = match TcpListener::bind("0.0.0.0:53319").await {
        Ok(l) => l,
        Err(_) => TcpListener::bind("0.0.0.0:0")
            .await
            .map_err(|e| format!("Failed to bind Second Screen HTTP server: {e}"))?,
    };

    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to read bound port: {e}"))?
        .port();

    let token = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    let url = format!("http://{local_ip}:{port}/?token={token}");

    let active_flag = Arc::new(AtomicBool::new(true));
    let (frame_tx, frame_rx) = watch::channel::<Vec<u8>>(Vec::new());

    *second_screen.active_flag.lock().unwrap() = Some(active_flag.clone());
    *second_screen.frame_tx.lock().unwrap() = Some(frame_tx);

    let token_clone = token.clone();
    tokio::spawn(async move {
        loop {
            if !active_flag.load(Ordering::Relaxed) {
                break;
            }
            let accept_res = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                listener.accept(),
            )
            .await;

            let (mut socket, _) = match accept_res {
                Ok(Ok(conn)) => conn,
                Ok(Err(_)) => break,
                Err(_) => continue, // timeout, check active_flag
            };

            let tok = token_clone.clone();
            let mut rx = frame_rx.clone();
            let flag = active_flag.clone();

            tokio::spawn(async move {
                let mut buf = [0u8; 8192];
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let req_str = String::from_utf8_lossy(&buf[..n]);
                let first_line = req_str.lines().next().unwrap_or("");
                let mut parts = first_line.split_whitespace();
                let method = parts.next().unwrap_or("");
                let full_path = parts.next().unwrap_or("/");

                let (path, query) = match full_path.split_once('?') {
                    Some((p, q)) => (p, q),
                    None => (full_path, ""),
                };

                // Simple token verification
                let has_valid_token = query
                    .split('&')
                    .any(|kv| kv == format!("token={tok}"));

                if !has_valid_token {
                    let resp = "HTTP/1.1 403 Forbidden\r\nContent-Length: 9\r\n\r\nForbidden";
                    let _ = socket.write_all(resp.as_bytes()).await;
                    return;
                }

                if method == "GET" && (path == "/" || path == "/screen") {
                    let html = build_viewer_html(&tok);
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\r\n",
                        html.len()
                    );
                    let _ = socket.write_all(header.as_bytes()).await;
                    let _ = socket.write_all(html.as_bytes()).await;
                    return;
                }

                if method == "GET" && path == "/stream" {
                    let header = "HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=raydropframe\r\nCache-Control: no-cache, no-store, must-revalidate\r\nConnection: close\r\n\r\n";
                    if socket.write_all(header.as_bytes()).await.is_err() {
                        return;
                    }

                    // Send initial frame if available
                    {
                        let initial = rx.borrow().clone();
                        if !initial.is_empty() {
                            let part_hdr = format!(
                                "--raydropframe\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                                initial.len()
                            );
                            if socket.write_all(part_hdr.as_bytes()).await.is_err()
                                || socket.write_all(&initial).await.is_err()
                                || socket.write_all(b"\r\n").await.is_err()
                            {
                                return;
                            }
                        }
                    }

                    while flag.load(Ordering::Relaxed) {
                        if rx.changed().await.is_err() {
                            break;
                        }
                        let frame = rx.borrow_and_update().clone();
                        if frame.is_empty() {
                            continue;
                        }
                        let part_hdr = format!(
                            "--raydropframe\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                            frame.len()
                        );
                        if socket.write_all(part_hdr.as_bytes()).await.is_err()
                            || socket.write_all(&frame).await.is_err()
                            || socket.write_all(b"\r\n").await.is_err()
                        {
                            break;
                        }
                    }
                    return;
                }

                if method == "POST" && path == "/input" {
                    if let Some((_, body)) = req_str.split_once("\r\n\r\n") {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(body.trim_matches('\0').trim()) {
                            let t = val.get("t").and_then(|v| v.as_str()).unwrap_or("");
                            match t {
                                "mm" => {
                                    let x = val.get("x").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                                    let y = val.get("y").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                                    let _ = crate::remote_input::simulate_mouse_move(x, y);
                                }
                                "mc" => {
                                    let b = val.get("b").and_then(|v| v.as_str()).unwrap_or("left");
                                    let a = val.get("a").and_then(|v| v.as_str()).unwrap_or("click");
                                    let _ = crate::remote_input::simulate_mouse_click(
                                        b, a, Some(false), Some(false), Some(false), Some(false),
                                    );
                                }
                                "mw" => {
                                    let dy = val.get("dy").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                                    let _ = crate::remote_input::simulate_mouse_wheel(dy);
                                }
                                _ => {}
                            }
                        }
                    }
                    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
                    let _ = socket.write_all(resp.as_bytes()).await;
                    return;
                }

                let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nNot Found";
                let _ = socket.write_all(resp.as_bytes()).await;
            });
        }
    });

    Ok(SecondScreenInfo { url, token, port })
}

fn build_viewer_html(token: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no" />
<meta name="apple-mobile-web-app-capable" content="yes" />
<title>Raydrop Second Display</title>
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; user-select: none; -webkit-user-select: none; }}
  html, body {{ width: 100%; height: 100dvh; background: #000; overflow: hidden; touch-action: none; font-family: -apple-system, BlinkMacSystemFont, sans-serif; }}
  #wrap {{ width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; position: relative; }}
  #screen {{ max-width: 100%; max-height: 100%; object-fit: contain; pointer-events: none; }}
  #screen.fill {{ width: 100%; height: 100%; object-fit: fill; }}
  #bar {{ position: fixed; top: 10px; right: 12px; z-index: 20; display: flex; gap: 8px; opacity: 0.75; transition: opacity 0.2s; }}
  #bar:hover {{ opacity: 1; }}
  button {{ background: rgba(15, 23, 42, 0.85); color: #38bdf8; border: 1px solid rgba(56, 189, 248, 0.35); border-radius: 10px; padding: 6px 12px; font-size: 12px; font-weight: 600; backdrop-filter: blur(8px); cursor: pointer; }}
</style>
</head>
<body>
  <div id="bar">
    <button id="fitBtn">Fill Screen</button>
    <button id="fsBtn">Fullscreen</button>
  </div>
  <div id="wrap">
    <img id="screen" src="/stream?token={token}" alt="Raydrop Second Display" />
  </div>
<script>
  const TOKEN = "{token}";
  const wrap = document.getElementById("wrap");
  const img = document.getElementById("screen");
  const fitBtn = document.getElementById("fitBtn");
  const fsBtn = document.getElementById("fsBtn");

  fitBtn.addEventListener("click", (e) => {{
    e.stopPropagation();
    img.classList.toggle("fill");
    fitBtn.textContent = img.classList.contains("fill") ? "Fit Aspect" : "Fill Screen";
  }});

  fsBtn.addEventListener("click", (e) => {{
    e.stopPropagation();
    if (!document.fullscreenElement && document.documentElement.requestFullscreen) {{
      document.documentElement.requestFullscreen().catch(() => {{}});
    }} else if (document.exitFullscreen) {{
      document.exitFullscreen().catch(() => {{}});
    }}
  }});

  function sendInput(payload) {{
    fetch("/input?token=" + TOKEN, {{
      method: "POST",
      headers: {{ "Content-Type": "application/json" }},
      body: JSON.stringify(payload)
    }}).catch(() => {{}});
  }}

  function normCoords(clientX, clientY) {{
    const r = img.getBoundingClientRect();
    if (!r.width || !r.height) return null;
    return {{
      x: Math.max(0, Math.min(1, (clientX - r.left) / r.width)),
      y: Math.max(0, Math.min(1, (clientY - r.top) / r.height))
    }};
  }}

  let lastMove = 0;
  let lastTouchY = 0;

  wrap.addEventListener("touchstart", (e) => {{
    if (e.touches.length === 2) {{
      lastTouchY = (e.touches[0].clientY + e.touches[1].clientY) / 2;
      return;
    }}
    const c = normCoords(e.touches[0].clientX, e.touches[0].clientY);
    if (!c) return;
    sendInput({{ t: "mm", x: c.x, y: c.y }});
    sendInput({{ t: "mc", b: "left", a: "down" }});
  }}, {{ passive: false }});

  wrap.addEventListener("touchmove", (e) => {{
    e.preventDefault();
    if (e.touches.length === 2) {{
      const midY = (e.touches[0].clientY + e.touches[1].clientY) / 2;
      const diff = midY - lastTouchY;
      if (Math.abs(diff) > 12) {{
        sendInput({{ t: "mw", dy: diff > 0 ? -120 : 120 }});
        lastTouchY = midY;
      }}
      return;
    }}
    const now = performance.now();
    if (now - lastMove < 16) return;
    lastMove = now;
    const c = normCoords(e.touches[0].clientX, e.touches[0].clientY);
    if (c) sendInput({{ t: "mm", x: c.x, y: c.y }});
  }}, {{ passive: false }});

  wrap.addEventListener("touchend", (e) => {{
    if (e.touches.length === 0) {{
      sendInput({{ t: "mc", b: "left", a: "up" }});
    }}
  }});

  wrap.addEventListener("mousedown", (e) => {{
    const c = normCoords(e.clientX, e.clientY);
    if (!c) return;
    sendInput({{ t: "mm", x: c.x, y: c.y }});
    sendInput({{ t: "mc", b: e.button === 2 ? "right" : "left", a: "down" }});
  }});
  wrap.addEventListener("mousemove", (e) => {{
    const now = performance.now();
    if (now - lastMove < 16) return;
    lastMove = now;
    const c = normCoords(e.clientX, e.clientY);
    if (c) sendInput({{ t: "mm", x: c.x, y: c.y }});
  }});
  wrap.addEventListener("mouseup", (e) => {{
    sendInput({{ t: "mc", b: e.button === 2 ? "right" : "left", a: "up" }});
  }});
</script>
</body>
</html>"#
    )
}
