//! Raydrop Remote Desktop Input Simulation Engine
//! Provides native low-overhead mouse and keyboard injection for remote control.

#[cfg(target_os = "windows")]
mod win_user32 {
    #[link(name = "user32")]
    extern "system" {
        pub fn SetCursorPos(x: i32, y: i32) -> i32;
        pub fn mouse_event(dw_flags: u32, dx: u32, dy: u32, dw_data: u32, dw_extra_info: usize);
        pub fn keybd_event(b_vk: u8, b_scan: u8, dw_flags: u32, dw_extra_info: usize);
        pub fn MapVirtualKeyA(u_code: u32, u_map_type: u32) -> u32;
        pub fn GetSystemMetrics(n_index: i32) -> i32;
        pub fn LockWorkStation() -> i32;
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

/// Moves cursor to normalized coordinates [0.0, 1.0]
pub fn simulate_mouse_move(norm_x: f32, norm_y: f32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let (screen_w, screen_h) = get_screen_size();
        let target_x = ((norm_x * screen_w as f32).round() as i32).clamp(0, screen_w as i32 - 1);
        let target_y = ((norm_y * screen_h as f32).round() as i32).clamp(0, screen_h as i32 - 1);
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
        "MetaLeft" | "MetaRight" | "OSLeft" | "OSRight" => return Some(0x5B),
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
        "meta" | "win" | "window" => Some(0x5B),
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
        if let Some(vk) = map_key_to_vk(key, code) {
            unsafe {
                let scan = (win_user32::MapVirtualKeyA(vk as u32, 0) & 0xFF) as u8;
                let mut flags = 0u32;
                // Extended key flag for navigation and system keys
                if matches!(vk, 0x21..=0x2E | 0x5B | 0x5C | 0x6F) {
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
