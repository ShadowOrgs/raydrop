pub mod linux_nautilus;
pub mod mac_finder;
pub mod win_registry;

pub fn register_context_menu() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        return win_registry::register_windows_context_menu();
    }
    #[cfg(target_os = "linux")]
    {
        return linux_nautilus::register_nautilus_script();
    }
    #[cfg(target_os = "macos")]
    {
        return mac_finder::register_macos_finder_service();
    }
    #[allow(unreachable_code)]
    Err("Unsupported operating system for context menu integration".to_string())
}

pub fn set_autostart(_enabled: bool) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        return win_registry::set_windows_autostart(_enabled);
    }
    #[cfg(target_os = "linux")]
    {
        return linux_nautilus::set_linux_autostart(_enabled);
    }
    #[cfg(target_os = "macos")]
    {
        return mac_finder::set_macos_autostart(_enabled);
    }
    #[allow(unreachable_code)]
    Err("Unsupported operating system for autostart".to_string())
}

pub fn get_win11_direct_menu_status() -> bool {
    #[cfg(target_os = "windows")]
    {
        win_registry::is_win11_classic_menu_enabled()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

pub fn set_win11_direct_menu(enabled: bool) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        win_registry::set_win11_classic_menu(enabled)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = enabled;
        Ok(false)
    }
}

pub fn register_magnet_protocol() -> Result<String, String> {
    win_registry::register_magnet_protocol_handler()
}

/// Returns the available free disk space in bytes for the drive/filesystem containing `path`.
pub fn get_available_disk_space(path: &std::path::Path) -> Option<u64> {
    // Find the nearest existing ancestor path
    let mut check_dir = if path.is_file() {
        path.parent().unwrap_or(path).to_path_buf()
    } else {
        path.to_path_buf()
    };

    while !check_dir.exists() {
        if let Some(parent) = check_dir.parent() {
            check_dir = parent.to_path_buf();
        } else {
            break;
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = check_dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let mut free_bytes_available = 0u64;
        let mut total_number_of_bytes = 0u64;
        let mut total_number_of_free_bytes = 0u64;

        extern "system" {
            fn GetDiskFreeSpaceExW(
                lpDirectoryName: *const u16,
                lpFreeBytesAvailableToCaller: *mut u64,
                lpTotalNumberOfBytes: *mut u64,
                lpTotalNumberOfFreeBytes: *mut u64,
            ) -> i32;
        }

        let ret = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_bytes_available,
                &mut total_number_of_bytes,
                &mut total_number_of_free_bytes,
            )
        };

        if ret != 0 {
            Some(free_bytes_available)
        } else {
            None
        }
    }

    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let c_str = CString::new(check_dir.as_os_str().as_bytes()).ok()?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        let ret = unsafe { libc::statvfs(c_str.as_ptr(), &mut stat) };

        if ret == 0 {
            // stat.f_bavail = free blocks available to unprivileged user
            // stat.f_frsize = fundamental filesystem block size
            Some((stat.f_bavail as u64) * (stat.f_frsize as u64))
        } else {
            None
        }
    }

    #[cfg(not(any(target_os = "windows", unix)))]
    {
        None
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}



