use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Arc,
};
use tokio::sync::RwLock;
use tracing::error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpDownloadTaskInfo {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub download_speed_bps: u64,
    pub progress: f32,
    pub status: String, // "queued", "starting", "downloading", "paused", "completed", "error", "cancelled"
    pub segments_count: u32,
    pub save_path: String,
    pub error: Option<String>,
    pub created_at: i64,
}

pub fn get_file_category(filename: &str) -> &'static str {
    let lower = filename.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "epub" | "rtf" | "csv" | "odt" | "ods" => "Documents",
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "3gp" | "ts" => "Videos",
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma" | "opus" | "mid" => "Music",
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" | "tgz" => "Compressed",
        "exe" | "msi" | "dmg" | "pkg" | "deb" | "rpm" | "apk" | "appimage" | "bat" | "cmd" | "sh" => "Programs",
        _ => "General",
    }
}

struct ActiveSegment {
    start: u64,
    end: u64,
    current: Arc<AtomicU64>,
}

#[allow(dead_code)]
struct ActiveDownload {
    id: String,
    url: Arc<RwLock<String>>,
    filename: String,
    total_bytes: u64,
    save_path: PathBuf,
    segments: Vec<ActiveSegment>,
    is_paused: Arc<AtomicBool>,
    is_cancelled: Arc<AtomicBool>,
    status: Arc<RwLock<String>>,
    error_msg: Arc<RwLock<Option<String>>>,
    created_at: i64,
    last_downloaded: Arc<AtomicU64>,
    speed_bps: Arc<AtomicU64>,
}

pub struct HttpDownloadManager {
    tasks: Arc<RwLock<HashMap<String, Arc<ActiveDownload>>>>,
    default_save_dir: Arc<RwLock<PathBuf>>,
    pub auto_categorize: Arc<AtomicBool>,
    pub max_concurrent_downloads: Arc<AtomicU32>,
    client: reqwest::Client,
}

#[cfg(target_os = "windows")]
fn enable_sparse_file(file: &std::fs::File) {
    use std::os::windows::io::AsRawHandle;
    extern "system" {
        fn DeviceIoControl(
            hDevice: *mut std::ffi::c_void,
            dwIoControlCode: u32,
            lpInBuffer: *mut std::ffi::c_void,
            nInBufferSize: u32,
            lpOutBuffer: *mut std::ffi::c_void,
            nOutBufferSize: u32,
            lpBytesReturned: *mut u32,
            lpOverlapped: *mut std::ffi::c_void,
        ) -> i32;
    }
    const FSCTL_SET_SPARSE: u32 = 0x000900C4;
    let mut bytes_returned = 0u32;
    unsafe {
        DeviceIoControl(
            file.as_raw_handle() as *mut std::ffi::c_void,
            FSCTL_SET_SPARSE,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            0,
            &mut bytes_returned,
            std::ptr::null_mut(),
        );
    }
}

impl HttpDownloadManager {
    pub fn new(default_dir: PathBuf, auto_categorize: bool, max_concurrent: u32) -> Self {
        // High-performance streaming client: do NOT set an overall timeout on streaming downloads!
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 Raydrop/1.2")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            default_save_dir: Arc::new(RwLock::new(default_dir)),
            auto_categorize: Arc::new(AtomicBool::new(auto_categorize)),
            max_concurrent_downloads: Arc::new(AtomicU32::new(max_concurrent)),
            client,
        }
    }

    pub fn set_auto_categorize(&self, enabled: bool) {
        self.auto_categorize.store(enabled, Ordering::Relaxed);
    }

    pub fn set_max_concurrent_downloads(&self, limit: u32) {
        self.max_concurrent_downloads.store(limit, Ordering::Relaxed);
        let tasks = self.tasks.clone();
        let max_c = self.max_concurrent_downloads.clone();
        let client = self.client.clone();
        tauri::async_runtime::spawn(async move {
            Self::check_and_start_next_queued(tasks, max_c, client).await;
        });
    }

    pub fn set_save_dir(&self, dir: PathBuf) {
        let save_dir = self.default_save_dir.clone();
        tauri::async_runtime::spawn(async move {
            *save_dir.write().await = dir;
        });
    }

    /// Helper to resolve target filename from response headers or URL pathname
    fn resolve_filename(url_str: &str, headers: &reqwest::header::HeaderMap) -> String {
        // 1. Try Content-Disposition header
        if let Some(cd) = headers.get(reqwest::header::CONTENT_DISPOSITION) {
            if let Ok(cd_str) = cd.to_str() {
                if let Some(pos) = cd_str.find("filename=") {
                    let mut name = cd_str[pos + 9..].trim();
                    if name.starts_with('"') && name.len() >= 2 {
                        if let Some(end) = name[1..].find('"') {
                            name = &name[1..=end];
                        }
                    } else if let Some(end) = name.find(';') {
                        name = &name[..end];
                    }
                    let clean = name.trim().trim_matches('"').trim();
                    if !clean.is_empty() {
                        return clean.to_string();
                    }
                }
            }
        }

        // 2. Parse from URL pathname
        if let Ok(parsed) = reqwest::Url::parse(url_str) {
            let path_segments = parsed.path_segments();
            if let Some(segments) = path_segments {
                let last = segments.last().unwrap_or("").trim();
                if !last.is_empty() && last.contains('.') {
                    return last.to_string();
                }
            }
        }

        // Fallback default
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("download_{ts}.bin")
    }

    /// Add a new HTTP/HTTPS download task with multi-part acceleration
    pub async fn add_download(
        &self,
        url: &str,
        custom_save_dir: Option<String>,
        custom_filename: Option<String>,
    ) -> Result<HttpDownloadTaskInfo, String> {
        let trimmed_url = url.trim();
        if !trimmed_url.starts_with("http://") && !trimmed_url.starts_with("https://") {
            return Err("URL must begin with http:// or https://".to_string());
        }

        let id = format!("{:x}", md5_or_uuid(trimmed_url));

        // Clean up any existing tasks for this URL that were cancelled or errored
        {
            let mut tasks = self.tasks.write().await;
            let mut to_remove = Vec::new();
            for (tid, t) in tasks.iter() {
                let st = t.status.read().await.clone();
                if tid == &id {
                    if st == "downloading" || st == "starting" || st == "queued" {
                        return Err("This URL is already in download queue".to_string());
                    }
                    to_remove.push(tid.clone());
                }
            }
            for old_id in to_remove {
                if let Some(t) = tasks.remove(&old_id) {
                    t.is_cancelled.store(true, Ordering::Relaxed);
                    t.is_paused.store(true, Ordering::Relaxed);
                }
            }
        }

        // 1. Probe remote server: IDM-style Range probe (Range: bytes=0-0)
        let probe_res = self
            .client
            .get(trimmed_url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .timeout(std::time::Duration::from_secs(12))
            .send()
            .await;

        let (total_bytes, supports_range, filename) = match probe_res {
            Ok(resp) if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT => {
                let headers = resp.headers().clone();
                let final_url = resp.url().clone();

                let cl = if let Some(cr) = headers.get(reqwest::header::CONTENT_RANGE) {
                    // Content-Range: bytes 0-0/6250332160
                    if let Ok(cr_str) = cr.to_str() {
                        if let Some(slash_pos) = cr_str.rfind('/') {
                            cr_str[slash_pos + 1..].trim().parse::<u64>().unwrap_or(0)
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                } else {
                    headers
                        .get(reqwest::header::CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0)
                };

                let fname = custom_filename.unwrap_or_else(|| {
                    Self::resolve_filename(final_url.as_str(), &headers)
                });

                (cl, true, fname)
            }
            Ok(resp) if resp.status().is_success() => {
                // Server returned 200 OK (ignored Range header)
                let headers = resp.headers().clone();
                let final_url = resp.url().clone();

                let cl = headers
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);

                let range = headers
                    .get(reqwest::header::ACCEPT_RANGES)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.eq_ignore_ascii_case("bytes"))
                    .unwrap_or(false);

                let fname = custom_filename.unwrap_or_else(|| {
                    Self::resolve_filename(final_url.as_str(), &headers)
                });

                (cl, range, fname)
            }
            _ => {
                // Fallback: try standard HEAD request
                let head_resp = self
                    .client
                    .head(trimmed_url)
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                    .map_err(|e| format!("Failed to reach download server: {e}"))?;

                let headers = head_resp.headers().clone();
                let final_url = head_resp.url().clone();

                let cl = headers
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);

                let range = headers
                    .get(reqwest::header::ACCEPT_RANGES)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.eq_ignore_ascii_case("bytes"))
                    .unwrap_or(false);

                let fname = custom_filename.unwrap_or_else(|| {
                    Self::resolve_filename(final_url.as_str(), &headers)
                });

                (cl, range, fname)
            }
        };

        // Determine destination directory (with automatic category sub-folder sorting if enabled)
        let save_dir = if let Some(csd) = custom_save_dir {
            PathBuf::from(csd)
        } else {
            let base = self.default_save_dir.read().await.clone();
            if self.auto_categorize.load(Ordering::Relaxed) {
                let cat = get_file_category(&filename);
                let cat_dir = base.join(cat);
                let _ = std::fs::create_dir_all(&cat_dir);
                cat_dir
            } else {
                base
            }
        };

        if !save_dir.exists() {
            let _ = std::fs::create_dir_all(&save_dir);
        }

        let target_file_path = save_dir.join(&filename);

        // Remove any old task targeting the exact same filepath that was cancelled/error
        {
            let mut tasks = self.tasks.write().await;
            let mut to_remove = Vec::new();
            for (tid, t) in tasks.iter() {
                let st = t.status.read().await.clone();
                if t.save_path == target_file_path && (st == "cancelled" || st == "error") {
                    to_remove.push(tid.clone());
                }
            }
            for old_id in to_remove {
                if let Some(t) = tasks.remove(&old_id) {
                    t.is_cancelled.store(true, Ordering::Relaxed);
                    t.is_paused.store(true, Ordering::Relaxed);
                }
            }
        }

        // Calculate size of existing file if already on disk (e.g. from an aborted/cancelled previous attempt)
        let existing_file_bytes = if target_file_path.exists() {
            target_file_path.metadata().map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        // PRE-FLIGHT DISK SPACE CHECK: Verify target filesystem has sufficient free capacity
        if total_bytes > 0 {
            if let Some(available_bytes) = crate::os::get_available_disk_space(&save_dir) {
                // Effective available space includes bytes that will be reused/reclaimed from existing partial file
                let effective_available = available_bytes.saturating_add(existing_file_bytes);
                if effective_available < total_bytes {
                    let err = format!(
                        "Insufficient disk space on destination drive. Needed: {}, Available: {}",
                        crate::os::format_bytes(total_bytes),
                        crate::os::format_bytes(effective_available)
                    );
                    error!("Download cancelled: {err}");
                    return Err(err);
                }
            }
        }

        // Calculate parallel segment count (up to 8 parallel streams for large files with range support)
        let segments_count: u32 = if supports_range && total_bytes >= 2 * 1024 * 1024 {
            if total_bytes >= 32 * 1024 * 1024 {
                8
            } else if total_bytes >= 8 * 1024 * 1024 {
                6
            } else {
                4
            }
        } else {
            1
        };

        let mut segments = Vec::new();
        if segments_count > 1 && total_bytes > 0 {
            let chunk_size = total_bytes / (segments_count as u64);
            for i in 0..segments_count {
                let start = (i as u64) * chunk_size;
                let end = if i == segments_count - 1 {
                    total_bytes - 1
                } else {
                    ((i as u64) + 1) * chunk_size - 1
                };
                segments.push(ActiveSegment {
                    start,
                    end,
                    current: Arc::new(AtomicU64::new(start)),
                });
            }
        } else {
            segments.push(ActiveSegment {
                start: 0,
                end: total_bytes,
                current: Arc::new(AtomicU64::new(0)),
            });
        }

        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let active_count = {
            let tasks = self.tasks.read().await;
            let mut count = 0;
            for t in tasks.values() {
                let st = t.status.read().await;
                if *st == "downloading" || *st == "starting" {
                    count += 1;
                }
            }
            count
        };
        let max_concurrent = self.max_concurrent_downloads.load(Ordering::Relaxed);
        let should_queue = max_concurrent > 0 && active_count >= max_concurrent;
        let initial_status = if should_queue { "queued" } else { "downloading" };

        let active = Arc::new(ActiveDownload {
            id: id.clone(),
            url: Arc::new(RwLock::new(trimmed_url.to_string())),
            filename: filename.clone(),
            total_bytes,
            save_path: target_file_path.clone(),
            segments,
            is_paused: Arc::new(AtomicBool::new(false)),
            is_cancelled: Arc::new(AtomicBool::new(false)),
            status: Arc::new(RwLock::new(initial_status.to_string())),
            error_msg: Arc::new(RwLock::new(None)),
            created_at: now_ts,
            last_downloaded: Arc::new(AtomicU64::new(0)),
            speed_bps: Arc::new(AtomicU64::new(0)),
        });

        self.tasks.write().await.insert(id.clone(), active.clone());

        if !should_queue {
            // Spawn background downloader execution
            let active_clone = active.clone();
            let client_clone = self.client.clone();
            let tasks_clone = self.tasks.clone();
            let max_c = self.max_concurrent_downloads.clone();
            tauri::async_runtime::spawn(async move {
                Self::execute_download(active_clone, client_clone.clone(), segments_count, tasks_clone.clone(), max_c.clone()).await;
                Self::check_and_start_next_queued(tasks_clone, max_c, client_clone).await;
            });

            // Spawn speed calculation background ticker for this task
            let active_speed = active.clone();
            tauri::async_runtime::spawn(async move {
                Self::track_speed(active_speed).await;
            });
        }

        Ok(HttpDownloadTaskInfo {
            id,
            url: trimmed_url.to_string(),
            filename,
            total_bytes,
            downloaded_bytes: 0,
            download_speed_bps: 0,
            progress: 0.0,
            status: initial_status.to_string(),
            segments_count,
            save_path: target_file_path.to_string_lossy().to_string(),
            error: None,
            created_at: now_ts,
        })
    }

    /// Background execution of multi-segment parallel downloads
    async fn execute_download(
        task: Arc<ActiveDownload>,
        client: reqwest::Client,
        segments_count: u32,
        tasks_map: Arc<RwLock<HashMap<String, Arc<ActiveDownload>>>>,
        max_concurrent: Arc<AtomicU32>,
    ) {
        // Prepare target file: If fresh download, recreate/truncate sparse file cleanly without zero-filling lag
        let is_resuming = task.segments.iter().any(|s| s.current.load(Ordering::Relaxed) > s.start);
        if task.total_bytes > 0 && (!task.save_path.exists() || !is_resuming) {
            if let Ok(f) = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(!is_resuming)
                .open(&task.save_path)
            {
                #[cfg(target_os = "windows")]
                enable_sparse_file(&f);

                let _ = f.set_len(task.total_bytes);
            }
        }

        if segments_count > 1 && task.total_bytes > 0 {
            let mut join_handles = Vec::new();
            let current_url = task.url.read().await.clone();

            for (idx, seg) in task.segments.iter().enumerate() {
                let seg_current = seg.current.clone();
                let seg_start = seg.start;
                let seg_end = seg.end;
                let url = current_url.clone();
                let save_path = task.save_path.clone();
                let client = client.clone();
                let is_paused = task.is_paused.clone();
                let is_cancelled = task.is_cancelled.clone();

                let handle = tauri::async_runtime::spawn(async move {
                    Self::download_chunk(
                        client,
                        url,
                        save_path,
                        seg_start,
                        seg_end,
                        seg_current,
                        is_paused,
                        is_cancelled,
                        idx,
                    )
                    .await
                });
                join_handles.push(handle);
            }

            let mut any_error = None;
            for h in join_handles {
                match h.await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        error!("Chunk download error: {e}");
                        any_error = Some(e);
                    }
                    Err(e) => {
                        error!("Task join error: {e}");
                        any_error = Some(format!("Task worker error: {e}"));
                    }
                }
            }

            if task.is_cancelled.load(Ordering::Relaxed) {
                *task.status.write().await = "cancelled".to_string();
                task.speed_bps.store(0, Ordering::Relaxed);
            } else if task.is_paused.load(Ordering::Relaxed) {
                *task.status.write().await = "paused".to_string();
                task.speed_bps.store(0, Ordering::Relaxed);
            } else if let Some(err) = any_error {
                *task.status.write().await = "error".to_string();
                *task.error_msg.write().await = Some(err);
                task.speed_bps.store(0, Ordering::Relaxed);
            } else {
                *task.status.write().await = "completed".to_string();
                task.speed_bps.store(0, Ordering::Relaxed);
            }
        } else {
            // Single-stream download with resume support
            Self::download_single_stream(task, client.clone()).await;
        }

        tauri::async_runtime::spawn(async move {
            Self::check_and_start_next_queued(tasks_map, max_concurrent, client).await;
        });
    }

    /// Worker to download a specific byte range and write directly to disk offset
    async fn download_chunk(
        client: reqwest::Client,
        url: String,
        save_path: PathBuf,
        _start_offset: u64,
        end_offset: u64,
        current_offset: Arc<AtomicU64>,
        is_paused: Arc<AtomicBool>,
        is_cancelled: Arc<AtomicBool>,
        chunk_idx: usize,
    ) -> Result<(), String> {
        let cur = current_offset.load(Ordering::Relaxed);
        if cur > end_offset {
            return Ok(());
        }

        let range_header = format!("bytes={cur}-{end_offset}");
        let resp = client
            .get(&url)
            .header(reqwest::header::RANGE, range_header)
            .send()
            .await
            .map_err(|e| format!("Chunk {chunk_idx} request failed: {e}"))?;

        let status = resp.status();
        if status != reqwest::StatusCode::PARTIAL_CONTENT && status != reqwest::StatusCode::OK {
            return Err(format!("Chunk {chunk_idx} failed with status: {status}"));
        }

        use std::fs::OpenOptions;
        #[cfg(target_os = "windows")]
        use std::os::windows::fs::FileExt;

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&save_path)
            .map_err(|e| format!("Failed to open file for chunk {chunk_idx}: {e}"))?;

        let mut stream = resp.bytes_stream();
        let mut write_pos = cur;

        while let Some(chunk_res) = stream.next().await {
            if is_cancelled.load(Ordering::Relaxed) || is_paused.load(Ordering::Relaxed) {
                return Ok(());
            }

            let bytes = chunk_res.map_err(|e| format!("Chunk {chunk_idx} read error: {e}"))?;
            let len = bytes.len() as u64;

            #[cfg(target_os = "windows")]
            {
                file.seek_write(&bytes, write_pos)
                    .map_err(|e| format!("Chunk {chunk_idx} write error: {e}"))?;
            }
            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::FileExt as UnixFileExt;
                file.write_all_at(&bytes, write_pos)
                    .map_err(|e| format!("Chunk {chunk_idx} write error: {e}"))?;
            }

            write_pos += len;
            current_offset.store(write_pos, Ordering::Relaxed);
        }

        Ok(())
    }

    /// Single stream download fallback with resume capability
    async fn download_single_stream(task: Arc<ActiveDownload>, client: reqwest::Client) {
        let current_downloaded = task
            .segments
            .first()
            .map(|s| s.current.load(Ordering::Relaxed))
            .unwrap_or(0);

        let url = task.url.read().await.clone();
        let mut req = client.get(&url);
        if current_downloaded > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={current_downloaded}-"));
        }

        let resp_res = req.send().await;
        let resp = match resp_res {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                if !task.is_paused.load(Ordering::Relaxed) {
                    *task.status.write().await = "error".to_string();
                    *task.error_msg.write().await =
                        Some(format!("Download failed with status: {}", r.status()));
                }
                return;
            }
            Err(e) => {
                if !task.is_paused.load(Ordering::Relaxed) {
                    *task.status.write().await = "error".to_string();
                    *task.error_msg.write().await = Some(format!("Request error: {e}"));
                }
                return;
            }
        };

        let is_resuming_range = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;

        use std::fs::OpenOptions;
        use std::io::Write;

        let file_res = if is_resuming_range {
            OpenOptions::new()
                .write(true)
                .append(true)
                .open(&task.save_path)
        } else {
            // New download or server doesn't support resuming: reset counter and truncate
            if let Some(first_seg) = task.segments.first() {
                first_seg.current.store(0, Ordering::Relaxed);
            }
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&task.save_path)
        };

        let mut file = match file_res {
            Ok(f) => f,
            Err(e) => {
                *task.status.write().await = "error".to_string();
                *task.error_msg.write().await = Some(format!("Failed to open file: {e}"));
                return;
            }
        };

        let mut stream = resp.bytes_stream();

        while let Some(chunk_res) = stream.next().await {
            if task.is_cancelled.load(Ordering::Relaxed) {
                *task.status.write().await = "cancelled".to_string();
                task.speed_bps.store(0, Ordering::Relaxed);
                return;
            }
            if task.is_paused.load(Ordering::Relaxed) {
                *task.status.write().await = "paused".to_string();
                return;
            }

            match chunk_res {
                Ok(bytes) => {
                    let len = bytes.len() as u64;
                    if let Err(e) = file.write_all(&bytes) {
                        *task.status.write().await = "error".to_string();
                        *task.error_msg.write().await = Some(format!("Write error: {e}"));
                        return;
                    }
                    if let Some(first_seg) = task.segments.first() {
                        first_seg.current.fetch_add(len, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    if !task.is_paused.load(Ordering::Relaxed) {
                        *task.status.write().await = "error".to_string();
                        *task.error_msg.write().await = Some(format!("Stream error: {e}"));
                    }
                    return;
                }
            }
        }

        if !task.is_paused.load(Ordering::Relaxed) && !task.is_cancelled.load(Ordering::Relaxed) {
            *task.status.write().await = "completed".to_string();
            task.speed_bps.store(0, Ordering::Relaxed);
        }
    }

    /// Background ticker calculating speed (bytes per second)
    async fn track_speed(task: Arc<ActiveDownload>) {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
        loop {
            interval.tick().await;

            let st = task.status.read().await.clone();
            if st != "downloading" {
                if st == "completed" || st == "error" || st == "cancelled" {
                    task.speed_bps.store(0, Ordering::Relaxed);
                    break;
                }
                task.speed_bps.store(0, Ordering::Relaxed);
                continue;
            }

            let mut current_total = 0u64;
            for s in &task.segments {
                let c = s.current.load(Ordering::Relaxed);
                if c >= s.start {
                    current_total += c - s.start;
                }
            }

            let prev = task.last_downloaded.swap(current_total, Ordering::Relaxed);
            let diff = if current_total >= prev {
                current_total - prev
            } else {
                0
            };

            task.speed_bps.store(diff, Ordering::Relaxed);
        }
    }

    /// List all active and completed HTTP download tasks
    pub async fn list_tasks(&self) -> Vec<HttpDownloadTaskInfo> {
        let tasks = self.tasks.read().await;
        let mut result = Vec::new();

        for (id, t) in tasks.iter() {
            let status = t.status.read().await.clone();
            let error = t.error_msg.read().await.clone();
            let url = t.url.read().await.clone();

            let mut downloaded = 0u64;
            for s in &t.segments {
                let cur = s.current.load(Ordering::Relaxed);
                if cur >= s.start {
                    downloaded += cur - s.start;
                }
            }

            let progress = if t.total_bytes > 0 {
                (downloaded as f32 / t.total_bytes as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };

            result.push(HttpDownloadTaskInfo {
                id: id.clone(),
                url,
                filename: t.filename.clone(),
                total_bytes: t.total_bytes,
                downloaded_bytes: downloaded,
                download_speed_bps: t.speed_bps.load(Ordering::Relaxed),
                progress,
                status,
                segments_count: t.segments.len() as u32,
                save_path: t.save_path.to_string_lossy().to_string(),
                error,
                created_at: t.created_at,
            });
        }

        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        result
    }

    /// Check if queue has waiting tasks and start them if active download slots are available
    fn check_and_start_next_queued(
        tasks: Arc<RwLock<HashMap<String, Arc<ActiveDownload>>>>,
        max_concurrent_downloads: Arc<AtomicU32>,
        client: reqwest::Client,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(async move {
            let max_concurrent = max_concurrent_downloads.load(Ordering::Relaxed);
            if max_concurrent == 0 {
                return;
            }

            let mut active_count = 0;
            let mut queued_tasks: Vec<(i64, Arc<ActiveDownload>)> = Vec::new();

            {
                let t_map = tasks.read().await;
                for t in t_map.values() {
                    let st = t.status.read().await.clone();
                    if st == "downloading" || st == "starting" {
                        active_count += 1;
                    } else if st == "queued" {
                        queued_tasks.push((t.created_at, t.clone()));
                    }
                }
            }

            if active_count < max_concurrent {
                queued_tasks.sort_by_key(|(ts, _)| *ts);
                let slots = (max_concurrent - active_count) as usize;
                for (_, next_t) in queued_tasks.into_iter().take(slots) {
                    *next_t.status.write().await = "downloading".to_string();
                    next_t.is_paused.store(false, Ordering::Relaxed);
                    next_t.is_cancelled.store(false, Ordering::Relaxed);
                    let segs = next_t.segments.len() as u32;

                    let t_exec = next_t.clone();
                    let c_exec = client.clone();
                    let tasks_c = tasks.clone();
                    let max_c = max_concurrent_downloads.clone();

                    tauri::async_runtime::spawn(async move {
                        Self::execute_download(t_exec, c_exec, segs, tasks_c, max_c).await;
                    });

                    let t_spd = next_t.clone();
                    tauri::async_runtime::spawn(async move {
                        Self::track_speed(t_spd).await;
                    });
                }
            }
        })
    }

    /// Pause an active download task
    pub async fn pause_task(&self, id: &str) -> Result<(), String> {
        let tasks = self.tasks.read().await;
        let t = tasks
            .get(id)
            .ok_or_else(|| format!("Task {id} not found"))?;

        t.is_paused.store(true, Ordering::Relaxed);
        *t.status.write().await = "paused".to_string();
        t.speed_bps.store(0, Ordering::Relaxed);
        drop(tasks);

        let tasks_clone = self.tasks.clone();
        let max_c = self.max_concurrent_downloads.clone();
        let client_clone = self.client.clone();
        tauri::async_runtime::spawn(async move {
            Self::check_and_start_next_queued(tasks_clone, max_c, client_clone).await;
        });

        Ok(())
    }

    /// Force start a queued download task immediately without waiting in queue
    pub async fn start_task_now(&self, id: &str) -> Result<(), String> {
        let tasks = self.tasks.read().await;
        let t = tasks
            .get(id)
            .ok_or_else(|| format!("Task {id} not found"))?
            .clone();
        drop(tasks);

        *t.error_msg.write().await = None;
        t.is_paused.store(false, Ordering::Relaxed);
        t.is_cancelled.store(false, Ordering::Relaxed);
        *t.status.write().await = "downloading".to_string();

        let t_clone = t.clone();
        let client_clone = self.client.clone();
        let seg_len = t.segments.len() as u32;
        let tasks_clone = self.tasks.clone();
        let max_c = self.max_concurrent_downloads.clone();

        tauri::async_runtime::spawn(async move {
            Self::execute_download(t_clone, client_clone, seg_len, tasks_clone, max_c).await;
        });

        let t_speed = t.clone();
        tauri::async_runtime::spawn(async move {
            Self::track_speed(t_speed).await;
        });

        Ok(())
    }

    /// Resume a paused or errored download task
    pub async fn resume_task(&self, id: &str) -> Result<(), String> {
        let tasks = self.tasks.read().await;
        let t = tasks
            .get(id)
            .ok_or_else(|| format!("Task {id} not found"))?;

        let cur_status = t.status.read().await.clone();
        if cur_status == "downloading" {
            return Ok(());
        }

        // Reset error message and paused/cancelled flags
        *t.error_msg.write().await = None;
        t.is_paused.store(false, Ordering::Relaxed);
        t.is_cancelled.store(false, Ordering::Relaxed);

        let active_count = {
            let mut count = 0;
            for (tid, other) in tasks.iter() {
                if tid != id {
                    let st = other.status.read().await.clone();
                    if st == "downloading" || st == "starting" {
                        count += 1;
                    }
                }
            }
            count
        };
        let max_concurrent = self.max_concurrent_downloads.load(Ordering::Relaxed);
        if max_concurrent > 0 && active_count >= max_concurrent {
            *t.status.write().await = "queued".to_string();
            return Ok(());
        }

        *t.status.write().await = "downloading".to_string();

        let t_clone = t.clone();
        let client_clone = self.client.clone();
        let seg_len = t.segments.len() as u32;
        let tasks_clone = self.tasks.clone();
        let max_c = self.max_concurrent_downloads.clone();

        tauri::async_runtime::spawn(async move {
            Self::execute_download(t_clone, client_clone, seg_len, tasks_clone, max_c).await;
        });

        let t_speed = t.clone();
        tauri::async_runtime::spawn(async move {
            Self::track_speed(t_speed).await;
        });

        Ok(())
    }

    /// Update download address/URL for an existing task (e.g. IDM-style session expired / new temporary token)
    /// Seamlessly resumes remaining byte chunks without redownloading what's already saved on disk
    pub async fn update_download_url(&self, id: &str, new_url: &str) -> Result<HttpDownloadTaskInfo, String> {
        let trimmed = new_url.trim();
        if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
            return Err("URL must begin with http:// or https://".to_string());
        }

        {
            let tasks = self.tasks.read().await;
            let t = tasks
                .get(id)
                .ok_or_else(|| format!("Task {id} not found"))?;

            // Update live URL pointer and clear previous error
            *t.url.write().await = trimmed.to_string();
            *t.error_msg.write().await = None;
        }

        // Resume remaining chunks
        self.resume_task(id).await?;

        // Return updated task info
        let list = self.list_tasks().await;
        list.into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| "Updated task info not found".to_string())
    }

    /// Match an existing paused or errored task with the same filename and > 0 downloaded bytes
    /// This enables automatic seamless address refresh when the user re-triggers download in the browser
    pub async fn find_matching_refresh_task(&self, filename: &str) -> Option<String> {
        let tasks = self.tasks.read().await;
        for (id, t) in tasks.iter() {
            if t.filename.eq_ignore_ascii_case(filename) {
                let st = t.status.read().await.clone();
                if st == "error" || st == "paused" {
                    let mut downloaded = 0u64;
                    for s in &t.segments {
                        let cur = s.current.load(Ordering::Relaxed);
                        if cur >= s.start {
                            downloaded += cur - s.start;
                        }
                    }
                    if downloaded > 0 {
                        return Some(id.clone());
                    }
                }
            }
        }
        None
    }

    /// Delete a download task and optionally delete downloaded file from disk
    pub async fn delete_task(&self, id: &str, delete_file: bool) -> Result<(), String> {
        let mut tasks = self.tasks.write().await;
        if let Some(t) = tasks.remove(id) {
            t.is_cancelled.store(true, Ordering::Relaxed);
            t.is_paused.store(true, Ordering::Relaxed);
            t.speed_bps.store(0, Ordering::Relaxed);
            drop(tasks);

            let tasks_clone = self.tasks.clone();
            let max_c = self.max_concurrent_downloads.clone();
            let client_clone = self.client.clone();
            tauri::async_runtime::spawn(async move {
                Self::check_and_start_next_queued(tasks_clone, max_c, client_clone).await;
            });

            if delete_file && t.save_path.exists() {
                let save_path = t.save_path.clone();
                if std::fs::remove_file(&save_path).is_err() {
                    // If still briefly held by chunk workers closing handles on Windows, retry asynchronously
                    tauri::async_runtime::spawn(async move {
                        for _ in 0..10 {
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            if !save_path.exists() || std::fs::remove_file(&save_path).is_ok() {
                                break;
                            }
                        }
                    });
                }
            }
            Ok(())
        } else {
            Err(format!("Task {id} not found"))
        }
    }
}

fn md5_or_uuid(input: &str) -> u128 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut s = DefaultHasher::new();
    input.hash(&mut s);
    let h1 = s.finish();
    let mut s2 = DefaultHasher::new();
    (input, "raydrop").hash(&mut s2);
    let h2 = s2.finish();
    ((h1 as u128) << 64) | (h2 as u128)
}
