use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex as AsyncMutex;

static FFMPEG_INSTALL_LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaEngineInfo {
    pub has_ffmpeg: bool,
    pub ffmpeg_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaConvertResult {
    pub input_path: String,
    pub output_path: String,
    pub original_bytes: u64,
    pub output_bytes: u64,
    pub format: String,
    pub mode: String,
}

fn managed_ffmpeg_path() -> Result<PathBuf, String> {
    let data_dir = crate::db::storage::resolve_app_data_dir()?;
    let bin_dir = data_dir.join("bin");
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    let bin_name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    Ok(bin_dir.join(bin_name))
}

pub async fn find_existing_ffmpeg() -> Option<String> {
    if let Ok(managed) = managed_ffmpeg_path() {
        if let Ok(meta) = tokio::fs::metadata(&managed).await {
            if meta.len() > 1_000_000 {
                return Some(managed.to_string_lossy().to_string());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
            let winget_link = format!("{}\\Microsoft\\WinGet\\Links\\ffmpeg.exe", local_app);
            if tokio::fs::metadata(&winget_link).await.is_ok() {
                return Some(winget_link);
            }
        }
    }

    let mut probe = tokio::process::Command::new("ffmpeg");
    probe.arg("-version");
    #[cfg(target_os = "windows")]
    probe.creation_flags(0x08000000);
    if let Ok(out) = probe.output().await {
        if out.status.success() {
            return Some("ffmpeg".to_string());
        }
    }

    None
}

pub async fn detect_media_engine() -> MediaEngineInfo {
    let found = find_existing_ffmpeg().await;
    MediaEngineInfo {
        has_ffmpeg: found.is_some(),
        ffmpeg_path: found,
    }
}

/// Automatically download a static FFmpeg binary into RayDrop's data folder if not installed.
pub async fn ensure_ffmpeg_installed(app: Option<&AppHandle>) -> Result<String, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = app;
        return Err("FFmpeg auto-download is only supported on Desktop (Windows/macOS/Linux).".to_string());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
    if let Some(existing) = find_existing_ffmpeg().await {
        return Ok(existing);
    }

    let lock = FFMPEG_INSTALL_LOCK.get_or_init(|| AsyncMutex::new(()));
    let _guard = lock.lock().await;

    if let Some(existing) = find_existing_ffmpeg().await {
        return Ok(existing);
    }

    if let Some(handle) = app {
        let _ = handle.emit(
            "media_ffmpeg_status",
            json!({
                "status": "downloading",
                "message": "Downloading FFmpeg engine (first time only)..."
            }),
        );
    }

    let target_path = managed_ffmpeg_path()?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    let os = std::env::consts::OS;
    let urls: Vec<&str> = match os {
        "windows" => vec![
            "https://github.com/ffbinaries/ffbinaries-prebuilt/releases/download/v6.1/ffmpeg-6.1-win-64.zip",
            "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip",
        ],
        "macos" => vec![
            "https://github.com/ffbinaries/ffbinaries-prebuilt/releases/download/v6.1/ffmpeg-6.1-macos-64.zip",
            "https://evermeet.cx/ffmpeg/getrelease/zip",
        ],
        _ => vec![
            "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz",
        ],
    };

    let mut downloaded_bytes: Option<Vec<u8>> = None;
    let mut last_err = String::new();

    for url in urls {
        match client
            .get(url)
            .header("User-Agent", "Raydrop-Media-Converter")
            .send()
            .await
        {
            Ok(mut resp) if resp.status().is_success() => {
                let total_len = resp.content_length().unwrap_or(0);
                let mut buf = Vec::with_capacity(total_len as usize);
                let mut last_emit = std::time::Instant::now();
                let mut read_ok = true;

                while let Some(chunk) = match resp.chunk().await {
                    Ok(c) => c,
                    Err(e) => {
                        last_err = format!("Failed reading FFmpeg stream: {}", e);
                        read_ok = false;
                        None
                    }
                } {
                    buf.extend_from_slice(&chunk);
                    if last_emit.elapsed().as_millis() >= 250 {
                        last_emit = std::time::Instant::now();
                        if let Some(handle) = app {
                            let dl_mb = buf.len() as f64 / (1024.0 * 1024.0);
                            let pct = if total_len > 0 {
                                (buf.len() as f64 / total_len as f64 * 100.0).clamp(0.0, 99.0)
                            } else {
                                0.0
                            };
                            let msg = if total_len > 0 {
                                let tot_mb = total_len as f64 / (1024.0 * 1024.0);
                                format!(
                                    "Downloading FFmpeg engine: {:.0}% ({:.1} / {:.1} MB)...",
                                    pct, dl_mb, tot_mb
                                )
                            } else {
                                format!("Downloading FFmpeg engine ({:.1} MB)...", dl_mb)
                            };
                            let _ = handle.emit(
                                "media_ffmpeg_status",
                                json!({
                                    "status": "downloading",
                                    "percent": pct,
                                    "message": msg
                                }),
                            );
                        }
                    }
                }

                if read_ok && buf.len() > 1_000_000 {
                    downloaded_bytes = Some(buf);
                    break;
                }
            }
            Ok(resp) => {
                last_err = format!("HTTP {} from {}", resp.status(), url);
            }
            Err(e) => {
                last_err = format!("Request error: {}", e);
            }
        }
    }

    let raw_vec = downloaded_bytes
        .ok_or_else(|| format!("Failed to download FFmpeg: {}", last_err))?;

    if let Some(handle) = app {
        let _ = handle.emit(
            "media_ffmpeg_status",
            json!({
                "status": "extracting",
                "percent": 99.0,
                "message": "Extracting FFmpeg engine..."
            }),
        );
    }

    if os == "windows" || os == "macos" {
        let target_clone = target_path.clone();
        let wanted_name = if os == "windows" { "ffmpeg.exe" } else { "ffmpeg" };
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let reader = std::io::Cursor::new(raw_vec);
            let mut archive = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
                let name = file.name().replace('\\', "/");
                if name.ends_with(&format!("/{}", wanted_name)) || name == wanted_name {
                    let tmp_path = target_clone.with_extension("tmp");
                    let mut buf = Vec::new();
                    file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
                    std::fs::write(&tmp_path, &buf).map_err(|e| e.to_string())?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Ok(meta) = std::fs::metadata(&tmp_path) {
                            let mut perms = meta.permissions();
                            perms.set_mode(0o755);
                            let _ = std::fs::set_permissions(&tmp_path, perms);
                        }
                    }
                    std::fs::rename(&tmp_path, &target_clone).map_err(|e| e.to_string())?;
                    return Ok(());
                }
            }
            Err(format!("{} not found inside downloaded archive", wanted_name))
        })
        .await
        .map_err(|e| e.to_string())??;
    } else {
        // Linux .tar.xz extraction via system tar
        let archive_path = target_path.with_extension("tar.xz");
        let extract_dir = target_path.parent().unwrap_or(Path::new(".")).join("ffmpeg_tmp");
        let _ = std::fs::create_dir_all(&extract_dir);
        tokio::fs::write(&archive_path, &raw_vec)
            .await
            .map_err(|e| e.to_string())?;
        let out = tokio::process::Command::new("tar")
            .arg("-xJf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&extract_dir)
            .output()
            .await
            .map_err(|e| format!("tar extraction failed: {}", e))?;
        let _ = tokio::fs::remove_file(&archive_path).await;
        if !out.status.success() {
            return Err("Failed to extract FFmpeg .tar.xz archive".to_string());
        }
        // Locate ffmpeg binary inside extracted folder
        let mut found_bin = None;
        for entry in walkdir::WalkDir::new(&extract_dir).into_iter().flatten() {
            if entry.file_type().is_file() && entry.file_name() == "ffmpeg" {
                found_bin = Some(entry.path().to_path_buf());
                break;
            }
        }
        let bin = found_bin.ok_or_else(|| "ffmpeg binary not found in archive".to_string())?;
        tokio::fs::rename(&bin, &target_path)
            .await
            .map_err(|e| e.to_string())?;
        let _ = tokio::fs::remove_dir_all(&extract_dir).await;
    }

    if let Some(handle) = app {
        let _ = handle.emit(
            "media_ffmpeg_status",
            json!({
                "status": "ready",
                "message": ""
            }),
        );
    }

    Ok(target_path.to_string_lossy().to_string())
    }
}

async fn probe_duration_secs(ffmpeg_bin: &str, input_path: &str) -> f64 {
    let mut cmd = tokio::process::Command::new(ffmpeg_bin);
    cmd.args(["-i", input_path]);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);
    let Ok(output) = cmd.output().await else {
        return 0.0;
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stderr.lines() {
        if let Some(pos) = line.find("Duration:") {
            let after = &line[pos + 9..];
            let dur_str = after.split(',').next().unwrap_or("").trim();
            let parts: Vec<&str> = dur_str.split(':').collect();
            if parts.len() == 3 {
                let h = parts[0].parse::<f64>().unwrap_or(0.0);
                let m = parts[1].parse::<f64>().unwrap_or(0.0);
                let s = parts[2].parse::<f64>().unwrap_or(0.0);
                return h * 3600.0 + m * 60.0 + s;
            }
        }
    }
    0.0
}

fn build_unique_output_path(input_path: &str, target_ext: &str, output_dir: Option<&str>) -> PathBuf {
    let in_p = Path::new(input_path);
    let stem = in_p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("converted");
    let parent = match output_dir.map(str::trim).filter(|s| !s.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => in_p.parent().unwrap_or_else(|| Path::new(".")).to_path_buf(),
    };
    let _ = std::fs::create_dir_all(&parent);

    let mut candidate = parent.join(format!("{}_converted.{}", stem, target_ext));
    let mut idx = 2u32;
    while candidate.exists() {
        candidate = parent.join(format!("{}_converted_{}.{}", stem, idx, target_ext));
        idx += 1;
    }
    candidate
}

#[allow(clippy::too_many_arguments)]
pub async fn convert_media_files(
    app: &AppHandle,
    input_paths: Vec<String>,
    mode: String,            // "video_to_video" | "video_to_audio"
    target_format: String,   // mp4, mkv, webm, mov, avi, gif | mp3, m4a, wav, flac, ogg, opus
    quality_preset: String,  // "copy" | "high" | "balanced" | "compact"
    resolution: String,      // "original" | "1080p" | "720p" | "480p"
    audio_bitrate: String,   // "128k" | "192k" | "256k" | "320k"
    output_dir: Option<String>,
) -> Result<Vec<MediaConvertResult>, String> {
    if input_paths.is_empty() {
        return Err("No input files selected".to_string());
    }

    let ffmpeg_bin = ensure_ffmpeg_installed(Some(app)).await?;
    let fmt = target_format.trim().to_lowercase();
    let total_files = input_paths.len();
    let mut results = Vec::new();

    for (idx, input_path) in input_paths.iter().enumerate() {
        let filename = Path::new(input_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();

        let original_bytes = tokio::fs::metadata(input_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let duration_secs = probe_duration_secs(&ffmpeg_bin, input_path).await;
        let out_path = build_unique_output_path(input_path, &fmt, output_dir.as_deref());
        let out_str = out_path.to_string_lossy().to_string();

        let _ = app.emit(
            "media_convert_progress",
            json!({
                "file_index": idx,
                "total_files": total_files,
                "filename": filename,
                "percent": 0.0,
                "status": "converting"
            }),
        );

        let mut args: Vec<String> = vec![
            "-y".to_string(),
            "-i".to_string(),
            input_path.clone(),
        ];

        if mode == "video_to_audio" {
            args.push("-vn".to_string());
            match fmt.as_str() {
                "mp3" => {
                    args.extend(["-c:a".into(), "libmp3lame".into(), "-b:a".into(), audio_bitrate.clone()]);
                }
                "m4a" => {
                    args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), audio_bitrate.clone()]);
                }
                "wav" => {
                    args.extend(["-c:a".into(), "pcm_s16le".into()]);
                }
                "flac" => {
                    args.extend(["-c:a".into(), "flac".into()]);
                }
                "ogg" => {
                    args.extend(["-c:a".into(), "libvorbis".into(), "-b:a".into(), audio_bitrate.clone()]);
                }
                "opus" => {
                    args.extend(["-c:a".into(), "libopus".into(), "-b:a".into(), audio_bitrate.clone()]);
                }
                _ => return Err(format!("Unsupported audio format: {}", fmt)),
            }
        } else {
            // video_to_video
            if quality_preset == "copy" && resolution == "original" && fmt != "gif" {
                args.extend(["-c".into(), "copy".into()]);
            } else if fmt == "gif" {
                let scale_h = match resolution.as_str() {
                    "1080p" => "1080",
                    "720p" => "720",
                    "480p" => "480",
                    _ => "480",
                };
                args.extend([
                    "-vf".into(),
                    format!("fps=15,scale=-1:{}:flags=lanczos", scale_h),
                ]);
            } else {
                let crf = match quality_preset.as_str() {
                    "high" => "18",
                    "compact" => "28",
                    _ => "23",
                };
                if resolution != "original" {
                    let h = match resolution.as_str() {
                        "1080p" => "1080",
                        "720p" => "720",
                        "480p" => "480",
                        _ => "-2",
                    };
                    args.extend(["-vf".into(), format!("scale=-2:{}", h)]);
                }
                match fmt.as_str() {
                    "webm" => {
                        args.extend([
                            "-c:v".into(),
                            "libvpx-vp9".into(),
                            "-crf".into(),
                            crf.into(),
                            "-b:v".into(),
                            "0".into(),
                            "-c:a".into(),
                            "libopus".into(),
                        ]);
                    }
                    "avi" => {
                        args.extend(["-c:v".into(), "mpeg4".into(), "-q:v".into(), "5".into(), "-c:a".into(), "libmp3lame".into()]);
                    }
                    _ => {
                        // mp4, mkv, mov
                        args.extend([
                            "-c:v".into(),
                            "libx264".into(),
                            "-preset".into(),
                            "medium".into(),
                            "-crf".into(),
                            crf.into(),
                            "-c:a".into(),
                            "aac".into(),
                            "-b:a".into(),
                            "192k".into(),
                        ]);
                        if fmt == "mp4" {
                            args.extend(["-movflags".into(), "+faststart".into()]);
                        }
                    }
                }
            }
        }

        args.extend([
            "-progress".into(),
            "pipe:1".into(),
            "-nostats".into(),
            out_str.clone(),
        ]);

        let mut cmd = tokio::process::Command::new(&ffmpeg_bin);
        cmd.args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(target_os = "windows")]
        cmd.creation_flags(0x08000000);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start FFmpeg: {}", e))?;

        if let Some(stdout) = child.stdout.take() {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Some(us_str) = line.strip_prefix("out_time_us=") {
                    if let Ok(us) = us_str.trim().parse::<f64>() {
                        if duration_secs > 0.0 {
                            let pct = ((us / 1_000_000.0) / duration_secs * 100.0).clamp(0.0, 99.0);
                            let _ = app.emit(
                                "media_convert_progress",
                                json!({
                                    "file_index": idx,
                                    "total_files": total_files,
                                    "filename": filename,
                                    "percent": pct,
                                    "status": "converting"
                                }),
                            );
                        }
                    }
                }
            }
        }

        let status = child
            .wait_with_output()
            .await
            .map_err(|e| format!("FFmpeg wait error: {}", e))?;

        if !status.status.success() {
            let err_tail = String::from_utf8_lossy(&status.stderr);
            let last_lines: Vec<&str> = err_tail.lines().rev().take(6).collect();
            return Err(format!(
                "FFmpeg failed on '{}': {}",
                filename,
                last_lines.into_iter().rev().collect::<Vec<_>>().join(" | ")
            ));
        }

        let output_bytes = tokio::fs::metadata(&out_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let _ = app.emit(
            "media_convert_progress",
            json!({
                "file_index": idx,
                "total_files": total_files,
                "filename": filename,
                "percent": 100.0,
                "status": "completed"
            }),
        );

        results.push(MediaConvertResult {
            input_path: input_path.clone(),
            output_path: out_str,
            original_bytes,
            output_bytes,
            format: fmt.clone(),
            mode: mode.clone(),
        });
    }

    Ok(results)
}
