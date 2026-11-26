//! Native Zero-Prompt Desktop Screen Capture (Windows GDI / StretchBlt)
//! Captures the host desktop silently in the background without triggering browser
//! `getDisplayMedia()` permission popups, enabling true unattended remote access after reboot.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);
static CAPTURE_GEN: AtomicU64 = AtomicU64::new(0);
static FRAME_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static RESET_FRAME_HASH: AtomicBool = AtomicBool::new(false);
static LAST_EMIT_EPOCH_MS: AtomicU64 = AtomicU64::new(0);
pub static DRAW_HOST_CURSOR: AtomicBool = AtomicBool::new(false);
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

/// Captures a single PNG screenshot (base64) of the primary/active display for MCP AI tools.
pub fn capture_screenshot_png_base64() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        use base64::Engine as _;
        use image::codecs::png::{CompressionType, FilterType, PngEncoder};
        use image::{ExtendedColorType, ImageEncoder};

        const SRCCOPY: u32 = 0x00CC0020;
        const HALFTONE: i32 = 4;

        crate::win_service::switch_to_active_input_desktop();
        unsafe {
            let src_w = win_gdi::GetSystemMetrics(0).max(1280);
            let src_h = win_gdi::GetSystemMetrics(1).max(720);
            let scale = if src_w > 1920 { 1920.0 / src_w as f32 } else { 1.0 };
            let dst_w = ((src_w as f32 * scale).round() as i32).max(2) & !1;
            let dst_h = ((src_h as f32 * scale).round() as i32).max(2) & !1;

            let hdc_screen = win_gdi::GetDC(0);
            if hdc_screen == 0 {
                return Err("Failed to acquire screen DC".to_string());
            }
            let hdc_mem = win_gdi::CreateCompatibleDC(hdc_screen);
            let hbm = win_gdi::CreateCompatibleBitmap(hdc_screen, dst_w, dst_h);
            if hdc_mem == 0 || hbm == 0 {
                if hbm != 0 { win_gdi::DeleteObject(hbm); }
                if hdc_mem != 0 { win_gdi::DeleteDC(hdc_mem); }
                win_gdi::ReleaseDC(0, hdc_screen);
                return Err("Failed to create compatible bitmap".to_string());
            }

            let old_obj = win_gdi::SelectObject(hdc_mem, hbm);
            let ok = if dst_w == src_w && dst_h == src_h {
                win_gdi::BitBlt(hdc_mem, 0, 0, dst_w, dst_h, hdc_screen, 0, 0, SRCCOPY)
            } else {
                win_gdi::SetStretchBltMode(hdc_mem, HALFTONE);
                win_gdi::StretchBlt(hdc_mem, 0, 0, dst_w, dst_h, hdc_screen, 0, 0, src_w, src_h, SRCCOPY)
            };

            let mut result = Err("BitBlt failed".to_string());
            if ok != 0 {
                let mut bmi = win_gdi::BITMAPINFO {
                    bmi_header: win_gdi::BITMAPINFOHEADER {
                        bi_size: std::mem::size_of::<win_gdi::BITMAPINFOHEADER>() as u32,
                        bi_width: dst_w,
                        bi_height: -dst_h,
                        bi_planes: 1,
                        bi_bit_count: 32,
                        bi_compression: 0,
                        bi_size_image: 0,
                        bi_x_pels_per_meter: 0,
                        bi_y_pels_per_meter: 0,
                        bi_clr_used: 0,
                        bi_clr_important: 0,
                    },
                    bmi_colors: [0],
                };
                let mut bgra = vec![0u8; (dst_w * dst_h * 4) as usize];
                if win_gdi::GetDIBits(hdc_mem, hbm, 0, dst_h as u32, bgra.as_mut_ptr(), &mut bmi, 0) > 0 {
                    let mut rgb = Vec::with_capacity((dst_w * dst_h * 3) as usize);
                    for px in bgra.chunks_exact(4) {
                        rgb.push(px[2]);
                        rgb.push(px[1]);
                        rgb.push(px[0]);
                    }
                    let mut png_buf = Vec::with_capacity(128 * 1024);
                    let enc = PngEncoder::new_with_quality(&mut png_buf, CompressionType::Fast, FilterType::NoFilter);
                    if enc.write_image(&rgb, dst_w as u32, dst_h as u32, ExtendedColorType::Rgb8).is_ok() {
                        result = Ok(base64::engine::general_purpose::STANDARD.encode(&png_buf));
                    }
                }
            }

            win_gdi::SelectObject(hdc_mem, old_obj);
            win_gdi::DeleteObject(hbm);
            win_gdi::DeleteDC(hdc_mem);
            win_gdi::ReleaseDC(0, hdc_screen);
            result
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("Screenshot capture via MCP is currently supported on Windows hosts".to_string())
    }
}

pub fn start_native_screen_capture(app: AppHandle) -> Result<(), String> {
    FRAME_IN_FLIGHT.store(false, Ordering::Release);
    RESET_FRAME_HASH.store(true, Ordering::Release);
    let my_gen = CAPTURE_GEN.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    CAPTURE_ACTIVE.store(true, Ordering::SeqCst);

    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(move || {
            if let Err(e) = run_windows_capture_loop(app, my_gen) {
                tracing::warn!("Native screen capture stopped: {}", e);
            }
            if CAPTURE_GEN.load(Ordering::SeqCst) == my_gen {
                CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
            }
        });
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, my_gen);
        CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
        Err("Native zero-prompt screen capture is currently supported on Windows hosts".to_string())
    }
}

pub fn stop_native_screen_capture() {
    CAPTURE_GEN.fetch_add(1, Ordering::SeqCst);
    CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
    FRAME_IN_FLIGHT.store(false, Ordering::Release);
    set_capture_bounds(None);
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

    #[repr(C)]
    pub struct POINT {
        pub x: i32,
        pub y: i32,
    }

    #[repr(C)]
    pub struct CURSORINFO {
        pub cb_size: u32,
        pub flags: u32,
        pub h_cursor: isize,
        pub pt_screen_pos: POINT,
    }

    #[repr(C)]
    pub struct ICONINFO {
        pub f_icon: i32,
        pub x_hotspot: u32,
        pub y_hotspot: u32,
        pub hbm_mask: isize,
        pub hbm_color: isize,
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn GetDC(hwnd: isize) -> isize;
        pub fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
        pub fn GetSystemMetrics(n_index: i32) -> i32;
        pub fn GetCursorInfo(pci: *mut CURSORINFO) -> i32;
        pub fn LoadCursorW(h_instance: isize, lp_cursor_name: usize) -> isize;
        pub fn GetIconInfo(h_icon: isize, piconinfo: *mut ICONINFO) -> i32;
        pub fn DrawIconEx(
            hdc: isize,
            x_left: i32,
            y_top: i32,
            h_icon: isize,
            cx_width: i32,
            cy_width: i32,
            istep_if_ani_cur: u32,
            hbr_flicker_free_draw: isize,
            di_flags: u32,
        ) -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        pub fn CreateCompatibleDC(hdc: isize) -> isize;
        pub fn CreateCompatibleBitmap(hdc: isize, cx: i32, cy: i32) -> isize;
        pub fn SelectObject(hdc: isize, h: isize) -> isize;
        pub fn SetStretchBltMode(hdc: isize, mode: i32) -> i32;
        pub fn BitBlt(
            hdc_dest: isize,
            x_dest: i32,
            y_dest: i32,
            w_dest: i32,
            h_dest: i32,
            hdc_src: isize,
            x_src: i32,
            y_src: i32,
            rop: u32,
        ) -> i32;
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
fn detect_cursor_css_shape() -> &'static str {
    const CURSOR_SHOWING: u32 = 0x0000_0001;
    let mut ci = win_gdi::CURSORINFO {
        cb_size: std::mem::size_of::<win_gdi::CURSORINFO>() as u32,
        flags: 0,
        h_cursor: 0,
        pt_screen_pos: win_gdi::POINT { x: 0, y: 0 },
    };
    unsafe {
        if win_gdi::GetCursorInfo(&mut ci) == 0 || (ci.flags & CURSOR_SHOWING) == 0 || ci.h_cursor == 0 {
            return "default";
        }
        let h = ci.h_cursor;
        let table: &[(usize, &str)] = &[
            (32513, "text"),        // IDC_IBEAM
            (32649, "pointer"),     // IDC_HAND
            (32644, "ew-resize"),   // IDC_SIZEWE
            (32645, "ns-resize"),   // IDC_SIZENS
            (32642, "nwse-resize"), // IDC_SIZENWSE
            (32643, "nesw-resize"), // IDC_SIZENESW
            (32646, "move"),        // IDC_SIZEALL
            (32514, "wait"),        // IDC_WAIT
            (32650, "progress"),    // IDC_APPSTARTING
            (32515, "crosshair"),   // IDC_CROSS
            (32648, "not-allowed"), // IDC_NO
            (32651, "help"),        // IDC_HELP
        ];
        for &(idc, css) in table {
            if win_gdi::LoadCursorW(0, idc) == h {
                return css;
            }
        }
    }
    "default"
}

#[cfg(target_os = "windows")]
fn run_windows_capture_loop(app: AppHandle, my_gen: u64) -> Result<(), String> {
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
    let mut last_cursor_shape = "";

    while CAPTURE_ACTIVE.load(Ordering::Acquire) && CAPTURE_GEN.load(Ordering::Acquire) == my_gen {
        let loop_start = std::time::Instant::now();
        if RESET_FRAME_HASH.swap(false, Ordering::AcqRel) {
            last_frame_hash = 0;
        }

        let cursor_shape = detect_cursor_css_shape();
        if cursor_shape != last_cursor_shape {
            last_cursor_shape = cursor_shape;
            let _ = app.emit("host_cursor_shape", cursor_shape);
        }

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

        // Cap at 1920px width for fast, low-lag 30fps capture while keeping crisp 1080p text
        let scale = if src_w > 1920 {
            1920.0 / src_w as f32
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
                    let ok = if dst_w == src_w && dst_h == src_h {
                        win_gdi::BitBlt(
                            hdc_mem,
                            0,
                            0,
                            dst_w,
                            dst_h,
                            hdc_screen,
                            src_x,
                            src_y,
                            SRCCOPY,
                        )
                    } else {
                        win_gdi::SetStretchBltMode(hdc_mem, HALFTONE);
                        win_gdi::StretchBlt(
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
                        )
                    };

                    if ok != 0 {
                        if DRAW_HOST_CURSOR.load(Ordering::Acquire) {
                            let mut ci = win_gdi::CURSORINFO {
                                cb_size: std::mem::size_of::<win_gdi::CURSORINFO>() as u32,
                                flags: 0,
                                h_cursor: 0,
                                pt_screen_pos: win_gdi::POINT { x: 0, y: 0 },
                            };
                            if win_gdi::GetCursorInfo(&mut ci) != 0
                                && (ci.flags & 0x0000_0001) != 0
                                && ci.h_cursor != 0
                            {
                                let cx = ci.pt_screen_pos.x;
                                let cy = ci.pt_screen_pos.y;
                                if cx >= src_x
                                    && cx < src_x + src_w
                                    && cy >= src_y
                                    && cy < src_y + src_h
                                {
                                    let mut ii = win_gdi::ICONINFO {
                                        f_icon: 0,
                                        x_hotspot: 0,
                                        y_hotspot: 0,
                                        hbm_mask: 0,
                                        hbm_color: 0,
                                    };
                                    let (hx, hy) = if win_gdi::GetIconInfo(ci.h_cursor, &mut ii) != 0 {
                                        if ii.hbm_mask != 0 {
                                            win_gdi::DeleteObject(ii.hbm_mask);
                                        }
                                        if ii.hbm_color != 0 {
                                            win_gdi::DeleteObject(ii.hbm_color);
                                        }
                                        (ii.x_hotspot as i32, ii.y_hotspot as i32)
                                    } else {
                                        (0, 0)
                                    };
                                    let draw_x = ((cx - hx - src_x) * dst_w) / src_w.max(1);
                                    let draw_y = ((cy - hy - src_y) * dst_h) / src_h.max(1);
                                    win_gdi::DrawIconEx(
                                        hdc_mem,
                                        draw_x,
                                        draw_y,
                                        ci.h_cursor,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0x0003,
                                    );
                                }
                            }
                        }

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
                                let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_buf, 88);
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
                    } else {
                        // Desktop transition (Winlogon <-> Default) or UAC prompt: switch desktop immediately
                        crate::win_service::switch_to_active_input_desktop();
                        last_frame_hash = 0;
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

        // Target ~30 fps (33ms per frame) matching Chrome Remote Desktop's smooth low-lag cadence
        let elapsed = loop_start.elapsed();
        if elapsed < std::time::Duration::from_millis(33) {
            std::thread::sleep(std::time::Duration::from_millis(33) - elapsed);
        }
    }

    Ok(())
}

/// Captures a specific monitor index directly via Windows GDI (`GetDC(0)` + `StretchBlt`)
/// and pushes JPEG bytes to the Second Screen `watch::Sender`.
/// Avoids Chromium/WebView2 `getDisplayMedia()` WGC black-screen issues on virtual displays (e.g. MttVDD).
pub fn spawn_second_screen_gdi_capture(
    active_flag: Arc<AtomicBool>,
    enabled_flag: Arc<AtomicBool>,
    monitor_index: Arc<AtomicI32>,
    frame_tx: tokio::sync::watch::Sender<Vec<u8>>,
) {
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(move || {
            use image::codecs::jpeg::JpegEncoder;
            use image::ExtendedColorType;

            const SRCCOPY: u32 = 0x00CC0020;
            const HALFTONE: i32 = 4;

            let mut bgra: Vec<u8> = Vec::new();
            let mut rgb: Vec<u8> = Vec::new();
            let mut jpeg_buf: Vec<u8> = Vec::with_capacity(65536);
            let mut last_frame_hash: u64 = 0;
            let mut last_monitor_idx: i32 = -999;
            let mut last_force_emit =
                std::time::Instant::now() - std::time::Duration::from_secs(2);

            while active_flag.load(Ordering::Relaxed) {
                if !enabled_flag.load(Ordering::Relaxed) {
                    last_frame_hash = 0;
                    std::thread::sleep(std::time::Duration::from_millis(60));
                    continue;
                }

                let loop_start = std::time::Instant::now();
        if RESET_FRAME_HASH.swap(false, Ordering::AcqRel) {
            last_frame_hash = 0;
        }
                let idx = monitor_index.load(Ordering::Relaxed);
                if idx != last_monitor_idx {
                    last_monitor_idx = idx;
                    last_frame_hash = 0;
                }

                let monitors = crate::remote_input::list_monitors();
                let target_mon = if idx >= 0 {
                    monitors.get(idx as usize).or_else(|| monitors.first())
                } else {
                    monitors.first()
                };

                let (src_x, src_y, src_w, src_h) = match target_mon {
                    Some(m) if m.width > 0 && m.height > 0 => (m.x, m.y, m.width, m.height),
                    _ => unsafe {
                        let w = win_gdi::GetSystemMetrics(0).max(1280);
                        let h = win_gdi::GetSystemMetrics(1).max(720);
                        (0, 0, w, h)
                    },
                };

                let scale = if src_w > 1600 {
                    1600.0 / src_w as f32
                } else {
                    1.0
                };
                let dst_w = ((src_w as f32 * scale).round() as i32).max(2) & !1;
                let dst_h = ((src_h as f32 * scale).round() as i32).max(2) & !1;

                unsafe {
                    let hdc_screen = win_gdi::GetDC(0);
                    if hdc_screen != 0 {
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
                                // Overlay the real Windows cursor onto Display 2 capture so it behaves like a normal 2nd monitor
                                let mut ci = win_gdi::CURSORINFO {
                                    cb_size: std::mem::size_of::<win_gdi::CURSORINFO>() as u32,
                                    flags: 0,
                                    h_cursor: 0,
                                    pt_screen_pos: win_gdi::POINT { x: 0, y: 0 },
                                };
                                if win_gdi::GetCursorInfo(&mut ci) != 0
                                    && (ci.flags & 1) != 0
                                    && ci.h_cursor != 0
                                {
                                    let cx = ci.pt_screen_pos.x;
                                    let cy = ci.pt_screen_pos.y;
                                    if cx >= src_x - 32
                                        && cx < src_x + src_w + 32
                                        && cy >= src_y - 32
                                        && cy < src_y + src_h + 32
                                    {
                                        let mut ii = win_gdi::ICONINFO {
                                            f_icon: 0,
                                            x_hotspot: 0,
                                            y_hotspot: 0,
                                            hbm_mask: 0,
                                            hbm_color: 0,
                                        };
                                        let (hx, hy) = if win_gdi::GetIconInfo(ci.h_cursor, &mut ii) != 0 {
                                            if ii.hbm_mask != 0 {
                                                win_gdi::DeleteObject(ii.hbm_mask);
                                            }
                                            if ii.hbm_color != 0 {
                                                win_gdi::DeleteObject(ii.hbm_color);
                                            }
                                            (ii.x_hotspot as i32, ii.y_hotspot as i32)
                                        } else {
                                            (0, 0)
                                        };
                                        let draw_x = ((cx - src_x - hx) as f32 * scale).round() as i32;
                                        let draw_y = ((cy - src_y - hy) as f32 * scale).round() as i32;
                                        win_gdi::DrawIconEx(
                                            hdc_mem,
                                            draw_x,
                                            draw_y,
                                            ci.h_cursor,
                                            0,
                                            0,
                                            0,
                                            0,
                                            0x0003, // DI_NORMAL
                                        );
                                    }
                                }

                                let mut bmi = win_gdi::BITMAPINFO {
                                    bmi_header: win_gdi::BITMAPINFOHEADER {
                                        bi_size: std::mem::size_of::<win_gdi::BITMAPINFOHEADER>()
                                            as u32,
                                        bi_width: dst_w,
                                        bi_height: -dst_h,
                                        bi_planes: 1,
                                        bi_bit_count: 32,
                                        bi_compression: 0,
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
                                    let mut h: u64 = 0xcbf29ce484222325;
                                    for &b in bgra.iter().step_by(64) {
                                        h ^= b as u64;
                                        h = h.wrapping_mul(0x100000001b3);
                                    }

                                    if h != last_frame_hash
                                        || last_force_emit.elapsed()
                                            >= std::time::Duration::from_millis(1000)
                                    {
                                        last_frame_hash = h;
                                        last_force_emit = std::time::Instant::now();

                                        rgb.clear();
                                        let needed_rgb = (dst_w * dst_h * 3) as usize;
                                        if rgb.capacity() < needed_rgb {
                                            rgb.reserve(needed_rgb - rgb.capacity());
                                        }
                                        for px in bgra.chunks_exact(4) {
                                            rgb.push(px[2]);
                                            rgb.push(px[1]);
                                            rgb.push(px[0]);
                                        }

                                        jpeg_buf.clear();
                                        let mut encoder =
                                            JpegEncoder::new_with_quality(&mut jpeg_buf, 76);
                                        if encoder
                                            .encode(
                                                &rgb,
                                                dst_w as u32,
                                                dst_h as u32,
                                                ExtendedColorType::Rgb8,
                                            )
                                            .is_ok()
                                        {
                                            let _ = frame_tx.send(jpeg_buf.clone());
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

                let elapsed = loop_start.elapsed();
                if elapsed < std::time::Duration::from_millis(42) {
                    std::thread::sleep(std::time::Duration::from_millis(42) - elapsed);
                }
            }
        });
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (active_flag, enabled_flag, monitor_index, frame_tx);
    }
}
