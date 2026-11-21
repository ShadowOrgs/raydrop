//! Native System Audio Loopback Capture (WASAPI on Windows)
//! Captures host playback audio (YouTube, music, system sounds) and streams PCM chunks
//! to the frontend to be forwarded over WebRTC DataChannel to remote clients.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

static AUDIO_CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn start_host_audio_loopback(app: AppHandle) -> Result<(), String> {
    if AUDIO_CAPTURE_ACTIVE.swap(true, Ordering::SeqCst) {
        return Ok(()); // Already running
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        std::thread::spawn(move || {
            if let Err(e) = run_loopback_thread(app) {
                tracing::warn!("Host audio loopback ended with error: {}", e);
            }
            AUDIO_CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
        });
        Ok(())
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = app;
        AUDIO_CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
        Err("System audio loopback capture is only supported on desktop hosts".to_string())
    }
}

pub fn stop_host_audio_loopback() {
    AUDIO_CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn run_loopback_thread(app: AppHandle) -> Result<(), String> {
    use base64::Engine;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{SampleFormat, StreamConfig};

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "No default audio output device found".to_string())?;

    let default_cfg = device
        .default_output_config()
        .map_err(|e| format!("Failed to get default output config: {e}"))?;

    let sample_rate = default_cfg.sample_rate().0;
    let channels = default_cfg.channels() as usize;
    let target_rate = 24_000u32;

    let stream_cfg = StreamConfig {
        channels: default_cfg.channels(),
        sample_rate: default_cfg.sample_rate(),
        buffer_size: cpal::BufferSize::Default,
    };

    let buffer: Arc<std::sync::Mutex<Vec<i16>>> = Arc::new(std::sync::Mutex::new(Vec::with_capacity(2048)));
    let buffer_clone = buffer.clone();
    let app_clone = app.clone();

    // Simple linear decimator state
    let ratio = sample_rate as f64 / target_rate as f64;
    let phase = Arc::new(std::sync::Mutex::new(0.0f64));

    let emit_chunk = move |mono_samples: &[f32]| {
        if !AUDIO_CAPTURE_ACTIVE.load(Ordering::Relaxed) {
            return;
        }
        let mut ph = phase.lock().unwrap();
        let mut buf = buffer_clone.lock().unwrap();

        let mut idx = *ph;
        while (idx as usize) < mono_samples.len() {
            let s = mono_samples[idx as usize].clamp(-1.0, 1.0);
            buf.push((s * 32767.0) as i16);
            idx += ratio;
        }
        *ph = idx - mono_samples.len() as f64;

        // Send ~40ms chunks (960 samples @ 24kHz)
        if buf.len() >= 960 {
            let chunk: Vec<i16> = buf.drain(..960).collect();
            let mut bytes = Vec::with_capacity(chunk.len() * 2);
            for s in chunk {
                bytes.extend_from_slice(&s.to_le_bytes());
            }
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let _ = app_clone.emit("host_audio_chunk", b64);
        }
    };

    let err_fn = |err| {
        tracing::warn!("Audio loopback stream error: {}", err);
    };

    let stream = match default_cfg.sample_format() {
        SampleFormat::F32 => {
            let emit = emit_chunk;
            device.build_input_stream(
                &stream_cfg,
                move |data: &[f32], _| {
                    let mut mono = Vec::with_capacity(data.len() / channels.max(1));
                    for frame in data.chunks(channels.max(1)) {
                        let sum: f32 = frame.iter().sum();
                        mono.push(sum / frame.len().max(1) as f32);
                    }
                    emit(&mono);
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let emit = emit_chunk;
            device.build_input_stream(
                &stream_cfg,
                move |data: &[i16], _| {
                    let mut mono = Vec::with_capacity(data.len() / channels.max(1));
                    for frame in data.chunks(channels.max(1)) {
                        let sum: f32 = frame.iter().map(|&s| s as f32 / 32768.0).sum();
                        mono.push(sum / frame.len().max(1) as f32);
                    }
                    emit(&mono);
                },
                err_fn,
                None,
            )
        }
        SampleFormat::U16 => {
            let emit = emit_chunk;
            device.build_input_stream(
                &stream_cfg,
                move |data: &[u16], _| {
                    let mut mono = Vec::with_capacity(data.len() / channels.max(1));
                    for frame in data.chunks(channels.max(1)) {
                        let sum: f32 = frame.iter().map(|&s| (s as f32 - 32768.0) / 32768.0).sum();
                        mono.push(sum / frame.len().max(1) as f32);
                    }
                    emit(&mono);
                },
                err_fn,
                None,
            )
        }
        _ => return Err("Unsupported audio sample format for loopback".to_string()),
    }
    .map_err(|e| format!("Failed to build loopback stream: {e}"))?;

    stream
        .play()
        .map_err(|e| format!("Failed to start loopback stream: {e}"))?;

    while AUDIO_CAPTURE_ACTIVE.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    drop(stream);
    Ok(())
}
