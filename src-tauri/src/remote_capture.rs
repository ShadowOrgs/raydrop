//! Native Zero-Prompt Desktop Screen Capture (Windows GDI / StretchBlt)
//! Captures the host desktop silently in the background without triggering browser
//! `getDisplayMedia()` permission popups, enabling true unattended remote access after reboot.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);
static FRAME_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static LAST_EMIT_EPOCH_MS: AtomicU64 = AtomicU64::new(0);
static CAPTURE_BOUNDS: Mutex<Option<(i32, i32, i32, i32)>> = Mutex::new(None);

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn ack_native_screen_frame() {
    FRAME_IN_FLIGHT.store(false, Ordering::Release);
}

pub fn set_capture_bounds(bounds: Option<(i32, i32, i32, i32)>) {
    if let Ok(mut guard) = CAPTURE_BOUNDS.lock() {
        *guard = bounds;
    }
}

pub fn is_native_capture_active() -> bool {
    CAPTURE_ACTIVE.load(Ordering::Relaxed)
}

pub fn start_native_screen_capture(app: AppHandle) -> Result<(), String> {
    FRAME_IN_FLIGHT.store(false, Ordering::Release);
    if CAPTURE_ACTIVE.swap(true, Ordering::SeqCst) {
        return Ok(()); // Already running
    }

    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(move || {
            if let Err(e) = run_windows_capture_loop(app) {
                tracing::warn!("Native screen capture stopped: {}", e);
            }
            CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
        });
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
        Err("Native zero-prompt screen capture is currently supported on Windows hosts".to_string())
    }
}

pub fn stop_native_screen_capture() {
    CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
    FRAME_IN_FLIGHT.store(false, Ordering::Release);
}

#[cfg(target_os = "windows")]
mod win_gdi {
    #[repr(C)]
    pub struct BITMAPINFOHEADER {
        pub bi_size: u32,
        pub bi_width: i32,
        pub bi_height: i32,
        pub bi_planes: u16,
        pub bi_bit_count: u16,
        pub bi_compression: u32,
        pub bi_size_image: u32,
        pub bi_x_pels_per_meter: i32,
        pub bi_y_pels_per_meter: i32,
        pub bi_clr_used: u32,
        pub bi_clr_important: u32,
    }

    #[repr(C)]
    pub struct BITMAPINFO {
        pub bmi_header: BITMAPINFOHEADER,
        pub bmi_colors: [u32; 1],
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn GetDC(hwnd: isize) -> isize;
        pub fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
        pub fn GetSystemMetrics(n_index: i32) -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        pub fn CreateCompatibleDC(hdc: isize) -> isize;
        pub fn CreateCompatibleBitmap(hdc: isize, cx: i32, cy: i32) -> isize;
        pub fn SelectObject(hdc: isize, h: isize) -> isize;
        pub fn SetStretchBltMode(hdc: isize, mode: i32) -> i32;
        pub fn StretchBlt(
            hdc_dest: isize,
            x_dest: i32,
            y_dest: i32,
            w_dest: i32,
            h_dest: i32,
            hdc_src: isize,
            x_src: i32,
            y_src: i32,
            w_src: i32,
            h_src: i32,
            rop: u32,
        ) -> i32;
        pub fn GetDIBits(
            hdc: isize,
            hbm: isize,
            start: u32,
            c_lines: u32,
            lpv_bits: *mut u8,
            lpbmi: *mut BITMAPINFO,
            usage: u32,
        ) -> i32;
        pub fn DeleteObject(ho: isize) -> i32;
        pub fn DeleteDC(hdc: isize) -> i32;
    }
}

#[cfg(target_os = "windows")]
fn run_windows_capture_loop(app: AppHandle) -> Result<(), String> {
    use base64::Engine;
    use image::codecs::jpeg::JpegEncoder;
    use image::ExtendedColorType;

    const SRCCOPY: u32 = 0x00CC0020;
    const HALFTONE: i32 = 4;

    let mut bgra: Vec<u8> = Vec::new();
    let mut rgb: Vec<u8> = Vec::new();
    let mut jpeg_buf: Vec<u8> = Vec::with_capacity(65536);
    let mut last_frame_hash: u64 = 0;
    let mut last_force_emit = std::time::Instant::now() - std::time::Duration::from_secs(2);
    let mut last_desktop_switch = std::time::Instant::now() - std::time::Duration::from_secs(2);

    while CAPTURE_ACTIVE.load(Ordering::Relaxed) {
        let loop_start = std::time::Instant::now();

        // Backpressure: do not flood WebView2 IPC if the previous frame is still decoding
        if FRAME_IN_FLIGHT.load(Ordering::Acquire) {
            let elapsed_ms = now_epoch_ms().saturating_sub(LAST_EMIT_EPOCH_MS.load(Ordering::Acquire));
            if elapsed_ms < 250 {
                std::thread::sleep(std::time::Duration::from_millis(12));
                continue;
            }
            FRAME_IN_FLIGHT.store(false, Ordering::Release);
        }

        // Switch thread desktop periodically (or on capture failure) for Winlogon / UAC support
        if last_desktop_switch.elapsed() >= std::time::Duration::from_millis(500) {
            crate::win_service::switch_to_active_input_desktop();
            last_desktop_switch = std::time::Instant::now();
        }

        let custom_bounds = CAPTURE_BOUNDS.lock().ok().and_then(|g| *g);
        let (src_x, src_y, src_w, src_h) = match custom_bounds {
            Some((x, y, w, h)) if w > 0 && h > 0 => (x, y, w, h),
            _ => unsafe {
                let w = win_gdi::GetSystemMetrics(0).max(1280);
                let h = win_gdi::GetSystemMetrics(1).max(720);
                (0, 0, w, h)
            },
        };

        // Scale to max 1280px width for crisp, zero-freeze WebView2 streaming
        let scale = if src_w > 1280 {
            1280.0 / src_w as f32
        } else {
            1.0
        };
        let dst_w = ((src_w as f32 * scale).round() as i32).max(2) & !1;
        let dst_h = ((src_h as f32 * scale).round() as i32).max(2) & !1;

        unsafe {
            let hdc_screen = win_gdi::GetDC(0);
            if hdc_screen == 0 {
                crate::win_service::switch_to_active_input_desktop();
            } else {
                let hdc_mem = win_gdi::CreateCompatibleDC(hdc_screen);
                let hbm = win_gdi::CreateCompatibleBitmap(hdc_screen, dst_w, dst_h);

                if hdc_mem != 0 && hbm != 0 {
                    let old_obj = win_gdi::SelectObject(hdc_mem, hbm);
                    win_gdi::SetStretchBltMode(hdc_mem, HALFTONE);
                    let ok = win_gdi::StretchBlt(
                        hdc_mem,
                        0,
                        0,
                        dst_w,
                        dst_h,
                        hdc_screen,
                        src_x,
                        src_y,
                        src_w,
                        src_h,
                        SRCCOPY,
                    );

                    if ok != 0 {
                        let mut bmi = win_gdi::BITMAPINFO {
                            bmi_header: win_gdi::BITMAPINFOHEADER {
                                bi_size: std::mem::size_of::<win_gdi::BITMAPINFOHEADER>() as u32,
                                bi_width: dst_w,
                                bi_height: -dst_h, // Top-down
                                bi_planes: 1,
                                bi_bit_count: 32,
                                bi_compression: 0, // BI_RGB
                                bi_size_image: 0,
                                bi_x_pels_per_meter: 0,
                                bi_y_pels_per_meter: 0,
                                bi_clr_used: 0,
                                bi_clr_important: 0,
                            },
                            bmi_colors: [0],
                        };

                        let needed_bgra = (dst_w * dst_h * 4) as usize;
                        if bgra.len() != needed_bgra {
                            bgra.resize(needed_bgra, 0);
                        }
                        let lines = win_gdi::GetDIBits(
                            hdc_mem,
                            hbm,
                            0,
                            dst_h as u32,
                            bgra.as_mut_ptr(),
                            &mut bmi,
                            0,
                        );

                        if lines > 0 {
                            // Fast sampled FNV-1a hash to skip unchanged idle frames (saves 100% encode/IPC CPU)
                            let mut h: u64 = 0xcbf29ce484222325;
                            for &b in bgra.iter().step_by(64) {
                                h ^= b as u64;
                                h = h.wrapping_mul(0x100000001b3);
                            }

                            if h != last_frame_hash || last_force_emit.elapsed() >= std::time::Duration::from_millis(1000) {
                                last_frame_hash = h;
                                last_force_emit = std::time::Instant::now();

                                rgb.clear();
                                let needed_rgb = (dst_w * dst_h * 3) as usize;
                                if rgb.capacity() < needed_rgb {
                                    rgb.reserve(needed_rgb - rgb.capacity());
                                }
                                for px in bgra.chunks_exact(4) {
                                    rgb.push(px[2]); // R
                                    rgb.push(px[1]); // G
                                    rgb.push(px[0]); // B
                                }

                                jpeg_buf.clear();
                                let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_buf, 68);
                                if encoder
                                    .encode(&rgb, dst_w as u32, dst_h as u32, ExtendedColorType::Rgb8)
                                    .is_ok()
                                {
                                    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_buf);
                                    LAST_EMIT_EPOCH_MS.store(now_epoch_ms(), Ordering::Release);
                                    FRAME_IN_FLIGHT.store(true, Ordering::Release);
                                    if app.emit("host_screen_frame", b64).is_err() {
                                        FRAME_IN_FLIGHT.store(false, Ordering::Release);
                                    }
                                }
                            }
                        }
                    }

                    win_gdi::SelectObject(hdc_mem, old_obj);
                }

                if hbm != 0 {
                    win_gdi::DeleteObject(hbm);
                }
                if hdc_mem != 0 {
                    win_gdi::DeleteDC(hdc_mem);
                }
                win_gdi::ReleaseDC(0, hdc_screen);
            }
        }

        // Target ~24 fps (42ms per frame) for smooth & responsive remote without UI saturation
        let elapsed = loop_start.elapsed();
        if elapsed < std::time::Duration::from_millis(42) {
            std::thread::sleep(std::time::Duration::from_millis(42) - elapsed);
        }
    }

    Ok(())
}
