//! Raydrop Remote Desktop Input Simulation Engine
//! Provides native low-overhead mouse and keyboard injection for remote control.

use std::sync::Mutex;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub device_name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub is_primary: bool,
}

static TARGET_MONITOR_BOUNDS: Mutex<Option<(i32, i32, i32, i32)>> = Mutex::new(None);

#[cfg(target_os = "windows")]
mod win_user32 {
    #[repr(C)]
    pub struct RECT {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[repr(C)]
    pub struct MONITORINFOEXW {
        pub cb_size: u32,
        pub rc_monitor: RECT,
        pub rc_work: RECT,
        pub dw_flags: u32,
        pub sz_device: [u16; 32],
    }

    pub type MonitorEnumProc = unsafe extern "system" fn(isize, isize, *mut RECT, isize) -> i32;

    #[link(name = "user32")]
    extern "system" {
        pub fn SetCursorPos(x: i32, y: i32) -> i32;
        pub fn mouse_event(dw_flags: u32, dx: u32, dy: u32, dw_data: u32, dw_extra_info: usize);
        pub fn keybd_event(b_vk: u8, b_scan: u8, dw_flags: u32, dw_extra_info: usize);
        pub fn MapVirtualKeyA(u_code: u32, u_map_type: u32) -> u32;
        pub fn GetSystemMetrics(n_index: i32) -> i32;
        pub fn LockWorkStation() -> i32;
        pub fn EnumDisplayMonitors(
            hdc: isize,
            lprc_clip: *const RECT,
            lpfn_enum: MonitorEnumProc,
            dw_data: isize,
        ) -> i32;
        pub fn GetMonitorInfoW(hmonitor: isize, lpmi: *mut MONITORINFOEXW) -> i32;
        pub fn CreateCursor(
            h_inst: usize,
            x_hot_spot: i32,
            y_hot_spot: i32,
            n_width: i32,
            n_height: i32,
            pv_and_plane: *const u8,
            pv_xor_plane: *const u8,
        ) -> usize;
        pub fn SetSystemCursor(hcur: usize, id: u32) -> i32;
        pub fn SystemParametersInfoA(ui_action: u32, ui_param: u32, pv_param: usize, f_win_ini: u32) -> i32;
        pub fn EnumDisplaySettingsW(
            lpsz_device_name: *const u16,
            i_mode_num: u32,
            lp_dev_mode: *mut DEVMODEW,
        ) -> i32;
        pub fn ChangeDisplaySettingsExW(
            lpsz_device_name: *const u16,
            lp_dev_mode: *mut DEVMODEW,
            hwnd: isize,
            dwflags: u32,
            l_param: *const u8,
        ) -> i32;
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct DEVMODEW {
        pub dm_device_name: [u16; 32],
        pub dm_spec_version: u16,
        pub dm_driver_version: u16,
        pub dm_size: u16,
        pub dm_driver_extra: u16,
        pub dm_fields: u32,
        pub dm_union: [u8; 16],
        pub dm_color: i16,
        pub dm_duplex: i16,
        pub dm_y_resolution: i16,
        pub dm_tt_option: i16,
        pub dm_collate: i16,
        pub dm_form_name: [u16; 32],
        pub dm_log_pixels: u16,
        pub dm_bits_per_pel: u32,
        pub dm_pels_width: u32,
        pub dm_pels_height: u32,
        pub dm_display_flags: u32,
        pub dm_display_frequency: u32,
        pub dm_icm_method: u32,
        pub dm_icm_intent: u32,
        pub dm_media_type: u32,
        pub dm_dither_type: u32,
        pub dm_reserved1: u32,
        pub dm_reserved2: u32,
        pub dm_panning_width: u32,
        pub dm_panning_height: u32,
    }

    const _: () = assert!(std::mem::size_of::<DEVMODEW>() == 220);
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn monitor_enum_callback(
    hmonitor: isize,
    _hdc: isize,
    _lprc: *mut win_user32::RECT,
    dw_data: isize,
) -> i32 {
    let list = &mut *(dw_data as *mut Vec<MonitorInfo>);
    let mut info = win_user32::MONITORINFOEXW {
        cb_size: std::mem::size_of::<win_user32::MONITORINFOEXW>() as u32,
        rc_monitor: win_user32::RECT { left: 0, top: 0, right: 0, bottom: 0 },
        rc_work: win_user32::RECT { left: 0, top: 0, right: 0, bottom: 0 },
        dw_flags: 0,
        sz_device: [0u16; 32],
    };
    if win_user32::GetMonitorInfoW(hmonitor, &mut info) != 0 {
        let len = info.sz_device.iter().position(|&c| c == 0).unwrap_or(32);
        let raw_name = String::from_utf16_lossy(&info.sz_device[..len]);
        let idx = list.len();
        let is_primary = (info.dw_flags & 1) != 0;
        let label = if is_primary {
            format!("Display {} (Primary)", idx + 1)
        } else {
            format!("Display {}", idx + 1)
        };
        let clean_dev = raw_name.trim_start_matches("\\\\.\\");
        list.push(MonitorInfo {
            index: idx,
            name: if clean_dev.is_empty() { label } else { format!("{label} ({clean_dev})") },
            device_name: raw_name,
            x: info.rc_monitor.left,
            y: info.rc_monitor.top,
            width: info.rc_monitor.right - info.rc_monitor.left,
            height: info.rc_monitor.bottom - info.rc_monitor.top,
            is_primary,
        });
    }
    1
}

/// Lists all active display monitors on the host
pub fn list_monitors() -> Vec<MonitorInfo> {
    #[cfg(target_os = "windows")]
    {
        let mut monitors: Vec<MonitorInfo> = Vec::new();
        unsafe {
            win_user32::EnumDisplayMonitors(
                0,
                std::ptr::null(),
                monitor_enum_callback,
                &mut monitors as *mut Vec<MonitorInfo> as isize,
            );
        }
        if monitors.is_empty() {
            let (w, h) = get_screen_size();
            monitors.push(MonitorInfo {
                index: 0,
                name: "Display 1 (Primary)".to_string(),
                device_name: String::new(),
                x: 0,
                y: 0,
                width: w as i32,
                height: h as i32,
                is_primary: true,
            });
        }
        monitors
    }
    #[cfg(not(target_os = "windows"))]
    {
        vec![MonitorInfo {
            index: 0,
            name: "Display 1 (Primary)".to_string(),
            device_name: String::new(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            is_primary: true,
        }]
    }
}

/// Lists supported resolutions `(width, height)` for a given monitor index
pub fn list_monitor_resolutions(monitor_index: i32) -> Vec<(u32, u32)> {
    #[cfg(target_os = "windows")]
    {
        let monitors = list_monitors();
        let Some(mon) = monitors.get(monitor_index.max(0) as usize) else {
            return Vec::new();
        };
        if mon.device_name.is_empty() {
            return Vec::new();
        }

        let dev_w: Vec<u16> = mon.device_name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut modes: Vec<(u32, u32)> = Vec::new();
        let mut mode_num: u32 = 0;

        loop {
            let mut dm: win_user32::DEVMODEW = unsafe { std::mem::zeroed() };
            dm.dm_size = std::mem::size_of::<win_user32::DEVMODEW>() as u16;
            let ok = unsafe { win_user32::EnumDisplaySettingsW(dev_w.as_ptr(), mode_num, &mut dm) };
            if ok == 0 {
                break;
            }
            if dm.dm_pels_width >= 800 && dm.dm_pels_height >= 600 {
                let res = (dm.dm_pels_width, dm.dm_pels_height);
                if !modes.contains(&res) {
                    modes.push(res);
                }
            }
            mode_num += 1;
            if mode_num > 512 {
                break;
            }
        }

        modes.sort_by_key(|&(w, h)| (w as u64) * (h as u64));
        modes
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = monitor_index;
        Vec::new()
    }
}

/// Changes resolution `(width, height)` for a given monitor index
pub fn set_monitor_resolution(monitor_index: i32, width: u32, height: u32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        const ENUM_CURRENT_SETTINGS: u32 = u32::MAX;
        const DM_PELSWIDTH: u32 = 0x0008_0000;
        const DM_PELSHEIGHT: u32 = 0x0010_0000;
        const CDS_UPDATEREGISTRY: u32 = 0x0000_0001;
        const CDS_GLOBAL: u32 = 0x0000_0008;
        const DISP_CHANGE_SUCCESSFUL: i32 = 0;
        const DISP_CHANGE_RESTART: i32 = 1;

        let monitors = list_monitors();
        let mon = monitors
            .get(monitor_index.max(0) as usize)
            .ok_or_else(|| "Monitor not found".to_string())?;
        if mon.device_name.is_empty() {
            return Err("Monitor device name unavailable".to_string());
        }

        let dev_w: Vec<u16> = mon.device_name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut dm: win_user32::DEVMODEW = unsafe { std::mem::zeroed() };
        dm.dm_size = std::mem::size_of::<win_user32::DEVMODEW>() as u16;

        let ok = unsafe {
            win_user32::EnumDisplaySettingsW(dev_w.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm)
        };
        if ok == 0 {
            return Err("Failed to read current display settings".to_string());
        }

        dm.dm_pels_width = width;
        dm.dm_pels_height = height;
        dm.dm_fields = DM_PELSWIDTH | DM_PELSHEIGHT;

        let res = unsafe {
            win_user32::ChangeDisplaySettingsExW(
                dev_w.as_ptr(),
                &mut dm,
                0,
                CDS_UPDATEREGISTRY | CDS_GLOBAL,
                std::ptr::null(),
            )
        };
        if res == DISP_CHANGE_SUCCESSFUL || res == DISP_CHANGE_RESTART {
            let _ = set_target_monitor(Some(monitor_index));
            Ok(())
        } else {
            Err(format!("Windows rejected resolution {width}x{height} (code {res})"))
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (monitor_index, width, height);
        Err("Resolution switching is only supported on Windows hosts".to_string())
    }
}

/// Sets the active target monitor bounds for mouse coordinate mapping (-1 or None = virtual all screens)
pub fn set_target_monitor(monitor_index: Option<i32>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let bounds = match monitor_index {
            None | Some(-1) => {
                let vx = unsafe { win_user32::GetSystemMetrics(76) }; // SM_XVIRTUALSCREEN
                let vy = unsafe { win_user32::GetSystemMetrics(77) }; // SM_YVIRTUALSCREEN
                let vw = unsafe { win_user32::GetSystemMetrics(78) }; // SM_CXVIRTUALSCREEN
                let vh = unsafe { win_user32::GetSystemMetrics(79) }; // SM_CYVIRTUALSCREEN
                if vw > 0 && vh > 0 {
                    Some((vx, vy, vw, vh))
                } else {
                    None
                }
            }
            Some(idx) if idx >= 0 => {
                let monitors = list_monitors();
                monitors
                    .get(idx as usize)
                    .map(|m| (m.x, m.y, m.width, m.height))
            }
            _ => None,
        };
        if let Ok(mut guard) = TARGET_MONITOR_BOUNDS.lock() {
            *guard = bounds;
        }
        crate::remote_capture::set_capture_bounds(bounds);
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = monitor_index;
        Ok(())
    }
}

/// Retrieves primary display resolution
pub fn get_screen_size() -> (u32, u32) {
    #[cfg(target_os = "windows")]
    {
        let w = unsafe { win_user32::GetSystemMetrics(0) }; // SM_CXSCREEN
        let h = unsafe { win_user32::GetSystemMetrics(1) }; // SM_CYSCREEN
        let width = if w > 0 { w as u32 } else { 1920 };
        let height = if h > 0 { h as u32 } else { 1080 };
        (width, height)
    }
    #[cfg(not(target_os = "windows"))]
    {
        (1920, 1080)
    }
}

/// Moves cursor to normalized coordinates [0.0, 1.0] mapped to active monitor bounds
pub fn simulate_mouse_move(norm_x: f32, norm_y: f32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        crate::win_service::switch_to_active_input_desktop();
        let active_bounds = TARGET_MONITOR_BOUNDS.lock().ok().and_then(|g| *g);
        let (ox, oy, screen_w, screen_h) = match active_bounds {
            Some((x, y, w, h)) if w > 0 && h > 0 => (x, y, w, h),
            _ => {
                let (w, h) = get_screen_size();
                (0, 0, w as i32, h as i32)
            }
        };
        let target_x = ox + ((norm_x * screen_w as f32).round() as i32).clamp(0, screen_w - 1);
        let target_y = oy + ((norm_y * screen_h as f32).round() as i32).clamp(0, screen_h - 1);
        unsafe {
            win_user32::SetCursorPos(target_x, target_y);
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (norm_x, norm_y);
        Err("Mouse movement simulation currently supported on Windows host".to_string())
    }
}

/// Releases all modifier keys (Alt, Ctrl, Shift, Windows) to prevent stuck keys
pub fn release_all_modifiers() {
    #[cfg(target_os = "windows")]
    unsafe {
        // Release VK_MENU (Alt 0x12), VK_CONTROL (0x11), VK_SHIFT (0x10), VK_LWIN (0x5B), VK_RWIN (0x5C)
        win_user32::keybd_event(0x12, 0, 0x0002, 0);
        win_user32::keybd_event(0x11, 0, 0x0002, 0);
        win_user32::keybd_event(0x10, 0, 0x0002, 0);
        win_user32::keybd_event(0x5B, 0, 0x0002, 0);
        win_user32::keybd_event(0x5C, 0, 0x0002, 0);
    }
}

/// Sets whether the host system cursor is hidden (transparent) during remote desktop sessions,
/// completely preventing the host cursor from being baked into the screen capture video stream.
pub fn set_host_cursor_hidden(hidden: bool) {
    #[cfg(target_os = "windows")]
    unsafe {
        if hidden {
            let and_mask = [0xFFu8; 128];
            let xor_mask = [0x00u8; 128];
            // Replace standard system cursors with a transparent 1x1 blank cursor:
            // 32512 = OCR_NORMAL, 32513 = OCR_IBEAM, 32514 = OCR_WAIT, 32515 = OCR_CROSS, 32649 = OCR_HAND
            for id in [32512, 32513, 32514, 32515, 32649] {
                let h_blank = win_user32::CreateCursor(
                    0,
                    0,
                    0,
                    32,
                    32,
                    and_mask.as_ptr(),
                    xor_mask.as_ptr(),
                );
                if h_blank != 0 {
                    let _ = win_user32::SetSystemCursor(h_blank, id);
                }
            }
        } else {
            // Restore default Windows cursor scheme from registry
            win_user32::SystemParametersInfoA(0x0057, 0, 0, 0); // SPI_SETCURSORS
        }
    }
}

/// Simulates mouse button clicks (left, right, middle) and actions (down, up, click)
pub fn simulate_mouse_click(
    button: &str,
    action: &str,
    alt: Option<bool>,
    ctrl: Option<bool>,
    shift: Option<bool>,
    meta: Option<bool>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        crate::win_service::switch_to_active_input_desktop();
        unsafe {
            // Synchronize modifier states with client to prevent phantom stuck keys (e.g. Alt + Click opening file properties)
            if alt == Some(false) {
                win_user32::keybd_event(0x12, 0, 0x0002, 0); // Alt up
            }
            if ctrl == Some(false) {
                win_user32::keybd_event(0x11, 0, 0x0002, 0); // Ctrl up
            }
            if shift == Some(false) {
                win_user32::keybd_event(0x10, 0, 0x0002, 0); // Shift up
            }
            if meta == Some(false) {
                win_user32::keybd_event(0x5B, 0, 0x0002, 0); // Win up
            }

            let (down_flag, up_flag) = match button.to_lowercase().as_str() {
                "left" | "0" => (0x0002, 0x0004),   // MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP
                "right" | "2" => (0x0008, 0x0010),  // MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP
                "middle" | "1" => (0x0020, 0x0040), // MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP
                _ => (0x0002, 0x0004),
            };

            match action.to_lowercase().as_str() {
                "down" => win_user32::mouse_event(down_flag, 0, 0, 0, 0),
                "up" => win_user32::mouse_event(up_flag, 0, 0, 0, 0),
                "click" => {
                    win_user32::mouse_event(down_flag, 0, 0, 0, 0);
                    win_user32::mouse_event(up_flag, 0, 0, 0, 0);
                }
                _ => {}
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (button, action, alt, ctrl, shift, meta);
        Err("Mouse click simulation currently supported on Windows host".to_string())
    }
}

/// Simulates mouse vertical wheel scrolling
pub fn simulate_mouse_wheel(delta_y: i32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Windows wheel delta: 120 per notch, inverted delta from browser
        let win_delta = (delta_y * -1) as u32;
        unsafe {
            win_user32::mouse_event(0x0800, 0, 0, win_delta, 0); // MOUSEEVENTF_WHEEL
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = delta_y;
        Err("Mouse wheel simulation currently supported on Windows host".to_string())
    }
}

/// Maps common JavaScript key and code names to Windows Virtual Key codes (VK)
fn map_key_to_vk(key: &str, code: &str) -> Option<u8> {
    // 1. Check physical code first (immune to layout shifts)
    match code {
        "Backspace" => return Some(0x08),
        "Tab" => return Some(0x09),
        "Enter" | "NumpadEnter" => return Some(0x0D),
        "ShiftLeft" | "ShiftRight" => return Some(0x10),
        "ControlLeft" | "ControlRight" => return Some(0x11),
        "AltLeft" | "AltRight" => return Some(0x12),
        "Pause" => return Some(0x13),
        "CapsLock" => return Some(0x14),
        "Escape" => return Some(0x1B),
        "Space" => return Some(0x20),
        "PageUp" => return Some(0x21),
        "PageDown" => return Some(0x22),
        "End" => return Some(0x23),
        "Home" => return Some(0x24),
        "ArrowLeft" => return Some(0x25),
        "ArrowUp" => return Some(0x26),
        "ArrowRight" => return Some(0x27),
        "ArrowDown" => return Some(0x28),
        "Insert" => return Some(0x2D),
        "Delete" => return Some(0x2E),
        "PrintScreen" => return Some(0x2C),
        "ContextMenu" => return Some(0x5D),
        "ScrollLock" => return Some(0x91),
        "NumLock" => return Some(0x90),
        "MetaLeft" | "OSLeft" => return Some(0x5B),
        "MetaRight" | "OSRight" => return Some(0x5C),
        // OEM Punctuations & Symbols
        "Semicolon" => return Some(0xBA),
        "Equal" => return Some(0xBB),
        "Comma" => return Some(0xBC),
        "Minus" => return Some(0xBD),
        "Period" => return Some(0xBE),
        "Slash" => return Some(0xBF),
        "Backquote" => return Some(0xC0),
        "BracketLeft" => return Some(0xDB),
        "Backslash" => return Some(0xDC),
        "BracketRight" => return Some(0xDD),
        "Quote" => return Some(0xDE),
        // Numpad Keys
        "Numpad0" => return Some(0x60),
        "Numpad1" => return Some(0x61),
        "Numpad2" => return Some(0x62),
        "Numpad3" => return Some(0x63),
        "Numpad4" => return Some(0x64),
        "Numpad5" => return Some(0x65),
        "Numpad6" => return Some(0x66),
        "Numpad7" => return Some(0x67),
        "Numpad8" => return Some(0x68),
        "Numpad9" => return Some(0x69),
        "NumpadMultiply" => return Some(0x6A),
        "NumpadAdd" => return Some(0x6B),
        "NumpadSubtract" => return Some(0x6D),
        "NumpadDecimal" => return Some(0x6E),
        "NumpadDivide" => return Some(0x6F),
        _ => {}
    }

    // Alpha keys: "KeyA" -> 'A'
    if code.starts_with("Key") && code.len() == 4 {
        let ch = code.chars().nth(3).unwrap().to_ascii_uppercase();
        return Some(ch as u8);
    }
    // Number keys: "Digit1" -> '1'
    if code.starts_with("Digit") && code.len() == 6 {
        let ch = code.chars().nth(5).unwrap();
        return Some(ch as u8);
    }
    // Function keys: "F1" .. "F24"
    if code.starts_with('F') && code.len() >= 2 && code.len() <= 3 {
        if let Ok(num) = code[1..].parse::<u8>() {
            if (1..=24).contains(&num) {
                return Some(0x70 + (num - 1));
            }
        }
    }

    // 2. Fallback to key name/character
    match key.to_lowercase().as_str() {
        "backspace" => Some(0x08),
        "tab" => Some(0x09),
        "enter" => Some(0x0D),
        "shift" => Some(0x10),
        "control" | "ctrl" => Some(0x11),
        "alt" => Some(0x12),
        "pause" => Some(0x13),
        "capslock" => Some(0x14),
        "escape" | "esc" => Some(0x1B),
        " " | "space" => Some(0x20),
        "pageup" => Some(0x21),
        "pagedown" => Some(0x22),
        "end" => Some(0x23),
        "home" => Some(0x24),
        "arrowleft" => Some(0x25),
        "arrowup" => Some(0x26),
        "arrowright" => Some(0x27),
        "arrowdown" => Some(0x28),
        "insert" => Some(0x2D),
        "delete" => Some(0x2E),
        "printscreen" | "snapshot" | "prtscn" => Some(0x2C),
        "contextmenu" | "apps" => Some(0x5D),
        "scrolllock" => Some(0x91),
        "numlock" => Some(0x90),
        "meta" | "win" | "window" | "os" => Some(0x5B),
        ";" | ":" => Some(0xBA),
        "=" | "+" => Some(0xBB),
        "," | "<" => Some(0xBC),
        "-" | "_" => Some(0xBD),
        "." | ">" => Some(0xBE),
        "/" | "?" => Some(0xBF),
        "`" | "~" => Some(0xC0),
        "[" | "{" => Some(0xDB),
        "\\" | "|" => Some(0xDC),
        "]" | "}" => Some(0xDD),
        "'" | "\"" => Some(0xDE),
        _ => {
            if key.len() == 1 {
                let ch = key.chars().next().unwrap().to_ascii_uppercase();
                if ch.is_ascii_alphanumeric() {
                    return Some(ch as u8);
                }
            }
            None
        }
    }
}

/// Simulates a single keyboard key down, up, or press with scan code and extended key support
pub fn simulate_keyboard_key(key: &str, code: &str, action: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        crate::win_service::switch_to_active_input_desktop();
        if let Some(vk) = map_key_to_vk(key, code) {
            unsafe {
                let scan = (win_user32::MapVirtualKeyA(vk as u32, 0) & 0xFF) as u8;
                let mut flags = 0u32;
                // Extended key flag for navigation and system keys
                if matches!(vk, 0x21..=0x2E | 0x5B | 0x5C | 0x5D | 0x6F) {
                    flags |= 0x0001; // KEYEVENTF_EXTENDEDKEY
                }

                match action.to_lowercase().as_str() {
                    "down" => win_user32::keybd_event(vk, scan, flags, 0),
                    "up" => win_user32::keybd_event(vk, scan, flags | 0x0002, 0), // KEYEVENTF_KEYUP
                    "press" => {
                        win_user32::keybd_event(vk, scan, flags, 0);
                        win_user32::keybd_event(vk, scan, flags | 0x0002, 0);
                    }
                    _ => {}
                }
            }
            Ok(())
        } else {
            Ok(())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (key, code, action);
        Err("Keyboard simulation currently supported on Windows host".to_string())
    }
}

/// Simulates common multi-key desktop shortcuts
pub fn simulate_shortcut(shortcut: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        unsafe {
            match shortcut.to_lowercase().as_str() {
                // Show Desktop (Win + D)
                "win_d" => {
                    win_user32::keybd_event(0x5B, 0, 0, 0); // Win down
                    win_user32::keybd_event(0x44, 0, 0, 0); // D down
                    win_user32::keybd_event(0x44, 0, 0x0002, 0); // D up
                    win_user32::keybd_event(0x5B, 0, 0x0002, 0); // Win up
                }
                // Open Task Manager (Ctrl + Shift + Esc)
                "ctrl_shift_esc" | "task_manager" => {
                    win_user32::keybd_event(0x11, 0, 0, 0); // Ctrl down
                    win_user32::keybd_event(0x10, 0, 0, 0); // Shift down
                    win_user32::keybd_event(0x1B, 0, 0, 0); // Esc down
                    win_user32::keybd_event(0x1B, 0, 0x0002, 0); // Esc up
                    win_user32::keybd_event(0x10, 0, 0x0002, 0); // Shift up
                    win_user32::keybd_event(0x11, 0, 0x0002, 0); // Ctrl up
                }
                // Alt + Tab
                "alt_tab" => {
                    win_user32::keybd_event(0x12, 0, 0, 0); // Alt down
                    win_user32::keybd_event(0x09, 0, 0, 0); // Tab down
                    win_user32::keybd_event(0x09, 0, 0x0002, 0); // Tab up
                    win_user32::keybd_event(0x12, 0, 0x0002, 0); // Alt up
                }
                // Lock Workstation (Win + L)
                "win_l" | "lock_pc" => {
                    win_user32::LockWorkStation();
                }
                // Paste (Ctrl + V)
                "paste" | "ctrl_v" => {
                    win_user32::keybd_event(0x11, 0, 0, 0); // Ctrl down
                    win_user32::keybd_event(0x56, 0, 0, 0); // V down
                    win_user32::keybd_event(0x56, 0, 0x0002, 0); // V up
                    win_user32::keybd_event(0x11, 0, 0x0002, 0); // Ctrl up
                }
                // Copy (Ctrl + C)
                "copy" | "ctrl_c" => {
                    win_user32::keybd_event(0x11, 0, 0, 0); // Ctrl down
                    win_user32::keybd_event(0x43, 0, 0, 0); // C down
                    win_user32::keybd_event(0x43, 0, 0x0002, 0); // C up
                    win_user32::keybd_event(0x11, 0, 0x0002, 0); // Ctrl up
                }
                // Select All (Ctrl + A)
                "select_all" | "ctrl_a" => {
                    win_user32::keybd_event(0x11, 0, 0, 0); // Ctrl down
                    win_user32::keybd_event(0x41, 0, 0, 0); // A down
                    win_user32::keybd_event(0x41, 0, 0x0002, 0); // A up
                    win_user32::keybd_event(0x11, 0, 0x0002, 0); // Ctrl up
                }
                _ => return Err(format!("Unknown shortcut: {shortcut}")),
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = shortcut;
        Err("Shortcut simulation currently supported on Windows host".to_string())
    }
}

/// Executes system power actions on the host machine (lock, sleep, restart, shutdown)
pub fn execute_power_action(action: &str) -> Result<(), String> {
    match action.to_lowercase().as_str() {
        "lock" => {
            #[cfg(target_os = "windows")]
            unsafe {
                win_user32::LockWorkStation();
            }
            #[cfg(target_os = "macos")]
            {
                let _ = std::process::Command::new("pmset").arg("displaysleepnow").spawn();
            }
            #[cfg(target_os = "linux")]
            {
                let _ = std::process::Command::new("loginctl").arg("lock-session").spawn();
            }
            Ok(())
        }
        "sleep" => {
            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("rundll32.exe")
                    .args(["powrprof.dll,SetSuspendState", "0,1,0"])
                    .spawn()
                    .map_err(|e| format!("Failed to sleep host: {e}"))?;
            }
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("pmset")
                    .arg("sleepnow")
                    .spawn()
                    .map_err(|e| format!("Failed to sleep host: {e}"))?;
            }
            #[cfg(target_os = "linux")]
            {
                std::process::Command::new("systemctl")
                    .arg("suspend")
                    .spawn()
                    .map_err(|e| format!("Failed to sleep host: {e}"))?;
            }
            Ok(())
        }
        "restart" => {
            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("shutdown")
                    .args(["/r", "/t", "3", "/c", "Raydrop Remote Restart"])
                    .spawn()
                    .map_err(|e| format!("Failed to restart host: {e}"))?;
            }
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                std::process::Command::new("shutdown")
                    .args(["-r", "now"])
                    .spawn()
                    .map_err(|e| format!("Failed to restart host: {e}"))?;
            }
            Ok(())
        }
        "shutdown" => {
            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("shutdown")
                    .args(["/s", "/t", "3", "/c", "Raydrop Remote Shutdown"])
                    .spawn()
                    .map_err(|e| format!("Failed to shutdown host: {e}"))?;
            }
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                std::process::Command::new("shutdown")
                    .args(["-h", "now"])
                    .spawn()
                    .map_err(|e| format!("Failed to shutdown host: {e}"))?;
            }
            Ok(())
        }
        _ => Err(format!("Unsupported power action: {action}")),
    }
}
