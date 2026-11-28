use crate::db::storage::{now_unix_secs, Database};
use crate::state::AppState;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbConnection {
    pub id: String,
    pub name: String,
    pub db_type: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database_name: String,
    pub ssh_server_id: String,
    pub admin_tool: String,
    pub backup_dir: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbBackupSchedule {
    pub id: String,
    pub connection_id: String,
    pub interval_minutes: i64,
    #[serde(default)]
    pub cron_expr: String,
    pub keep_last: i64,
    pub enabled: bool,
    pub last_run_at: i64,
    pub last_status: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbBackupRecord {
    pub id: String,
    pub connection_id: String,
    pub file_path: String,
    pub size_bytes: u64,
    pub status: String,
    pub error_msg: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbToolsStatus {
    pub tools_dir: String,
    pub mysql_client: Option<String>,
    pub mysqldump_client: Option<String>,
    pub psql_client: Option<String>,
    pub pg_dump_client: Option<String>,
    pub sqlite_client: Option<String>,
    pub php_bin: Option<String>,
    pub adminer_installed: bool,
    pub phpmyadmin_installed: bool,
    pub pgweb_bin: Option<String>,
    pub running_admins: Vec<RunningDbAdmin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningDbAdmin {
    pub connection_id: String,
    pub tool: String,
    pub port: u16,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbOverview {
    pub connections: Vec<DbConnection>,
    pub schedules: Vec<DbBackupSchedule>,
    pub history: Vec<DbBackupRecord>,
    pub tools: DbToolsStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbHealthStatus {
    pub connection_id: String,
    pub ok: bool,
    pub latency_ms: u64,
    pub version: String,
    pub uptime_secs: u64,
    pub active_connections: u64,
    pub table_count: u64,
    pub size_mb: f64,
    pub message: String,
    pub checked_at: u64,
}

pub struct AdminProc {
    pub info: RunningDbAdmin,
    pub child: Child,
}

#[derive(Default)]
pub struct DbManagerState {
    pub admins: Mutex<HashMap<String, AdminProc>>,
}

pub fn devops_tools_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.data_local_dir().join("Raydrop").join("devops-tools"))
        .unwrap_or_else(|| PathBuf::from(".raydrop-tools"))
}

pub fn default_backup_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.data_local_dir().join("Raydrop").join("db-backups"))
        .unwrap_or_else(|| PathBuf::from(".raydrop-backups"))
}

fn find_binary_in_dirs(names: &[&str], extra_dirs: &[PathBuf]) -> Option<String> {
    for dir in extra_dirs {
        for name in names {
            let cand = dir.join(name);
            if cand.is_file() {
                return Some(cand.to_string_lossy().to_string());
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        for name in names {
            if let Ok(out) = Command::new("where")
                .arg(name)
                .creation_flags(0x0800_0000)
                .output()
            {
                if out.status.success() {
                    if let Some(first) = String::from_utf8_lossy(&out.stdout)
                        .lines()
                        .map(str::trim)
                        .find(|s| !s.is_empty())
                    {
                        return Some(first.to_string());
                    }
                }
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        for name in names {
            if let Ok(out) = Command::new("which").arg(name).output() {
                if out.status.success() {
                    let first = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !first.is_empty() {
                        return Some(first);
                    }
                }
            }
        }
    }
    None
}

fn collect_client_search_dirs() -> Vec<PathBuf> {
    let root = devops_tools_dir();
    let mut dirs = vec![
        root.join("clients"),
        root.join("php"),
        root.join("pgweb"),
        root.join("mysql").join("bin"),
        root.join("postgres").join("bin"),
        root.join("sqlite"),
    ];
    #[cfg(target_os = "windows")]
    {
        for base in [
            "C:\\wamp64\\bin\\mysql",
            "C:\\wamp64\\bin\\mariadb",
            "C:\\wamp64\\bin\\php",
            "C:\\xampp\\mysql\\bin",
            "C:\\xampp\\php",
            "C:\\laragon\\bin\\mysql",
            "C:\\laragon\\bin\\php",
            "C:\\Program Files\\MySQL",
            "C:\\Program Files\\PostgreSQL",
        ] {
            let p = Path::new(base);
            if p.is_dir() {
                dirs.push(p.to_path_buf());
                if let Ok(entries) = std::fs::read_dir(p) {
                    for e in entries.flatten() {
                        let sub = e.path();
                        if sub.is_dir() {
                            dirs.push(sub.join("bin"));
                            dirs.push(sub);
                        }
                    }
                }
            }
        }
    }
    dirs
}

pub fn detect_tools_status(state: &DbManagerState) -> DbToolsStatus {
    let root = devops_tools_dir();
    let dirs = collect_client_search_dirs();

    let mysql_names = if cfg!(target_os = "windows") { &["mysql.exe"][..] } else { &["mysql"][..] };
    let mysqldump_names = if cfg!(target_os = "windows") { &["mysqldump.exe"][..] } else { &["mysqldump"][..] };
    let psql_names = if cfg!(target_os = "windows") { &["psql.exe"][..] } else { &["psql"][..] };
    let pgdump_names = if cfg!(target_os = "windows") { &["pg_dump.exe"][..] } else { &["pg_dump"][..] };
    let sqlite_names = if cfg!(target_os = "windows") { &["sqlite3.exe"][..] } else { &["sqlite3"][..] };
    let php_names = if cfg!(target_os = "windows") { &["php.exe"][..] } else { &["php"][..] };
    let pgweb_names = if cfg!(target_os = "windows") { &["pgweb.exe"][..] } else { &["pgweb"][..] };

    let running_admins = state
        .admins
        .lock()
        .map(|g| g.values().map(|a| a.info.clone()).collect())
        .unwrap_or_default();

    DbToolsStatus {
        tools_dir: root.to_string_lossy().to_string(),
        mysql_client: find_binary_in_dirs(mysql_names, &dirs),
        mysqldump_client: find_binary_in_dirs(mysqldump_names, &dirs),
        psql_client: find_binary_in_dirs(psql_names, &dirs),
        pg_dump_client: find_binary_in_dirs(pgdump_names, &dirs),
        sqlite_client: find_binary_in_dirs(sqlite_names, &dirs),
        php_bin: find_binary_in_dirs(php_names, &dirs),
        adminer_installed: root.join("adminer").join("adminer.php").is_file(),
        phpmyadmin_installed: root.join("phpmyadmin").join("index.php").is_file(),
        pgweb_bin: find_binary_in_dirs(pgweb_names, &dirs),
        running_admins,
    }
}

fn find_free_admin_port() -> Result<u16, String> {
    // Uncommon high ports (48910..=48999) so it never conflicts with WAMP/XAMPP/Laragon/Docker
    for port in 48910..=48999u16 {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    let l = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    l.local_addr()
        .map(|a| a.port())
        .map_err(|e| e.to_string())
}

fn hidden_cmd(bin: &str) -> Command {
    let mut cmd = Command::new(bin);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

pub fn get_connection_by_id(db: &Database, id: &str) -> Result<DbConnection, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, name, db_type, host, port, username, password, database_name, ssh_server_id, admin_tool, backup_dir, created_at
         FROM db_connections WHERE id = ?1",
        params![id],
        |r| {
            Ok(DbConnection {
                id: r.get(0)?,
                name: r.get(1)?,
                db_type: r.get(2)?,
                host: r.get(3)?,
                port: r.get::<_, i64>(4)? as u16,
                username: r.get(5)?,
                password: r.get(6)?,
                database_name: r.get(7)?,
                ssh_server_id: r.get(8)?,
                admin_tool: r.get(9)?,
                backup_dir: r.get(10)?,
                created_at: r.get(11)?,
            })
        },
    )
    .map_err(|e| format!("Database connection '{}' not found: {}", id, e))
}

fn list_connections(db: &Database) -> Result<Vec<DbConnection>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, db_type, host, port, username, password, database_name, ssh_server_id, admin_tool, backup_dir, created_at
             FROM db_connections ORDER BY name ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DbConnection {
                id: r.get(0)?,
                name: r.get(1)?,
                db_type: r.get(2)?,
                host: r.get(3)?,
                port: r.get::<_, i64>(4)? as u16,
                username: r.get(5)?,
                password: r.get(6)?,
                database_name: r.get(7)?,
                ssh_server_id: r.get(8)?,
                admin_tool: r.get(9)?,
                backup_dir: r.get(10)?,
                created_at: r.get(11)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn list_schedules(db: &Database) -> Result<Vec<DbBackupSchedule>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, connection_id, interval_minutes, COALESCE(cron_expr, ''), keep_last, enabled, last_run_at, last_status, created_at
             FROM db_backup_schedules ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DbBackupSchedule {
                id: r.get(0)?,
                connection_id: r.get(1)?,
                interval_minutes: r.get(2)?,
                cron_expr: r.get(3)?,
                keep_last: r.get(4)?,
                enabled: r.get::<_, i64>(5)? != 0,
                last_run_at: r.get(6)?,
                last_status: r.get(7)?,
                created_at: r.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn list_history(db: &Database) -> Result<Vec<DbBackupRecord>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, connection_id, file_path, size_bytes, status, error_msg, created_at
             FROM db_backup_history ORDER BY created_at DESC LIMIT 80",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DbBackupRecord {
                id: r.get(0)?,
                connection_id: r.get(1)?,
                file_path: r.get(2)?,
                size_bytes: r.get::<_, i64>(3)? as u64,
                status: r.get(4)?,
                error_msg: r.get(5)?,
                created_at: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[tauri::command]
pub fn dbm_list_overview(state: State<'_, Arc<AppState>>) -> Result<DbOverview, String> {
    Ok(DbOverview {
        connections: list_connections(&state.db)?,
        schedules: list_schedules(&state.db)?,
        history: list_history(&state.db)?,
        tools: detect_tools_status(&state.db_manager),
    })
}

#[derive(Debug, Deserialize)]
pub struct SaveDbConnInput {
    pub id: Option<String>,
    pub name: String,
    pub db_type: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub database_name: Option<String>,
    pub ssh_server_id: Option<String>,
    pub admin_tool: Option<String>,
    pub backup_dir: Option<String>,
}

#[tauri::command]
pub fn dbm_save_connection(
    state: State<'_, Arc<AppState>>,
    input: SaveDbConnInput,
) -> Result<DbConnection, String> {
    if input.name.trim().is_empty() {
        return Err("Connection name is required".to_string());
    }
    let cid = input
        .id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("dbc_{}", uuid::Uuid::new_v4().simple()));
    let now = now_unix_secs() as i64;
    let db_type = input.db_type.trim().to_lowercase();
    let default_port = match db_type.as_str() {
        "postgres" | "postgresql" => 5432,
        "sqlite" => 0,
        _ => 3306,
    };
    let host = input.host.unwrap_or_else(|| "127.0.0.1".to_string());
    let port = input.port.unwrap_or(default_port);
    let username = input.username.unwrap_or_else(|| "root".to_string());
    let password = input.password.unwrap_or_default();
    let database_name = input.database_name.unwrap_or_default();
    let ssh_server_id = input.ssh_server_id.unwrap_or_default();
    let admin_tool = input.admin_tool.unwrap_or_else(|| "adminer".to_string());
    let backup_dir = input
        .backup_dir
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| default_backup_dir().to_string_lossy().to_string());

    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO db_connections (id, name, db_type, host, port, username, password, database_name, ssh_server_id, admin_tool, backup_dir, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(id) DO UPDATE SET
           name = excluded.name,
           db_type = excluded.db_type,
           host = excluded.host,
           port = excluded.port,
           username = excluded.username,
           password = excluded.password,
           database_name = excluded.database_name,
           ssh_server_id = excluded.ssh_server_id,
           admin_tool = excluded.admin_tool,
           backup_dir = excluded.backup_dir",
        params![
            cid,
            input.name.trim(),
            db_type,
            host.trim(),
            port as i64,
            username.trim(),
            password,
            database_name.trim(),
            ssh_server_id.trim(),
            admin_tool.trim(),
            backup_dir.trim(),
            now
        ],
    )
    .map_err(|e| e.to_string())?;

    drop(conn);
    get_connection_by_id(&state.db, &cid)
}

#[tauri::command]
pub fn dbm_delete_connection(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM db_backup_schedules WHERE connection_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM db_connections WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(180))
        .user_agent("Raydrop-DevOps/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} downloading {}", resp.status(), url));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    Ok(bytes.to_vec())
}

fn extract_zip_bytes(bytes: &[u8], dest_dir: &Path, strip_top_dir: bool) -> Result<(), String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("Invalid zip: {}", e))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let raw_name = file.name().replace('\\', "/");
        let rel = if strip_top_dir {
            raw_name
                .split_once('/')
                .map(|(_, rest)| rest)
                .unwrap_or(&raw_name)
                .to_string()
        } else {
            raw_name
        };
        if rel.is_empty() || rel.contains("..") {
            continue;
        }
        let out_path = dest_dir.join(rel);
        if file.is_dir() {
            let _ = std::fs::create_dir_all(&out_path);
        } else {
            if let Some(p) = out_path.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let mut out = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut out).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn dbm_install_tool(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    tool_id: String,
) -> Result<DbToolsStatus, String> {
    let root = devops_tools_dir();
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let _ = app.emit(
        "dbm_tool_progress",
        serde_json::json!({ "tool_id": tool_id, "status": "downloading" }),
    );

    match tool_id.as_str() {
        "adminer" => {
            let dir = root.join("adminer");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let bytes = download_bytes("https://github.com/vrana/adminer/releases/latest/download/adminer.php")
                .await
                .or(download_bytes("https://www.adminer.org/latest.php").await)?;
            std::fs::write(dir.join("adminer.php"), &bytes).map_err(|e| e.to_string())?;
            // Create index.php wrapper that redirects to adminer.php
            let _ = std::fs::write(dir.join("index.php"), b"<?php include __DIR__ . '/adminer.php';");
        }
        "phpmyadmin" => {
            let dir = root.join("phpmyadmin");
            let bytes = download_bytes("https://www.phpmyadmin.net/downloads/phpMyAdmin-latest-all-languages.zip").await?;
            extract_zip_bytes(&bytes, &dir, true)?;
            let cfg = "<?php\n$cfg['blowfish_secret'] = 'RaydropPhpMyAdminSecretKey32Chars!';\n$cfg['AllowArbitraryServer'] = true;\n$i = 1;\n$cfg['Servers'][$i]['auth_type'] = 'cookie';\n$cfg['Servers'][$i]['AllowNoPassword'] = true;\n";
            let _ = std::fs::write(dir.join("config.inc.php"), cfg.as_bytes());
        }
        "pgweb" => {
            let dir = root.join("pgweb");
            let url = if cfg!(target_os = "windows") {
                "https://github.com/sosedoff/pgweb/releases/latest/download/pgweb_windows_amd64.zip"
            } else if cfg!(target_os = "macos") {
                "https://github.com/sosedoff/pgweb/releases/latest/download/pgweb_darwin_arm64.zip"
            } else {
                "https://github.com/sosedoff/pgweb/releases/latest/download/pgweb_linux_amd64.zip"
            };
            let bytes = download_bytes(url).await?;
            extract_zip_bytes(&bytes, &dir, false)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for e in entries.flatten() {
                        if let Some(n) = e.file_name().to_str() {
                            if n.starts_with("pgweb") {
                                let _ = std::fs::set_permissions(e.path(), std::fs::Permissions::from_mode(0o755));
                            }
                        }
                    }
                }
            }
        }
        "sqlite_client" => {
            let dir = root.join("clients");
            let url = if cfg!(target_os = "windows") {
                "https://www.sqlite.org/2024/sqlite-tools-win-x64-3470200.zip"
            } else {
                return Err("Please install sqlite3 via your OS package manager (apt/brew install sqlite3).".to_string());
            };
            let bytes = download_bytes(url).await?;
            extract_zip_bytes(&bytes, &dir, false)?;
        }
        "php" => {
            if !cfg!(target_os = "windows") {
                return Err("Please install PHP CLI via apt/brew (`sudo apt install php-cli php-mysqli php-pgsql` or `brew install php`).".to_string());
            }
            let dir = root.join("php");
            let bytes = download_bytes("https://windows.php.net/downloads/releases/latest/php-8.3-nts-Win32-vs16-x64.zip")
                .await
                .or(download_bytes("https://windows.php.net/downloads/releases/archives/php-8.3.14-nts-Win32-vs16-x64.zip").await)?;
            extract_zip_bytes(&bytes, &dir, false)?;
            let ini = "extension_dir = \"ext\"\nextension=mysqli\nextension=pdo_mysql\nextension=pdo_pgsql\nextension=pgsql\nextension=pdo_sqlite\nextension=sqlite3\nextension=mbstring\nextension=openssl\n";
            let _ = std::fs::write(dir.join("php.ini"), ini.as_bytes());
        }
        _ => {
            return Err(format!(
                "Auto-installer for '{}' is not configured; install the official CLI package and Raydrop will auto-detect it.",
                tool_id
            ));
        }
    }

    let status = detect_tools_status(&state.db_manager);
    let _ = app.emit(
        "dbm_tool_progress",
        serde_json::json!({ "tool_id": tool_id, "status": "installed" }),
    );
    Ok(status)
}

#[tauri::command]
pub async fn dbm_launch_admin(
    state: State<'_, Arc<AppState>>,
    connection_id: String,
) -> Result<RunningDbAdmin, String> {
    let conn = get_connection_by_id(&state.db, &connection_id)?;
    {
        let mut admins = state.db_manager.admins.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = admins.get_mut(&connection_id) {
            if let Ok(None) = existing.child.try_wait() {
                return Ok(existing.info.clone());
            }
            admins.remove(&connection_id);
        }
    }

    let tools = detect_tools_status(&state.db_manager);
    let root = devops_tools_dir();
    let port = find_free_admin_port()?;
    let tool = conn.admin_tool.to_lowercase();

    let (child, url) = if tool == "pgweb" {
        let bin = tools
            .pgweb_bin
            .ok_or_else(|| "pgweb is not installed yet. Click 'Install pgweb' first.".to_string())?;
        let pg_url = format!(
            "postgres://{}:{}@{}:{}/{}?sslmode=disable",
            conn.username, conn.password, conn.host, conn.port, conn.database_name
        );
        let child = hidden_cmd(&bin)
            .args([
                "--bind=127.0.0.1",
                &format!("--listen={}", port),
                &format!("--url={}", pg_url),
                "--skip-open",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start pgweb: {}", e))?;
        (child, format!("http://127.0.0.1:{}/", port))
    } else {
        let php = tools
            .php_bin
            .ok_or_else(|| "PHP CLI is required to run Adminer/phpMyAdmin. Click 'Install PHP' first.".to_string())?;
        let (doc_root, env_driver, env_server) = if tool == "phpmyadmin" {
            if !tools.phpmyadmin_installed {
                return Err("phpMyAdmin is not installed yet. Click 'Install phpMyAdmin' first.".to_string());
            }
            let pma_dir = root.join("phpmyadmin");
            // Read per-process env vars and isolate PHP session name per connection so multiple ports never collide
            let pma_cfg = "<?php\n$connId = preg_replace('/[^a-zA-Z0-9]/', '', getenv('RAYDROP_CONN_ID') ?: '1');\nini_set('session.name', 'PMA_' . $connId);\n$cfg['blowfish_secret'] = 'raydrop_pma_autologin_secret_key_32b';\n$i = 0;\n$i++;\n$cfg['Servers'][$i]['auth_type'] = 'config';\n$cfg['Servers'][$i]['host'] = getenv('RAYDROP_DB_HOST') ?: '127.0.0.1';\n$cfg['Servers'][$i]['port'] = getenv('RAYDROP_DB_PORT') ?: '3306';\n$cfg['Servers'][$i]['user'] = getenv('RAYDROP_DB_USER') ?: 'root';\n$cfg['Servers'][$i]['password'] = getenv('RAYDROP_DB_PASS') !== false ? getenv('RAYDROP_DB_PASS') : '';\n$cfg['Servers'][$i]['AllowNoPassword'] = true;\n$cfg['Servers'][$i]['compress'] = false;\n";
            let _ = std::fs::write(pma_dir.join("config.inc.php"), pma_cfg);
            (pma_dir, "server".to_string(), format!("{}:{}", conn.host, conn.port))
        } else {
            if !tools.adminer_installed {
                return Err("Adminer is not installed yet. Click 'Install Adminer' first.".to_string());
            }
            let adm_dir = root.join("adminer");
            let driver = if conn.db_type.contains("pg") || conn.db_type.contains("post") {
                "pgsql"
            } else if conn.db_type.contains("sqlite") {
                "sqlite"
            } else {
                "server"
            };
            let server_str = if driver == "sqlite" {
                String::new()
            } else {
                format!("{}:{}", conn.host, conn.port)
            };
            let autologin_php = "<?php\n$connId = preg_replace('/[^a-zA-Z0-9]/', '', getenv('RAYDROP_CONN_ID') ?: '1');\nini_set('session.name', 'ADM_' . $connId);\nheader('Content-Type: text/html; charset=utf-8');\n$e = fn($v) => htmlspecialchars((string)$v, ENT_QUOTES, 'UTF-8');\n?>\n<!DOCTYPE html><html><body style=\"background:#020617;color:#e2e8f0;font-family:system-ui;padding:24px\">\n<p>Logging in to Adminer...</p>\n<form id=\"f\" method=\"post\" action=\"adminer.php\">\n<input type=\"hidden\" name=\"auth[driver]\" value=\"<?= $e(getenv('RAYDROP_DB_DRIVER')) ?>\">\n<input type=\"hidden\" name=\"auth[server]\" value=\"<?= $e(getenv('RAYDROP_DB_SERVER')) ?>\">\n<input type=\"hidden\" name=\"auth[username]\" value=\"<?= $e(getenv('RAYDROP_DB_USER')) ?>\">\n<input type=\"hidden\" name=\"auth[password]\" value=\"<?= $e(getenv('RAYDROP_DB_PASS')) ?>\">\n<input type=\"hidden\" name=\"auth[db]\" value=\"<?= $e(getenv('RAYDROP_DB_NAME')) ?>\">\n</form>\n<script>document.getElementById('f').submit();</script>\n</body></html>";
            let _ = std::fs::write(adm_dir.join("_raydrop_autologin.php"), autologin_php);
            (adm_dir, driver.to_string(), server_str)
        };
        let child = hidden_cmd(&php)
            .args([
                "-S",
                &format!("127.0.0.1:{}", port),
                "-t",
                &doc_root.to_string_lossy(),
            ])
            .env("RAYDROP_CONN_ID", &conn.id)
            .env("RAYDROP_DB_HOST", &conn.host)
            .env("RAYDROP_DB_PORT", conn.port.to_string())
            .env("RAYDROP_DB_USER", &conn.username)
            .env("RAYDROP_DB_PASS", &conn.password)
            .env("RAYDROP_DB_NAME", &conn.database_name)
            .env("RAYDROP_DB_DRIVER", &env_driver)
            .env("RAYDROP_DB_SERVER", &env_server)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start PHP server: {}", e))?;

        let entry_path = if tool == "adminer" {
            "_raydrop_autologin.php".to_string()
        } else if !conn.database_name.trim().is_empty() {
            format!("index.php?route=/database/structure&db={}", urlencoding_simple(&conn.database_name))
        } else {
            "index.php".to_string()
        };
        (child, format!("http://127.0.0.1:{}/{}", port, entry_path))
    };

    // Wait until the local DB Admin HTTP server is listening and responding (prevents 404 / connection refused on auto-open)
    let probe_url = format!("http://127.0.0.1:{}/", port);
    if let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(400))
        .build()
    {
        for _ in 0..40 {
            if let Ok(resp) = client.get(&probe_url).send().await {
                let code = resp.status().as_u16();
                if code < 500 && code != 404 {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        }
    }

    let info = RunningDbAdmin {
        connection_id: connection_id.clone(),
        tool,
        port,
        url: url.clone(),
    };
    let mut admins = state.db_manager.admins.lock().map_err(|e| e.to_string())?;
    admins.insert(
        connection_id,
        AdminProc {
            info: info.clone(),
            child,
        },
    );
    Ok(info)
}

#[tauri::command]
pub fn dbm_stop_admin(state: State<'_, Arc<AppState>>, connection_id: String) -> Result<(), String> {
    let mut admins = state.db_manager.admins.lock().map_err(|e| e.to_string())?;
    if let Some(mut proc) = admins.remove(&connection_id) {
        let _ = proc.child.kill();
    }
    Ok(())
}

fn urlencoding_simple(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{:02X}", b),
        })
        .collect()
}

/// Opens a temporary local TCP tunnel over SSH if `ssh_server_id` is configured.
async fn maybe_open_ssh_tunnel(
    db: &Database,
    conn: &DbConnection,
) -> Result<(String, u16, Option<tokio::task::JoinHandle<()>>), String> {
    if conn.ssh_server_id.trim().is_empty() {
        return Ok((conn.host.clone(), conn.port, None));
    }
    let server = crate::ssh_manager::get_server_by_id(db, &conn.ssh_server_id)?;
    let session = Arc::new(crate::ssh_manager::open_authenticated_session(&server).await?);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Tunnel bind error: {}", e))?;
    let local_port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let target_host = conn.host.clone();
    let target_port = conn.port as u32;

    let handle = tokio::spawn(async move {
        while let Ok((mut local_stream, _)) = listener.accept().await {
            let sess = session.clone();
            let th = target_host.clone();
            tokio::spawn(async move {
                if let Ok(channel) = sess
                    .channel_open_direct_tcpip(&th, target_port, "127.0.0.1", 0)
                    .await
                {
                    let mut ssh_stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut local_stream, &mut ssh_stream).await;
                }
            });
        }
    });
    Ok(("127.0.0.1".to_string(), local_port, Some(handle)))
}

pub async fn run_backup_for_connection(
    db: &Database,
    tools: &DbToolsStatus,
    connection_id: &str,
) -> Result<DbBackupRecord, String> {
    let conn = get_connection_by_id(db, connection_id)?;
    let (eff_host, eff_port, tunnel_handle) = maybe_open_ssh_tunnel(db, &conn).await?;

    let backup_dir = if conn.backup_dir.trim().is_empty() {
        default_backup_dir()
    } else {
        PathBuf::from(&conn.backup_dir)
    };
    std::fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;
    let safe_name: String = conn
        .name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let file_path = backup_dir.join(format!("{}_{}.sql.gz", safe_name, now_unix_secs()));

    let dump_bytes: Vec<u8> = match conn.db_type.as_str() {
        "mysql" | "mariadb" => {
            let bin = tools
                .mysqldump_client
                .as_deref()
                .ok_or("mysqldump client not found. Install MySQL/MariaDB client first.")?;
            let mut cmd = hidden_cmd(bin);
            cmd.args([
                "-h",
                &eff_host,
                "-P",
                &eff_port.to_string(),
                "-u",
                &conn.username,
                "--single-transaction",
                "--routines",
                "--triggers",
            ]);
            if !conn.password.is_empty() {
                cmd.env("MYSQL_PWD", &conn.password);
            }
            if !conn.database_name.is_empty() {
                cmd.arg(&conn.database_name);
            } else {
                cmd.arg("--all-databases");
            }
            let out = cmd.output().map_err(|e| format!("Failed to run mysqldump: {}", e))?;
            if !out.status.success() {
                if let Some(h) = tunnel_handle {
                    h.abort();
                }
                return Err(String::from_utf8_lossy(&out.stderr).to_string());
            }
            out.stdout
        }
        "postgres" | "postgresql" => {
            let bin = tools
                .pg_dump_client
                .as_deref()
                .ok_or("pg_dump client not found. Install PostgreSQL client first.")?;
            let mut cmd = hidden_cmd(bin);
            cmd.args(["-h", &eff_host, "-p", &eff_port.to_string(), "-U", &conn.username]);
            if !conn.password.is_empty() {
                cmd.env("PGPASSWORD", &conn.password);
            }
            if !conn.database_name.is_empty() {
                cmd.arg(&conn.database_name);
            }
            let out = cmd.output().map_err(|e| format!("Failed to run pg_dump: {}", e))?;
            if !out.status.success() {
                if let Some(h) = tunnel_handle {
                    h.abort();
                }
                return Err(String::from_utf8_lossy(&out.stderr).to_string());
            }
            out.stdout
        }
        "sqlite" => {
            let bin = tools
                .sqlite_client
                .as_deref()
                .ok_or("sqlite3 client not found. Click 'Install SQLite' first.")?;
            let out = hidden_cmd(bin)
                .args([&conn.database_name, ".dump"])
                .output()
                .map_err(|e| format!("Failed to run sqlite3: {}", e))?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).to_string());
            }
            out.stdout
        }
        other => {
            if let Some(h) = tunnel_handle {
                h.abort();
            }
            return Err(format!("Unsupported db_type '{}'", other));
        }
    };

    if let Some(h) = tunnel_handle {
        h.abort();
    }

    let out_file = std::fs::File::create(&file_path).map_err(|e| e.to_string())?;
    let mut encoder = GzEncoder::new(out_file, Compression::default());
    encoder.write_all(&dump_bytes).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())?;

    let size_bytes = std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
    let rec = DbBackupRecord {
        id: format!("dbb_{}", uuid::Uuid::new_v4().simple()),
        connection_id: connection_id.to_string(),
        file_path: file_path.to_string_lossy().to_string(),
        size_bytes,
        status: "success".to_string(),
        error_msg: String::new(),
        created_at: now_unix_secs() as i64,
    };

    {
        let db_conn = db.conn.lock().map_err(|e| e.to_string())?;
        db_conn
            .execute(
                "INSERT INTO db_backup_history (id, connection_id, file_path, size_bytes, status, error_msg, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    rec.id,
                    rec.connection_id,
                    rec.file_path,
                    rec.size_bytes as i64,
                    rec.status,
                    rec.error_msg,
                    rec.created_at
                ],
            )
            .map_err(|e| e.to_string())?;
    }

    Ok(rec)
}

#[tauri::command]
pub async fn dbm_backup_now(
    state: State<'_, Arc<AppState>>,
    connection_id: String,
) -> Result<DbBackupRecord, String> {
    let tools = detect_tools_status(&state.db_manager);
    run_backup_for_connection(&state.db, &tools, &connection_id).await
}

fn count_existing_tables(
    conn: &DbConnection,
    eff_host: &str,
    eff_port: u16,
    tools: &DbToolsStatus,
) -> Result<u64, String> {
    match conn.db_type.as_str() {
        "mysql" | "mariadb" => {
            let bin = tools
                .mysql_client
                .as_deref()
                .ok_or("mysql client not found. Install MySQL/MariaDB client first.")?;
            let mut cmd = hidden_cmd(bin);
            cmd.args([
                "-h",
                eff_host,
                "-P",
                &eff_port.to_string(),
                "-u",
                &conn.username,
                "-N",
                "-B",
                "-e",
                &format!(
                    "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = '{}';",
                    conn.database_name.replace('\'', "''")
                ),
            ]);
            if !conn.password.is_empty() {
                cmd.env("MYSQL_PWD", &conn.password);
            }
            let out = cmd.output().map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).to_string());
            }
            Ok(String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(0))
        }
        "postgres" | "postgresql" => {
            let bin = tools
                .psql_client
                .as_deref()
                .ok_or("psql client not found. Install PostgreSQL client first.")?;
            let mut cmd = hidden_cmd(bin);
            cmd.args([
                "-h",
                eff_host,
                "-p",
                &eff_port.to_string(),
                "-U",
                &conn.username,
                "-d",
                &conn.database_name,
                "-t",
                "-A",
                "-c",
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';",
            ]);
            if !conn.password.is_empty() {
                cmd.env("PGPASSWORD", &conn.password);
            }
            let out = cmd.output().map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).to_string());
            }
            Ok(String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(0))
        }
        "sqlite" => {
            let bin = tools
                .sqlite_client
                .as_deref()
                .ok_or("sqlite3 client not found.")?;
            let out = hidden_cmd(bin)
                .args([
                    &conn.database_name,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%';",
                ])
                .output()
                .map_err(|e| e.to_string())?;
            Ok(String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(0))
        }
        _ => Ok(0),
    }
}

#[tauri::command]
pub async fn dbm_restore_backup(
    state: State<'_, Arc<AppState>>,
    connection_id: String,
    file_path: String,
    _allow_non_empty: Option<bool>,
) -> Result<String, String> {
    let conn = get_connection_by_id(&state.db, &connection_id)?;
    let tools = detect_tools_status(&state.db_manager);
    let (eff_host, eff_port, tunnel_handle) = maybe_open_ssh_tunnel(&state.db, &conn).await?;

    let existing_tables = count_existing_tables(&conn, &eff_host, eff_port, &tools)?;
    if existing_tables > 0 {
        if let Some(h) = tunnel_handle {
            h.abort();
        }
        return Err(format!(
            "SAFE_RESTORE_BLOCKED: Database '{}' masih memiliki {} tabel. Safe Restore dibatalkan — kosongkan database terlebih dahulu.",
            conn.database_name, existing_tables
        ));
    }

    let raw_bytes = std::fs::read(&file_path).map_err(|e| format!("Failed to read backup file: {}", e))?;
    let sql_bytes = if file_path.to_lowercase().ends_with(".gz") {
        let mut decoder = GzDecoder::new(&raw_bytes[..]);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|e| format!("Failed to decompress .sql.gz: {}", e))?;
        out
    } else {
        raw_bytes
    };

    let res = match conn.db_type.as_str() {
        "mysql" | "mariadb" => {
            let bin = tools.mysql_client.as_deref().ok_or("mysql client not found")?;
            let mut cmd = hidden_cmd(bin);
            cmd.args(["-h", &eff_host, "-P", &eff_port.to_string(), "-u", &conn.username, &conn.database_name]);
            if !conn.password.is_empty() {
                cmd.env("MYSQL_PWD", &conn.password);
            }
            cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
            let mut child = cmd.spawn().map_err(|e| e.to_string())?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&sql_bytes).map_err(|e| e.to_string())?;
            }
            let out = child.wait_with_output().map_err(|e| e.to_string())?;
            if !out.status.success() {
                Err(String::from_utf8_lossy(&out.stderr).to_string())
            } else {
                Ok(format!("Restored {} bytes into '{}'", sql_bytes.len(), conn.database_name))
            }
        }
        "postgres" | "postgresql" => {
            let bin = tools.psql_client.as_deref().ok_or("psql client not found")?;
            let mut cmd = hidden_cmd(bin);
            cmd.args(["-h", &eff_host, "-p", &eff_port.to_string(), "-U", &conn.username, "-d", &conn.database_name]);
            if !conn.password.is_empty() {
                cmd.env("PGPASSWORD", &conn.password);
            }
            cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
            let mut child = cmd.spawn().map_err(|e| e.to_string())?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&sql_bytes).map_err(|e| e.to_string())?;
            }
            let out = child.wait_with_output().map_err(|e| e.to_string())?;
            if !out.status.success() {
                Err(String::from_utf8_lossy(&out.stderr).to_string())
            } else {
                Ok(format!("Restored {} bytes into '{}'", sql_bytes.len(), conn.database_name))
            }
        }
        "sqlite" => {
            let bin = tools.sqlite_client.as_deref().ok_or("sqlite3 client not found")?;
            let mut cmd = hidden_cmd(bin);
            cmd.arg(&conn.database_name)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut child = cmd.spawn().map_err(|e| e.to_string())?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&sql_bytes).map_err(|e| e.to_string())?;
            }
            let out = child.wait_with_output().map_err(|e| e.to_string())?;
            if !out.status.success() {
                Err(String::from_utf8_lossy(&out.stderr).to_string())
            } else {
                Ok(format!("Restored {} bytes into '{}'", sql_bytes.len(), conn.database_name))
            }
        }
        other => Err(format!("Unsupported db_type '{}'", other)),
    };

    if let Some(h) = tunnel_handle {
        h.abort();
    }
    res
}

#[tauri::command]
pub fn dbm_save_schedule(
    state: State<'_, Arc<AppState>>,
    id: Option<String>,
    connection_id: String,
    interval_minutes: i64,
    cron_expr: Option<String>,
    keep_last: i64,
    enabled: bool,
) -> Result<DbBackupSchedule, String> {
    let sid = id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("dbs_{}", uuid::Uuid::new_v4().simple()));
    let cron = cron_expr.unwrap_or_default().trim().to_string();
    let now = now_unix_secs() as i64;
    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO db_backup_schedules (id, connection_id, interval_minutes, cron_expr, keep_last, enabled, last_run_at, last_status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, '', ?7)
         ON CONFLICT(id) DO UPDATE SET
           connection_id = excluded.connection_id,
           interval_minutes = excluded.interval_minutes,
           cron_expr = excluded.cron_expr,
           keep_last = excluded.keep_last,
           enabled = excluded.enabled",
        params![sid, connection_id, interval_minutes.max(1), cron, keep_last.max(1), if enabled { 1 } else { 0 }, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(DbBackupSchedule {
        id: sid,
        connection_id,
        interval_minutes: interval_minutes.max(1),
        cron_expr: cron,
        keep_last: keep_last.max(1),
        enabled,
        last_run_at: 0,
        last_status: String::new(),
        created_at: now,
    })
}

#[tauri::command]
pub fn dbm_delete_schedule(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM db_backup_schedules WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn dbm_check_health(
    state: State<'_, Arc<AppState>>,
    connection_id: String,
) -> Result<DbHealthStatus, String> {
    let conn = get_connection_by_id(&state.db, &connection_id)?;
    let tools = detect_tools_status(&state.db_manager);
    let (eff_host, eff_port, tunnel_handle) = maybe_open_ssh_tunnel(&state.db, &conn).await?;
    let start = std::time::Instant::now();

    let mut version = String::new();
    let mut uptime_secs = 0u64;
    let mut active_connections = 0u64;
    let mut table_count = 0u64;
    let mut size_mb = 0.0f64;

    let res: Result<(), String> = match conn.db_type.as_str() {
        "mysql" | "mariadb" => {
            let bin = tools
                .mysql_client
                .as_deref()
                .ok_or("mysql client not found. Install MySQL/MariaDB client first.")?;
            let q = format!(
                "SELECT VERSION(); SHOW GLOBAL STATUS WHERE Variable_name IN ('Uptime','Threads_connected'); SELECT COUNT(*), ROUND(COALESCE(SUM(data_length + index_length), 0) / 1048576, 2) FROM information_schema.tables WHERE table_schema = '{}';",
                conn.database_name.replace('\'', "''")
            );
            let mut cmd = hidden_cmd(bin);
            cmd.args(["-h", &eff_host, "-P", &eff_port.to_string(), "-u", &conn.username, "-N", "-B", "-e", &q]);
            if !conn.password.is_empty() {
                cmd.env("MYSQL_PWD", &conn.password);
            }
            let out = cmd.output().map_err(|e| e.to_string())?;
            if !out.status.success() {
                Err(String::from_utf8_lossy(&out.stderr).to_string())
            } else {
                let text = String::from_utf8_lossy(&out.stdout);
                let lines: Vec<&str> = text.lines().collect();
                if let Some(v) = lines.first() {
                    version = v.trim().to_string();
                }
                for l in &lines {
                    let parts: Vec<&str> = l.split('\t').collect();
                    if parts.len() == 2 {
                        match parts[0] {
                            "Uptime" => uptime_secs = parts[1].trim().parse().unwrap_or(0),
                            "Threads_connected" => active_connections = parts[1].trim().parse().unwrap_or(0),
                            _ => {
                                if let Ok(tc) = parts[0].trim().parse::<u64>() {
                                    table_count = tc;
                                    size_mb = parts[1].trim().parse().unwrap_or(0.0);
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
        }
        "postgres" | "postgresql" => {
            let bin = tools
                .psql_client
                .as_deref()
                .ok_or("psql client not found. Install PostgreSQL client first.")?;
            let q = "SELECT version(), (SELECT count(*) FROM pg_stat_activity), (SELECT count(*) FROM information_schema.tables WHERE table_schema='public'), ROUND(pg_database_size(current_database())/1048576.0, 2);";
            let mut cmd = hidden_cmd(bin);
            cmd.args(["-h", &eff_host, "-p", &eff_port.to_string(), "-U", &conn.username, "-d", &conn.database_name, "-t", "-A", "-F", "|", "-c", q]);
            if !conn.password.is_empty() {
                cmd.env("PGPASSWORD", &conn.password);
            }
            let out = cmd.output().map_err(|e| e.to_string())?;
            if !out.status.success() {
                Err(String::from_utf8_lossy(&out.stderr).to_string())
            } else {
                let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let parts: Vec<&str> = line.split('|').collect();
                if let Some(v) = parts.first() {
                    version = v.split(',').next().unwrap_or(v).trim().to_string();
                }
                if let Some(a) = parts.get(1) {
                    active_connections = a.trim().parse().unwrap_or(0);
                }
                if let Some(t) = parts.get(2) {
                    table_count = t.trim().parse().unwrap_or(0);
                }
                if let Some(s) = parts.get(3) {
                    size_mb = s.trim().parse().unwrap_or(0.0);
                }
                Ok(())
            }
        }
        "sqlite" => {
            let bin = tools.sqlite_client.as_deref().ok_or("sqlite3 client not found")?;
            let out = hidden_cmd(bin)
                .args([&conn.database_name, "SELECT sqlite_version(), (SELECT count(*) FROM sqlite_master WHERE type='table');"])
                .output()
                .map_err(|e| e.to_string())?;
            let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let parts: Vec<&str> = line.split('|').collect();
            if let Some(v) = parts.first() {
                version = format!("SQLite {}", v.trim());
            }
            if let Some(t) = parts.get(1) {
                table_count = t.trim().parse().unwrap_or(0);
            }
            size_mb = std::fs::metadata(&conn.database_name)
                .map(|m| (m.len() as f64) / 1_048_576.0)
                .unwrap_or(0.0);
            Ok(())
        }
        other => Err(format!("Unsupported db_type '{}'", other)),
    };

    if let Some(h) = tunnel_handle {
        h.abort();
    }

    let latency_ms = start.elapsed().as_millis() as u64;
    match res {
        Ok(()) => Ok(DbHealthStatus {
            connection_id,
            ok: true,
            latency_ms,
            version,
            uptime_secs,
            active_connections,
            table_count,
            size_mb,
            message: "Database healthy".to_string(),
            checked_at: now_unix_secs(),
        }),
        Err(e) => Ok(DbHealthStatus {
            connection_id,
            ok: false,
            latency_ms,
            version,
            uptime_secs,
            active_connections,
            table_count,
            size_mb,
            message: e,
            checked_at: now_unix_secs(),
        }),
    }
}

fn cron_field_matches(field: &str, value: i64) -> bool {
    for part in field.split(',') {
        let p = part.trim();
        if p == "*" {
            return true;
        }
        if let Some(step_str) = p.strip_prefix("*/") {
            if let Ok(step) = step_str.parse::<i64>() {
                if step > 0 && value % step == 0 {
                    return true;
                }
            }
        } else if let Some((a_str, b_str)) = p.split_once('-') {
            if let (Ok(a), Ok(b)) = (a_str.trim().parse::<i64>(), b_str.trim().parse::<i64>()) {
                if value >= a && value <= b {
                    return true;
                }
            }
        } else if let Ok(exact) = p.parse::<i64>() {
            if value == exact {
                return true;
            }
        }
    }
    false
}

fn cron_matches_local_now(db: &Database, expr: &str) -> bool {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return false;
    }
    let Ok(conn) = db.conn.lock() else {
        return false;
    };
    let res: Result<(i64, i64, i64, i64, i64), _> = conn.query_row(
        "SELECT CAST(strftime('%M','now','localtime') AS INTEGER),
                CAST(strftime('%H','now','localtime') AS INTEGER),
                CAST(strftime('%d','now','localtime') AS INTEGER),
                CAST(strftime('%m','now','localtime') AS INTEGER),
                CAST(strftime('%w','now','localtime') AS INTEGER)",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    );
    let Ok((min, hour, dom, month, dow)) = res else {
        return false;
    };
    cron_field_matches(parts[0], min)
        && cron_field_matches(parts[1], hour)
        && cron_field_matches(parts[2], dom)
        && cron_field_matches(parts[3], month)
        && cron_field_matches(parts[4], dow)
}

pub fn start_db_backup_scheduler(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let schedules = list_schedules(&state.db).unwrap_or_default();
            let now = now_unix_secs() as i64;
            for sched in schedules {
                if !sched.enabled {
                    continue;
                }
                let due = if !sched.cron_expr.trim().is_empty() {
                    now - sched.last_run_at >= 55 && cron_matches_local_now(&state.db, &sched.cron_expr)
                } else {
                    now - sched.last_run_at >= sched.interval_minutes * 60
                };
                if !due {
                    continue;
                }
                let tools = detect_tools_status(&state.db_manager);
                let res = run_backup_for_connection(&state.db, &tools, &sched.connection_id).await;
                let status_str = match &res {
                    Ok(_) => "ok".to_string(),
                    Err(e) => format!("err: {}", e.chars().take(120).collect::<String>()),
                };
                if let Ok(conn) = state.db.conn.lock() {
                    let _ = conn.execute(
                        "UPDATE db_backup_schedules SET last_run_at = ?1, last_status = ?2 WHERE id = ?3",
                        params![now, status_str, sched.id],
                    );
                }
                let _ = app.emit("dbm_backup_completed", serde_json::json!({
                    "connection_id": sched.connection_id,
                    "ok": res.is_ok(),
                    "status": status_str,
                }));
            }
        }
    });
}
