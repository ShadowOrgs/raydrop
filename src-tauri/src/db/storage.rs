use crate::crypto::identity::DeviceIdentity;
use directories::{ProjectDirs, UserDirs};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub device_id: String,
    pub alias_name: String,
    pub custom_name: Option<String>,
    pub user_email: Option<String>,
    pub user_name: Option<String>,
    pub os_type: Option<String>,
    pub is_own_device: bool,
    pub ed25519_pubkey: Option<String>,
    pub network_type: String, // "lan" or "wan"
    pub last_known_ip: Option<String>,
    pub last_known_port: Option<u16>,
    pub trust_level: String, // "ask", "trusted_auto_accept", "blocked"
    pub is_online: bool,
    pub last_seen_at: u64,
    pub created_at: u64,
    pub avatar_hash: Option<String>,
    #[serde(default)]
    pub rank_exp: u64,
    #[serde(default)]
    pub mac_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRecord {
    pub id: String,
    pub peer_device_id: String,
    pub peer_alias: String,
    pub direction: String,     // "incoming" or "outgoing"
    pub network_route: String, // "lan_direct" or "wan_p2p"
    pub total_files: usize,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub status: String, // "in_progress", "paused", "completed", "cancelled", "failed", "rejected"
    pub target_path: Option<String>,
    pub source_paths: Option<String>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub peer_device_id: String,
    pub direction: String, // "incoming" or "outgoing"
    pub content: String,
    pub msg_type: String, // "text" or "file_transfer"
    pub transfer_session_id: Option<String>,
    pub files_json: Option<String>,
    pub is_read: bool,
    pub created_at: u64,
    pub sender_device_id: Option<String>,
    pub sender_alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMember {
    pub device_id: String,
    pub alias: String,
    pub avatar: Option<String>,
    pub role: String, // "owner", "admin", "member"
    pub joined_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub avatar: Option<String>,
    pub owner_id: String,
    pub send_permission: String, // "all", "admins_only", "custom"
    pub allowed_senders: Vec<String>,
    pub members: Vec<GroupMember>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderSyncTask {
    pub id: String,
    pub local_path: String,
    pub target_device_id: String,
    pub target_alias: String,
    pub is_auto_sync: bool,
    pub last_sync_at: Option<u64>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFolder {
    pub id: String,
    pub name: String,
    pub local_path: String,
    pub is_active: bool,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub device_id: String,
    pub alias_name: String,
    pub user_id: Option<String>,
    pub user_email: Option<String>,
    pub user_name: Option<String>,
    pub auth_token: Option<String>,
    pub discoverable: bool,
    pub autostart: bool,
    pub speed_limit_mbps: u32,
    pub download_dir: String,
    pub signaling_url: String,
    pub stun_addr: String,
    pub auto_categorize: bool,
    pub max_concurrent_downloads: u32,
    pub theme: String,
}

pub struct Database {
    conn: Mutex<Connection>,
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn resolve_app_data_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    {
        // On Android, use the internal sandbox data directory for com.raydrop.p2p
        for candidate in &[
            "/data/user/0/com.raydrop.p2p/files",
            "/data/data/com.raydrop.p2p/files",
        ] {
            let p = PathBuf::from(candidate);
            if std::fs::create_dir_all(&p).is_ok() {
                return Ok(p);
            }
        }
        let fallback = PathBuf::from("/data/data/com.raydrop.p2p");
        let _ = std::fs::create_dir_all(&fallback);
        Ok(fallback)
    }

    #[cfg(not(target_os = "android"))]
    {
        if let Ok(override_dir) = std::env::var("RAYDROP_DATA_DIR") {
            let trimmed = override_dir.trim();
            if !trimmed.is_empty() {
                let p = PathBuf::from(trimmed);
                let _ = std::fs::create_dir_all(&p);
                return Ok(p);
            }
        }
        if let Some(proj_dirs) = ProjectDirs::from("com", "raydrop", "Raydrop") {
            let data_dir = proj_dirs.data_dir().to_path_buf();
            std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
            return Ok(data_dir);
        }
        let fallback = PathBuf::from(".");
        Ok(fallback)
    }
}

fn resolve_default_download_dir() -> String {
    #[cfg(target_os = "android")]
    {
        for candidate in &[
            "/storage/emulated/0/Download",
            "/sdcard/Download",
        ] {
            let p = PathBuf::from(candidate);
            if p.exists() || std::fs::create_dir_all(&p).is_ok() {
                return candidate.to_string();
            }
        }
        if let Ok(data_dir) = resolve_app_data_dir() {
            return data_dir.to_string_lossy().to_string();
        }
        "/storage/emulated/0/Download".to_string()
    }

    #[cfg(not(target_os = "android"))]
    {
        UserDirs::new()
            .and_then(|u| u.download_dir().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .to_string_lossy()
            .to_string()
    }
}

impl Database {
    pub fn init() -> Result<(Self, DeviceIdentity, AppSettings), String> {
        let data_dir = resolve_app_data_dir()?;
        let db_path = data_dir.join("storage.db");
        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
        let _ = conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 5000;",
        );

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS local_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS contacts (
                device_id TEXT PRIMARY KEY,
                alias_name TEXT NOT NULL,
                user_email TEXT,
                user_name TEXT,
                os_type TEXT,
                is_own_device INTEGER DEFAULT 0,
                ed25519_pubkey TEXT,
                network_type TEXT NOT NULL,
                last_known_ip TEXT,
                last_known_port INTEGER,
                trust_level TEXT DEFAULT 'ask',
                is_online INTEGER DEFAULT 0,
                last_seen_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS transfer_history (
                id TEXT PRIMARY KEY,
                peer_device_id TEXT NOT NULL,
                peer_alias TEXT NOT NULL,
                direction TEXT NOT NULL,
                network_route TEXT NOT NULL,
                total_files INTEGER NOT NULL,
                total_bytes INTEGER NOT NULL,
                transferred_bytes INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                target_path TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS chat_messages (
                id TEXT PRIMARY KEY,
                peer_device_id TEXT NOT NULL,
                direction TEXT NOT NULL,
                content TEXT NOT NULL,
                msg_type TEXT NOT NULL DEFAULT 'text',
                transfer_session_id TEXT,
                files_json TEXT,
                is_read INTEGER DEFAULT 0,
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_chat_peer_time ON chat_messages(peer_device_id, created_at);

            CREATE TABLE IF NOT EXISTS folder_sync_tasks (
                id TEXT PRIMARY KEY,
                local_path TEXT NOT NULL,
                target_device_id TEXT NOT NULL,
                target_alias TEXT NOT NULL,
                is_auto_sync INTEGER DEFAULT 0,
                last_sync_at INTEGER,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS shared_folders (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                local_path TEXT NOT NULL,
                is_active INTEGER DEFAULT 1,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS chat_groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                avatar TEXT,
                owner_id TEXT NOT NULL,
                send_permission TEXT NOT NULL DEFAULT 'all',
                allowed_senders_json TEXT,
                members_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS clipboard_history (
                id TEXT PRIMARY KEY,
                content_type TEXT NOT NULL,
                content TEXT NOT NULL,
                preview TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                pinned INTEGER DEFAULT 0,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS notes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL DEFAULT '',
                note_type TEXT NOT NULL DEFAULT 'note',
                checklist_json TEXT NOT NULL DEFAULT '[]',
                images_json TEXT NOT NULL DEFAULT '[]',
                color TEXT NOT NULL DEFAULT 'default',
                labels_json TEXT NOT NULL DEFAULT '[]',
                pinned INTEGER DEFAULT 0,
                archived INTEGER DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            ",
        )
        .map_err(|e| e.to_string())?;

        // Migrate existing contacts & transfer_history tables if needed
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN user_email TEXT", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN user_name TEXT", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN os_type TEXT", []);
        let _ = conn.execute(
            "ALTER TABLE contacts ADD COLUMN is_own_device INTEGER DEFAULT 0",
            [],
        );
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN custom_name TEXT", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN avatar_hash TEXT", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN rank_exp INTEGER DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN mac_address TEXT", []);
        let _ = conn.execute("ALTER TABLE transfer_history ADD COLUMN source_paths TEXT", []);
        let _ = conn.execute("ALTER TABLE chat_messages ADD COLUMN sender_device_id TEXT", []);
        let _ = conn.execute("ALTER TABLE chat_messages ADD COLUMN sender_alias TEXT", []);
        // Any transfer left in 'in_progress' from a previous PC shutdown/crash becomes 'paused' (resumable)
        let _ = conn.execute(
            "UPDATE transfer_history SET status = 'paused' WHERE status = 'in_progress'",
            [],
        );

        let db = Self {
            conn: Mutex::new(conn),
        };

        let identity = match db.get_setting("ed25519_secret_hex")? {
            Some(sec) => DeviceIdentity::from_secret_hex(&sec)?,
            None => {
                let id = DeviceIdentity::generate();
                db.set_setting("ed25519_secret_hex", &id.secret_hex())?;
                db.set_setting("ed25519_public_hex", &id.public_hex())?;
                db.set_setting("device_id", &id.device_id)?;
                id
            }
        };

        let short_id: String = identity
            .device_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .skip(3)
            .take(6)
            .collect();
        let host_candidate = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .ok()
            .or_else(sysinfo::System::host_name)
            .map(|h| h.trim().to_string())
            .filter(|h| {
                !h.is_empty()
                    && !h.eq_ignore_ascii_case("localhost")
                    && !h.eq_ignore_ascii_case("raydrop-device")
            });
        let default_alias = match host_candidate {
            Some(h) => format!("{}-{}", h, short_id),
            None => format!("Raydrop-{}", short_id),
        };

        let alias_name = match db.get_setting("alias_name")? {
            Some(a) if !a.trim().is_empty() && a != "Raydrop-Device" => a,
            _ => {
                db.set_setting("alias_name", &default_alias)?;
                default_alias
            }
        };

        let user_id = db.get_setting("user_id")?.filter(|s| !s.is_empty());
        let user_email = db.get_setting("user_email")?.filter(|s| !s.is_empty());
        let user_name = db.get_setting("user_name")?.filter(|s| !s.is_empty());
        let auth_token = db.get_setting("auth_token")?.filter(|s| !s.is_empty());

        let discoverable = match db.get_setting("discoverable")? {
            Some(v) => v == "true",
            None => {
                db.set_setting("discoverable", "true")?;
                true
            }
        };

        let autostart = match db.get_setting("autostart")? {
            Some(v) => v == "true",
            None => {
                db.set_setting("autostart", "true")?;
                true
            }
        };

        let speed_limit_mbps = match db.get_setting("speed_limit_mbps")? {
            Some(s) => s.parse::<u32>().unwrap_or(0),
            None => {
                db.set_setting("speed_limit_mbps", "0")?;
                0
            }
        };

        let default_dl = resolve_default_download_dir();

        let download_dir = match db.get_setting("download_dir")? {
            Some(d) => d,
            None => {
                db.set_setting("download_dir", &default_dl)?;
                default_dl
            }
        };

        let signaling_url = match db.get_setting("signaling_url")? {
            Some(s) if !s.contains("signal.example.com") && !s.contains("gapleh.online") => s,
            _ => {
                let def = "wss://stun.raydrop.web.id/ws";
                db.set_setting("signaling_url", def)?;
                def.to_string()
            }
        };

        let stun_addr = match db.get_setting("stun_addr")? {
            Some(s) if !s.contains("signal.example.com") && !s.contains("gapleh.online") => s,
            _ => {
                let def = "stun.raydrop.web.id:3478";
                db.set_setting("stun_addr", def)?;
                def.to_string()
            }
        };

        let auto_categorize = match db.get_setting("auto_categorize")? {
            Some(v) => v == "true" || v == "1",
            None => {
                let _ = db.set_setting("auto_categorize", "true");
                true
            }
        };

        let max_concurrent_downloads = match db.get_setting("max_concurrent_downloads")? {
            Some(v) => v.parse::<u32>().unwrap_or(3),
            None => {
                let _ = db.set_setting("max_concurrent_downloads", "3");
                3
            }
        };

        let theme = match db.get_setting("theme")? {
            Some(v) if v == "noir" => {
                let _ = db.set_setting("theme", "azure");
                "azure".to_string()
            }
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                let _ = db.set_setting("theme", "azure");
                "azure".to_string()
            }
        };

        let settings = AppSettings {
            device_id: identity.device_id.clone(),
            alias_name,
            user_id,
            user_email,
            user_name,
            auth_token,
            discoverable,
            autostart,
            speed_limit_mbps,
            download_dir,
            signaling_url,
            stun_addr,
            auto_categorize,
            max_concurrent_downloads,
            theme,
        };

        Ok((db, identity, settings))
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT value FROM local_settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO local_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn upsert_contact(&self, c: &Contact) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO contacts (
                device_id, alias_name, user_email, user_name, os_type, is_own_device,
                ed25519_pubkey, network_type, last_known_ip, last_known_port,
                trust_level, is_online, last_seen_at, created_at, custom_name, avatar_hash, rank_exp, mac_address
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
            ON CONFLICT(device_id) DO UPDATE SET
                alias_name = excluded.alias_name,
                user_email = COALESCE(excluded.user_email, contacts.user_email),
                user_name = COALESCE(excluded.user_name, contacts.user_name),
                os_type = COALESCE(excluded.os_type, contacts.os_type),
                is_own_device = excluded.is_own_device,
                ed25519_pubkey = COALESCE(excluded.ed25519_pubkey, contacts.ed25519_pubkey),
                network_type = excluded.network_type,
                last_known_ip = COALESCE(excluded.last_known_ip, contacts.last_known_ip),
                last_known_port = COALESCE(excluded.last_known_port, contacts.last_known_port),
                trust_level = excluded.trust_level,
                is_online = excluded.is_online,
                last_seen_at = CASE
                    WHEN excluded.is_online = 1 OR contacts.is_online = 1 THEN excluded.last_seen_at
                    ELSE contacts.last_seen_at
                END,
                custom_name = COALESCE(excluded.custom_name, contacts.custom_name),
                avatar_hash = COALESCE(excluded.avatar_hash, contacts.avatar_hash),
                rank_exp = MAX(excluded.rank_exp, COALESCE(contacts.rank_exp, 0)),
                mac_address = COALESCE(excluded.mac_address, contacts.mac_address)",
            params![
                c.device_id,
                c.alias_name,
                c.user_email,
                c.user_name,
                c.os_type,
                if c.is_own_device { 1 } else { 0 },
                c.ed25519_pubkey,
                c.network_type,
                c.last_known_ip,
                c.last_known_port,
                c.trust_level,
                if c.is_online { 1 } else { 0 },
                c.last_seen_at as i64,
                c.created_at as i64,
                c.custom_name,
                c.avatar_hash,
                c.rank_exp as i64,
                c.mac_address,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_contact_trust(&self, device_id: &str, trust_level: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE contacts SET trust_level = ?1 WHERE device_id = ?2",
            params![trust_level, device_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_contact_trust_by_email(
        &self,
        user_email: &str,
        trust_level: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE contacts SET trust_level = ?1 WHERE LOWER(user_email) = LOWER(?2)",
            params![trust_level, user_email],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_contact_by_device_id(&self, device_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM contacts WHERE device_id = ?1", params![device_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_contacts_by_email(&self, email: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM contacts WHERE LOWER(user_email) = LOWER(?1)",
            params![email],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_contact_online(&self, device_id: &str, is_online: bool) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let online_int = if is_online { 1 } else { 0 };
        conn.execute(
            "UPDATE contacts
             SET is_online = ?1,
                 last_seen_at = CASE
                     WHEN ?1 = 1 OR is_online = 1 THEN ?2
                     ELSE last_seen_at
                 END
             WHERE device_id = ?3",
            params![online_int, now_unix_secs() as i64, device_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_contact_lan(&self, device_id: &str, ip: &str, port: u16) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE contacts SET last_known_ip = ?1, last_known_port = ?2, network_type = 'lan', is_online = 1, last_seen_at = ?3 WHERE device_id = ?4",
            params![ip, port as i32, now_unix_secs() as i64, device_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_contact_rank_exp(&self, device_id: &str, rank_exp: u64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE contacts SET rank_exp = MAX(?1, COALESCE(rank_exp, 0)) WHERE device_id = ?2",
            params![rank_exp as i64, device_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn compute_my_rank_exp_locked(conn: &Connection) -> Result<u64, String> {
        let read_u64 = |k: &str| -> u64 {
            conn.query_row(
                "SELECT value FROM local_settings WHERE key = ?1",
                params![k],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
        };

        let hist_bytes: u64 = conn
            .query_row(
                "SELECT COALESCE(SUM(MAX(total_bytes, transferred_bytes)), 0) FROM transfer_history WHERE status = 'completed'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            .max(0) as u64;
        let hist_sessions: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transfer_history WHERE status = 'completed'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            .max(0) as u64;
        let chat_count: u64 = conn
            .query_row("SELECT COUNT(*) FROM chat_messages", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
            .max(0) as u64;
        let sync_count: u64 = conn
            .query_row("SELECT COUNT(*) FROM folder_sync_tasks", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
            .max(0) as u64;
        let share_count: u64 = conn
            .query_row("SELECT COUNT(*) FROM shared_folders", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
            .max(0) as u64;

        let rd_secs = read_u64("usage_remote_desktop_secs");
        let meeting_secs = read_u64("usage_meeting_secs");
        let second_screen_secs = read_u64("usage_second_screen_secs");

        let lifetime_bytes = read_u64("analytics_lifetime_sent_bytes")
            .saturating_add(read_u64("analytics_lifetime_received_bytes"))
            .max(hist_bytes);
        let computed = (lifetime_bytes / 524_288)
            .saturating_add(hist_sessions.saturating_mul(15))
            .saturating_add(chat_count.saturating_mul(5))
            .saturating_add(rd_secs / 6)
            .saturating_add(meeting_secs / 6)
            .saturating_add(second_screen_secs / 6)
            .saturating_add(sync_count.saturating_mul(25))
            .saturating_add(share_count.saturating_mul(25));
        let stored = read_u64("my_rank_exp");
        let final_exp = computed.max(stored);
        if final_exp > stored {
            let _ = conn.execute(
                "INSERT INTO local_settings (key, value) VALUES ('my_rank_exp', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![final_exp.to_string()],
            );
        }
        Ok(final_exp)
    }

    pub fn compute_my_rank_exp(&self) -> Result<u64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        Self::compute_my_rank_exp_locked(&conn)
    }

    pub fn add_activity_usage_seconds(
        &self,
        activities: &[String],
        seconds: u64,
    ) -> Result<u64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        if seconds > 0 && !activities.is_empty() {
            let mut seen_keys = std::collections::HashSet::new();
            for act in activities {
                let key = match act.as_str() {
                    "remote_desktop" => "usage_remote_desktop_secs",
                    "meeting" => "usage_meeting_secs",
                    "second_screen" => "usage_second_screen_secs",
                    _ => continue,
                };
                if !seen_keys.insert(key) {
                    continue;
                }
                let prev: u64 = conn
                    .query_row(
                        "SELECT value FROM local_settings WHERE key = ?1",
                        params![key],
                        |r| r.get::<_, String>(0),
                    )
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
                let next = prev.saturating_add(seconds);
                conn.execute(
                    "INSERT INTO local_settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, next.to_string()],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        Self::compute_my_rank_exp_locked(&conn)
    }

    pub fn get_contact(&self, device_id: &str) -> Result<Option<Contact>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT device_id, alias_name, user_email, user_name, os_type, is_own_device,
                    ed25519_pubkey, network_type, last_known_ip, last_known_port,
                    trust_level, is_online, last_seen_at, created_at, custom_name, avatar_hash, COALESCE(rank_exp, 0), mac_address
             FROM contacts WHERE device_id = ?1",
            params![device_id],
            |row| {
                Ok(Contact {
                    device_id: row.get(0)?,
                    alias_name: row.get(1)?,
                    user_email: row.get(2)?,
                    user_name: row.get(3)?,
                    os_type: row.get(4)?,
                    is_own_device: row.get::<_, i64>(5).unwrap_or(0) != 0,
                    ed25519_pubkey: row.get(6)?,
                    network_type: row.get(7)?,
                    last_known_ip: row.get(8)?,
                    last_known_port: row.get::<_, Option<i64>>(9)?.map(|p| p as u16),
                    trust_level: row.get(10)?,
                    is_online: row.get::<_, i64>(11)? != 0,
                    last_seen_at: row.get::<_, i64>(12)? as u64,
                    created_at: row.get::<_, i64>(13)? as u64,
                    custom_name: row.get(14)?,
                    avatar_hash: row.get(15)?,
                    rank_exp: row.get::<_, i64>(16).unwrap_or(0).max(0) as u64,
                    mac_address: row.get(17)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn list_contacts(&self) -> Result<Vec<Contact>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT device_id, alias_name, user_email, user_name, os_type, is_own_device,
                        ed25519_pubkey, network_type, last_known_ip, last_known_port,
                        trust_level, is_online, last_seen_at, created_at, custom_name, avatar_hash, COALESCE(rank_exp, 0), mac_address
                 FROM contacts ORDER BY is_online DESC, last_seen_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok(Contact {
                    device_id: row.get(0)?,
                    alias_name: row.get(1)?,
                    user_email: row.get(2)?,
                    user_name: row.get(3)?,
                    os_type: row.get(4)?,
                    is_own_device: row.get::<_, i64>(5).unwrap_or(0) != 0,
                    ed25519_pubkey: row.get(6)?,
                    network_type: row.get(7)?,
                    last_known_ip: row.get(8)?,
                    last_known_port: row.get::<_, Option<i64>>(9)?.map(|p| p as u16),
                    trust_level: row.get(10)?,
                    is_online: row.get::<_, i64>(11)? != 0,
                    last_seen_at: row.get::<_, i64>(12)? as u64,
                    created_at: row.get::<_, i64>(13)? as u64,
                    custom_name: row.get(14)?,
                    avatar_hash: row.get(15)?,
                    rank_exp: row.get::<_, i64>(16).unwrap_or(0).max(0) as u64,
                    mac_address: row.get(17)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| e.to_string())?);
        }
        Ok(list)
    }

    pub fn set_contact_custom_name(&self, device_id: &str, custom_name: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE contacts SET custom_name = ?1 WHERE device_id = ?2",
            params![custom_name, device_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_contact_avatar_hash(&self, device_id: &str, avatar_hash: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE contacts SET avatar_hash = ?1 WHERE device_id = ?2",
            params![avatar_hash, device_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn has_incoming_chat(&self, peer_device_id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chat_messages WHERE peer_device_id = ?1 AND direction = 'incoming'",
            params![peer_device_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
        Ok(count > 0)
    }

    pub fn has_outgoing_chat(&self, peer_device_id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chat_messages WHERE peer_device_id = ?1 AND direction = 'outgoing'",
            params![peer_device_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
        Ok(count > 0)
    }

    pub fn has_two_way_chat(&self, peer_device_id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT direction) FROM chat_messages WHERE peer_device_id = ?1",
            params![peer_device_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
        Ok(count >= 2)
    }

    pub fn upsert_transfer(&self, t: &TransferRecord) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO transfer_history (
                id, peer_device_id, peer_alias, direction, network_route,
                total_files, total_bytes, transferred_bytes, status, target_path, source_paths, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                transferred_bytes = MAX(excluded.transferred_bytes, transfer_history.transferred_bytes),
                status = excluded.status,
                network_route = excluded.network_route,
                target_path = COALESCE(excluded.target_path, transfer_history.target_path),
                source_paths = COALESCE(excluded.source_paths, transfer_history.source_paths)",
            params![
                t.id,
                t.peer_device_id,
                t.peer_alias,
                t.direction,
                t.network_route,
                t.total_files as i64,
                t.total_bytes as i64,
                t.transferred_bytes as i64,
                t.status,
                t.target_path,
                t.source_paths,
                t.created_at as i64,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_transfer(&self, id: &str) -> Result<Option<TransferRecord>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, peer_device_id, peer_alias, direction, network_route,
                    total_files, total_bytes, transferred_bytes, status, target_path, source_paths, created_at
             FROM transfer_history WHERE id = ?1",
            params![id],
            |row| {
                Ok(TransferRecord {
                    id: row.get(0)?,
                    peer_device_id: row.get(1)?,
                    peer_alias: row.get(2)?,
                    direction: row.get(3)?,
                    network_route: row.get(4)?,
                    total_files: row.get::<_, i64>(5)? as usize,
                    total_bytes: row.get::<_, i64>(6)? as u64,
                    transferred_bytes: row.get::<_, i64>(7)? as u64,
                    status: row.get(8)?,
                    target_path: row.get(9)?,
                    source_paths: row.get(10)?,
                    created_at: row.get::<_, i64>(11)? as u64,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn delete_transfer_record(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM transfer_history WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear_transfer_history(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM transfer_history WHERE status NOT IN ('in_progress', 'paused')",
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_transfers(&self) -> Result<Vec<TransferRecord>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, peer_device_id, peer_alias, direction, network_route,
                        total_files, total_bytes, transferred_bytes, status, target_path, source_paths, created_at
                 FROM transfer_history ORDER BY created_at DESC LIMIT 100",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok(TransferRecord {
                    id: row.get(0)?,
                    peer_device_id: row.get(1)?,
                    peer_alias: row.get(2)?,
                    direction: row.get(3)?,
                    network_route: row.get(4)?,
                    total_files: row.get::<_, i64>(5)? as usize,
                    total_bytes: row.get::<_, i64>(6)? as u64,
                    transferred_bytes: row.get::<_, i64>(7)? as u64,
                    status: row.get(8)?,
                    target_path: row.get(9)?,
                    source_paths: row.get(10)?,
                    created_at: row.get::<_, i64>(11)? as u64,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| e.to_string())?);
        }
        Ok(list)
    }

    pub fn insert_chat_message(&self, msg: &ChatMessage) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO chat_messages (id, peer_device_id, direction, content, msg_type, transfer_session_id, files_json, is_read, created_at, sender_device_id, sender_alias)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                msg.id,
                msg.peer_device_id,
                msg.direction,
                msg.content,
                msg.msg_type,
                msg.transfer_session_id,
                msg.files_json,
                if msg.is_read { 1 } else { 0 },
                msg.created_at as i64,
                msg.sender_device_id,
                msg.sender_alias,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_chat_messages(
        &self,
        peer_device_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ChatMessage>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, peer_device_id, direction, content, msg_type, transfer_session_id, files_json, is_read, created_at, sender_device_id, sender_alias
                 FROM (
                     SELECT id, peer_device_id, direction, content, msg_type, transfer_session_id, files_json, is_read, created_at, sender_device_id, sender_alias
                     FROM chat_messages
                     WHERE peer_device_id = ?1
                     ORDER BY created_at DESC
                     LIMIT ?2 OFFSET ?3
                 )
                 ORDER BY created_at ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![peer_device_id, limit, offset], |row| {
                Ok(ChatMessage {
                    id: row.get(0)?,
                    peer_device_id: row.get(1)?,
                    direction: row.get(2)?,
                    content: row.get(3)?,
                    msg_type: row.get(4)?,
                    transfer_session_id: row.get(5)?,
                    files_json: row.get(6)?,
                    is_read: row.get::<_, i64>(7)? != 0,
                    created_at: row.get::<_, i64>(8)? as u64,
                    sender_device_id: row.get(9)?,
                    sender_alias: row.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| e.to_string())?);
        }
        Ok(list)
    }

    // ----------------------------------------------------
    // Chat Groups Operations
    // ----------------------------------------------------
    pub fn upsert_chat_group(&self, group: &ChatGroup) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let allowed_senders_json = serde_json::to_string(&group.allowed_senders).unwrap_or_else(|_| "[]".to_string());
        let members_json = serde_json::to_string(&group.members).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT OR REPLACE INTO chat_groups (id, name, description, avatar, owner_id, send_permission, allowed_senders_json, members_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                group.id,
                group.name,
                group.description,
                group.avatar,
                group.owner_id,
                group.send_permission,
                allowed_senders_json,
                members_json,
                group.created_at as i64,
                group.updated_at as i64,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_chat_group(&self, id: &str) -> Result<Option<ChatGroup>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, avatar, owner_id, send_permission, allowed_senders_json, members_json, created_at, updated_at
                 FROM chat_groups WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;

        let mut rows = stmt
            .query_map(params![id], |row| {
                let allowed_raw: String = row.get(6)?;
                let members_raw: String = row.get(7)?;
                let allowed_senders: Vec<String> = serde_json::from_str(&allowed_raw).unwrap_or_default();
                let members: Vec<GroupMember> = serde_json::from_str(&members_raw).unwrap_or_default();
                Ok(ChatGroup {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    avatar: row.get(3)?,
                    owner_id: row.get(4)?,
                    send_permission: row.get(5)?,
                    allowed_senders,
                    members,
                    created_at: row.get::<_, i64>(8)? as u64,
                    updated_at: row.get::<_, i64>(9)? as u64,
                })
            })
            .map_err(|e| e.to_string())?;

        if let Some(r) = rows.next() {
            Ok(Some(r.map_err(|e| e.to_string())?))
        } else {
            Ok(None)
        }
    }

    pub fn list_chat_groups(&self) -> Result<Vec<ChatGroup>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, avatar, owner_id, send_permission, allowed_senders_json, members_json, created_at, updated_at
                 FROM chat_groups ORDER BY updated_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                let allowed_raw: String = row.get(6)?;
                let members_raw: String = row.get(7)?;
                let allowed_senders: Vec<String> = serde_json::from_str(&allowed_raw).unwrap_or_default();
                let members: Vec<GroupMember> = serde_json::from_str(&members_raw).unwrap_or_default();
                Ok(ChatGroup {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    avatar: row.get(3)?,
                    owner_id: row.get(4)?,
                    send_permission: row.get(5)?,
                    allowed_senders,
                    members,
                    created_at: row.get::<_, i64>(8)? as u64,
                    updated_at: row.get::<_, i64>(9)? as u64,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| e.to_string())?);
        }
        Ok(list)
    }

    pub fn delete_chat_group(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM chat_groups WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn mark_chat_read(&self, peer_device_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE chat_messages SET is_read = 1 WHERE peer_device_id = ?1 AND is_read = 0",
            params![peer_device_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear_chat_messages(&self, peer_device_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM chat_messages WHERE peer_device_id = ?1",
            params![peer_device_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_unread_chat_counts(&self) -> Result<std::collections::HashMap<String, u32>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT peer_device_id, COUNT(*)
                 FROM chat_messages
                 WHERE is_read = 0 AND direction = 'incoming'
                 GROUP BY peer_device_id",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                let peer_id: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((peer_id, count as u32))
            })
            .map_err(|e| e.to_string())?;

        let mut map = std::collections::HashMap::new();
        for r in rows {
            let (k, v) = r.map_err(|e| e.to_string())?;
            map.insert(k, v);
        }
        Ok(map)
    }

    pub fn insert_folder_sync_task(&self, task: &FolderSyncTask) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO folder_sync_tasks 
             (id, local_path, target_device_id, target_alias, is_auto_sync, last_sync_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                task.id,
                task.local_path,
                task.target_device_id,
                task.target_alias,
                if task.is_auto_sync { 1 } else { 0 },
                task.last_sync_at,
                task.created_at
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_folder_sync_tasks(&self) -> Result<Vec<FolderSyncTask>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, local_path, target_device_id, target_alias, is_auto_sync, last_sync_at, created_at
                 FROM folder_sync_tasks
                 ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                let auto: i64 = row.get(4)?;
                Ok(FolderSyncTask {
                    id: row.get(0)?,
                    local_path: row.get(1)?,
                    target_device_id: row.get(2)?,
                    target_alias: row.get(3)?,
                    is_auto_sync: auto != 0,
                    last_sync_at: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| e.to_string())?);
        }
        Ok(list)
    }

    pub fn get_folder_sync_task(&self, id: &str) -> Result<Option<FolderSyncTask>, String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, local_path, target_device_id, target_alias, is_auto_sync, last_sync_at, created_at
             FROM folder_sync_tasks
             WHERE id = ?1",
            params![id],
            |row| {
                let auto: i64 = row.get(4)?;
                Ok(FolderSyncTask {
                    id: row.get(0)?,
                    local_path: row.get(1)?,
                    target_device_id: row.get(2)?,
                    target_alias: row.get(3)?,
                    is_auto_sync: auto != 0,
                    last_sync_at: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn update_folder_sync_last_time(&self, id: &str, last_time: u64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE folder_sync_tasks SET last_sync_at = ?1 WHERE id = ?2",
            params![last_time, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_folder_sync_auto_flag(&self, id: &str, is_auto: bool) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE folder_sync_tasks SET is_auto_sync = ?1 WHERE id = ?2",
            params![if is_auto { 1 } else { 0 }, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_folder_sync_task(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM folder_sync_tasks WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn insert_shared_folder(&self, folder: &SharedFolder) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO shared_folders (id, name, local_path, is_active, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                folder.id,
                folder.name,
                folder.local_path,
                if folder.is_active { 1 } else { 0 },
                folder.created_at
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_shared_folders(&self) -> Result<Vec<SharedFolder>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, local_path, is_active, created_at
                 FROM shared_folders
                 ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                let active: i64 = row.get(3)?;
                Ok(SharedFolder {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    local_path: row.get(2)?,
                    is_active: active != 0,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| e.to_string())?);
        }
        Ok(list)
    }

    pub fn get_shared_folder(&self, id: &str) -> Result<Option<SharedFolder>, String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, local_path, is_active, created_at
             FROM shared_folders
             WHERE id = ?1",
            params![id],
            |row| {
                let active: i64 = row.get(3)?;
                Ok(SharedFolder {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    local_path: row.get(2)?,
                    is_active: active != 0,
                    created_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn delete_shared_folder(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM shared_folders WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_transfer_analytics(&self) -> Result<TransferAnalytics, String> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT direction, network_route, status, total_files, total_bytes, transferred_bytes, created_at
                 FROM transfer_history",
            )
            .map_err(|e| e.to_string())?;

        let mut sent_bytes_hist: u64 = 0;
        let mut received_bytes_hist: u64 = 0;
        let mut sent_files_hist: u64 = 0;
        let mut received_files_hist: u64 = 0;
        let mut sent_sessions: u64 = 0;
        let mut received_sessions: u64 = 0;
        let mut failed_sessions: u64 = 0;
        let mut lan_bytes: u64 = 0;
        let mut wan_bytes: u64 = 0;
        let mut http_bytes: u64 = 0;

        let mut day_map: std::collections::BTreeMap<String, (u64, u64)> =
            std::collections::BTreeMap::new();

        let rows = stmt
            .query_map([], |row| {
                let direction: String = row.get(0)?;
                let route: String = row.get(1)?;
                let status: String = row.get(2)?;
                let total_files: i64 = row.get(3)?;
                let total_bytes: i64 = row.get(4)?;
                let transferred_bytes: i64 = row.get(5)?;
                let created_at: i64 = row.get(6)?;
                Ok((
                    direction,
                    route,
                    status,
                    total_files.max(0) as u64,
                    total_bytes.max(0) as u64,
                    transferred_bytes.max(0) as u64,
                    created_at,
                ))
            })
            .map_err(|e| e.to_string())?;

        for r in rows {
            if let Ok((dir, route, status, files, total_b, xfer_b, created_at)) = r {
                let effective_bytes = if status == "completed" {
                    total_b.max(xfer_b)
                } else {
                    xfer_b
                };

                if status == "failed" {
                    failed_sessions += 1;
                }

                if status == "completed" {
                    if dir == "outgoing" {
                        sent_bytes_hist = sent_bytes_hist.saturating_add(effective_bytes);
                        sent_files_hist = sent_files_hist.saturating_add(files.max(1));
                        sent_sessions += 1;
                    } else {
                        received_bytes_hist = received_bytes_hist.saturating_add(effective_bytes);
                        received_files_hist = received_files_hist.saturating_add(files.max(1));
                        received_sessions += 1;
                    }

                    if route.contains("lan") {
                        lan_bytes = lan_bytes.saturating_add(effective_bytes);
                    } else if route.contains("http") {
                        http_bytes = http_bytes.saturating_add(effective_bytes);
                    } else {
                        wan_bytes = wan_bytes.saturating_add(effective_bytes);
                    }
                }

                if effective_bytes > 0 && created_at > 0 {
                    let day_secs = (created_at / 86400) * 86400;
                    let key = format!("{}", day_secs);
                    let entry = day_map.entry(key).or_insert((0, 0));
                    if dir == "outgoing" {
                        entry.0 = entry.0.saturating_add(effective_bytes);
                    } else {
                        entry.1 = entry.1.saturating_add(effective_bytes);
                    }
                }
            }
        }
        drop(stmt);

        // Persist lifetime cumulative high-water marks in local_settings so stats survive history cleanup
        let read_u64 = |k: &str| -> u64 {
            conn.query_row(
                "SELECT value FROM local_settings WHERE key = ?1",
                params![k],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
        };
        let write_u64 = |k: &str, v: u64| {
            let _ = conn.execute(
                "INSERT INTO local_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![k, v.to_string()],
            );
        };

        let lifetime_sent_bytes = read_u64("analytics_lifetime_sent_bytes").max(sent_bytes_hist);
        let lifetime_received_bytes =
            read_u64("analytics_lifetime_received_bytes").max(received_bytes_hist);
        let lifetime_sent_files = read_u64("analytics_lifetime_sent_files").max(sent_files_hist);
        let lifetime_received_files =
            read_u64("analytics_lifetime_received_files").max(received_files_hist);

        write_u64("analytics_lifetime_sent_bytes", lifetime_sent_bytes);
        write_u64("analytics_lifetime_received_bytes", lifetime_received_bytes);
        write_u64("analytics_lifetime_sent_files", lifetime_sent_files);
        write_u64("analytics_lifetime_received_files", lifetime_received_files);

        // Last 7 days bucketed
        let now_day = ((now_unix_secs() as i64) / 86400) * 86400;
        let mut daily = Vec::with_capacity(7);
        for offset in (0..7).rev() {
            let d_ts = now_day - offset * 86400;
            let k = format!("{}", d_ts);
            let (s_b, r_b) = day_map.get(&k).copied().unwrap_or((0, 0));
            daily.push(DailyTransferStat {
                day_epoch_secs: d_ts as u64,
                sent_bytes: s_b,
                received_bytes: r_b,
            });
        }

        Ok(TransferAnalytics {
            total_sent_bytes: lifetime_sent_bytes,
            total_received_bytes: lifetime_received_bytes,
            total_sent_files: lifetime_sent_files,
            total_received_files: lifetime_received_files,
            sent_sessions,
            received_sessions,
            failed_sessions,
            lan_bytes,
            wan_bytes,
            http_bytes,
            daily,
        })
    }

    pub fn record_clipboard_entry(&self, content: &str) -> Result<bool, String> {
        let trimmed = content.trim();
        if trimmed.is_empty() || content.len() > 6_000_000 {
            return Ok(false);
        }
        let content_hash = blake3::hash(content.as_bytes()).to_hex().to_string();
        let now = now_unix_secs() as i64;
        let conn = self.conn.lock().map_err(|e| e.to_string())?;

        // Check if this exact content already exists; if so, bump its timestamp
        let existing_id: Option<String> = conn
            .query_row(
                "SELECT id FROM clipboard_history WHERE content_hash = ?1 LIMIT 1",
                params![&content_hash],
                |r| r.get(0),
            )
            .ok();

        if let Some(id) = existing_id {
            conn.execute(
                "UPDATE clipboard_history SET created_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(|e| e.to_string())?;
            return Ok(true);
        }

        let is_image = trimmed.starts_with("data:image/");
        let content_type = if is_image { "image" } else { "text" };
        let preview = if is_image {
            "Image Snapshot".to_string()
        } else {
            trimmed.chars().take(200).collect::<String>()
        };
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO clipboard_history (id, content_type, content, preview, content_hash, pinned, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
            params![id, content_type, content, preview, content_hash, now],
        )
        .map_err(|e| e.to_string())?;

        // Keep at most 30 unpinned items (pinned items are never auto-deleted)
        let _ = conn.execute(
            "DELETE FROM clipboard_history
             WHERE pinned = 0 AND id NOT IN (
                 SELECT id FROM clipboard_history WHERE pinned = 0 ORDER BY created_at DESC LIMIT 30
             )",
            [],
        );

        Ok(true)
    }

    pub fn list_clipboard_history(&self, limit: usize) -> Result<Vec<ClipboardHistoryItem>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "DELETE FROM clipboard_history
             WHERE pinned = 0 AND id NOT IN (
                 SELECT id FROM clipboard_history WHERE pinned = 0 ORDER BY created_at DESC LIMIT 30
             )",
            [],
        );
        let mut stmt = conn
            .prepare(
                "SELECT id, content_type, content, preview, pinned, created_at
                 FROM clipboard_history
                 ORDER BY pinned DESC, created_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok(ClipboardHistoryItem {
                    id: r.get(0)?,
                    content_type: r.get(1)?,
                    content: r.get(2)?,
                    preview: r.get(3)?,
                    pinned: r.get::<_, i64>(4).unwrap_or(0) != 0,
                    created_at: r.get::<_, i64>(5).unwrap_or(0) as u64,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            if let Ok(item) = row {
                out.push(item);
            }
        }
        Ok(out)
    }

    pub fn get_clipboard_history_content(&self, id: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let res = conn
            .query_row(
                "SELECT content FROM clipboard_history WHERE id = ?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .ok();
        Ok(res)
    }

    pub fn toggle_pin_clipboard_item(&self, id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE clipboard_history SET pinned = CASE WHEN pinned = 0 THEN 1 ELSE 0 END WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        let pinned: i64 = conn
            .query_row(
                "SELECT pinned FROM clipboard_history WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Ok(pinned != 0)
    }

    pub fn delete_clipboard_item(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM clipboard_history WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear_unpinned_clipboard_history(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM clipboard_history WHERE pinned = 0", [])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_notes(&self, include_archived: bool) -> Result<Vec<NoteItem>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let sql = if include_archived {
            "SELECT id, title, content, note_type, checklist_json, images_json, color, labels_json, pinned, archived, created_at, updated_at
             FROM notes
             ORDER BY pinned DESC, updated_at DESC"
        } else {
            "SELECT id, title, content, note_type, checklist_json, images_json, color, labels_json, pinned, archived, created_at, updated_at
             FROM notes
             WHERE archived = 0
             ORDER BY pinned DESC, updated_at DESC"
        };
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                let checklist_str: String = r.get(4)?;
                let images_str: String = r.get(5)?;
                let labels_str: String = r.get(7)?;
                Ok(NoteItem {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    content: r.get(2)?,
                    note_type: r.get(3)?,
                    checklist: serde_json::from_str(&checklist_str).unwrap_or_default(),
                    images: serde_json::from_str(&images_str).unwrap_or_default(),
                    color: r.get(6)?,
                    labels: serde_json::from_str(&labels_str).unwrap_or_default(),
                    pinned: r.get::<_, i64>(8).unwrap_or(0) != 0,
                    archived: r.get::<_, i64>(9).unwrap_or(0) != 0,
                    created_at: r.get::<_, i64>(10).unwrap_or(0) as u64,
                    updated_at: r.get::<_, i64>(11).unwrap_or(0) as u64,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            if let Ok(item) = row {
                out.push(item);
            }
        }
        Ok(out)
    }

    pub fn upsert_note(&self, mut note: NoteItem) -> Result<NoteItem, String> {
        let now = now_unix_secs();
        if note.id.trim().is_empty() {
            note.id = uuid::Uuid::new_v4().to_string();
        }
        if note.created_at == 0 {
            note.created_at = now;
        }
        note.updated_at = now;
        let checklist_json = serde_json::to_string(&note.checklist).unwrap_or_else(|_| "[]".into());
        let images_json = serde_json::to_string(&note.images).unwrap_or_else(|_| "[]".into());
        let labels_json = serde_json::to_string(&note.labels).unwrap_or_else(|_| "[]".into());

        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO notes (id, title, content, note_type, checklist_json, images_json, color, labels_json, pinned, archived, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
               title = excluded.title,
               content = excluded.content,
               note_type = excluded.note_type,
               checklist_json = excluded.checklist_json,
               images_json = excluded.images_json,
               color = excluded.color,
               labels_json = excluded.labels_json,
               pinned = excluded.pinned,
               archived = excluded.archived,
               updated_at = excluded.updated_at",
            params![
                note.id,
                note.title,
                note.content,
                note.note_type,
                checklist_json,
                images_json,
                note.color,
                labels_json,
                if note.pinned { 1 } else { 0 },
                if note.archived { 1 } else { 0 },
                note.created_at as i64,
                note.updated_at as i64,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(note)
    }

    pub fn toggle_pin_note(&self, id: &str) -> Result<bool, String> {
        let now = now_unix_secs() as i64;
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE notes SET pinned = CASE WHEN pinned = 0 THEN 1 ELSE 0 END, updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )
        .map_err(|e| e.to_string())?;
        let pinned: i64 = conn
            .query_row("SELECT pinned FROM notes WHERE id = ?1", params![id], |r| r.get(0))
            .unwrap_or(0);
        Ok(pinned != 0)
    }

    pub fn toggle_archive_note(&self, id: &str) -> Result<bool, String> {
        let now = now_unix_secs() as i64;
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE notes SET archived = CASE WHEN archived = 0 THEN 1 ELSE 0 END, pinned = CASE WHEN archived = 0 THEN 0 ELSE pinned END, updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )
        .map_err(|e| e.to_string())?;
        let archived: i64 = conn
            .query_row("SELECT archived FROM notes WHERE id = ?1", params![id], |r| r.get(0))
            .unwrap_or(0);
        Ok(archived != 0)
    }

    pub fn delete_note(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM notes WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteChecklistEntry {
    pub id: String,
    pub text: String,
    pub checked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteItem {
    pub id: String,
    pub title: String,
    pub content: String,
    pub note_type: String,
    pub checklist: Vec<NoteChecklistEntry>,
    pub images: Vec<String>,
    pub color: String,
    pub labels: Vec<String>,
    pub pinned: bool,
    pub archived: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardHistoryItem {
    pub id: String,
    pub content_type: String,
    pub content: String,
    pub preview: String,
    pub pinned: bool,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyTransferStat {
    pub day_epoch_secs: u64,
    pub sent_bytes: u64,
    pub received_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferAnalytics {
    pub total_sent_bytes: u64,
    pub total_received_bytes: u64,
    pub total_sent_files: u64,
    pub total_received_files: u64,
    pub sent_sessions: u64,
    pub received_sessions: u64,
    pub failed_sessions: u64,
    pub lan_bytes: u64,
    pub wan_bytes: u64,
    pub http_bytes: u64,
    pub daily: Vec<DailyTransferStat>,
}
