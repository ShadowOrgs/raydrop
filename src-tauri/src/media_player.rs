use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::OnceLock;
#[allow(unused_imports)]
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

/// Search YouTube Music (WEB_REMIX) + standard YouTube (WEB) so covers, remixes, and videos are always found.
pub async fn search_ytmusic(query: &str, filter: &str) -> Result<Vec<YtMusicSearchItem>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let filter_lower = filter.to_lowercase();
    let default_is_video = matches!(filter_lower.as_str(), "videos" | "video");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let mut items = Vec::new();

    if filter_lower != "videos" && filter_lower != "video" {
        if let Ok(ytm_items) = search_ytmusic_innertube(&client, trimmed, &filter_lower, default_is_video).await {
            items.extend(ytm_items);
        }
    }

    // For "all", "videos", or when YTMusic returns too few results (e.g. VTuber covers / remixes),
    // also search standard YouTube WEB InnerTube (`videoRenderer`).
    if filter_lower == "all" || filter_lower == "videos" || filter_lower == "video" || items.len() < 5 {
        if let Ok(yt_items) = search_youtube_web_innertube(&client, trimmed).await {
            items.extend(yt_items);
        }
    }

    // Deduplicate by video_id preserving order
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.video_id.clone()));
    items.truncate(30);
    Ok(items)
}

async fn search_ytmusic_innertube(
    client: &reqwest::Client,
    query: &str,
    filter_lower: &str,
    default_is_video: bool,
) -> Result<Vec<YtMusicSearchItem>, String> {
    let params_code = match filter_lower {
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
        "query": query
    });

    if let Some(p) = params_code {
        body["params"] = json!(p);
    }

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
    collect_music_renderers(&json_val, default_is_video, &mut items);
    Ok(items)
}

async fn search_youtube_web_innertube(
    client: &reqwest::Client,
    query: &str,
) -> Result<Vec<YtMusicSearchItem>, String> {
    let body = json!({
        "context": {
            "client": {
                "clientName": "WEB",
                "clientVersion": "2.20241120.01.00",
                "hl": "en",
                "gl": "US"
            }
        },
        "query": query
    });

    let resp = client
        .post("https://www.youtube.com/youtubei/v1/search?prettyPrint=false")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .header("Origin", "https://www.youtube.com")
        .header("Referer", "https://www.youtube.com/")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("YouTube search request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("YouTube API returned HTTP {}", resp.status()));
    }

    let json_val: Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse YouTube JSON: {}", e))?;

    let mut items = Vec::new();
    collect_youtube_video_renderers(&json_val, &mut items);
    Ok(items)
}

fn collect_youtube_video_renderers(val: &Value, out: &mut Vec<YtMusicSearchItem>) {
    match val {
        Value::Object(map) => {
            if let Some(vr) = map.get("videoRenderer") {
                if let Some(video_id) = vr.get("videoId").and_then(|v| v.as_str()) {
                    let title = vr
                        .get("title")
                        .map(extract_runs_text)
                        .filter(|s| !s.is_empty())
                        .or_else(|| {
                            vr.pointer("/title/simpleText")
                                .and_then(|t| t.as_str())
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_else(|| "YouTube Video".to_string());
                    let artist = vr
                        .get("ownerText")
                        .map(extract_runs_text)
                        .filter(|s| !s.is_empty())
                        .or_else(|| {
                            vr.get("longBylineText")
                                .map(extract_runs_text)
                                .filter(|s| !s.is_empty())
                        })
                        .unwrap_or_else(|| "YouTube".to_string());
                    let duration_text = vr
                        .pointer("/lengthText/simpleText")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    let duration_secs = parse_duration_secs(&duration_text);
                    let thumbnail_url = vr
                        .pointer("/thumbnail/thumbnails")
                        .and_then(|t| t.as_array())
                        .and_then(|arr| arr.last())
                        .and_then(|last| last.get("url"))
                        .and_then(|u| u.as_str())
                        .map(|s| s.to_string());

                    out.push(YtMusicSearchItem {
                        video_id: video_id.to_string(),
                        title,
                        artist,
                        album: None,
                        duration_text,
                        duration_secs,
                        thumbnail_url,
                        is_video: true,
                    });
                }
            }
            for v in map.values() {
                collect_youtube_video_renderers(v, out);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_youtube_video_renderers(v, out);
            }
        }
        _ => {}
    }
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
        "best[acodec!=none][vcodec!=none]/best"
    } else {
        "bestaudio/best"
    };
    let url = format!("https://www.youtube.com/watch?v={}", vid);

    // Attempt 1: with android/ios/web player_client & preferred format selector
    let attempts: Vec<Vec<&str>> = vec![
        vec![
            "--no-playlist",
            "--no-warnings",
            "--extractor-args",
            "youtube:player_client=android,ios,web",
            "-f",
            format_selector,
            "-g",
            &url,
        ],
        // Attempt 2: fallback without -f so yt-dlp picks any playable format available
        vec![
            "--no-playlist",
            "--no-warnings",
            "--extractor-args",
            "youtube:player_client=android,ios,web",
            "-g",
            &url,
        ],
        // Attempt 3: plain default client without -f
        vec!["--no-playlist", "--no-warnings", "-g", &url],
    ];

    let mut last_err = String::new();
    for args in attempts {
        let mut cmd = tokio::process::Command::new(&ytdlp_bin);
        cmd.args(&args);
        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(0x08000000);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to run yt-dlp: {}", e))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(stream_url) = stdout
                .lines()
                .map(str::trim)
                .find(|l| l.starts_with("http"))
            {
                return Ok(ResolvedMediaStream {
                    video_id: vid.to_string(),
                    stream_url: stream_url.to_string(),
                    is_video: prefer_video,
                });
            }
        }
        last_err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    }

    Err(format!("yt-dlp error: {}", last_err))
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
        "--extractor-args".into(),
        "youtube:player_client=android,ios,web".into(),
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
        "--newline".into(),
        "--progress".into(),
        "--progress-template".into(),
        "download:RAYDROP_PROG:%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s".into(),
        "-o".into(),
        out_tmpl,
        "--print".into(),
        "after_move:RAYDROP_FILE:%(filepath)s".into(),
        url,
    ]);

    if let Some(handle) = app {
        let _ = handle.emit(
            "media_ytdlp_download_progress",
            json!({
                "video_id": vid,
                "format": target_ext,
                "percent": 0.0,
                "status_text": format!("Starting {} download...", target_ext.to_uppercase())
            }),
        );
    }

    let mut cmd = tokio::process::Command::new(&ytdlp_bin);
    cmd.args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to run yt-dlp download: {}", e))?;

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let app_out = app.cloned();
    let vid_out = vid.to_string();
    let ext_out = target_ext.to_string();

    let stdout_task = tauri::async_runtime::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut path_lines = Vec::new();
        if let Some(out) = stdout_pipe {
            let mut reader = BufReader::new(out).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim();
                if let Some(prog) = trimmed.strip_prefix("RAYDROP_PROG:") {
                    let parts: Vec<&str> = prog.split('|').collect();
                    let pct_str = parts.first().copied().unwrap_or("0%").trim().trim_end_matches('%');
                    let pct = pct_str.parse::<f64>().unwrap_or(0.0).clamp(0.0, 99.0);
                    let speed = parts.get(1).copied().unwrap_or("").trim();
                    let eta = parts.get(2).copied().unwrap_or("").trim();
                    if let Some(ref handle) = app_out {
                        let _ = handle.emit(
                            "media_ytdlp_download_progress",
                            json!({
                                "video_id": vid_out,
                                "format": ext_out,
                                "percent": pct,
                                "status_text": format!("Downloading {}: {:.0}% • {} • ETA {}", ext_out.to_uppercase(), pct, speed, eta)
                            }),
                        );
                    }
                } else if trimmed.starts_with("[ExtractAudio]") || trimmed.starts_with("[Merger]") {
                    if let Some(ref handle) = app_out {
                        let _ = handle.emit(
                            "media_ytdlp_download_progress",
                            json!({
                                "video_id": vid_out,
                                "format": ext_out,
                                "percent": 99.0,
                                "status_text": format!("Converting / merging {}...", ext_out.to_uppercase())
                            }),
                        );
                    }
                } else if let Some(fpath) = trimmed.strip_prefix("RAYDROP_FILE:") {
                    path_lines.push(fpath.trim().to_string());
                }
            }
        }
        path_lines
    });

    let stderr_task = tauri::async_runtime::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut errs = Vec::new();
        if let Some(err) = stderr_pipe {
            let mut reader = BufReader::new(err).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if !line.trim().starts_with("RAYDROP_PROG:") {
                    errs.push(line);
                }
            }
        }
        errs.join("\n")
    });

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed waiting for yt-dlp: {}", e))?;
    let path_lines = stdout_task.await.unwrap_or_default();
    let stderr_text = stderr_task.await.unwrap_or_default();

    if !status.success() {
        return Err(format!("Download failed: {}", stderr_text.trim()));
    }

    let saved_path = path_lines
        .into_iter()
        .rfind(|l| !l.is_empty())
        .ok_or_else(|| "Download completed but output path was not returned".to_string())?;

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
