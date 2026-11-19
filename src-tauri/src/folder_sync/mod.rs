use crate::network::transport::initiate_peer_transfer;
use crate::state::AppState;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub struct FolderSyncManager {
    watchers: Arc<Mutex<HashMap<String, RecommendedWatcher>>>,
    last_triggers: Arc<Mutex<HashMap<String, Instant>>>,
}

impl FolderSyncManager {
    pub fn new() -> Self {
        Self {
            watchers: Arc::new(Mutex::new(HashMap::new())),
            last_triggers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Initialize and start watching all saved tasks with `is_auto_sync == true`
    pub fn init_all_auto_tasks(&self, app: AppHandle, state: Arc<AppState>) {
        if let Ok(tasks) = state.db.list_folder_sync_tasks() {
            for task in tasks {
                if task.is_auto_sync {
                    let path = PathBuf::from(&task.local_path);
                    if path.exists() && path.is_dir() {
                        let _ = self.start_watching(
                            task.id,
                            task.local_path,
                            task.target_device_id,
                            app.clone(),
                            state.clone(),
                        );
                    }
                }
            }
        }
    }

    /// Start watching a folder for real-time changes
    pub fn start_watching(
        &self,
        task_id: String,
        path_str: String,
        target_device_id: String,
        app: AppHandle,
        state: Arc<AppState>,
    ) -> Result<(), String> {
        let path = PathBuf::from(&path_str);
        if !path.exists() || !path.is_dir() {
            return Err("Path is not a valid directory".to_string());
        }

        // Stop existing watcher for this task if any
        self.stop_watching(&task_id);

        let last_triggers = self.last_triggers.clone();
        let tid_for_event = task_id.clone();
        let path_for_sync = path_str.clone();
        let target_dev = target_device_id.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                // Ignore pure access/open events (only trigger on modify, create, remove)
                if event.kind.is_access() {
                    return;
                }

                let now = Instant::now();
                let should_trigger = {
                    let mut map = last_triggers.lock().unwrap();
                    if let Some(prev) = map.get(&tid_for_event) {
                        if now.duration_since(*prev) < Duration::from_secs(4) {
                            false // Debounce: skip rapid writes
                        } else {
                            map.insert(tid_for_event.clone(), now);
                            true
                        }
                    } else {
                        map.insert(tid_for_event.clone(), now);
                        true
                    }
                };

                if should_trigger {
                    let app_c = app.clone();
                    let state_c = state.clone();
                    let tid = tid_for_event.clone();
                    let target_id = target_dev.clone();
                    let p_str = path_for_sync.clone();

                    tauri::async_runtime::spawn(async move {
                        // Debounce sleep to let file saves / copying settle
                        tokio::time::sleep(Duration::from_millis(3000)).await;

                        let path_check = Path::new(&p_str);
                        if !path_check.exists() {
                            return;
                        }

                        let _ = app_c.emit(
                            "folder_sync_mirror_event",
                            serde_json::json!({
                                "task_id": tid,
                                "status": "mirroring",
                                "message": "Changes detected. Mirroring to target device...",
                            }),
                        );

                        match initiate_peer_transfer(
                            app_c.clone(),
                            state_c.clone(),
                            target_id,
                            vec![p_str],
                        )
                        .await
                        {
                            Ok(_) => {
                                let now_sec = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0);
                                let _ = state_c.db.update_folder_sync_last_time(&tid, now_sec);
                                let _ = app_c.emit("folder_sync_updated", ());
                                let _ = app_c.emit(
                                    "folder_sync_mirror_event",
                                    serde_json::json!({
                                        "task_id": tid,
                                        "status": "idle",
                                        "message": "Real-time sync complete.",
                                    }),
                                );
                            }
                            Err(e) => {
                                let _ = app_c.emit(
                                    "folder_sync_mirror_event",
                                    serde_json::json!({
                                        "task_id": tid,
                                        "status": "error",
                                        "message": format!("Sync failed: {e}"),
                                    }),
                                );
                            }
                        }
                    });
                }
            }
        })
        .map_err(|e| format!("Failed to create filesystem watcher: {e}"))?;

        watcher
            .watch(&path, RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to watch directory {path_str}: {e}"))?;

        self.watchers.lock().unwrap().insert(task_id, watcher);
        Ok(())
    }

    /// Stop watching a task
    pub fn stop_watching(&self, task_id: &str) {
        let mut map = self.watchers.lock().unwrap();
        map.remove(task_id);
    }
}
