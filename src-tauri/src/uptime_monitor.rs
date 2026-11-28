use crate::db::storage::{now_unix_secs, Database};
use crate::state::AppState;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UptimeGroup {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UptimeMonitor {
    pub id: String,
    pub group_id: String,
    pub name: String,
    pub monitor_type: String,
    pub target: String,
    pub interval_minutes: i64,
    pub timeout_secs: i64,
    pub expected_status: i64,
    pub enabled: bool,
    pub current_status: String,
    pub last_latency_ms: i64,
    pub last_checked_at: i64,
    pub last_error: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UptimeCheckRecord {
    pub id: i64,
    pub monitor_id: String,
    pub status: String,
    pub latency_ms: i64,
    pub status_code: i64,
    pub error_msg: String,
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UptimeOverview {
    pub groups: Vec<UptimeGroup>,
    pub monitors: Vec<UptimeMonitor>,
    pub recent_checks: Vec<UptimeCheckRecord>,
}

fn load_monitors(db: &Database) -> Result<Vec<UptimeMonitor>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, group_id, name, monitor_type, target, interval_minutes, timeout_secs, expected_status, enabled, current_status, last_latency_ms, last_checked_at, last_error, created_at
             FROM uptime_monitors ORDER BY name ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(UptimeMonitor {
                id: r.get(0)?,
                group_id: r.get(1)?,
                name: r.get(2)?,
                monitor_type: r.get(3)?,
                target: r.get(4)?,
                interval_minutes: r.get(5)?,
                timeout_secs: r.get(6)?,
                expected_status: r.get(7)?,
                enabled: r.get::<_, i64>(8)? != 0,
                current_status: r.get(9)?,
                last_latency_ms: r.get(10)?,
                last_checked_at: r.get(11)?,
                last_error: r.get(12)?,
                created_at: r.get(13)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[tauri::command]
pub fn uptime_list_overview(state: State<'_, Arc<AppState>>) -> Result<UptimeOverview, String> {
    let groups = {
        let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, name, color, created_at FROM uptime_groups ORDER BY name ASC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(UptimeGroup {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    color: r.get(2)?,
                    created_at: r.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let monitors = load_monitors(&state.db)?;
    let recent_checks = {
        let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, monitor_id, status, latency_ms, status_code, error_msg, checked_at
                 FROM uptime_checks ORDER BY id DESC LIMIT 200",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(UptimeCheckRecord {
                    id: r.get(0)?,
                    monitor_id: r.get(1)?,
                    status: r.get(2)?,
                    latency_ms: r.get(3)?,
                    status_code: r.get(4)?,
                    error_msg: r.get(5)?,
                    checked_at: r.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    Ok(UptimeOverview {
        groups,
        monitors,
        recent_checks,
    })
}

#[tauri::command]
pub fn uptime_save_group(
    state: State<'_, Arc<AppState>>,
    id: Option<String>,
    name: String,
    color: Option<String>,
) -> Result<UptimeGroup, String> {
    if name.trim().is_empty() {
        return Err("Group name cannot be empty".to_string());
    }
    let gid = id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("upg_{}", uuid::Uuid::new_v4().simple()));
    let col = color.unwrap_or_else(|| "#10b981".to_string());
    let now = now_unix_secs() as i64;
    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO uptime_groups (id, name, color, created_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, color = excluded.color",
        params![gid, name.trim(), col, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(UptimeGroup {
        id: gid,
        name: name.trim().to_string(),
        color: col,
        created_at: now,
    })
}

#[tauri::command]
pub fn uptime_delete_group(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE uptime_monitors SET group_id = '' WHERE group_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM uptime_groups WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct SaveUptimeMonitorInput {
    pub id: Option<String>,
    pub group_id: Option<String>,
    pub name: String,
    pub monitor_type: Option<String>,
    pub target: String,
    pub interval_minutes: Option<i64>,
    pub timeout_secs: Option<i64>,
    pub expected_status: Option<i64>,
    pub enabled: Option<bool>,
}

#[tauri::command]
pub fn uptime_save_monitor(
    state: State<'_, Arc<AppState>>,
    input: SaveUptimeMonitorInput,
) -> Result<UptimeMonitor, String> {
    if input.name.trim().is_empty() || input.target.trim().is_empty() {
        return Err("Monitor name and target URL/host are required".to_string());
    }
    let mid = input
        .id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("upm_{}", uuid::Uuid::new_v4().simple()));
    let now = now_unix_secs() as i64;
    let group_id = input.group_id.unwrap_or_default();
    let monitor_type = input.monitor_type.unwrap_or_else(|| "http".to_string());
    let interval = input.interval_minutes.unwrap_or(5).max(1);
    let timeout = input.timeout_secs.unwrap_or(10).clamp(2, 60);
    let expected = input.expected_status.unwrap_or(200);
    let enabled = input.enabled.unwrap_or(true);

    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO uptime_monitors (id, group_id, name, monitor_type, target, interval_minutes, timeout_secs, expected_status, enabled, current_status, last_latency_ms, last_checked_at, last_error, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'unknown', 0, 0, '', ?10)
         ON CONFLICT(id) DO UPDATE SET
           group_id = excluded.group_id,
           name = excluded.name,
           monitor_type = excluded.monitor_type,
           target = excluded.target,
           interval_minutes = excluded.interval_minutes,
           timeout_secs = excluded.timeout_secs,
           expected_status = excluded.expected_status,
           enabled = excluded.enabled",
        params![
            mid,
            group_id,
            input.name.trim(),
            monitor_type,
            input.target.trim(),
            interval,
            timeout,
            expected,
            if enabled { 1 } else { 0 },
            now
        ],
    )
    .map_err(|e| e.to_string())?;

    drop(conn);
    load_monitors(&state.db)?
        .into_iter()
        .find(|m| m.id == mid)
        .ok_or_else(|| "Monitor not found".to_string())
}

#[tauri::command]
pub fn uptime_delete_monitor(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM uptime_checks WHERE monitor_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM uptime_monitors WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn send_telegram_uptime_alert(db: &Database, text: &str) {
    let cfg = crate::ai_agent::load_ai_mcp_config(db);
    if !cfg.telegram_enabled
        || cfg.telegram_bot_token.trim().is_empty()
        || cfg.telegram_allowed_users.is_empty()
    {
        return;
    }
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        cfg.telegram_bot_token.trim()
    );
    for uid in cfg.telegram_allowed_users {
        let _ = client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": uid,
                "text": text,
                "parse_mode": "Markdown"
            }))
            .send()
            .await;
    }
}

pub async fn run_single_uptime_check(
    app: &AppHandle,
    db: &Database,
    mon: &UptimeMonitor,
) -> UptimeCheckRecord {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(mon.timeout_secs.clamp(2, 60) as u64);

    let (status, status_code, error_msg) = if mon.monitor_type == "tcp" {
        match tokio::time::timeout(timeout, tokio::net::TcpStream::connect(mon.target.trim())).await {
            Ok(Ok(_)) => ("up".to_string(), 0i64, String::new()),
            Ok(Err(e)) => ("down".to_string(), 0i64, e.to_string()),
            Err(_) => ("down".to_string(), 0i64, "TCP connection timed out".to_string()),
        }
    } else {
        let client = reqwest::Client::builder()
            .connect_timeout(timeout)
            .timeout(timeout)
            .build();
        match client {
            Ok(c) => match c.get(mon.target.trim()).send().await {
                Ok(resp) => {
                    let code = resp.status().as_u16() as i64;
                    if code == mon.expected_status || (mon.expected_status == 200 && (200..400).contains(&code)) {
                        ("up".to_string(), code, String::new())
                    } else {
                        ("down".to_string(), code, format!("Unexpected HTTP {}", code))
                    }
                }
                Err(e) => ("down".to_string(), 0i64, e.to_string()),
            },
            Err(e) => ("down".to_string(), 0i64, e.to_string()),
        }
    };

    let latency_ms = start.elapsed().as_millis() as i64;
    let now = now_unix_secs() as i64;

    if let Ok(conn) = db.conn.lock() {
        let _ = conn.execute(
            "UPDATE uptime_monitors
             SET current_status = ?1, last_latency_ms = ?2, last_checked_at = ?3, last_error = ?4
             WHERE id = ?5",
            params![status, latency_ms, now, error_msg, mon.id],
        );
        let _ = conn.execute(
            "INSERT INTO uptime_checks (monitor_id, status, latency_ms, status_code, error_msg, checked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![mon.id, status, latency_ms, status_code, error_msg, now],
        );
        let _ = conn.execute(
            "DELETE FROM uptime_checks WHERE monitor_id = ?1 AND id NOT IN (
                SELECT id FROM uptime_checks WHERE monitor_id = ?1 ORDER BY id DESC LIMIT 200
            )",
            params![mon.id],
        );
    }

    if mon.current_status != status && (status == "down" || (mon.current_status == "down" && status == "up")) {
        let _ = app.emit(
            "uptime_alert",
            serde_json::json!({
                "monitor_id": mon.id,
                "name": mon.name,
                "target": mon.target,
                "status": status,
                "latency_ms": latency_ms,
                "error_msg": error_msg,
            }),
        );
        let tg_msg = if status == "down" {
            format!(
                "🔴 *SERVER DOWN*: `{}`\nTarget: `{}`\nError: `{}`",
                mon.name, mon.target, error_msg
            )
        } else {
            format!(
                "🟢 *SERVER UP*: `{}`\nTarget: `{}`\nLatency: `{}ms`",
                mon.name, mon.target, latency_ms
            )
        };
        send_telegram_uptime_alert(db, &tg_msg).await;
    }

    UptimeCheckRecord {
        id: now,
        monitor_id: mon.id.clone(),
        status,
        latency_ms,
        status_code,
        error_msg,
        checked_at: now,
    }
}

#[tauri::command]
pub async fn uptime_check_now(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    monitor_id: String,
) -> Result<UptimeCheckRecord, String> {
    let mon = load_monitors(&state.db)?
        .into_iter()
        .find(|m| m.id == monitor_id)
        .ok_or_else(|| "Monitor not found".to_string())?;
    Ok(run_single_uptime_check(&app, &state.db, &mon).await)
}

#[tauri::command]
pub fn uptime_export_status_html(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let monitors = load_monitors(&state.db)?;
    let mut rows = String::new();
    for m in &monitors {
        let badge = if m.current_status == "up" {
            "<span style='color:#10b981;font-weight:700'>UP</span>"
        } else if m.current_status == "down" {
            "<span style='color:#f43f5e;font-weight:700'>DOWN</span>"
        } else {
            "<span style='color:#94a3b8'>UNKNOWN</span>"
        };
        rows.push_str(&format!(
            "<tr><td>{}</td><td><code>{}</code></td><td>{}</td><td>{} ms</td></tr>",
            m.name, m.target, badge, m.last_latency_ms
        ));
    }
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset='utf-8'><title>Raydrop Status Page</title><style>body{{font-family:system-ui;background:#020617;color:#e2e8f0;padding:32px}}table{{width:100%;border-collapse:collapse;margin-top:16px}}th,td{{padding:12px;border-bottom:1px solid #1e293b;text-align:left}}</style></head><body><h1>Raydrop Uptime Status</h1><table><thead><tr><th>Service</th><th>Target</th><th>Status</th><th>Latency</th></tr></thead><tbody>{}</tbody></table></body></html>",
        rows
    );
    let out_path = std::env::temp_dir().join("raydrop-status-page.html");
    std::fs::write(&out_path, html.as_bytes()).map_err(|e| e.to_string())?;
    Ok(out_path.to_string_lossy().to_string())
}

pub fn start_uptime_monitor_loop(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let monitors = load_monitors(&state.db).unwrap_or_default();
            let now = now_unix_secs() as i64;
            for mon in monitors {
                if !mon.enabled {
                    continue;
                }
                if now - mon.last_checked_at < mon.interval_minutes * 60 {
                    continue;
                }
                run_single_uptime_check(&app, &state.db, &mon).await;
            }
        }
    });
}
