use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as AsyncMutex;

static YTDLP_INSTALL_LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaTrack {
    pub id: String,
    pub source_type: String, // "local" | "ytmusic"
    pub title: String,
    pub artist: String,
    pub duration_secs: u64,
    pub uri_or_id: String, // local file path or YouTube videoId
    pub thumbnail_url: Option<String>,
    pub is_video: bool,
    pub stream_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaPlayerState {
    pub current_track: Option<MediaTrack>,
    pub queue: Vec<MediaTrack>,
    pub current_index: Option<usize>,
    pub is_playing: bool,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub volume: f32,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat_mode: String, // "off" | "all" | "one"
    pub video_mode: bool,
    pub active_playlist_id: Option<String>,
}

impl Default for MediaPlayerState {
    fn default() -> Self {
        Self {
            current_track: None,
            queue: Vec::new(),
            current_index: None,
            is_playing: false,
            position_secs: 0.0,
            duration_secs: 0.0,
            volume: 0.85,
            muted: false,
            shuffle: false,
            repeat_mode: "off".to_string(),
            video_mode: false,
            active_playlist_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YtMusicSearchItem {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_text: String,
    pub duration_secs: u64,
    pub thumbnail_url: Option<String>,
    pub is_video: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedMediaStream {
    pub video_id: String,
    pub stream_url: String,
    pub is_video: bool,
}

/// Search YouTube Music via the public InnerTube WEB_REMIX API.
pub async fn search_ytmusic(query: &str, filter: &str) -> Result<Vec<YtMusicSearchItem>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let params_code = match filter.to_lowercase().as_str() {
        "songs" | "song" | "audio" => Some("EgWKAQIIAWoMEAMQBBAJEAoQBRAV"),
        "videos" | "video" => Some("EgWKAQIQAWoMEAMQBBAJEAoQBRAV"),
        _ => None,
    };

    let mut body = json!({
        "context": {
            "client": {
                "clientName": "WEB_REMIX",
                "clientVersion": "1.20241120.01.00",
                "hl": "en",
                "gl": "US"
            }
        },
        "query": trimmed
    });

    if let Some(p) = params_code {
        body["params"] = json!(p);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post("https://music.youtube.com/youtubei/v1/search?prettyPrint=false")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .header("Origin", "https://music.youtube.com")
        .header("Referer", "https://music.youtube.com/")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("YTMusic search request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("YTMusic API returned HTTP {}", resp.status()));
    }

    let json_val: Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse YTMusic JSON: {}", e))?;

    let mut items = Vec::new();
    let default_is_video = matches!(filter.to_lowercase().as_str(), "videos" | "video");
    collect_music_renderers(&json_val, default_is_video, &mut items);

    // Deduplicate by video_id preserving order
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.video_id.clone()));
    items.truncate(25);
    Ok(items)
}

fn collect_music_renderers(val: &Value, default_is_video: bool, out: &mut Vec<YtMusicSearchItem>) {
    match val {
        Value::Object(map) => {
            if let Some(renderer) = map.get("musicResponsiveListItemRenderer") {
                if let Some(item) = parse_responsive_item(renderer, default_is_video) {
                    out.push(item);
                }
            }
            for v in map.values() {
                collect_music_renderers(v, default_is_video, out);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_music_renderers(v, default_is_video, out);
            }
        }
        _ => {}
    }
}

fn extract_runs_text(val: &Value) -> String {
    val.get("runs")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

fn parse_duration_secs(text: &str) -> u64 {
    let parts: Vec<u64> = text
        .trim()
        .split(':')
        .filter_map(|p| p.trim().parse::<u64>().ok())
        .collect();
    match parts.len() {
        2 => parts[0] * 60 + parts[1],
        3 => parts[0] * 3600 + parts[1] * 60 + parts[2],
        _ => 0,
    }
}

fn parse_responsive_item(renderer: &Value, default_is_video: bool) -> Option<YtMusicSearchItem> {
    let video_id = renderer
        .pointer("/playlistItemData/videoId")
        .and_then(|v| v.as_str())
        .or_else(|| {
            renderer
                .pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId")
                .and_then(|v| v.as_str())
        })?
        .to_string();

    let flex_cols = renderer.get("flexColumns").and_then(|c| c.as_array())?;
    let title = flex_cols
        .first()
        .and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text"))
        .map(extract_runs_text)
        .unwrap_or_else(|| "Unknown Track".to_string());

    let subtitle_runs = flex_cols
        .get(1)
        .and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    let subtitle_parts: Vec<String> = subtitle_runs
        .iter()
        .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "•" && s != "·")
        .collect();

    let mut artist = String::new();
    let mut album = None;
    let mut duration_text = String::new();
    let mut is_video = default_is_video;

    for part in &subtitle_parts {
        let lower = part.to_lowercase();
        if lower == "video" {
            is_video = true;
            continue;
        }
        if lower == "song" {
            is_video = false;
            continue;
        }
        if part.contains(':') && part.chars().all(|c| c.is_ascii_digit() || c == ':') {
            duration_text = part.clone();
            continue;
        }
        if artist.is_empty() {
            artist = part.clone();
        } else if album.is_none() && !lower.contains("views") && !lower.contains("plays") {
            album = Some(part.clone());
        }
    }

    if artist.is_empty() {
        artist = "YouTube Music".to_string();
    }

    let thumbnail_url = renderer
        .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
        .and_then(|t| t.as_array())
        .and_then(|arr| arr.last())
        .and_then(|last| last.get("url"))
        .and_then(|u| u.as_str())
        .map(|s| s.to_string());

    let duration_secs = parse_duration_secs(&duration_text);

    Some(YtMusicSearchItem {
        video_id,
        title,
        artist,
        album,
        duration_text,
        duration_secs,
        thumbnail_url,
        is_video,
    })
}

fn managed_ytdlp_path() -> Result<PathBuf, String> {
    let data_dir = crate::db::storage::resolve_app_data_dir()?;
    let bin_dir = data_dir.join("bin");
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    let bin_name = if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    };
    Ok(bin_dir.join(bin_name))
}

async fn find_existing_ytdlp() -> Option<String> {
    if let Ok(managed) = managed_ytdlp_path() {
        if let Ok(meta) = tokio::fs::metadata(&managed).await {
            if meta.len() > 1_000_000 {
                return Some(managed.to_string_lossy().to_string());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
            let winget_link = format!("{}\\Microsoft\\WinGet\\Links\\yt-dlp.exe", local_app);
            if tokio::fs::metadata(&winget_link).await.is_ok() {
                return Some(winget_link);
            }
        }
    }

    // Check system PATH
    let mut probe = tokio::process::Command::new("yt-dlp");
    probe.arg("--version");
    #[cfg(target_os = "windows")]
    probe.creation_flags(0x08000000);
    if let Ok(out) = probe.output().await {
        if out.status.success() {
            return Some("yt-dlp".to_string());
        }
    }

    None
}

/// Automatically downloads the official yt-dlp binary into RayDrop's data folder if not installed.
pub async fn ensure_ytdlp_installed(app: Option<&AppHandle>) -> Result<String, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = app;
        return Err("yt-dlp auto-download is only supported on Desktop (Windows/macOS/Linux).".to_string());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
    if let Some(existing) = find_existing_ytdlp().await {
        return Ok(existing);
    }

    let lock = YTDLP_INSTALL_LOCK.get_or_init(|| AsyncMutex::new(()));
    let _guard = lock.lock().await;

    // Re-check after acquiring lock
    if let Some(existing) = find_existing_ytdlp().await {
        return Ok(existing);
    }

    if let Some(handle) = app {
        let _ = handle.emit(
            "media_ytdlp_status",
            json!({
                "status": "downloading",
                "message": "Downloading media engine (first time only)..."
            }),
        );
    }

    let download_url = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", _) => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
        ("macos", _) => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos",
        ("linux", "aarch64") => {
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64"
        }
        _ => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux",
    };

    let target_path = managed_ytdlp_path()?;
    let tmp_path = target_path.with_extension("tmp");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(download_url)
        .header("User-Agent", "Raydrop-Media-Engine")
        .send()
        .await
        .map_err(|e| format!("Failed to download media engine (yt-dlp): {}", e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Failed to download yt-dlp (HTTP {})",
            resp.status()
        ));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed reading yt-dlp download: {}", e))?;

    if bytes.len() < 1_000_000 {
        return Err("Downloaded yt-dlp binary appears incomplete".to_string());
    }

    tokio::fs::write(&tmp_path, &bytes)
        .await
        .map_err(|e| format!("Failed to save yt-dlp: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = tokio::fs::metadata(&tmp_path).await {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = tokio::fs::set_permissions(&tmp_path, perms).await;
        }
    }

    tokio::fs::rename(&tmp_path, &target_path)
        .await
        .map_err(|e| format!("Failed to finalize yt-dlp binary: {}", e))?;

    if let Some(handle) = app {
        let _ = handle.emit(
            "media_ytdlp_status",
            json!({
                "status": "ready",
                "message": ""
            }),
        );
    }

    Ok(target_path.to_string_lossy().to_string())
    }
}

/// Resolve a direct playable audio/video stream URL using yt-dlp (auto-downloads yt-dlp if missing).
pub async fn resolve_ytmusic_stream(
    app: Option<&AppHandle>,
    video_id: &str,
    prefer_video: bool,
) -> Result<ResolvedMediaStream, String> {
    let vid = video_id.trim();
    if vid.is_empty() {
        return Err("Missing video ID".to_string());
    }

    let ytdlp_bin = ensure_ytdlp_installed(app).await?;
    let format_selector = if prefer_video {
        "best[ext=mp4][acodec!=none][vcodec!=none]/best[acodec!=none][vcodec!=none]/best"
    } else {
        "bestaudio[ext=m4a]/bestaudio[ext=webm]/bestaudio/best"
    };
    let url = format!("https://www.youtube.com/watch?v={}", vid);

    let mut cmd = tokio::process::Command::new(&ytdlp_bin);
    cmd.args([
        "--no-playlist",
        "--no-warnings",
        "-f",
        format_selector,
        "-g",
        &url,
    ]);

    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let output = cmd.output().await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "yt-dlp is not installed or not in PATH. Install it first (Windows: `winget install yt-dlp`, macOS: `brew install yt-dlp`, Linux: `pip install yt-dlp`).".to_string()
        } else {
            format!("Failed to run yt-dlp: {}", e)
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp error: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stream_url = stdout
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("http"))
        .ok_or_else(|| "yt-dlp did not return a valid stream URL".to_string())?
        .to_string();

    Ok(ResolvedMediaStream {
        video_id: vid.to_string(),
        stream_url,
        is_video: prefer_video,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaDownloadResult {
    pub video_id: String,
    pub title: String,
    pub output_path: String,
    pub format: String,
    pub size_bytes: u64,
}

/// Download a YouTube / YouTube Music item directly as MP3 (audio) or MP4 (video).
pub async fn download_ytmusic_media(
    app: Option<&AppHandle>,
    video_id: &str,
    title_hint: Option<&str>,
    format: &str, // "mp3" | "mp4"
    output_dir: &str,
) -> Result<MediaDownloadResult, String> {
    let vid = video_id.trim();
    if vid.is_empty() {
        return Err("Missing video ID".to_string());
    }

    let ytdlp_bin = ensure_ytdlp_installed(app).await?;
    let ffmpeg_bin = crate::media_converter::ensure_ffmpeg_installed(app).await?;

    let out_dir_path = std::path::Path::new(output_dir);
    let _ = std::fs::create_dir_all(out_dir_path);

    let url = format!("https://www.youtube.com/watch?v={}", vid);
    let out_tmpl = out_dir_path
        .join("%(title)s.%(ext)s")
        .to_string_lossy()
        .to_string();

    let fmt_lower = format.trim().to_lowercase();
    let is_video = fmt_lower == "mp4" || fmt_lower == "video";
    let target_ext = if is_video { "mp4" } else { "mp3" };

    let mut args: Vec<String> = vec![
        "--no-playlist".into(),
        "--no-warnings".into(),
    ];

    if let Some(parent) = std::path::Path::new(&ffmpeg_bin).parent() {
        if !parent.as_os_str().is_empty() {
            args.push("--ffmpeg-location".into());
            args.push(parent.to_string_lossy().to_string());
        }
    }

    if is_video {
        args.extend([
            "-f".into(),
            "bv*+ba/b".into(),
            "--merge-output-format".into(),
            "mp4".into(),
        ]);
    } else {
        args.extend([
            "-x".into(),
            "--audio-format".into(),
            "mp3".into(),
            "--audio-quality".into(),
            "0".into(),
            "--add-metadata".into(),
        ]);
    }

    args.extend([
        "-o".into(),
        out_tmpl,
        "--print".into(),
        "after_move:filepath".into(),
        url,
    ]);

    let mut cmd = tokio::process::Command::new(&ytdlp_bin);
    cmd.args(&args);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run yt-dlp download: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Download failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let saved_path = stdout
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .ok_or_else(|| "Download completed but output path was not returned".to_string())?
        .to_string();

    let size_bytes = tokio::fs::metadata(&saved_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    let final_title = title_hint
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            std::path::Path::new(&saved_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Downloaded Media")
                .to_string()
        });

    Ok(MediaDownloadResult {
        video_id: vid.to_string(),
        title: final_title,
        output_path: saved_path,
        format: target_ext.to_string(),
        size_bytes,
    })
}
