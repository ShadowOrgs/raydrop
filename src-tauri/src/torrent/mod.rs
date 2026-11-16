use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentTaskInfo {
    pub id: String,
    pub name: String,
    pub magnet_url: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub download_speed_bps: u64,
    pub upload_speed_bps: u64,
    pub progress: f32,
    pub status: String,
    pub peers_connected: u32,
    pub save_path: String,
    pub error: Option<String>,
    pub created_at: i64,
}

pub struct TorrentManager {
    session: Arc<RwLock<Option<Arc<librqbit::Session>>>>,
    active_torrents: Arc<RwLock<HashMap<String, Arc<librqbit::ManagedTorrent>>>>,
    download_dir: Arc<RwLock<PathBuf>>,
    torrent_urls: Arc<RwLock<HashMap<String, String>>>,
    created_timestamps: Arc<RwLock<HashMap<String, i64>>>,
}

impl TorrentManager {
    pub fn new(default_download_dir: PathBuf) -> Self {
        Self {
            session: Arc::new(RwLock::new(None)),
            active_torrents: Arc::new(RwLock::new(HashMap::new())),
            download_dir: Arc::new(RwLock::new(default_download_dir)),
            torrent_urls: Arc::new(RwLock::new(HashMap::new())),
            created_timestamps: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn ensure_session(&self) -> Result<Arc<librqbit::Session>, String> {
        let mut session_guard = self.session.write().await;
        if let Some(s) = session_guard.as_ref() {
            return Ok(s.clone());
        }

        let dl_path = self.download_dir.read().await.clone();
        if !dl_path.exists() {
            let _ = std::fs::create_dir_all(&dl_path);
        }

        let session = librqbit::Session::new(dl_path)
            .await
            .map_err(|e| format!("Failed to initialize BitTorrent engine: {e}"))?;

        *session_guard = Some(session.clone());
        Ok(session)
    }

    pub async fn add_magnet(
        &self,
        magnet_url: &str,
        custom_path: Option<String>,
    ) -> Result<TorrentTaskInfo, String> {
        let trimmed_url = magnet_url.trim();
        if !trimmed_url.starts_with("magnet:?") {
            return Err("Invalid magnet URL: must begin with 'magnet:?'".to_string());
        }

        let session = self.ensure_session().await?;
        let add_torrent = librqbit::AddTorrent::from_url(trimmed_url);

        let opts = custom_path.map(|cp| librqbit::AddTorrentOptions {
            output_folder: Some(cp),
            ..Default::default()
        });

        let response = session
            .add_torrent(add_torrent, opts)
            .await
            .map_err(|e| format!("Failed to add magnet link: {e}"))?;

        let handle = response
            .into_handle()
            .ok_or_else(|| "Failed to acquire torrent handle".to_string())?;

        let info_hash = hex::encode(handle.info_hash().0);
        let name = handle
            .name()
            .unwrap_or_else(|| "Fetching metadata...".to_string());
        let save_path = handle.output_folder().to_string_lossy().to_string();
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        self.torrent_urls
            .write()
            .await
            .insert(info_hash.clone(), trimmed_url.to_string());
        self.created_timestamps
            .write()
            .await
            .insert(info_hash.clone(), now_ts);

        let task_info = TorrentTaskInfo {
            id: info_hash.clone(),
            name,
            magnet_url: trimmed_url.to_string(),
            total_bytes: 0,
            downloaded_bytes: 0,
            uploaded_bytes: 0,
            download_speed_bps: 0,
            upload_speed_bps: 0,
            progress: 0.0,
            status: "fetching_metadata".to_string(),
            peers_connected: 0,
            save_path,
            error: None,
            created_at: now_ts,
        };

        self.active_torrents
            .write()
            .await
            .insert(info_hash, handle);

        Ok(task_info)
    }

    pub async fn list_torrents(&self) -> Vec<TorrentTaskInfo> {
        let torrents = self.active_torrents.read().await;
        let urls = self.torrent_urls.read().await;
        let timestamps = self.created_timestamps.read().await;
        let mut result = Vec::new();

        for (id, handle) in torrents.iter() {
            let stats = handle.stats();
            let name = handle
                .name()
                .unwrap_or_else(|| "Fetching metadata...".to_string());
            let save_path = handle.output_folder().to_string_lossy().to_string();

            let total_bytes = stats.total_bytes;
            let downloaded_bytes = stats.progress_bytes;
            let uploaded_bytes = stats.uploaded_bytes;
            let progress = if total_bytes > 0 {
                (downloaded_bytes as f32 / total_bytes as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let status = match &stats.state {
                librqbit::TorrentStatsState::Live => {
                    if stats.finished {
                        "completed".to_string()
                    } else {
                        "downloading".to_string()
                    }
                }
                librqbit::TorrentStatsState::Paused
                | librqbit::TorrentStatsState::Initializing { paused: true } => {
                    "paused".to_string()
                }
                librqbit::TorrentStatsState::Initializing { paused: false } => {
                    "fetching_metadata".to_string()
                }
                librqbit::TorrentStatsState::Error => "error".to_string(),
            };

            let (download_speed_bps, upload_speed_bps, peers_connected) =
                if let Some(live) = &stats.live {
                    (
                        live.download_speed.as_bytes(),
                        live.upload_speed.as_bytes(),
                        live.snapshot.peer_stats.live,
                    )
                } else {
                    (0, 0, 0)
                };

            result.push(TorrentTaskInfo {
                id: id.clone(),
                name,
                magnet_url: urls.get(id).cloned().unwrap_or_default(),
                total_bytes,
                downloaded_bytes,
                uploaded_bytes,
                download_speed_bps,
                upload_speed_bps,
                progress,
                status,
                peers_connected,
                save_path,
                error: stats.error,
                created_at: timestamps.get(id).cloned().unwrap_or(0),
            });
        }

        result
    }

    pub async fn pause_torrent(&self, id: &str) -> Result<(), String> {
        let torrents = self.active_torrents.read().await;
        let handle = torrents
            .get(id)
            .ok_or_else(|| format!("Torrent {id} not found"))?;

        if let Some(session) = self.session.read().await.as_ref() {
            session
                .pause(handle)
                .await
                .map_err(|e| format!("Failed to pause torrent: {e}"))?;
            Ok(())
        } else {
            Err("Session not initialized".to_string())
        }
    }

    pub async fn resume_torrent(&self, id: &str) -> Result<(), String> {
        let torrents = self.active_torrents.read().await;
        let handle = torrents
            .get(id)
            .ok_or_else(|| format!("Torrent {id} not found"))?;

        if let Some(session) = self.session.read().await.as_ref() {
            session
                .unpause(handle)
                .await
                .map_err(|e| format!("Failed to resume torrent: {e}"))?;
            Ok(())
        } else {
            Err("Session not initialized".to_string())
        }
    }

    pub async fn delete_torrent(&self, id: &str, delete_files: bool) -> Result<(), String> {
        let mut torrents = self.active_torrents.write().await;
        let handle = torrents
            .remove(id)
            .ok_or_else(|| format!("Torrent {id} not found"))?;

        self.torrent_urls.write().await.remove(id);
        self.created_timestamps.write().await.remove(id);

        if let Some(session) = self.session.read().await.as_ref() {
            session
                .delete(librqbit::api::TorrentIdOrHash::Id(handle.id()), delete_files)
                .await
                .map_err(|e| format!("Failed to delete torrent: {e}"))?;
            Ok(())
        } else {
            Err("Session not initialized".to_string())
        }
    }
}
