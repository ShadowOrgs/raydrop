//! Raydrop Second Display Local Web Server (iPad / Tablet / Phone via QR Code)
//! Serves a zero-install, low-latency MJPEG + Touch-to-Mouse web viewer over LAN HTTP
//! so any iPad (Safari), Android tablet, or phone can act as a wireless second display.

use base64::Engine;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
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
    pub native_capture_enabled: Arc<AtomicBool>,
    pub native_monitor_index: Arc<AtomicI32>,
}

impl SecondScreenState {
    pub fn new() -> Self {
        Self {
            active_flag: Mutex::new(None),
            frame_tx: Mutex::new(None),
            native_capture_enabled: Arc::new(AtomicBool::new(false)),
            native_monitor_index: Arc::new(AtomicI32::new(0)),
        }
    }

    pub fn set_native_capture(&self, enabled: bool, monitor_index: Option<i32>) {
        if let Some(idx) = monitor_index {
            self.native_monitor_index.store(idx, Ordering::Relaxed);
        }
        self.native_capture_enabled.store(enabled, Ordering::SeqCst);
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
        self.native_capture_enabled.store(false, Ordering::SeqCst);
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
    native_capture: bool,
    monitor_index: i32,
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
    *second_screen.frame_tx.lock().unwrap() = Some(frame_tx.clone());
    second_screen.set_native_capture(native_capture, Some(monitor_index));

    crate::remote_capture::spawn_second_screen_gdi_capture(
        active_flag.clone(),
        second_screen.native_capture_enabled.clone(),
        second_screen.native_monitor_index.clone(),
        frame_tx,
    );

    let token_clone = token.clone();
    let monitor_idx_ref = second_screen.native_monitor_index.clone();
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
            let mon_idx_atomic = monitor_idx_ref.clone();

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
                            let mon_idx = mon_idx_atomic.load(Ordering::Relaxed);
                            match t {
                                "mm" => {
                                    let x = val.get("x").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                                    let y = val.get("y").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                                    let _ = crate::remote_input::simulate_mouse_move_on_monitor(mon_idx, x, y);
                                }
                                "mc" => {
                                    if let (Some(x), Some(y)) = (
                                        val.get("x").and_then(|v| v.as_f64()),
                                        val.get("y").and_then(|v| v.as_f64()),
                                    ) {
                                        let _ = crate::remote_input::simulate_mouse_move_on_monitor(
                                            mon_idx,
                                            x as f32,
                                            y as f32,
                                        );
                                    }
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
  #bar {{ position: fixed; top: 12px; right: 12px; z-index: 20; display: flex; gap: 6px; padding: 6px; border-radius: 14px; background: rgba(15, 23, 42, 0.86); border: 1px solid rgba(56, 189, 248, 0.25); box-shadow: 0 10px 28px rgba(0, 0, 0, 0.55); backdrop-filter: blur(12px); opacity: 0.92; transition: opacity 0.22s ease, transform 0.22s ease; }}
  #bar:hover {{ opacity: 1; }}
  #bar.fs-mode {{ top: 50%; right: 26px; left: auto; flex-direction: column; align-items: stretch; transform: translateY(-50%) translateX(0); }}
  #bar.hidden {{ opacity: 0; pointer-events: none; transform: translateY(-50%) translateX(14px); }}
  #arrowBtn {{ position: fixed; top: 50%; right: 0; left: auto; transform: translateY(-50%); z-index: 30; width: 20px; height: 52px; padding: 0; display: none; align-items: center; justify-content: center; background: rgba(15, 23, 42, 0.9); color: #94a3b8; border: 1px solid rgba(56, 189, 248, 0.3); border-right: none; border-radius: 10px 0 0 10px; backdrop-filter: blur(10px); cursor: pointer; box-shadow: -4px 0 14px rgba(0, 0, 0, 0.5); transition: background 0.15s, color 0.15s; }}
  #arrowBtn.visible {{ display: flex; }}
  #arrowBtn.open {{ color: #38bdf8; border-color: rgba(56, 189, 248, 0.55); }}
  #arrowBtn svg {{ width: 14px; height: 14px; transition: transform 0.2s ease; }}
  #arrowBtn.open svg {{ transform: rotate(180deg); }}
  .ctrl-btn {{ display: inline-flex; align-items: center; justify-content: center; gap: 7px; background: rgba(30, 41, 59, 0.78); color: #e2e8f0; border: 1px solid rgba(148, 163, 184, 0.22); border-radius: 10px; padding: 8px 13px; font-size: 12px; font-weight: 600; letter-spacing: 0.01em; cursor: pointer; transition: background 0.15s, color 0.15s, border-color 0.15s, transform 0.1s; white-space: nowrap; }}
  .ctrl-btn:hover, .ctrl-btn:active {{ background: rgba(56, 189, 248, 0.18); color: #38bdf8; border-color: rgba(56, 189, 248, 0.45); }}
  .ctrl-btn.active {{ background: rgba(6, 182, 212, 0.22); color: #67e8f9; border-color: rgba(34, 211, 238, 0.55); }}
  .ctrl-btn svg {{ width: 15px; height: 15px; flex-shrink: 0; }}
</style>
</head>
<body>
  <button id="arrowBtn" title="Toggle display controls" aria-label="Toggle display controls">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
      <polyline points="15 18 9 12 15 6"></polyline>
    </svg>
  </button>
  <div id="bar">
    <button id="fitBtn" class="ctrl-btn">
      <svg id="fitIcon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <rect x="3" y="5" width="18" height="14" rx="2"></rect>
        <path d="M7 15h10M7 9h4"></path>
      </svg>
      <span id="fitLabel">Fill Screen</span>
    </button>
    <button id="fsBtn" class="ctrl-btn">
      <svg id="fsIcon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="15 3 21 3 21 9"></polyline>
        <polyline points="9 21 3 21 3 15"></polyline>
        <line x1="21" y1="3" x2="14" y2="10"></line>
        <line x1="3" y1="21" x2="10" y2="14"></line>
      </svg>
      <span id="fsLabel">Fullscreen</span>
    </button>
  </div>
  <div id="wrap">
    <img id="screen" src="/stream?token={token}" alt="Raydrop Second Display" />
  </div>
<script>
  const TOKEN = "{token}";
  const wrap = document.getElementById("wrap");
  const img = document.getElementById("screen");
  const bar = document.getElementById("bar");
  const arrowBtn = document.getElementById("arrowBtn");
  const fitBtn = document.getElementById("fitBtn");
  const fitLabel = document.getElementById("fitLabel");
  const fitIcon = document.getElementById("fitIcon");
  const fsBtn = document.getElementById("fsBtn");
  const fsLabel = document.getElementById("fsLabel");
  const fsIcon = document.getElementById("fsIcon");

  let isFs = false;
  let showBarInFs = false;

  function isNativeFullscreen() {{
    return !!(document.fullscreenElement || document.webkitFullscreenElement);
  }}

  function syncControlsUi() {{
    fsLabel.textContent = isFs ? "Exit Fullscreen" : "Fullscreen";
    fsBtn.classList.toggle("active", isFs);
    fsIcon.innerHTML = isFs
      ? '<polyline points="4 14 10 14 10 20"></polyline><polyline points="20 10 14 10 14 4"></polyline><line x1="14" y1="10" x2="21" y2="3"></line><line x1="3" y1="21" x2="10" y2="14"></line>'
      : '<polyline points="15 3 21 3 21 9"></polyline><polyline points="9 21 3 21 3 15"></polyline><line x1="21" y1="3" x2="14" y2="10"></line><line x1="3" y1="21" x2="10" y2="14"></line>';
    if (isFs) {{
      bar.classList.add("fs-mode");
      arrowBtn.classList.add("visible");
      if (showBarInFs) {{
        bar.classList.remove("hidden");
        arrowBtn.classList.add("open");
      }} else {{
        bar.classList.add("hidden");
        arrowBtn.classList.remove("open");
      }}
    }} else {{
      bar.classList.remove("fs-mode", "hidden");
      arrowBtn.classList.remove("visible", "open");
      showBarInFs = false;
    }}
  }}

  function handleFullscreenChange() {{
    isFs = isNativeFullscreen();
    if (isFs) {{
      showBarInFs = false;
    }}
    syncControlsUi();
  }}

  document.addEventListener("fullscreenchange", handleFullscreenChange);
  document.addEventListener("webkitfullscreenchange", handleFullscreenChange);

  [arrowBtn, bar].forEach((el) => {{
    el.addEventListener("touchstart", (e) => e.stopPropagation(), {{ passive: true }});
    el.addEventListener("touchmove", (e) => e.stopPropagation(), {{ passive: true }});
    el.addEventListener("touchend", (e) => e.stopPropagation(), {{ passive: true }});
  }});

  arrowBtn.addEventListener("click", (e) => {{
    e.stopPropagation();
    showBarInFs = !showBarInFs;
    syncControlsUi();
  }});

  fitBtn.addEventListener("click", (e) => {{
    e.stopPropagation();
    img.classList.toggle("fill");
    const isFill = img.classList.contains("fill");
    fitLabel.textContent = isFill ? "Fit Aspect" : "Fill Screen";
    fitBtn.classList.toggle("active", isFill);
    fitIcon.innerHTML = isFill
      ? '<rect x="5" y="4" width="14" height="16" rx="2"></rect><path d="M9 12h6"></path>'
      : '<rect x="3" y="5" width="18" height="14" rx="2"></rect><path d="M7 15h10M7 9h4"></path>';
  }});

  fsBtn.addEventListener("click", (e) => {{
    e.stopPropagation();
    const docEl = document.documentElement;
    if (!isFs) {{
      const reqFs = docEl.requestFullscreen || docEl.webkitRequestFullscreen;
      if (reqFs) {{
        Promise.resolve(reqFs.call(docEl)).then(() => {{
          isFs = true;
          showBarInFs = false;
          syncControlsUi();
        }}).catch(() => {{
          isFs = true;
          showBarInFs = false;
          syncControlsUi();
        }});
      }} else {{
        isFs = true;
        showBarInFs = false;
        syncControlsUi();
      }}
    }} else {{
      const exitFs = document.exitFullscreen || document.webkitExitFullscreen;
      if (isNativeFullscreen() && exitFs) {{
        Promise.resolve(exitFs.call(document)).then(() => {{
          isFs = false;
          showBarInFs = false;
          syncControlsUi();
        }}).catch(() => {{
          isFs = false;
          showBarInFs = false;
          syncControlsUi();
        }});
      }} else {{
        isFs = false;
        showBarInFs = false;
        syncControlsUi();
      }}
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
  let lastTouchTime = 0;
  let isMouseDown = false;

  function mouseBtnName(b) {{
    if (b === 2) return "right";
    if (b === 1) return "middle";
    return "left";
  }}

  wrap.addEventListener("touchstart", (e) => {{
    lastTouchTime = Date.now();
    if (e.touches.length === 2) {{
      lastTouchY = (e.touches[0].clientY + e.touches[1].clientY) / 2;
      return;
    }}
    const c = normCoords(e.touches[0].clientX, e.touches[0].clientY);
    if (!c) return;
    sendInput({{ t: "mc", b: "left", a: "down", x: c.x, y: c.y }});
  }}, {{ passive: false }});

  wrap.addEventListener("touchmove", (e) => {{
    lastTouchTime = Date.now();
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
    lastTouchTime = Date.now();
    if (e.touches.length === 0) {{
      sendInput({{ t: "mc", b: "left", a: "up" }});
    }}
  }});

  // Desktop / Laptop / MacBook mouse & trackpad support
  wrap.addEventListener("mousedown", (e) => {{
    if (Date.now() - lastTouchTime < 500) return;
    isMouseDown = true;
    const c = normCoords(e.clientX, e.clientY);
    if (!c) return;
    sendInput({{ t: "mc", b: mouseBtnName(e.button), a: "down", x: c.x, y: c.y }});
  }});

  wrap.addEventListener("mousemove", (e) => {{
    if (Date.now() - lastTouchTime < 500) return;
    const now = performance.now();
    if (now - lastMove < 16) return;
    lastMove = now;
    const c = normCoords(e.clientX, e.clientY);
    if (c) sendInput({{ t: "mm", x: c.x, y: c.y }});
  }});

  window.addEventListener("mouseup", (e) => {{
    if (!isMouseDown || Date.now() - lastTouchTime < 500) return;
    isMouseDown = false;
    sendInput({{ t: "mc", b: mouseBtnName(e.button), a: "up" }});
  }});

  wrap.addEventListener("contextmenu", (e) => {{
    e.preventDefault();
  }});

  let lastWheel = 0;
  wrap.addEventListener("wheel", (e) => {{
    e.preventDefault();
    const now = performance.now();
    if (now - lastWheel < 25) return;
    lastWheel = now;
    if (Math.abs(e.deltaY) > 0) {{
      sendInput({{ t: "mw", dy: e.deltaY > 0 ? -120 : 120 }});
    }}
  }}, {{ passive: false }});

  [arrowBtn, bar].forEach((el) => {{
    el.addEventListener("mousedown", (e) => e.stopPropagation());
    el.addEventListener("mouseup", (e) => e.stopPropagation());
    el.addEventListener("mousemove", (e) => e.stopPropagation());
    el.addEventListener("wheel", (e) => e.stopPropagation(), {{ passive: true }});
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
