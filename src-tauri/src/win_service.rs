//! Windows System Service (`RaydropService`) + Session 0 -> Console Session Spawner
//! + Winlogon Secure Desktop Switching (`OpenInputDesktop` / `SetThreadDesktop`).
//! Allows Raydrop Remote Desktop to run at Windows boot before user login,
//! capture the Windows PIN/Lock screen, and inject PIN keystrokes directly.

#[cfg(target_os = "windows")]
mod win32 {
    use std::ffi::c_void;

    pub type Handle = isize;
    pub type Hdesk = isize;
    pub type Dword = u32;
    pub type Bool = i32;

    pub const SERVICE_WIN32_OWN_PROCESS: Dword = 0x00000010;
    pub const SERVICE_START_PENDING: Dword = 0x00000002;
    pub const SERVICE_RUNNING: Dword = 0x00000004;
    pub const SERVICE_STOP_PENDING: Dword = 0x00000003;
    pub const SERVICE_STOPPED: Dword = 0x00000001;
    pub const SERVICE_ACCEPT_STOP: Dword = 0x00000001;
    pub const SERVICE_ACCEPT_SHUTDOWN: Dword = 0x00000004;
    pub const SERVICE_CONTROL_STOP: Dword = 0x00000001;
    pub const SERVICE_CONTROL_SHUTDOWN: Dword = 0x00000005;

    pub const TOKEN_ALL_ACCESS: Dword = 0x000F01FF;
    pub const SECURITY_IMPERSONATION: i32 = 2;
    pub const TOKEN_PRIMARY: i32 = 1;
    pub const TOKEN_SESSION_ID: i32 = 12;
    pub const CREATE_UNICODE_ENVIRONMENT: Dword = 0x00000400;
    pub const CREATE_NO_WINDOW: Dword = 0x08000000;
    pub const DESKTOP_ALL_ACCESS: Dword = 0x01FF;

    #[repr(C)]
    pub struct SERVICE_TABLE_ENTRYW {
        pub lp_service_name: *const u16,
        pub lp_service_proc: Option<unsafe extern "system" fn(Dword, *mut *mut u16)>,
    }

    #[repr(C)]
    pub struct SERVICE_STATUS {
        pub dw_service_type: Dword,
        pub dw_current_state: Dword,
        pub dw_controls_accepted: Dword,
        pub dw_win32_exit_code: Dword,
        pub dw_service_specific_exit_code: Dword,
        pub dw_check_point: Dword,
        pub dw_wait_hint: Dword,
    }

    #[repr(C)]
    pub struct STARTUPINFOW {
        pub cb: Dword,
        pub lp_reserved: *mut u16,
        pub lp_desktop: *mut u16,
        pub lp_title: *mut u16,
        pub dw_x: Dword,
        pub dw_y: Dword,
        pub dw_x_size: Dword,
        pub dw_y_size: Dword,
        pub dw_x_count_chars: Dword,
        pub dw_y_count_chars: Dword,
        pub dw_fill_attribute: Dword,
        pub dw_flags: Dword,
        pub w_show_window: u16,
        pub cb_reserved2: u16,
        pub lp_reserved2: *mut u8,
        pub h_std_input: Handle,
        pub h_std_output: Handle,
        pub h_std_error: Handle,
    }

    #[repr(C)]
    pub struct PROCESS_INFORMATION {
        pub h_process: Handle,
        pub h_thread: Handle,
        pub dw_process_id: Dword,
        pub dw_thread_id: Dword,
    }

    pub type HandlerProc = unsafe extern "system" fn(Dword);

    #[link(name = "advapi32")]
    extern "system" {
        pub fn StartServiceCtrlDispatcherW(lp_service_start_table: *const SERVICE_TABLE_ENTRYW) -> Bool;
        pub fn RegisterServiceCtrlHandlerW(lp_service_name: *const u16, lp_handler_proc: HandlerProc) -> Handle;
        pub fn SetServiceStatus(h_service_status: Handle, lp_service_status: *const SERVICE_STATUS) -> Bool;
        pub fn OpenProcessToken(process_handle: Handle, desired_access: Dword, token_handle: *mut Handle) -> Bool;
        pub fn DuplicateTokenEx(
            h_existing_token: Handle,
            dw_desired_access: Dword,
            lp_token_attributes: *const c_void,
            impersonation_level: i32,
            token_type: i32,
            ph_new_token: *mut Handle,
        ) -> Bool;
        pub fn SetTokenInformation(
            token_handle: Handle,
            token_information_class: i32,
            token_information: *const c_void,
            token_information_length: Dword,
        ) -> Bool;
        pub fn CreateProcessAsUserW(
            h_token: Handle,
            lp_application_name: *const u16,
            lp_command_line: *mut u16,
            lp_process_attributes: *const c_void,
            lp_thread_attributes: *const c_void,
            b_inherit_handles: Bool,
            dw_creation_flags: Dword,
            lp_environment: *const c_void,
            lp_current_directory: *const u16,
            lp_startup_info: *const STARTUPINFOW,
            lp_process_information: *mut PROCESS_INFORMATION,
        ) -> Bool;
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn WTSGetActiveConsoleSessionId() -> Dword;
        pub fn GetCurrentProcess() -> Handle;
        pub fn CloseHandle(h_object: Handle) -> Bool;
        pub fn WaitForSingleObject(h_handle: Handle, dw_milliseconds: Dword) -> Dword;
        pub fn TerminateProcess(h_process: Handle, u_exit_code: u32) -> Bool;
        pub fn GetExitCodeProcess(h_process: Handle, lp_exit_code: *mut Dword) -> Bool;
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn OpenInputDesktop(dw_flags: Dword, f_inherit: Bool, dw_desired_access: Dword) -> Hdesk;
        pub fn SetThreadDesktop(h_desktop: Hdesk) -> Bool;
        pub fn CloseDesktop(h_desktop: Hdesk) -> Bool;
        pub fn GetThreadDesktop(dw_thread_id: Dword) -> Hdesk;
    }
}

/// Switches the calling thread's desktop to the currently active input desktop
/// (`Winlogon` when on Lock/PIN screen, `Default` when logged in, or `UAC` prompt).
/// Safe no-op if running without privileges or on non-Windows.
pub fn switch_to_active_input_desktop() {
    #[cfg(target_os = "windows")]
    unsafe {
        let h_desk = win32::OpenInputDesktop(0, 0, win32::DESKTOP_ALL_ACCESS);
        if h_desk != 0 {
            let _ = win32::SetThreadDesktop(h_desk);
            let _ = win32::CloseDesktop(h_desk);
        }
    }
}

#[cfg(target_os = "windows")]
static mut SERVICE_STATUS_HANDLE: win32::Handle = 0;
#[cfg(target_os = "windows")]
static SERVICE_STOP_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(target_os = "windows")]
fn to_wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

#[cfg(target_os = "windows")]
unsafe fn report_status(state: u32, controls: u32, wait_hint: u32) {
    if SERVICE_STATUS_HANDLE == 0 {
        return;
    }
    let status = win32::SERVICE_STATUS {
        dw_service_type: win32::SERVICE_WIN32_OWN_PROCESS,
        dw_current_state: state,
        dw_controls_accepted: controls,
        dw_win32_exit_code: 0,
        dw_service_specific_exit_code: 0,
        dw_check_point: 0,
        dw_wait_hint: wait_hint,
    };
    win32::SetServiceStatus(SERVICE_STATUS_HANDLE, &status);
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn service_ctrl_handler(ctrl: win32::Dword) {
    match ctrl {
        win32::SERVICE_CONTROL_STOP | win32::SERVICE_CONTROL_SHUTDOWN => {
            SERVICE_STOP_FLAG.store(true, std::sync::atomic::Ordering::SeqCst);
            report_status(win32::SERVICE_STOP_PENDING, 0, 3000);
        }
        _ => {}
    }
}

#[cfg(target_os = "windows")]
unsafe fn spawn_agent_in_session(session_id: u32) -> Option<win32::Handle> {
    let exe = std::env::current_exe().ok()?;
    let exe_str = exe.to_string_lossy().to_string();

    let mut h_token: win32::Handle = 0;
    if win32::OpenProcessToken(
        win32::GetCurrentProcess(),
        win32::TOKEN_ALL_ACCESS,
        &mut h_token,
    ) == 0
    {
        return None;
    }

    let mut h_dup_token: win32::Handle = 0;
    let dup_ok = win32::DuplicateTokenEx(
        h_token,
        win32::TOKEN_ALL_ACCESS,
        std::ptr::null(),
        win32::SECURITY_IMPERSONATION,
        win32::TOKEN_PRIMARY,
        &mut h_dup_token,
    );
    win32::CloseHandle(h_token);
    if dup_ok == 0 || h_dup_token == 0 {
        return None;
    }

    let sid_val = session_id;
    let set_ok = win32::SetTokenInformation(
        h_dup_token,
        win32::TOKEN_SESSION_ID,
        &sid_val as *const u32 as *const std::ffi::c_void,
        std::mem::size_of::<u32>() as u32,
    );
    if set_ok == 0 {
        win32::CloseHandle(h_dup_token);
        return None;
    }

    let mut cmd_line = to_wide(&format!("\"{}\" --system-agent", exe_str));
    let mut desktop = to_wide("winsta0\\default");

    let si = win32::STARTUPINFOW {
        cb: std::mem::size_of::<win32::STARTUPINFOW>() as u32,
        lp_reserved: std::ptr::null_mut(),
        lp_desktop: desktop.as_mut_ptr(),
        lp_title: std::ptr::null_mut(),
        dw_x: 0,
        dw_y: 0,
        dw_x_size: 0,
        dw_y_size: 0,
        dw_x_count_chars: 0,
        dw_y_count_chars: 0,
        dw_fill_attribute: 0,
        dw_flags: 0,
        w_show_window: 0,
        cb_reserved2: 0,
        lp_reserved2: std::ptr::null_mut(),
        h_std_input: 0,
        h_std_output: 0,
        h_std_error: 0,
    };

    let mut pi = win32::PROCESS_INFORMATION {
        h_process: 0,
        h_thread: 0,
        dw_process_id: 0,
        dw_thread_id: 0,
    };

    let created = win32::CreateProcessAsUserW(
        h_dup_token,
        std::ptr::null(),
        cmd_line.as_mut_ptr(),
        std::ptr::null(),
        std::ptr::null(),
        0,
        win32::CREATE_NO_WINDOW | win32::CREATE_UNICODE_ENVIRONMENT,
        std::ptr::null(),
        std::ptr::null(),
        &si,
        &mut pi,
    );
    win32::CloseHandle(h_dup_token);

    if created == 0 {
        return None;
    }
    if pi.h_thread != 0 {
        win32::CloseHandle(pi.h_thread);
    }
    Some(pi.h_process)
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn service_main(_argc: win32::Dword, _argv: *mut *mut u16) {
    let svc_name = to_wide("RaydropService");
    SERVICE_STATUS_HANDLE = win32::RegisterServiceCtrlHandlerW(svc_name.as_ptr(), service_ctrl_handler);
    if SERVICE_STATUS_HANDLE == 0 {
        return;
    }

    report_status(win32::SERVICE_START_PENDING, 0, 3000);
    report_status(
        win32::SERVICE_RUNNING,
        win32::SERVICE_ACCEPT_STOP | win32::SERVICE_ACCEPT_SHUTDOWN,
        0,
    );

    let mut current_agent: Option<(u32, win32::Handle)> = None;

    while !SERVICE_STOP_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
        let active_session = win32::WTSGetActiveConsoleSessionId();
        if active_session != 0xFFFFFFFF {
            let need_spawn = match current_agent {
                None => true,
                Some((sid, h_proc)) => {
                    if sid != active_session {
                        let _ = win32::TerminateProcess(h_proc, 0);
                        let _ = win32::CloseHandle(h_proc);
                        current_agent = None;
                        true
                    } else {
                        let mut code: win32::Dword = 0;
                        let _ = win32::GetExitCodeProcess(h_proc, &mut code);
                        if code != 259 {
                            // STILL_ACTIVE == 259
                            let _ = win32::CloseHandle(h_proc);
                            current_agent = None;
                            true
                        } else {
                            false
                        }
                    }
                }
            };

            if need_spawn {
                if let Some(h_proc) = spawn_agent_in_session(active_session) {
                    current_agent = Some((active_session, h_proc));
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(1500));
    }

    if let Some((_, h_proc)) = current_agent {
        let _ = win32::TerminateProcess(h_proc, 0);
        let _ = win32::CloseHandle(h_proc);
    }

    report_status(win32::SERVICE_STOPPED, 0, 0);
}

/// Entrypoint when launched with `--service` by Windows SCM
pub fn run_windows_service() {
    #[cfg(target_os = "windows")]
    unsafe {
        let svc_name = to_wide("RaydropService");
        let table = [
            win32::SERVICE_TABLE_ENTRYW {
                lp_service_name: svc_name.as_ptr(),
                lp_service_proc: Some(service_main),
            },
            win32::SERVICE_TABLE_ENTRYW {
                lp_service_name: std::ptr::null(),
                lp_service_proc: None,
            },
        ];
        let _ = win32::StartServiceCtrlDispatcherW(table.as_ptr());
    }
}

/// Copies the user's current Raydrop database (`storage.db`) to `C:\ProgramData\Raydrop\Data`
/// so the SYSTEM service shares the exact same Device ID, Cloud Account, Contacts, and Remote PIN.
pub fn sync_config_to_system_service_dir() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let user_dir = crate::db::storage::resolve_app_data_dir()?;
        let user_db = user_dir.join("storage.db");
        let svc_data_dir = std::path::PathBuf::from(r"C:\ProgramData\Raydrop\Data");
        let svc_wv_dir = std::path::PathBuf::from(r"C:\ProgramData\Raydrop\WebView2");
        std::fs::create_dir_all(&svc_data_dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&svc_wv_dir).map_err(|e| e.to_string())?;

        if user_db.exists() {
            let dst_db = svc_data_dir.join("storage.db");
            std::fs::copy(&user_db, &dst_db)
                .map_err(|e| format!("Failed to sync storage.db to service dir: {e}"))?;
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}

pub fn check_service_status() -> String {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        if let Ok(out) = std::process::Command::new("sc")
            .args(["query", "RaydropService"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if text.contains("RUNNING") {
                return "running".to_string();
            } else if out.status.success() {
                return "stopped".to_string();
            }
        }
        "not_installed".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        "unsupported".to_string()
    }
}

pub fn install_service_elevated() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        sync_config_to_system_service_dir()?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe.to_string_lossy().replace('\'', "''");
        let ps_inner = format!(
            "sc.exe stop RaydropService | Out-Null; sc.exe delete RaydropService | Out-Null; Start-Sleep -Milliseconds 500; sc.exe create RaydropService binPath= '\\\"{}\\\" --service' start= auto DisplayName= 'Raydrop Unattended Remote Service'; sc.exe description RaydropService 'Enables pre-login Windows Lock/PIN screen remote desktop access for Raydrop.'; sc.exe start RaydropService",
            exe_str
        );
        let status = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &format!(
                    "Start-Process powershell -ArgumentList '-NoProfile -WindowStyle Hidden -Command \"{}\"' -Verb RunAs -Wait",
                    ps_inner.replace('"', "\\\"")
                ),
            ])
            .status()
            .map_err(|e| format!("Failed to elevate service installer: {e}"))?;

        if status.success() && check_service_status() != "not_installed" {
            Ok("Raydrop Windows Service installed and started! Pre-login PIN screen remote access is now active.".to_string())
        } else {
            Err("Service installation was cancelled or did not complete.".to_string())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Windows Service is only supported on Windows.".to_string())
    }
}

pub fn uninstall_service_elevated() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let ps_inner = "sc.exe stop RaydropService | Out-Null; sc.exe delete RaydropService | Out-Null";
        let status = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &format!(
                    "Start-Process powershell -ArgumentList '-NoProfile -WindowStyle Hidden -Command \"{}\"' -Verb RunAs -Wait",
                    ps_inner
                ),
            ])
            .status()
            .map_err(|e| format!("Failed to uninstall service: {e}"))?;
        if status.success() {
            Ok("Raydrop Windows Service stopped and uninstalled.".to_string())
        } else {
            Err("Service uninstallation was cancelled.".to_string())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Windows Service is only supported on Windows.".to_string())
    }
}
