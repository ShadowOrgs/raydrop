use crate::db::storage::{now_unix_secs, AppSettings, ChatGroup, ChatMessage, Contact, GroupMember, TransferRecord};
use crate::network::transport::{initiate_peer_transfer, resume_peer_transfer};
use crate::os::{register_context_menu, set_autostart};
use crate::state::{AppState, IncomingRequestDecision};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{atomic::Ordering, Arc};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize)]
pub struct AppStateSnapshot {
    pub settings: AppSettings,
    pub ed25519_pubkey: String,
    pub os_type: String,
    pub local_ip: String,
    pub listen_port: u16,
    pub wan_reflexive_addr: Option<String>,
    pub signaling_connected: bool,
    pub queued_cli_files: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CloudDeviceItem {
    pub device_id: String,
    pub user_email: String,
    pub user_name: String,
    pub device_name: String,
    pub os_type: String,
    pub ed25519_pubkey: String,
    pub last_ip: Option<String>,
    pub is_online: bool,
}

#[derive(Debug, Deserialize)]
pub struct CloudContactItem {
    pub email: String,
    pub name: String,
    pub trust_level: String,
    pub devices: Vec<CloudDeviceItem>,
}

#[tauri::command]
pub fn get_app_state(state: State<'_, Arc<AppState>>) -> Result<AppStateSnapshot, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let wan_reflexive_addr = state
        .wan_reflexive_addr
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let mut queued = state
        .queued_cli_files
        .lock()
        .map_err(|e| e.to_string())?;
    let queued_cli_files = std::mem::take(&mut *queued);

    Ok(AppStateSnapshot {
        settings,
        ed25519_pubkey: state.identity.public_hex(),
        os_type: std::env::consts::OS.to_string(),
        local_ip: state.local_ip.clone(),
        listen_port: state.listen_port,
        wan_reflexive_addr,
        signaling_connected: state.is_signaling_connected(),
        queued_cli_files,
    })
}

#[tauri::command]
pub fn save_cloud_session(
    state: State<'_, Arc<AppState>>,
    user_id: String,
    user_email: String,
    user_name: String,
    auth_token: String,
    device_name: Option<String>,
) -> Result<AppSettings, String> {
    state.db.set_setting("user_id", &user_id)?;
    state.db.set_setting("user_email", &user_email)?;
    state.db.set_setting("user_name", &user_name)?;
    state.db.set_setting("auth_token", &auth_token)?;

    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.user_id = Some(user_id);
    s.user_email = Some(user_email);
    s.user_name = Some(user_name);
    s.auth_token = Some(auth_token.clone());

    if let Some(dn) = device_name {
        let trimmed = dn.trim();
        if !trimmed.is_empty() {
            state.db.set_setting("alias_name", trimmed)?;
            s.alias_name = trimmed.to_string();
        }
    }

    // Re-send authenticated REGISTER packet over active signaling WebSocket
    if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let ts = now_unix_secs();
        let sign_payload = format!("RAYDROP_REGISTER:{}:{ts}", state.identity.device_id);
        let sig = state.identity.sign_message(sign_payload.as_bytes());
        let reg_msg = json!({
            "type": "REGISTER",
            "device_id": state.identity.device_id,
            "alias_name": s.alias_name,
            "ed25519_pubkey": state.identity.public_hex(),
            "timestamp": ts,
            "signature": sig,
            "auth_token": auth_token
        })
        .to_string();
        let _ = tx.send(reg_msg);
    }

    Ok(s.clone())
}

#[tauri::command]
pub fn logout_cloud_session(state: State<'_, Arc<AppState>>) -> Result<AppSettings, String> {
    state.db.set_setting("user_id", "")?;
    state.db.set_setting("user_email", "")?;
    state.db.set_setting("user_name", "")?;
    state.db.set_setting("auth_token", "")?;

    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.user_id = None;
    s.user_email = None;
    s.user_name = None;
    s.auth_token = None;
    Ok(s.clone())
}

#[tauri::command]
pub fn sync_cloud_data_to_local(
    state: State<'_, Arc<AppState>>,
    my_devices: Vec<CloudDeviceItem>,
    contacts: Vec<CloudContactItem>,
) -> Result<Vec<Contact>, String> {
    let now = now_unix_secs();

    // 1. Upsert all other devices belonging to the user's own account (My Devices / Tailnet)
    for dev in my_devices {
        if dev.device_id == state.identity.device_id {
            continue;
        }
        let existing = state.db.get_contact(&dev.device_id).ok().flatten();
        let net_type = existing
            .as_ref()
            .map(|c| c.network_type.clone())
            .unwrap_or_else(|| "wan".to_string());
        let last_port = existing.as_ref().and_then(|c| c.last_known_port);
        let last_ip = existing
            .as_ref()
            .and_then(|c| c.last_known_ip.clone())
            .or(dev.last_ip);

        let c = Contact {
            device_id: dev.device_id,
            alias_name: dev.device_name,
            custom_name: existing.as_ref().and_then(|c| c.custom_name.clone()),
            user_email: Some(dev.user_email),
            user_name: Some(dev.user_name),
            os_type: Some(dev.os_type),
            is_own_device: true,
            ed25519_pubkey: Some(dev.ed25519_pubkey),
            network_type: net_type,
            last_known_ip: last_ip,
            last_known_port: last_port,
            trust_level: "trusted_auto_accept".to_string(),
            is_online: dev.is_online,
            last_seen_at: now,
            created_at: now,
            avatar_hash: existing.as_ref().and_then(|c| c.avatar_hash.clone()),
        };
        let _ = state.db.upsert_contact(&c);
    }

    // 2. Upsert all devices belonging to saved Cloud Contacts
    for contact_group in contacts {
        for dev in contact_group.devices {
            if dev.device_id == state.identity.device_id {
                continue;
            }
            let existing = state.db.get_contact(&dev.device_id).ok().flatten();
            let net_type = existing
                .as_ref()
                .map(|c| c.network_type.clone())
                .unwrap_or_else(|| "wan".to_string());
            let last_port = existing.as_ref().and_then(|c| c.last_known_port);
            let last_ip = existing
                .as_ref()
                .and_then(|c| c.last_known_ip.clone())
                .or(dev.last_ip);

            let c = Contact {
                device_id: dev.device_id,
                alias_name: dev.device_name,
                custom_name: existing.as_ref().and_then(|c| c.custom_name.clone()),
                user_email: Some(contact_group.email.clone()),
                user_name: Some(contact_group.name.clone()),
                os_type: Some(dev.os_type),
                is_own_device: false,
                ed25519_pubkey: Some(dev.ed25519_pubkey),
                network_type: net_type,
                last_known_ip: last_ip,
                last_known_port: last_port,
                trust_level: contact_group.trust_level.clone(),
                is_online: dev.is_online,
                last_seen_at: now,
                created_at: now,
                avatar_hash: existing.as_ref().and_then(|c| c.avatar_hash.clone()),
            };
            let _ = state.db.upsert_contact(&c);
        }
    }

    state.db.list_contacts()
}

#[tauri::command]
pub fn delete_local_contact(
    state: State<'_, Arc<AppState>>,
    device_id: Option<String>,
    email: Option<String>,
) -> Result<(), String> {
    if let Some(did) = device_id {
        state.db.delete_contact_by_device_id(&did)?;
    }
    if let Some(em) = email {
        state.db.delete_contacts_by_email(&em)?;
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_discoverability(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<bool, String> {
    state
        .db
        .set_setting("discoverable", if enabled { "true" } else { "false" })?;
    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.discoverable = enabled;
    Ok(enabled)
}

#[tauri::command]
pub fn toggle_autostart(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<bool, String> {
    let _ = set_autostart(enabled)?;
    state
        .db
        .set_setting("autostart", if enabled { "true" } else { "false" })?;
    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.autostart = enabled;
    Ok(enabled)
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, Arc<AppState>>,
    alias_name: String,
    download_dir: String,
    signaling_url: String,
    stun_addr: String,
    autostart: Option<bool>,
    speed_limit_mbps: Option<u32>,
    auto_categorize: Option<bool>,
    max_concurrent_downloads: Option<u32>,
) -> Result<AppSettings, String> {
    let resolved_stun = if stun_addr.trim().is_empty() {
        let host = signaling_url
            .trim()
            .trim_start_matches("wss://")
            .trim_start_matches("ws://")
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("stun.raydrop.web.id")
            .split(':')
            .next()
            .unwrap_or("stun.raydrop.web.id");
        format!("{host}:3478")
    } else {
        stun_addr.trim().to_string()
    };

    state.db.set_setting("alias_name", &alias_name)?;
    state.db.set_setting("download_dir", &download_dir)?;
    state.db.set_setting("signaling_url", signaling_url.trim())?;
    state.db.set_setting("stun_addr", &resolved_stun)?;

    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.alias_name = alias_name;
    s.download_dir = download_dir.clone();
    s.signaling_url = signaling_url.trim().to_string();
    s.stun_addr = resolved_stun;

    state.http_downloader.set_save_dir(std::path::PathBuf::from(download_dir));

    if let Some(limit) = speed_limit_mbps {
        state.db.set_setting("speed_limit_mbps", &limit.to_string())?;
        s.speed_limit_mbps = limit;
    }

    if let Some(cat) = auto_categorize {
        state.db.set_setting("auto_categorize", if cat { "true" } else { "false" })?;
        s.auto_categorize = cat;
        state.http_downloader.set_auto_categorize(cat);
    }

    if let Some(conc) = max_concurrent_downloads {
        state.db.set_setting("max_concurrent_downloads", &conc.to_string())?;
        s.max_concurrent_downloads = conc;
        state.http_downloader.set_max_concurrent_downloads(conc);
    }

    if let Some(auto) = autostart {
        let _ = set_autostart(auto);
        state
            .db
            .set_setting("autostart", if auto { "true" } else { "false" })?;
        s.autostart = auto;
    }

    Ok(s.clone())
}

#[tauri::command]
pub fn set_speed_limit(state: State<'_, Arc<AppState>>, speed_limit_mbps: u32) -> Result<u32, String> {
    state.db.set_setting("speed_limit_mbps", &speed_limit_mbps.to_string())?;
    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.speed_limit_mbps = speed_limit_mbps;
    Ok(speed_limit_mbps)
}

#[tauri::command]
pub fn get_contacts(state: State<'_, Arc<AppState>>) -> Result<Vec<Contact>, String> {
    state.db.list_contacts()
}

#[tauri::command]
pub fn add_wan_contact(
    state: State<'_, Arc<AppState>>,
    device_id: String,
    alias_name: String,
    direct_ip_port: Option<String>,
) -> Result<Contact, String> {
    let clean_id = device_id.trim().to_uppercase();
    if !clean_id.starts_with("RAY-") {
        return Err("Device ID must start with RAY-XXXX-XXXX-XXXX".to_string());
    }

    let (last_known_ip, last_known_port) = if let Some(addr_str) = direct_ip_port {
        let trimmed = addr_str.trim();
        if let Some((ip, port_s)) = trimmed.rsplit_once(':') {
            (Some(ip.to_string()), port_s.parse::<u16>().ok())
        } else if !trimmed.is_empty() {
            (Some(trimmed.to_string()), None)
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let contact = Contact {
        device_id: clean_id.clone(),
        alias_name: alias_name.trim().to_string(),
        custom_name: None,
        user_email: None,
        user_name: None,
        os_type: None,
        is_own_device: false,
        ed25519_pubkey: None,
        network_type: "wan".to_string(),
        last_known_ip,
        last_known_port,
        trust_level: "ask".to_string(),
        is_online: false,
        last_seen_at: now_unix_secs(),
        created_at: now_unix_secs(),
        avatar_hash: None,
    };
    state.db.upsert_contact(&contact)?;

    if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let lookup = json!({
            "type": "PEER_LOOKUP",
            "target_device_id": clean_id
        })
        .to_string();
        let _ = tx.send(lookup);
    }

    Ok(contact)
}

#[tauri::command]
pub fn update_contact_trust(
    state: State<'_, Arc<AppState>>,
    device_id: String,
    trust_level: String,
    user_email: Option<String>,
) -> Result<(), String> {
    if let Some(email) = user_email {
        if !email.trim().is_empty() {
            state.db.update_contact_trust_by_email(&email, &trust_level)?;
            return Ok(());
        }
    }
    state.db.update_contact_trust(&device_id, &trust_level)
}

#[tauri::command]
pub async fn initiate_transfer(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    target_device_id: String,
    file_paths: Vec<String>,
) -> Result<String, String> {
    initiate_peer_transfer(app, state.inner().clone(), target_device_id, file_paths).await
}

#[tauri::command]
pub async fn resume_transfer(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<String, String> {
    resume_peer_transfer(app, state.inner().clone(), session_id).await
}

#[tauri::command]
pub fn pause_transfer(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<(), String> {
    if let Some(flag) = state
        .active_cancellations
        .lock()
        .map_err(|e| e.to_string())?
        .get(&session_id)
    {
        flag.store(true, Ordering::Relaxed);
    }
    if let Ok(Some(mut rec)) = state.db.get_transfer(&session_id) {
        rec.status = "paused".to_string();
        let _ = state.db.upsert_transfer(&rec);
        let _ = app.emit("transfer_state_changed", &rec);
    }
    Ok(())
}

#[tauri::command]
pub fn respond_transfer_request(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    accepted: bool,
    target_directory: Option<String>,
) -> Result<(), String> {
    if let Some(sender) = state
        .pending_requests
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&session_id)
    {
        let _ = sender.send(IncomingRequestDecision {
            accepted,
            target_directory,
        });
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_transfer(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<(), String> {
    if let Some(flag) = state
        .active_cancellations
        .lock()
        .map_err(|e| e.to_string())?
        .get(&session_id)
    {
        flag.store(true, Ordering::Relaxed);
    }
    if let Ok(Some(mut rec)) = state.db.get_transfer(&session_id) {
        rec.status = "cancelled".to_string();
        let _ = state.db.upsert_transfer(&rec);
        let _ = app.emit("transfer_state_changed", &rec);
    }
    Ok(())
}

#[tauri::command]
pub fn get_transfer_history(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<TransferRecord>, String> {
    state.db.list_transfers()
}

#[tauri::command]
pub fn register_os_context_menu() -> Result<String, String> {
    register_context_menu()
}

#[tauri::command]
pub fn get_win11_direct_menu() -> bool {
    crate::os::get_win11_direct_menu_status()
}

#[tauri::command]
pub fn toggle_win11_direct_menu(enabled: bool) -> Result<bool, String> {
    crate::os::set_win11_direct_menu(enabled)
}

#[cfg(target_os = "windows")]
mod win_shell {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::ptr;

    #[link(name = "shell32")]
    extern "system" {
        fn ILCreateFromPathW(psz_path: *const u16) -> *mut std::ffi::c_void;
        fn ILFree(pidl: *mut std::ffi::c_void);
        fn SHOpenFolderAndSelectItems(
            pidl_folder: *const std::ffi::c_void,
            cidl: u32,
            apidl: *const *const std::ffi::c_void,
            dw_flags: u32,
        ) -> i32;
        fn ShellExecuteW(
            hwnd: *mut std::ffi::c_void,
            lpOperation: *const u16,
            lpFile: *const u16,
            lpParameters: *const u16,
            lpDirectory: *const u16,
            nShowCmd: i32,
        ) -> usize;
    }

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(pv_reserved: *mut std::ffi::c_void, dw_co_init: u32) -> i32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn AllowSetForegroundWindow(dw_process_id: u32) -> i32;
    }

    pub fn allow_foreground() {
        unsafe {
            let _ = AllowSetForegroundWindow(0xFFFF_FFFF);
        }
    }

    pub fn open_file_with_default_app(path: &Path) -> Result<(), String> {
        let clean_str = path
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', r"\");
        let wide: Vec<u16> = OsStr::new(&clean_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let open_op: Vec<u16> = OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let _ = AllowSetForegroundWindow(0xFFFF_FFFF);
            let res = ShellExecuteW(
                ptr::null_mut(),
                open_op.as_ptr(),
                wide.as_ptr(),
                ptr::null(),
                ptr::null(),
                1, // SW_SHOWNORMAL
            );
            if res > 32 {
                return Ok(());
            }
        }

        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &clean_str])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open file: {e}"))?;
        Ok(())
    }

    pub fn reveal_and_select_file(path: &Path) -> Result<(), String> {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let clean_str = canonical
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', r"\");
        let wide: Vec<u16> = OsStr::new(&clean_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let _ = AllowSetForegroundWindow(0xFFFF_FFFF);
            let _ = CoInitializeEx(ptr::null_mut(), 0x2); // COINIT_APARTMENTTHREADED
            let pidl = ILCreateFromPathW(wide.as_ptr());
            if !pidl.is_null() {
                let hr = SHOpenFolderAndSelectItems(pidl, 0, ptr::null(), 0);
                ILFree(pidl);
                if hr >= 0 {
                    return Ok(());
                }
            }
        }

        // Fallback to explorer.exe /select,"<path>"
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", clean_str))
            .spawn()
            .map_err(|e| format!("Failed to open folder: {e}"))?;
        Ok(())
    }
}

#[tauri::command]
pub fn open_file_path(app: AppHandle, path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("File or folder not found: {path}"));
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_always_on_top(false);
    }

    #[cfg(target_os = "windows")]
    {
        win_shell::open_file_with_default_app(&p)?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&p)
            .spawn()
            .map_err(|e| format!("Failed to open file: {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&p)
            .spawn()
            .map_err(|e| format!("Failed to open file: {e}"))?;
    }

    #[cfg(target_os = "android")]
    {
        use tauri::Emitter;
        let _ = app.emit("open_file_requested", &path);
    }

    Ok(())
}

#[tauri::command]
pub fn open_folder_path(app: AppHandle, path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_always_on_top(false);
    }

    // If the file is still downloading/paused as `.raypart`, select the `.raypart` file in Explorer!
    let target_item = if p.exists() {
        Some(p.clone())
    } else {
        let part_candidate = p.with_file_name(format!(
            "{}.raypart",
            p.file_name().and_then(|n| n.to_str()).unwrap_or("file")
        ));
        if part_candidate.exists() {
            Some(part_candidate)
        } else {
            None
        }
    };

    #[cfg(target_os = "windows")]
    {
        win_shell::allow_foreground();
        if let Some(ref item) = target_item {
            if item.is_file() {
                let item_for_ui = item.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                let _ = app.run_on_main_thread(move || {
                    let res = win_shell::reveal_and_select_file(&item_for_ui);
                    let _ = tx.send(res);
                });
                if let Ok(res) = rx.recv_timeout(std::time::Duration::from_secs(3)) {
                    return res;
                }
                return win_shell::reveal_and_select_file(item);
            }
        }

        let dir_to_open = if let Some(ref item) = target_item {
            if item.is_dir() {
                item.clone()
            } else if let Some(parent) = item.parent() {
                parent.to_path_buf()
            } else {
                return Err(format!("Folder not found: {path}"));
            }
        } else if let Some(parent) = p.parent() {
            if parent.exists() {
                parent.to_path_buf()
            } else {
                return Err(format!("Folder not found: {path}"));
            }
        } else {
            return Err(format!("Folder not found: {path}"));
        };

        let canonical = std::fs::canonicalize(&dir_to_open).unwrap_or(dir_to_open);
        let win_dir = canonical
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', "\\");
        std::process::Command::new("explorer.exe")
            .arg(&win_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {e}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(ref item) = target_item {
            if item.is_file() {
                std::process::Command::new("open")
                    .arg("-R")
                    .arg(item)
                    .spawn()
                    .map_err(|e| format!("Failed to open folder: {e}"))?;
                return Ok(());
            }
        }
        let dir_to_open = if p.exists() && p.is_dir() {
            p
        } else if let Some(parent) = p.parent() {
            parent.to_path_buf()
        } else {
            return Err(format!("Folder not found: {path}"));
        };
        std::process::Command::new("open")
            .arg(&dir_to_open)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        let dir_to_open = if p.exists() && p.is_dir() {
            p
        } else if let Some(parent) = p.parent() {
            parent.to_path_buf()
        } else {
            return Err(format!("Folder not found: {path}"));
        };
        std::process::Command::new("xdg-open")
            .arg(&dir_to_open)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {e}"))?;
    }

    #[cfg(target_os = "android")]
    {
        use tauri::Emitter;
        let folder_path = if p.is_dir() {
            path.clone()
        } else if let Some(parent) = p.parent() {
            parent.to_string_lossy().to_string()
        } else {
            path.clone()
        };
        let _ = app.emit("open_folder_requested", &folder_path);
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_title: String,
    pub release_notes: String,
    pub release_date: String,
    pub html_url: String,
    pub download_url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: Option<u64>,
    pub is_building: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgressPayload {
    pub downloaded: u64,
    pub total: u64,
    pub percent: f64,
    pub speed_bytes_sec: u64,
}

fn parse_semver(v: &str) -> Vec<u64> {
    let clean = v.trim_start_matches('v').trim_start_matches('V');
    clean
        .split(|c: char| c == '.' || c == '-' || c == '+')
        .filter_map(|s| s.parse::<u64>().ok())
        .collect()
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    let l_parts = parse_semver(latest);
    let c_parts = parse_semver(current);
    let max_len = l_parts.len().max(c_parts.len());
    for i in 0..max_len {
        let l = l_parts.get(i).copied().unwrap_or(0);
        let c = c_parts.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        } else if l < c {
            return false;
        }
    }
    false
}

#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<UpdateCheckResult, String> {
    let current_version = app.package_info().version.to_string();

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    // Strategy 1: Attempt standard GitHub API call
    let api_res = client
        .get("https://api.github.com/repos/asepindrak/raydrop/releases/latest")
        .send()
        .await;

    let mut parsed_release: Option<(String, String, String, String, String, Option<String>, Option<String>, Option<u64>)> = None;

    if let Ok(res) = api_res {
        if res.status().is_success() {
            if let Ok(release_val) = res.json::<serde_json::Value>().await {
                let tag_name = release_val["tag_name"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let release_title = release_val["name"]
                    .as_str()
                    .unwrap_or(&tag_name)
                    .to_string();
                let release_notes = release_val["body"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let release_date = release_val["published_at"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let html_url = release_val["html_url"]
                    .as_str()
                    .unwrap_or("https://github.com/asepindrak/raydrop/releases")
                    .to_string();

                let mut download_url = None;
                let mut asset_name = None;
                let mut asset_size = None;

                if let Some(assets) = release_val["assets"].as_array() {
                    #[cfg(target_os = "android")]
                    let target_asset = assets.iter().find(|a| {
                        let name = a["name"].as_str().unwrap_or("").to_lowercase();
                        name.ends_with(".apk")
                    });

                    #[cfg(target_os = "windows")]
                    let target_asset = assets.iter().find(|a| {
                        let name = a["name"].as_str().unwrap_or("").to_lowercase();
                        name.ends_with(".msi")
                    }).or_else(|| {
                        assets.iter().find(|a| {
                            let name = a["name"].as_str().unwrap_or("").to_lowercase();
                            name.ends_with(".exe")
                        })
                    });

                    #[cfg(target_os = "macos")]
                    let target_asset = assets.iter().find(|a| {
                        let name = a["name"].as_str().unwrap_or("").to_lowercase();
                        name.ends_with(".dmg")
                    });

                    #[cfg(target_os = "linux")]
                    let target_asset = assets.iter().find(|a| {
                        let name = a["name"].as_str().unwrap_or("").to_lowercase();
                        name.ends_with(".appimage")
                    }).or_else(|| {
                        assets.iter().find(|a| {
                            let name = a["name"].as_str().unwrap_or("").to_lowercase();
                            name.ends_with(".deb")
                        })
                    });

                    #[cfg(not(any(target_os = "android", target_os = "windows", target_os = "macos", target_os = "linux")))]
                    let target_asset = assets.iter().find(|a| {
                        let name = a["name"].as_str().unwrap_or("").to_lowercase();
                        name.ends_with(".msi") || name.ends_with(".apk") || name.ends_with(".dmg") || name.ends_with(".appimage")
                    });

                    if let Some(a) = target_asset {
                        download_url = a["browser_download_url"].as_str().map(String::from);
                        asset_name = a["name"].as_str().map(String::from);
                        asset_size = a["size"].as_u64();
                    }
                }

                parsed_release = Some((tag_name, release_title, release_notes, release_date, html_url, download_url, asset_name, asset_size));
            }
        }
    }

    // Strategy 2: If API was rate-limited (HTTP 403) or unreachable, fallback to direct GitHub web redirect
    if parsed_release.is_none() {
        let web_res = client
            .get("https://github.com/asepindrak/raydrop/releases/latest")
            .send()
            .await
            .map_err(|e| format!("Unable to reach GitHub update server: {e}"))?;

        let final_url = web_res.url().to_string();
        // Expected redirect format: https://github.com/asepindrak/raydrop/releases/tag/vX.X.X
        let tag_name = final_url
            .split("/tag/")
            .nth(1)
            .map(|s| s.trim_matches('/').to_string())
            .unwrap_or_default();

        if tag_name.is_empty() {
            return Err("Unable to determine latest release tag from GitHub".to_string());
        }

        let clean_latest = tag_name.trim_start_matches('v').trim_start_matches('V').to_string();
        let release_title = format!("Raydrop {tag_name}");
        let html_url = format!("https://github.com/asepindrak/raydrop/releases/tag/{tag_name}");
        let release_notes = "Latest Raydrop Release. Visit GitHub Releases for full changelog.".to_string();
        let release_date = "".to_string();

        let mut download_url = None;
        let mut asset_name = None;
        let mut asset_size = None;

        #[cfg(target_os = "windows")]
        let candidate_filename = format!("Raydrop_{clean_latest}_x64_en-US.msi");
        #[cfg(target_os = "android")]
        let candidate_filename = "Raydrop-arm64-v8a.apk".to_string();
        #[cfg(target_os = "macos")]
        let candidate_filename = format!("Raydrop_{clean_latest}_universal.dmg");
        #[cfg(target_os = "linux")]
        let candidate_filename = format!("Raydrop_{clean_latest}_amd64.AppImage");
        #[cfg(not(any(target_os = "android", target_os = "windows", target_os = "macos", target_os = "linux")))]
        let candidate_filename = format!("Raydrop_{clean_latest}_x64_en-US.msi");

        let candidate_url = format!(
            "https://github.com/asepindrak/raydrop/releases/download/{tag_name}/{candidate_filename}"
        );

        // Check if candidate asset is available on GitHub release assets CDN
        if let Ok(head_res) = client.head(&candidate_url).send().await {
            if head_res.status().is_success() || head_res.status().is_redirection() {
                asset_size = head_res.content_length();
                download_url = Some(candidate_url);
                asset_name = Some(candidate_filename);
            }
        }

        parsed_release = Some((tag_name, release_title, release_notes, release_date, html_url, download_url, asset_name, asset_size));
    }

    let (tag_name, release_title, release_notes, release_date, html_url, download_url, asset_name, asset_size) =
        parsed_release.ok_or_else(|| "Failed to resolve release details".to_string())?;

    let clean_latest = tag_name.trim_start_matches('v').trim_start_matches('V').to_string();
    let is_newer = is_newer_version(&clean_latest, &current_version);

    let has_update = is_newer && download_url.is_some();
    let is_building = is_newer && download_url.is_none();

    Ok(UpdateCheckResult {
        has_update,
        current_version,
        latest_version: tag_name,
        release_title,
        release_notes,
        release_date,
        html_url,
        download_url,
        asset_name,
        asset_size,
        is_building,
    })
}

#[tauri::command]
pub async fn download_and_install_update(
    app: AppHandle,
    download_url: String,
    asset_name: String,
) -> Result<String, String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .user_agent("Raydrop-Desktop-App")
        .build()
        .map_err(|e| format!("Failed to create download client: {e}"))?;

    let res = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to initiate download: {e}"))?;

    if !res.status().is_success() {
        return Err(format!("Download failed with HTTP {}", res.status()));
    }

    let total_bytes = res.content_length().unwrap_or(0);

    #[cfg(target_os = "android")]
    let temp_file_path = {
        let dl_dir = std::path::PathBuf::from("/storage/emulated/0/Download");
        if dl_dir.exists() {
            dl_dir.join(&asset_name)
        } else {
            crate::db::storage::resolve_app_data_dir()?.join(&asset_name)
        }
    };

    #[cfg(not(target_os = "android"))]
    let temp_file_path = std::env::temp_dir().join(&asset_name);

    let mut file = tokio::fs::File::create(&temp_file_path)
        .await
        .map_err(|e| format!("Failed to create temporary file: {e}"))?;

    let mut stream = res.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    let mut last_bytes: u64 = 0;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| format!("Error downloading chunk: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write chunk to disk: {e}"))?;
        downloaded += chunk.len() as u64;

        if last_emit.elapsed() >= std::time::Duration::from_millis(150) || downloaded == total_bytes {
            let elapsed_sec = last_emit.elapsed().as_secs_f64();
            let speed = if elapsed_sec > 0.0 {
                ((downloaded.saturating_sub(last_bytes)) as f64 / elapsed_sec) as u64
            } else {
                0
            };
            last_bytes = downloaded;
            last_emit = std::time::Instant::now();

            let percent = if total_bytes > 0 {
                (downloaded as f64 / total_bytes as f64) * 100.0
            } else {
                0.0
            };

            let _ = app.emit(
                "app_update_progress",
                UpdateProgressPayload {
                    downloaded,
                    total: total_bytes,
                    percent,
                    speed_bytes_sec: speed,
                },
            );
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush downloaded file: {e}"))?;
    drop(file);

    #[cfg(target_os = "windows")]
    {
        let is_msi = asset_name.to_lowercase().ends_with(".msi");
        let path_str = temp_file_path.to_string_lossy().to_string();

        if is_msi {
            std::process::Command::new("msiexec.exe")
                .arg("/i")
                .arg(&path_str)
                .spawn()
                .map_err(|e| format!("Failed to launch MSI installer: {e}"))?;

            let app_exit = app.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
                app_exit.exit(0);
            });

            return Ok("Installer launched. Upgrading Raydrop...".to_string());
        } else {
            std::process::Command::new("explorer.exe")
                .arg(format!("/select,{}", path_str))
                .spawn()
                .ok();
            return Ok(format!("Updated executable saved to {}", path_str));
        }
    }

    #[cfg(target_os = "android")]
    {
        use tauri::Emitter;
        let path_str = temp_file_path.to_string_lossy().to_string();
        let _ = app.emit("open_file_requested", &path_str);
        return Ok(format!("File APK pembaruan tersimpan di Download: {asset_name}. Silakan buka pengelola file untuk menginstal."));
    }

    #[cfg(target_os = "macos")]
    {
        let path_str = temp_file_path.to_string_lossy().to_string();
        std::process::Command::new("open")
            .arg(&path_str)
            .spawn()
            .map_err(|e| format!("Failed to open DMG installer: {e}"))?;
        return Ok(format!("DMG installer opened from {}", path_str));
    }

    #[cfg(target_os = "linux")]
    {
        let path_str = temp_file_path.to_string_lossy().to_string();
        if path_str.to_lowercase().ends_with(".appimage") {
            let _ = std::process::Command::new("chmod")
                .arg("+x")
                .arg(&path_str)
                .status();
        }
        std::process::Command::new("xdg-open")
            .arg(&path_str)
            .spawn()
            .map_err(|e| format!("Failed to open package: {e}"))?;
        return Ok(format!("Package saved to {}", path_str));
    }

    #[cfg(not(any(target_os = "windows", target_os = "android", target_os = "macos", target_os = "linux")))]
    {
        return Ok(format!("Downloaded update to {}", temp_file_path.to_string_lossy()));
    }
}

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

pub fn generate_image_thumbnail(file_path: &str, max_width: u32, max_height: u32) -> Result<String, String> {
    let path = std::path::Path::new(file_path);
    if !path.exists() {
        return Err("File not found".to_string());
    }

    let img = image::open(path).map_err(|e| format!("Failed to open image: {e}"))?;
    let thumb = img.thumbnail(max_width, max_height);

    let mut buf = std::io::Cursor::new(Vec::new());
    thumb
        .write_to(&mut buf, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode JPEG: {e}"))?;

    let encoded = BASE64_STANDARD.encode(buf.into_inner());
    Ok(format!("data:image/jpeg;base64,{encoded}"))
}

#[tauri::command]
pub fn get_file_thumbnail(path: String) -> Result<String, String> {
    generate_image_thumbnail(&path, 320, 240)
}

pub fn get_avatars_dir() -> std::path::PathBuf {
    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "raydrop", "Raydrop") {
        let dir = proj_dirs.data_dir().join("avatars");
        let _ = std::fs::create_dir_all(&dir);
        dir
    } else {
        let dir = std::path::PathBuf::from("avatars");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }
}

pub fn get_my_avatar_internal() -> Option<String> {
    let file = get_avatars_dir().join("my_avatar.png");
    if file.exists() {
        if let Ok(bytes) = std::fs::read(&file) {
            let b64 = BASE64_STANDARD.encode(&bytes);
            return Some(format!("data:image/png;base64,{b64}"));
        }
    }
    None
}

pub fn save_peer_avatar_internal(
    app: &AppHandle,
    state: &Arc<AppState>,
    peer_device_id: &str,
    base64_data: &str,
    avatar_hash: Option<&str>,
) -> Result<(), String> {
    let clean_data = if let Some(idx) = base64_data.find(',') {
        &base64_data[idx + 1..]
    } else {
        base64_data
    };

    let bytes = BASE64_STANDARD
        .decode(clean_data.trim())
        .map_err(|e| format!("Base64 decode error: {e}"))?;

    let safe_id: String = peer_device_id
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect();

    let target_path = get_avatars_dir().join(format!("{safe_id}.png"));
    std::fs::write(&target_path, &bytes).map_err(|e| format!("Failed to save avatar: {e}"))?;

    if let Some(h) = avatar_hash {
        let _ = state.db.set_contact_avatar_hash(peer_device_id, h);
    }

    let full_data_url = if base64_data.starts_with("data:image/") {
        base64_data.to_string()
    } else {
        format!("data:image/png;base64,{clean_data}")
    };

    let _ = app.emit(
        "peer_avatar_updated",
        serde_json::json!({
            "device_id": peer_device_id,
            "hash": avatar_hash,
            "avatar_data": full_data_url
        }),
    );

    Ok(())
}

pub fn auto_add_contact_from_chat(
    state: &Arc<AppState>,
    app: &AppHandle,
    peer_device_id: &str,
    peer_alias: Option<&str>,
) {
    let now = now_unix_secs();
    let existing = state.db.get_contact(peer_device_id).ok().flatten();
    let default_alias = format!(
        "Raydrop-{}",
        if peer_device_id.len() >= 6 {
            &peer_device_id[..6]
        } else {
            peer_device_id
        }
    );
    let alias = peer_alias
        .filter(|a| !a.trim().is_empty())
        .map(|a| a.to_string())
        .or_else(|| existing.as_ref().map(|c| c.alias_name.clone()))
        .unwrap_or(default_alias);

    let contact = Contact {
        device_id: peer_device_id.to_string(),
        alias_name: alias,
        custom_name: existing.as_ref().and_then(|c| c.custom_name.clone()),
        user_email: existing.as_ref().and_then(|c| c.user_email.clone()),
        user_name: existing.as_ref().and_then(|c| c.user_name.clone()),
        os_type: existing.as_ref().and_then(|c| c.os_type.clone()),
        is_own_device: existing.as_ref().map(|c| c.is_own_device).unwrap_or(false),
        ed25519_pubkey: existing.as_ref().and_then(|c| c.ed25519_pubkey.clone()),
        network_type: existing
            .as_ref()
            .map(|c| c.network_type.clone())
            .unwrap_or_else(|| "wan".to_string()),
        last_known_ip: existing.as_ref().and_then(|c| c.last_known_ip.clone()),
        last_known_port: existing.as_ref().and_then(|c| c.last_known_port),
        trust_level: existing
            .as_ref()
            .map(|c| c.trust_level.clone())
            .unwrap_or_else(|| "ask".to_string()),
        is_online: true,
        last_seen_at: now,
        created_at: existing.as_ref().map(|c| c.created_at).unwrap_or(now),
        avatar_hash: existing.as_ref().and_then(|c| c.avatar_hash.clone()),
    };
    let _ = state.db.upsert_contact(&contact);
    let _ = app.emit("contacts_updated", ());
    let _ = app.emit("peer_discovered", &contact);
}

#[tauri::command]
pub fn set_my_avatar(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    image_data: String,
) -> Result<String, String> {
    let bytes = if image_data.starts_with("data:image/") || image_data.contains(";base64,") {
        let clean = if let Some(pos) = image_data.find(',') {
            &image_data[pos + 1..]
        } else {
            &image_data
        };
        BASE64_STANDARD
            .decode(clean.trim())
            .map_err(|e| format!("Base64 decode error: {e}"))?
    } else {
        let p = std::path::Path::new(&image_data);
        if !p.exists() {
            return Err("Image file does not exist".to_string());
        }
        std::fs::read(p).map_err(|e| format!("Failed to read image file: {e}"))?
    };

    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("Failed to decode image format: {e}"))?;

    // Square crop / resize to 192x192
    let resized = img.resize_to_fill(192, 192, image::imageops::FilterType::Lanczos3);

    let mut buf = std::io::Cursor::new(Vec::new());
    resized
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("Failed to encode avatar PNG: {e}"))?;

    let png_bytes = buf.into_inner();
    let hash = blake3::hash(&png_bytes).to_hex().to_string();

    let my_avatar_file = get_avatars_dir().join("my_avatar.png");
    std::fs::write(&my_avatar_file, &png_bytes)
        .map_err(|e| format!("Failed to save avatar file: {e}"))?;

    state.db.set_setting("avatar_hash", &hash)?;

    let b64 = format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(&png_bytes)
    );

    let _ = app.emit(
        "my_avatar_updated",
        serde_json::json!({
            "hash": &hash,
            "avatar_data": &b64
        }),
    );

    // Broadcast new avatar to all contacts & connected peers
    broadcast_avatar_update(&state, &b64, &hash);

    Ok(b64)
}

pub fn send_avatar_update_to_peer(
    state: &Arc<AppState>,
    target_device_id: &str,
    avatar_data: &str,
    avatar_hash: &str,
) {
    let payload = serde_json::json!({
        "type": "AVATAR_UPDATE",
        "avatar_hash": avatar_hash,
        "avatar_data": avatar_data
    });
    let payload_str = payload.to_string();
    let hex_payload = hex::encode(payload_str.as_bytes());

    // 1. Send via active WAN signaling relay
    if let Some(sig_tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let packet = serde_json::json!({
            "type": "RELAY_CONTROL",
            "session_id": "avatar:sync",
            "from_device_id": state.identity.device_id,
            "target_device_id": target_device_id,
            "payload_json": hex_payload
        })
        .to_string();
        let _ = sig_tx.send(packet);
    }

    // 2. Also send directly via LAN TCP if contact has known local IP/port
    if let Ok(Some(contact)) = state.db.get_contact(target_device_id) {
        if let (Some(ip), Some(port)) = (contact.last_known_ip, contact.last_known_port) {
            let target_addr = format!("{}:{}", ip, port);
            let my_dev_id = state.identity.device_id.clone();
            let av_hash = avatar_hash.to_string();
            let av_data = avatar_data.to_string();
            tauri::async_runtime::spawn(async move {
                if let Ok(Ok(mut stream)) = tokio::time::timeout(
                    std::time::Duration::from_millis(2000),
                    tokio::net::TcpStream::connect(&target_addr),
                )
                .await
                {
                    let packet_val = serde_json::json!({
                        "type": "AVATAR_UPDATE",
                        "from_device_id": my_dev_id,
                        "avatar_hash": av_hash,
                        "avatar_data": av_data
                    });
                    if let Ok(data) = serde_json::to_vec(&packet_val) {
                        let len_prefix = (data.len() as u32).to_be_bytes();
                        use tokio::io::AsyncWriteExt;
                        let _ = stream.write_all(&len_prefix).await;
                        let _ = stream.write_all(&data).await;
                    }
                }
            });
        }
    }
}

pub fn broadcast_avatar_update(
    state: &Arc<AppState>,
    avatar_data: &str,
    avatar_hash: &str,
) {
    if let Ok(contacts) = state.db.list_contacts() {
        for c in contacts {
            if c.device_id != state.identity.device_id {
                send_avatar_update_to_peer(state, &c.device_id, avatar_data, avatar_hash);
            }
        }
    }
}

pub fn request_peer_avatar_internal(
    state: &Arc<AppState>,
    peer_device_id: &str,
) {
    // 1. Send via active WAN signaling relay
    if let Some(sig_tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let payload = serde_json::json!({
            "type": "AVATAR_REQUEST"
        });
        let payload_str = payload.to_string();
        let hex_payload = hex::encode(payload_str.as_bytes());
        let packet = serde_json::json!({
            "type": "RELAY_CONTROL",
            "session_id": "avatar:sync",
            "from_device_id": state.identity.device_id,
            "target_device_id": peer_device_id,
            "payload_json": hex_payload
        })
        .to_string();
        let _ = sig_tx.send(packet);
    }

    // 2. Also send via LAN TCP
    if let Ok(Some(contact)) = state.db.get_contact(peer_device_id) {
        if let (Some(ip), Some(port)) = (contact.last_known_ip, contact.last_known_port) {
            let target_addr = format!("{}:{}", ip, port);
            let my_dev_id = state.identity.device_id.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(Ok(mut stream)) = tokio::time::timeout(
                    std::time::Duration::from_millis(2000),
                    tokio::net::TcpStream::connect(&target_addr),
                )
                .await
                {
                    let packet_val = serde_json::json!({
                        "type": "AVATAR_REQUEST",
                        "from_device_id": my_dev_id
                    });
                    if let Ok(data) = serde_json::to_vec(&packet_val) {
                        let len_prefix = (data.len() as u32).to_be_bytes();
                        use tokio::io::AsyncWriteExt;
                        let _ = stream.write_all(&len_prefix).await;
                        let _ = stream.write_all(&data).await;
                    }
                }
            });
        }
    }
}

#[tauri::command]
pub async fn request_peer_avatar(
    state: State<'_, Arc<AppState>>,
    peer_device_id: String,
) -> Result<(), String> {
    request_peer_avatar_internal(&state, &peer_device_id);
    Ok(())
}

#[tauri::command]
pub fn get_my_avatar() -> Result<Option<String>, String> {
    Ok(get_my_avatar_internal())
}

#[tauri::command]
pub fn get_peer_avatar(peer_device_id: String) -> Result<Option<String>, String> {
    let safe_id: String = peer_device_id
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect();

    let file = get_avatars_dir().join(format!("{safe_id}.png"));
    if file.exists() {
        if let Ok(bytes) = std::fs::read(&file) {
            let b64 = BASE64_STANDARD.encode(&bytes);
            return Ok(Some(format!("data:image/png;base64,{b64}")));
        }
    }
    Ok(None)
}

#[tauri::command]
pub fn set_contact_custom_name(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    device_id: String,
    custom_name: String,
) -> Result<Contact, String> {
    let trimmed = custom_name.trim();
    state.db.set_contact_custom_name(&device_id, trimmed)?;
    let _ = app.emit("contacts_updated", ());
    state
        .db
        .get_contact(&device_id)?
        .ok_or_else(|| "Contact not found".to_string())
}

#[tauri::command]
pub fn send_chat_message(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    peer_device_id: String,
    content: String,
    msg_type: Option<String>,
    transfer_session_id: Option<String>,
    files_json: Option<String>,
) -> Result<ChatMessage, String> {
    let now = now_unix_secs();
    let msg_id = uuid::Uuid::new_v4().to_string();
    let effective_type = msg_type.unwrap_or_else(|| "text".to_string());

    // Enrich files_json with real file size and fast lightweight thumbnails (max 300x220)
    let enriched_files_json = if let Some(ref raw_json) = files_json {
        if let Ok(mut items) = serde_json::from_str::<Vec<serde_json::Value>>(raw_json) {
            for item in &mut items {
                if let Some(obj) = item.as_object_mut() {
                    let path_str = obj
                        .get("path")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let name_str = obj
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_lowercase();

                    if let Some(ref p) = path_str {
                        // 1. Populate real file size if missing or 0
                        let current_size = obj.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                        if current_size == 0 {
                            if let Ok(meta) = std::fs::metadata(p) {
                                obj.insert("size".to_string(), json!(meta.len()));
                            }
                        }

                        // 2. Generate lightweight base64 thumbnail if it's an image
                        let is_img = name_str.ends_with(".png")
                            || name_str.ends_with(".jpg")
                            || name_str.ends_with(".jpeg")
                            || name_str.ends_with(".webp")
                            || name_str.ends_with(".bmp")
                            || name_str.ends_with(".gif");

                        if is_img && obj.get("thumbnail").is_none() {
                            if let Ok(thumb_data) = generate_image_thumbnail(p, 300, 220) {
                                obj.insert("thumbnail".to_string(), json!(thumb_data));
                            }
                        }
                    }
                }
            }
            serde_json::to_string(&items).ok()
        } else {
            files_json.clone()
        }
    } else {
        None
    };

    let my_id = state.identity.device_id.clone();
    let my_alias = state.settings.lock().unwrap().alias_name.clone();

    let chat_msg = ChatMessage {
        id: msg_id.clone(),
        peer_device_id: peer_device_id.clone(),
        direction: "outgoing".to_string(),
        content: content.clone(),
        msg_type: effective_type.clone(),
        transfer_session_id: transfer_session_id.clone(),
        files_json: enriched_files_json.clone(),
        is_read: true,
        created_at: now,
        sender_device_id: Some(my_id.clone()),
        sender_alias: Some(my_alias.clone()),
    };

    state.db.insert_chat_message(&chat_msg)?;

    // If this is a reply to an incoming chat, automatically add the peer to contacts on both sides
    if let Ok(has_incoming) = state.db.has_incoming_chat(&peer_device_id) {
        if has_incoming {
            auto_add_contact_from_chat(&state, &app, &peer_device_id, None);
        }
    }

    let my_avatar_hash = state.db.get_setting("avatar_hash").ok().flatten();
    let my_avatar_b64 = if my_avatar_hash.is_some() {
        get_my_avatar_internal()
    } else {
        None
    };

    let payload_obj = json!({
        "id": msg_id,
        "content": content,
        "msg_type": effective_type,
        "transfer_session_id": transfer_session_id,
        "files_json": enriched_files_json,
        "from_alias": state.settings.lock().unwrap().alias_name,
        "from_avatar_hash": my_avatar_hash,
        "from_avatar_data": my_avatar_b64,
        "created_at": now
    })
    .to_string();

    let hex_payload = hex::encode(payload_obj.as_bytes());

    // 1. Send via active WAN signaling relay
    if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let packet = json!({
            "type": "RELAY_CONTROL",
            "session_id": format!("chat:{msg_id}"),
            "from_device_id": state.identity.device_id,
            "target_device_id": peer_device_id,
            "payload_json": hex_payload
        })
        .to_string();
        let _ = tx.send(packet);
    }

    // 2. Also attempt direct LAN / Tailscale TCP delivery if contact has known local IP & port
    if let Ok(Some(contact)) = state.db.get_contact(&peer_device_id) {
        if let (Some(ip), Some(port)) = (contact.last_known_ip, contact.last_known_port) {
            let target_addr = format!("{}:{}", ip, port);
            let hex_p = hex_payload.clone();
            let my_dev_id = state.identity.device_id.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(Ok(mut stream)) = tokio::time::timeout(
                    std::time::Duration::from_millis(1500),
                    tokio::net::TcpStream::connect(&target_addr),
                )
                .await
                {
                    let packet_val = json!({
                        "type": "CHAT",
                        "from_device_id": my_dev_id,
                        "payload_json": hex_p
                    });
                    if let Ok(data) = serde_json::to_vec(&packet_val) {
                        let len_prefix = (data.len() as u32).to_be_bytes();
                        use tokio::io::AsyncWriteExt;
                        let _ = stream.write_all(&len_prefix).await;
                        let _ = stream.write_all(&data).await;
                        let _ = stream.flush().await;
                    }
                }
            });
        }
    }

    let _ = app.emit("chat_message_sent", &chat_msg);
    Ok(chat_msg)
}

#[tauri::command]
pub fn get_chat_messages(
    state: State<'_, Arc<AppState>>,
    peer_device_id: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<ChatMessage>, String> {
    state
        .db
        .list_chat_messages(&peer_device_id, limit.unwrap_or(200), offset.unwrap_or(0))
}

#[tauri::command]
pub fn mark_chat_messages_read(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    peer_device_id: String,
) -> Result<(), String> {
    state.db.mark_chat_read(&peer_device_id)?;
    if let Ok(counts) = state.db.get_unread_chat_counts() {
        let total: u32 = counts.values().sum();
        crate::tray_badge::update_tray_unread_badge(&app, total);
    }
    Ok(())
}

#[tauri::command]
pub fn update_tray_badge(app: AppHandle, count: u32) -> Result<(), String> {
    crate::tray_badge::update_tray_unread_badge(&app, count);
    Ok(())
}

#[tauri::command]
pub fn clear_chat_messages(
    state: State<'_, Arc<AppState>>,
    peer_device_id: String,
) -> Result<(), String> {
    state.db.clear_chat_messages(&peer_device_id)
}

#[tauri::command]
pub fn get_unread_chat_counts(
    state: State<'_, Arc<AppState>>,
) -> Result<std::collections::HashMap<String, u32>, String> {
    state.db.get_unread_chat_counts()
}

pub fn send_raw_packet_to_device(
    state: &Arc<AppState>,
    target_device_id: &str,
    session_id: &str,
    payload_json: &str,
) {
    let hex_payload = hex::encode(payload_json.as_bytes());

    // 1. Send via WAN signaling relay
    if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let packet = json!({
            "type": "RELAY_CONTROL",
            "session_id": session_id,
            "from_device_id": state.identity.device_id,
            "target_device_id": target_device_id,
            "payload_json": hex_payload
        })
        .to_string();
        let _ = tx.send(packet);
    }

    // 2. Also send via direct LAN TCP if IP/port are known
    if let Ok(Some(contact)) = state.db.get_contact(target_device_id) {
        if let (Some(ip), Some(port)) = (contact.last_known_ip, contact.last_known_port) {
            let target_addr = format!("{}:{}", ip, port);
            let hex_p = hex_payload.clone();
            let my_dev_id = state.identity.device_id.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(Ok(mut stream)) = tokio::time::timeout(
                    std::time::Duration::from_millis(1500),
                    tokio::net::TcpStream::connect(&target_addr),
                )
                .await
                {
                    let packet_val = json!({
                        "type": "CHAT",
                        "from_device_id": my_dev_id,
                        "payload_json": hex_p
                    });
                    if let Ok(data) = serde_json::to_vec(&packet_val) {
                        let len_prefix = (data.len() as u32).to_be_bytes();
                        use tokio::io::AsyncWriteExt;
                        let _ = stream.write_all(&len_prefix).await;
                        let _ = stream.write_all(&data).await;
                        let _ = stream.flush().await;
                    }
                }
            });
        }
    }
}

#[tauri::command]
pub fn create_chat_group(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    name: String,
    description: Option<String>,
    avatar: Option<String>,
    member_device_ids: Vec<String>,
) -> Result<ChatGroup, String> {
    let now = now_unix_secs();
    let group_id = format!("group_{}_{}", now, &uuid::Uuid::new_v4().to_string()[..8]);
    let my_id = state.identity.device_id.clone();
    let my_alias = state.settings.lock().unwrap().alias_name.clone();
    let my_avatar = get_my_avatar_internal();

    let mut members = Vec::new();
    // 1. Add creator as Owner (Admin Utama)
    members.push(GroupMember {
        device_id: my_id.clone(),
        alias: if my_alias.trim().is_empty() { "You".to_string() } else { my_alias.clone() },
        avatar: my_avatar,
        role: "owner".to_string(),
        joined_at: now,
    });

    // 2. Add invited members
    for dev_id in &member_device_ids {
        if dev_id == &my_id {
            continue;
        }
        let alias = state
            .db
            .get_contact(dev_id)
            .ok()
            .flatten()
            .and_then(|c| c.custom_name.or(Some(c.alias_name)))
            .unwrap_or_else(|| format!("Member ({})", &dev_id[..6.min(dev_id.len())]));

        members.push(GroupMember {
            device_id: dev_id.clone(),
            alias,
            avatar: None,
            role: "member".to_string(),
            joined_at: now,
        });
    }

    let group = ChatGroup {
        id: group_id.clone(),
        name: name.trim().to_string(),
        description: description.map(|d| d.trim().to_string()).filter(|d| !d.is_empty()),
        avatar,
        owner_id: my_id.clone(),
        send_permission: "all".to_string(),
        allowed_senders: Vec::new(),
        members,
        created_at: now,
        updated_at: now,
    };

    state.db.upsert_chat_group(&group)?;

    // Broadcast GROUP_INVITE to all invited members
    let invite_payload = json!({
        "control_type": "GROUP_INVITE",
        "group": &group,
        "invited_by": my_id,
        "invited_by_alias": my_alias,
    })
    .to_string();

    let state_arc = state.inner().clone();
    for dev_id in &member_device_ids {
        if dev_id != &my_id {
            send_raw_packet_to_device(&state_arc, dev_id, &format!("grp_ctl:{group_id}"), &invite_payload);
        }
    }

    let _ = app.emit("chat_group_updated", &group);
    Ok(group)
}

#[tauri::command]
pub fn get_chat_groups(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ChatGroup>, String> {
    state.db.list_chat_groups()
}

#[tauri::command]
pub fn get_chat_group(
    state: State<'_, Arc<AppState>>,
    group_id: String,
) -> Result<Option<ChatGroup>, String> {
    state.db.get_chat_group(&group_id)
}

#[tauri::command]
pub fn update_chat_group_details(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    group_id: String,
    name: String,
    description: Option<String>,
    avatar: Option<String>,
) -> Result<ChatGroup, String> {
    let mut group = state
        .db
        .get_chat_group(&group_id)?
        .ok_or_else(|| format!("Group not found: {group_id}"))?;

    let my_id = &state.identity.device_id;
    let is_admin = group.owner_id == *my_id || group.members.iter().any(|m| m.device_id == *my_id && m.role == "admin");
    if !is_admin {
        return Err("Hanya admin yang dapat mengubah info grup".to_string());
    }

    group.name = name.trim().to_string();
    group.description = description.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
    if avatar.is_some() {
        group.avatar = avatar;
    }
    group.updated_at = now_unix_secs();

    state.db.upsert_chat_group(&group)?;

    let update_payload = json!({
        "control_type": "GROUP_UPDATE",
        "group": &group,
    })
    .to_string();

    let state_arc = state.inner().clone();
    for m in &group.members {
        if &m.device_id != my_id {
            send_raw_packet_to_device(&state_arc, &m.device_id, &format!("grp_ctl:{group_id}"), &update_payload);
        }
    }

    let _ = app.emit("chat_group_updated", &group);
    Ok(group)
}

#[tauri::command]
pub fn invite_group_members(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    group_id: String,
    new_device_ids: Vec<String>,
) -> Result<ChatGroup, String> {
    let mut group = state
        .db
        .get_chat_group(&group_id)?
        .ok_or_else(|| format!("Group not found: {group_id}"))?;

    let my_id = &state.identity.device_id;
    let my_alias = state.settings.lock().unwrap().alias_name.clone();

    // Requirement: Hanya admin yang bisa invite user
    let is_admin = group.owner_id == *my_id || group.members.iter().any(|m| m.device_id == *my_id && m.role == "admin");
    if !is_admin {
        return Err("Hanya admin yang dapat mengundang anggota ke grup ini".to_string());
    }

    let now = now_unix_secs();
    let mut added_ids = Vec::new();

    for dev_id in new_device_ids {
        if !group.members.iter().any(|m| m.device_id == dev_id) {
            let alias = state
                .db
                .get_contact(&dev_id)
                .ok()
                .flatten()
                .and_then(|c| c.custom_name.or(Some(c.alias_name)))
                .unwrap_or_else(|| format!("Member ({})", &dev_id[..6.min(dev_id.len())]));

            group.members.push(GroupMember {
                device_id: dev_id.clone(),
                alias,
                avatar: None,
                role: "member".to_string(),
                joined_at: now,
            });
            added_ids.push(dev_id);
        }
    }

    group.updated_at = now;
    state.db.upsert_chat_group(&group)?;

    let invite_payload = json!({
        "control_type": "GROUP_INVITE",
        "group": &group,
        "invited_by": my_id,
        "invited_by_alias": my_alias,
    })
    .to_string();

    let update_payload = json!({
        "control_type": "GROUP_UPDATE",
        "group": &group,
    })
    .to_string();

    let state_arc = state.inner().clone();
    for m in &group.members {
        if &m.device_id != my_id {
            if added_ids.contains(&m.device_id) {
                send_raw_packet_to_device(&state_arc, &m.device_id, &format!("grp_ctl:{group_id}"), &invite_payload);
            } else {
                send_raw_packet_to_device(&state_arc, &m.device_id, &format!("grp_ctl:{group_id}"), &update_payload);
            }
        }
    }

    let _ = app.emit("chat_group_updated", &group);
    Ok(group)
}

#[tauri::command]
pub fn remove_group_member(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    group_id: String,
    target_device_id: String,
) -> Result<ChatGroup, String> {
    let mut group = state
        .db
        .get_chat_group(&group_id)?
        .ok_or_else(|| format!("Group not found: {group_id}"))?;

    let my_id = &state.identity.device_id;
    let is_owner = group.owner_id == *my_id;
    let is_admin = is_owner || group.members.iter().any(|m| m.device_id == *my_id && m.role == "admin");

    if !is_admin {
        return Err("Hanya admin yang dapat mengeluarkan anggota".to_string());
    }

    if target_device_id == group.owner_id {
        return Err("Admin Utama tidak dapat dikeluarkan dari grup".to_string());
    }

    if !is_owner {
        let target_role = group.members.iter().find(|m| m.device_id == target_device_id).map(|m| &m.role);
        if target_role == Some(&"admin".to_string()) {
            return Err("Hanya Admin Utama yang dapat mengeluarkan sesama admin".to_string());
        }
    }

    group.members.retain(|m| m.device_id != target_device_id);
    group.allowed_senders.retain(|id| id != &target_device_id);
    group.updated_at = now_unix_secs();
    state.db.upsert_chat_group(&group)?;

    let removed_payload = json!({
        "control_type": "GROUP_MEMBER_REMOVED",
        "group_id": group_id,
        "target_device_id": target_device_id,
    })
    .to_string();

    let update_payload = json!({
        "control_type": "GROUP_UPDATE",
        "group": &group,
    })
    .to_string();

    let state_arc = state.inner().clone();
    send_raw_packet_to_device(&state_arc, &target_device_id, &format!("grp_ctl:{group_id}"), &removed_payload);

    for m in &group.members {
        if &m.device_id != my_id {
            send_raw_packet_to_device(&state_arc, &m.device_id, &format!("grp_ctl:{group_id}"), &update_payload);
        }
    }

    let _ = app.emit("chat_group_updated", &group);
    Ok(group)
}

#[tauri::command]
pub fn set_member_role(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    group_id: String,
    target_device_id: String,
    new_role: String,
) -> Result<ChatGroup, String> {
    let mut group = state
        .db
        .get_chat_group(&group_id)?
        .ok_or_else(|| format!("Group not found: {group_id}"))?;

    let my_id = &state.identity.device_id;

    // Requirement: Admin utama bisa menentukan siapa aja yang mau dijadikan admin
    if group.owner_id != *my_id {
        return Err("Hanya Admin Utama yang dapat mengangkat atau menurunkan admin".to_string());
    }

    if target_device_id == group.owner_id {
        return Err("Peran Admin Utama tidak dapat diubah".to_string());
    }

    if new_role != "admin" && new_role != "member" {
        return Err("Peran tidak valid. Pilihan: 'admin' atau 'member'".to_string());
    }

    let mut found = false;
    for m in &mut group.members {
        if m.device_id == target_device_id {
            m.role = new_role.clone();
            found = true;
            break;
        }
    }

    if !found {
        return Err("Anggota tidak ditemukan dalam grup".to_string());
    }

    group.updated_at = now_unix_secs();
    state.db.upsert_chat_group(&group)?;

    let update_payload = json!({
        "control_type": "GROUP_UPDATE",
        "group": &group,
    })
    .to_string();

    let state_arc = state.inner().clone();
    for m in &group.members {
        if &m.device_id != my_id {
            send_raw_packet_to_device(&state_arc, &m.device_id, &format!("grp_ctl:{group_id}"), &update_payload);
        }
    }

    let _ = app.emit("chat_group_updated", &group);
    Ok(group)
}

#[tauri::command]
pub fn set_group_send_permission(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    group_id: String,
    send_permission: String,
    allowed_senders: Option<Vec<String>>,
) -> Result<ChatGroup, String> {
    let mut group = state
        .db
        .get_chat_group(&group_id)?
        .ok_or_else(|| format!("Group not found: {group_id}"))?;

    let my_id = &state.identity.device_id;
    let is_admin = group.owner_id == *my_id || group.members.iter().any(|m| m.device_id == *my_id && m.role == "admin");
    if !is_admin {
        return Err("Hanya admin yang dapat mengatur izin kirim pesan grup".to_string());
    }

    // Requirement: kirim chat (hanya admin, all member atau custom)
    match send_permission.as_str() {
        "all" | "admins_only" | "custom" => {}
        _ => return Err("Izin kirim pesan tidak valid. Pilihan: 'all', 'admins_only', atau 'custom'".to_string()),
    }

    group.send_permission = send_permission;
    if let Some(list) = allowed_senders {
        group.allowed_senders = list;
    }
    group.updated_at = now_unix_secs();
    state.db.upsert_chat_group(&group)?;

    let update_payload = json!({
        "control_type": "GROUP_UPDATE",
        "group": &group,
    })
    .to_string();

    let state_arc = state.inner().clone();
    for m in &group.members {
        if &m.device_id != my_id {
            send_raw_packet_to_device(&state_arc, &m.device_id, &format!("grp_ctl:{group_id}"), &update_payload);
        }
    }

    let _ = app.emit("chat_group_updated", &group);
    Ok(group)
}

#[tauri::command]
pub fn leave_chat_group(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    group_id: String,
) -> Result<(), String> {
    let mut group = match state.db.get_chat_group(&group_id)? {
        Some(g) => g,
        None => return Ok(()),
    };

    let my_id = state.identity.device_id.clone();
    let was_owner = group.owner_id == my_id;

    group.members.retain(|m| m.device_id != my_id);
    group.allowed_senders.retain(|id| id != &my_id);

    if group.members.is_empty() {
        state.db.delete_chat_group(&group_id)?;
    } else {
        if was_owner {
            if let Some(admin) = group.members.iter_mut().find(|m| m.role == "admin") {
                admin.role = "owner".to_string();
                group.owner_id = admin.device_id.clone();
            } else if let Some(first_member) = group.members.first_mut() {
                first_member.role = "owner".to_string();
                group.owner_id = first_member.device_id.clone();
            }
        }
        group.updated_at = now_unix_secs();
        state.db.upsert_chat_group(&group)?;

        let left_payload = json!({
            "control_type": "GROUP_MEMBER_LEFT",
            "group_id": &group_id,
            "member_id": &my_id,
            "group": &group,
        })
        .to_string();

        let state_arc = state.inner().clone();
        for m in &group.members {
            send_raw_packet_to_device(&state_arc, &m.device_id, &format!("grp_ctl:{group_id}"), &left_payload);
        }
    }

    let _ = state.db.delete_chat_group(&group_id);
    let _ = app.emit("chat_group_removed", &group_id);
    Ok(())
}

#[tauri::command]
pub fn send_group_chat_message(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    group_id: String,
    content: String,
    msg_type: Option<String>,
    files_json: Option<String>,
) -> Result<ChatMessage, String> {
    let group = state
        .db
        .get_chat_group(&group_id)?
        .ok_or_else(|| format!("Group not found: {group_id}"))?;

    let my_id = state.identity.device_id.clone();
    let my_alias = state.settings.lock().unwrap().alias_name.clone();

    // Check membership
    let my_member = group.members.iter().find(|m| m.device_id == my_id);
    if my_member.is_none() {
        return Err("Anda bukan anggota grup ini".to_string());
    }
    let role = &my_member.unwrap().role;
    let is_admin = role == "owner" || role == "admin";

    // Permission enforcement: all, admins_only, custom
    if group.send_permission == "admins_only" && !is_admin {
        return Err("Pengiriman pesan dibatasi: Hanya admin yang dapat mengirim pesan di grup ini".to_string());
    }
    if group.send_permission == "custom" && !is_admin && !group.allowed_senders.contains(&my_id) {
        return Err("Pengiriman pesan dibatasi: Anda tidak memiliki izin mengirim pesan di grup ini".to_string());
    }

    let now = now_unix_secs();
    let msg_id = uuid::Uuid::new_v4().to_string();
    let effective_type = msg_type.unwrap_or_else(|| "text".to_string());

    let my_avatar_hash = state.db.get_setting("avatar_hash").ok().flatten();
    let my_avatar_b64 = if my_avatar_hash.is_some() {
        get_my_avatar_internal()
    } else {
        None
    };

    let chat_msg = ChatMessage {
        id: msg_id.clone(),
        peer_device_id: group_id.clone(),
        direction: "outgoing".to_string(),
        content: content.clone(),
        msg_type: effective_type.clone(),
        transfer_session_id: None,
        files_json: files_json.clone(),
        is_read: true,
        created_at: now,
        sender_device_id: Some(my_id.clone()),
        sender_alias: Some(my_alias.clone()),
    };

    state.db.insert_chat_message(&chat_msg)?;

    let payload_obj = json!({
        "id": msg_id,
        "group_id": group_id,
        "group_name": group.name,
        "from_device_id": my_id,
        "from_alias": my_alias,
        "from_avatar_hash": my_avatar_hash,
        "from_avatar_data": my_avatar_b64,
        "content": content,
        "msg_type": effective_type,
        "files_json": files_json,
        "created_at": now
    })
    .to_string();

    let state_arc = state.inner().clone();
    for m in &group.members {
        if m.device_id != my_id {
            send_raw_packet_to_device(&state_arc, &m.device_id, &format!("chat:{msg_id}"), &payload_obj);
        }
    }

    let _ = app.emit("chat_message_sent", &chat_msg);
    Ok(chat_msg)
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Invalid URL protocol".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn start_web_share(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    paths: Vec<String>,
) -> Result<crate::network::web_share::WebShareInfo, String> {
    crate::network::web_share::start_web_share_server(app, state.inner().clone(), paths).await
}

#[tauri::command]
pub fn stop_web_share(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    crate::network::web_share::stop_web_share_server(&state);
    Ok(())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn read_clipboard_content() -> Result<String, String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
    if let Ok(text) = cb.get_text() {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    if let Ok(img) = cb.get_image() {
        use image::ImageBuffer;
        let width = img.width as u32;
        let height = img.height as u32;
        if let Some(buf) = ImageBuffer::<image::Rgba<u8>, _>::from_raw(width, height, img.bytes.into_owned()) {
            let mut png_bytes = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut png_bytes);
            if buf.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
                return Ok(format!("data:image/png;base64,{b64}"));
            }
        }
    }
    cb.get_text().map_err(|e| format!("Clipboard read error: {e}"))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn write_clipboard_content(content: &str) -> Result<(), String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let mut cb = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(_) => {
            // Retry once after brief delay in case clipboard is momentarily held by OS/another app
            std::thread::sleep(std::time::Duration::from_millis(35));
            arboard::Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?
        }
    };

    if trimmed.starts_with("data:image/") {
        if let Some(comma_pos) = trimmed.find(',') {
            let b64_str = trimmed[comma_pos + 1..].trim();
            use base64::Engine;
            if let Ok(png_bytes) = base64::engine::general_purpose::STANDARD.decode(b64_str) {
                if let Ok(dyn_img) = image::load_from_memory(&png_bytes) {
                    let rgba = dyn_img.to_rgba8();
                    let img_data = arboard::ImageData {
                        width: rgba.width() as usize,
                        height: rgba.height() as usize,
                        bytes: std::borrow::Cow::Owned(rgba.into_raw()),
                    };
                    if let Err(e) = cb.set_image(img_data) {
                        tracing::warn!("Failed to set raw image to clipboard, falling back to text: {e}");
                    } else {
                        return Ok(());
                    }
                }
            }
        }
    }
    cb.set_text(trimmed.to_string()).map_err(|e| format!("Clipboard write error: {e}"))
}

#[tauri::command]
pub fn read_system_clipboard() -> Result<String, String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        read_clipboard_content()
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Err("Clipboard read is not supported on mobile native backend".to_string())
    }
}

#[tauri::command]
pub fn write_system_clipboard(content: String) -> Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        write_clipboard_content(&content)
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = content;
        Ok(())
    }
}

#[tauri::command]
pub fn save_base64_image(
    state: State<'_, Arc<AppState>>,
    data_url: String,
    custom_filename: Option<String>,
) -> Result<String, String> {
    let trimmed = data_url.trim();
    if trimmed.is_empty() {
        return Err("Empty image data".to_string());
    }

    let (ext, b64_str) = if let Some(comma_pos) = trimmed.find(',') {
        let header = &trimmed[..comma_pos].to_lowercase();
        let ext = if header.contains("image/jpeg") || header.contains("image/jpg") {
            "jpg"
        } else if header.contains("image/webp") {
            "webp"
        } else if header.contains("image/gif") {
            "gif"
        } else {
            "png"
        };
        (ext, trimmed[comma_pos + 1..].trim())
    } else {
        ("png", trimmed)
    };

    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64_str)
        .map_err(|e| format!("Base64 decode failed: {e}"))?;

    let dl_dir_str = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.download_dir.clone()
    };

    let download_dir = if !dl_dir_str.trim().is_empty() {
        std::path::PathBuf::from(dl_dir_str)
    } else {
        std::env::current_dir().unwrap_or_default()
    };

    if !download_dir.exists() {
        let _ = std::fs::create_dir_all(&download_dir);
    }

    let filename = if let Some(cf) = custom_filename {
        let t = cf.trim();
        if !t.is_empty() {
            if t.contains('.') {
                t.to_string()
            } else {
                format!("{t}.{ext}")
            }
        } else {
            format!("raydrop-image-{}.{ext}", now_unix_secs())
        }
    } else {
        format!("raydrop-image-{}.{ext}", now_unix_secs())
    };

    let target_path = download_dir.join(&filename);
    std::fs::write(&target_path, bytes)
        .map_err(|e| format!("Failed to write image file: {e}"))?;

    tracing::info!("Saved image to {}", target_path.display());
    Ok(target_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn broadcast_clipboard(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    content: String,
    target_device_ids: Option<Vec<String>>,
) -> Result<u32, String> {
    if content.trim().is_empty() {
        return Err("Clipboard text cannot be empty".to_string());
    }

    let targets: Vec<String> = if let Some(ids) = target_device_ids {
        if !ids.is_empty() {
            ids
        } else {
            let all = state.db.list_contacts().map_err(|e| e.to_string())?;
            all.into_iter()
                .filter(|c| c.is_own_device || c.trust_level == "trusted_auto_accept")
                .map(|c| c.device_id)
                .collect()
        }
    } else {
        let all = state.db.list_contacts().map_err(|e| e.to_string())?;
        all.into_iter()
            .filter(|c| c.is_own_device || c.trust_level == "trusted_auto_accept")
            .map(|c| c.device_id)
            .collect()
    };

    if targets.is_empty() {
        return Err("No trusted devices found. Add a contact or device first.".to_string());
    }

    let mut sent_count = 0;
    for peer_id in targets {
        if send_chat_message(
            app.clone(),
            state.clone(),
            peer_id,
            content.clone(),
            Some("clipboard".to_string()),
            None,
            None,
        )
        .is_ok()
        {
            sent_count += 1;
        }
    }

    Ok(sent_count)
}

#[tauri::command]
pub fn list_folder_sync_tasks(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<crate::db::storage::FolderSyncTask>, String> {
    state.db.list_folder_sync_tasks()
}

#[tauri::command]
pub fn add_folder_sync_task(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    local_path: String,
    target_device_id: String,
    is_auto_sync: bool,
) -> Result<crate::db::storage::FolderSyncTask, String> {
    let p = std::path::Path::new(&local_path);
    if !p.exists() || !p.is_dir() {
        return Err("Selected path does not exist or is not a directory".to_string());
    }

    let contact = state
        .db
        .get_contact(&target_device_id)?
        .ok_or_else(|| "Target device not found in contacts".to_string())?;

    let task = crate::db::storage::FolderSyncTask {
        id: uuid::Uuid::new_v4().to_string(),
        local_path,
        target_device_id,
        target_alias: contact.alias_name,
        is_auto_sync,
        last_sync_at: None,
        created_at: now_unix_secs(),
    };

    state.db.insert_folder_sync_task(&task)?;

    if is_auto_sync {
        let _ = state.folder_sync_watcher.start_watching(
            task.id.clone(),
            task.local_path.clone(),
            task.target_device_id.clone(),
            app,
            state.inner().clone(),
        );
    }

    Ok(task)
}

#[tauri::command]
pub fn toggle_folder_sync_auto(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    task_id: String,
    is_auto: bool,
) -> Result<(), String> {
    state.db.update_folder_sync_auto_flag(&task_id, is_auto)?;
    if is_auto {
        if let Ok(Some(task)) = state.db.get_folder_sync_task(&task_id) {
            let _ = state.folder_sync_watcher.start_watching(
                task.id,
                task.local_path,
                task.target_device_id,
                app,
                state.inner().clone(),
            );
        }
    } else {
        state.folder_sync_watcher.stop_watching(&task_id);
    }
    Ok(())
}

#[tauri::command]
pub fn delete_folder_sync_task(
    state: State<'_, Arc<AppState>>,
    task_id: String,
) -> Result<(), String> {
    state.folder_sync_watcher.stop_watching(&task_id);
    state.db.delete_folder_sync_task(&task_id)
}

#[tauri::command]
pub async fn trigger_folder_sync(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    task_id: String,
) -> Result<String, String> {
    let task = state
        .db
        .get_folder_sync_task(&task_id)?
        .ok_or_else(|| "Folder sync task not found".to_string())?;

    let p = std::path::Path::new(&task.local_path);
    if !p.exists() || !p.is_dir() {
        return Err("Local folder no longer exists on disk".to_string());
    }

    let session_id = initiate_peer_transfer(
        app,
        state.inner().clone(),
        task.target_device_id,
        vec![task.local_path],
    )
    .await?;

    let _ = state.db.update_folder_sync_last_time(&task_id, now_unix_secs());
    Ok(session_id)
}

#[tauri::command]
pub fn list_shared_folders(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<crate::db::storage::SharedFolder>, String> {
    state.db.list_shared_folders()
}

#[tauri::command]
pub fn add_shared_folder(
    state: State<'_, Arc<AppState>>,
    name: String,
    local_path: String,
) -> Result<crate::db::storage::SharedFolder, String> {
    let p = std::path::Path::new(&local_path);
    if !p.exists() || !p.is_dir() {
        return Err("Selected path does not exist or is not a directory".to_string());
    }

    let folder = crate::db::storage::SharedFolder {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        local_path,
        is_active: true,
        created_at: now_unix_secs(),
    };

    state.db.insert_shared_folder(&folder)?;
    Ok(folder)
}

#[tauri::command]
pub fn delete_shared_folder(
    state: State<'_, Arc<AppState>>,
    folder_id: String,
) -> Result<(), String> {
    state.db.delete_shared_folder(&folder_id)
}

#[tauri::command]
pub async fn browse_remote_shares(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    peer_device_id: String,
) -> Result<Vec<crate::remote_fs::RemoteShareEntry>, String> {
    let req = serde_json::json!({
        "action": "list_shares"
    });
    let resp = crate::remote_fs::send_remote_fs_request(&app, &state, &peer_device_id, req).await?;
    if !resp.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        let err = resp.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to list remote shared folders");
        return Err(err.to_string());
    }

    let shares = serde_json::from_value::<Vec<crate::remote_fs::RemoteShareEntry>>(
        resp.get("shares").cloned().unwrap_or(serde_json::json!([]))
    ).map_err(|e| format!("Failed to parse shares: {e}"))?;

    Ok(shares)
}

#[tauri::command]
pub async fn browse_remote_dir(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    peer_device_id: String,
    share_id: String,
    subpath: String,
) -> Result<Vec<crate::remote_fs::RemoteFileItem>, String> {
    let req = serde_json::json!({
        "action": "list_dir",
        "share_id": share_id,
        "subpath": subpath
    });
    let resp = crate::remote_fs::send_remote_fs_request(&app, &state, &peer_device_id, req).await?;
    if !resp.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        let err = resp.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to list remote directory");
        return Err(err.to_string());
    }

    let entries = serde_json::from_value::<Vec<crate::remote_fs::RemoteFileItem>>(
        resp.get("entries").cloned().unwrap_or(serde_json::json!([]))
    ).map_err(|e| format!("Failed to parse directory entries: {e}"))?;

    Ok(entries)
}

#[tauri::command]
pub async fn pull_remote_file(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    peer_device_id: String,
    share_id: String,
    relative_path: String,
) -> Result<String, String> {
    let req = serde_json::json!({
        "action": "pull_file",
        "share_id": share_id,
        "relative_path": relative_path
    });
    let resp = crate::remote_fs::send_remote_fs_request(&app, &state, &peer_device_id, req).await?;
    if !resp.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        let err = resp.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to pull remote file");
        return Err(err.to_string());
    }

    let msg = resp.get("message").and_then(|v| v.as_str()).unwrap_or("Transfer initiated");
    Ok(msg.to_string())
}

#[tauri::command]
pub async fn send_meeting_signal(
    _app: AppHandle,
    state: State<'_, Arc<AppState>>,
    target_device_id: String,
    session_id: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    let payload_str = payload.to_string();
    let hex_payload = hex::encode(payload_str.as_bytes());

    // 1. Send via active WAN signaling relay
    let sent_via_relay = if let Some(sig_tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let packet = serde_json::json!({
            "type": "RELAY_CONTROL",
            "session_id": session_id,
            "from_device_id": state.identity.device_id,
            "target_device_id": target_device_id,
            "payload_json": hex_payload
        })
        .to_string();
        sig_tx.send(packet).is_ok()
    } else {
        false
    };

    // 2. Also send directly via LAN TCP if contact has known local IP/port
    if let Ok(Some(contact)) = state.db.get_contact(&target_device_id) {
        if let (Some(ip), Some(port)) = (contact.last_known_ip, contact.last_known_port) {
            let target_addr = format!("{}:{}", ip, port);
            let my_dev_id = state.identity.device_id.clone();
            let sid = session_id.clone();
            let p_clone = payload.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(Ok(mut stream)) = tokio::time::timeout(
                    std::time::Duration::from_millis(1500),
                    tokio::net::TcpStream::connect(&target_addr),
                )
                .await
                {
                    let packet_val = serde_json::json!({
                        "type": "MEETING_SIGNAL",
                        "from_device_id": my_dev_id,
                        "session_id": sid,
                        "payload": p_clone
                    });
                    if let Ok(data) = serde_json::to_vec(&packet_val) {
                        let len_prefix = (data.len() as u32).to_be_bytes();
                        use tokio::io::AsyncWriteExt;
                        let _ = stream.write_all(&len_prefix).await;
                        let _ = stream.write_all(&data).await;
                    }
                }
            });
        }
    }

    if sent_via_relay {
        Ok(())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn add_torrent_magnet(
    state: State<'_, Arc<AppState>>,
    magnet_url: String,
    custom_path: Option<String>,
) -> Result<crate::torrent::TorrentTaskInfo, String> {
    state.torrent_manager.add_magnet(&magnet_url, custom_path).await
}

#[tauri::command]
pub async fn list_torrent_tasks(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<crate::torrent::TorrentTaskInfo>, String> {
    Ok(state.torrent_manager.list_torrents().await)
}

#[tauri::command]
pub async fn pause_torrent_task(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    state.torrent_manager.pause_torrent(&id).await
}

#[tauri::command]
pub async fn resume_torrent_task(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    state.torrent_manager.resume_torrent(&id).await
}

#[tauri::command]
pub async fn delete_torrent_task(
    state: State<'_, Arc<AppState>>,
    id: String,
    delete_files: bool,
) -> Result<(), String> {
    state.torrent_manager.delete_torrent(&id, delete_files).await
}

#[tauri::command]
pub fn get_pending_magnet_link(
    state: State<'_, Arc<AppState>>,
) -> Result<Option<String>, String> {
    let mut lock = state.pending_magnet_link.lock().map_err(|e| e.to_string())?;
    Ok(lock.take())
}

#[tauri::command]
pub async fn add_http_download(
    state: State<'_, Arc<AppState>>,
    url: String,
    custom_save_dir: Option<String>,
    custom_filename: Option<String>,
) -> Result<crate::http_downloader::HttpDownloadTaskInfo, String> {
    state
        .http_downloader
        .add_download(&url, custom_save_dir, custom_filename)
        .await
}

#[tauri::command]
pub async fn list_http_downloads(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<crate::http_downloader::HttpDownloadTaskInfo>, String> {
    Ok(state.http_downloader.list_tasks().await)
}

#[tauri::command]
pub async fn pause_http_download(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    state.http_downloader.pause_task(&id).await
}

#[tauri::command]
pub async fn resume_http_download(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    state.http_downloader.resume_task(&id).await
}

#[tauri::command]
pub async fn update_http_download_url(
    state: State<'_, Arc<AppState>>,
    id: String,
    new_url: String,
) -> Result<crate::http_downloader::HttpDownloadTaskInfo, String> {
    state.http_downloader.update_download_url(&id, &new_url).await
}

#[tauri::command]
pub async fn delete_http_download(
    state: State<'_, Arc<AppState>>,
    id: String,
    delete_file: bool,
) -> Result<(), String> {
    state.http_downloader.delete_task(&id, delete_file).await
}

#[tauri::command]
pub async fn open_browser_extension_dir(app: AppHandle) -> Result<String, String> {
    let mut candidate_paths = Vec::new();

    // 1. Persistent unpacked directory in user app data
    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "raydrop", "Raydrop") {
        let ext_dir = proj_dirs.data_dir().join("browser-extension");
        let icons_dir = ext_dir.join("icons");
        let _ = std::fs::create_dir_all(&icons_dir);

        let manifest_bytes = include_bytes!("../../browser-extension/manifest.json");
        let bg_bytes = include_bytes!("../../browser-extension/background.js");
        let popup_html_bytes = include_bytes!("../../browser-extension/popup.html");
        let popup_js_bytes = include_bytes!("../../browser-extension/popup.js");
        let icon16_bytes = include_bytes!("../../browser-extension/icons/icon16.png");
        let icon32_bytes = include_bytes!("../../browser-extension/icons/icon32.png");
        let icon48_bytes = include_bytes!("../../browser-extension/icons/icon48.png");
        let icon128_bytes = include_bytes!("../../browser-extension/icons/icon128.png");

        let _ = std::fs::write(ext_dir.join("manifest.json"), manifest_bytes);
        let _ = std::fs::write(ext_dir.join("background.js"), bg_bytes);
        let _ = std::fs::write(ext_dir.join("popup.html"), popup_html_bytes);
        let _ = std::fs::write(ext_dir.join("popup.js"), popup_js_bytes);
        let _ = std::fs::write(icons_dir.join("icon16.png"), icon16_bytes);
        let _ = std::fs::write(icons_dir.join("icon32.png"), icon32_bytes);
        let _ = std::fs::write(icons_dir.join("icon48.png"), icon48_bytes);
        let _ = std::fs::write(icons_dir.join("icon128.png"), icon128_bytes);

        candidate_paths.push(ext_dir);
    }

    // 2. Dev or source repository directory
    if let Ok(cwd) = std::env::current_dir() {
        candidate_paths.push(cwd.join("browser-extension"));
        candidate_paths.push(cwd.join("..").join("browser-extension"));
    }

    for p in candidate_paths {
        if p.exists() {
            let path_str = p.to_string_lossy().to_string();
            let _ = open_folder_path(app.clone(), path_str.clone());
            return Ok(path_str);
        }
    }

    Err("Browser extension folder could not be located.".to_string())
}

#[tauri::command]
pub async fn open_browser_extension_url(browser: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let browser_clean = browser.to_lowercase();
        if browser_clean == "chrome" {
            let _ = std::process::Command::new("cmd")
                .args(["/c", "start", "chrome", "chrome://extensions"])
                .spawn();
        } else if browser_clean == "edge" {
            let _ = std::process::Command::new("cmd")
                .args(["/c", "start", "msedge", "edge://extensions"])
                .spawn();
        } else if browser_clean == "brave" {
            let _ = std::process::Command::new("cmd")
                .args(["/c", "start", "brave", "brave://extensions"])
                .spawn();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn start_http_download_now(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    state.http_downloader.start_task_now(&id).await
}

#[tauri::command]
pub fn execute_system_power_action(action: String) -> Result<(), String> {
    match action.to_lowercase().as_str() {
        "shutdown" => {
            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("shutdown")
                    .args(["/s", "/t", "0"])
                    .spawn()
                    .map_err(|e| format!("Shutdown failed: {e}"))?;
            }
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("osascript")
                    .args(["-e", "tell app \"System Events\" to shut down"])
                    .spawn()
                    .map_err(|e| format!("Shutdown failed: {e}"))?;
            }
            #[cfg(target_os = "linux")]
            {
                std::process::Command::new("systemctl")
                    .arg("poweroff")
                    .spawn()
                    .map_err(|e| format!("Shutdown failed: {e}"))?;
            }
        }
        "sleep" => {
            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("rundll32.exe")
                    .args(["powrprof.dll,SetSuspendState", "0,1,0"])
                    .spawn()
                    .map_err(|e| format!("Sleep failed: {e}"))?;
            }
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("osascript")
                    .args(["-e", "tell app \"System Events\" to sleep"])
                    .spawn()
                    .map_err(|e| format!("Sleep failed: {e}"))?;
            }
            #[cfg(target_os = "linux")]
            {
                std::process::Command::new("systemctl")
                    .arg("suspend")
                    .spawn()
                    .map_err(|e| format!("Sleep failed: {e}"))?;
            }
        }
        "hibernate" => {
            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("shutdown")
                    .args(["/h"])
                    .spawn()
                    .map_err(|e| format!("Hibernate failed: {e}"))?;
            }
            #[cfg(target_os = "linux")]
            {
                std::process::Command::new("systemctl")
                    .arg("hibernate")
                    .spawn()
                    .map_err(|e| format!("Hibernate failed: {e}"))?;
            }
            #[cfg(not(any(target_os = "windows", target_os = "linux")))]
            {
                return Err("Hibernate is not supported on this platform".to_string());
            }
        }
        _ => return Err(format!("Unknown power action: {action}")),
    }
    Ok(())
}

#[tauri::command]
pub async fn calculate_file_hash(path: String) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| format!("Failed to open file: {e}"))?;
    let mut buffer = vec![0u8; 1024 * 1024]; // 1MB buffer
    let mut hasher = Sha256::new();

    loop {
        let n = file
            .read(&mut buffer)
            .await
            .map_err(|e| format!("Read error: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Serialize)]
pub struct DirectAddressInfo {
    pub ip: String,
    pub port: u16,
}

#[tauri::command]
pub fn get_direct_listen_address(state: State<'_, Arc<AppState>>) -> Result<DirectAddressInfo, String> {
    Ok(DirectAddressInfo {
        ip: state.local_ip.clone(),
        port: state.listen_port,
    })
}

#[tauri::command]
pub async fn connect_peer_by_ip(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    ip: String,
    port: Option<u16>,
) -> Result<Contact, String> {
    use tokio::net::TcpStream;
    let target_port = port.unwrap_or(state.listen_port);
    let target_addr = format!("{}:{}", ip.trim(), target_port);

    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        TcpStream::connect(&target_addr),
    )
    .await
    .map_err(|_| format!("Connection to {target_addr} timed out"))?
    .map_err(|e| format!("Failed to connect to {target_addr}: {e}"))?;

    let (mut reader, mut writer) = tokio::io::split(stream);

    let my_avatar_hash = state.db.get_setting("avatar_hash").ok().flatten();
    let my_alias = state.settings.lock().map(|s| s.alias_name.clone()).unwrap_or_else(|_| "Raydrop".to_string());
    let ping = serde_json::json!({
        "type": "DIRECT_PING",
        "device_id": state.identity.device_id,
        "alias_name": my_alias,
        "os_type": std::env::consts::OS,
        "listen_port": state.listen_port,
        "avatar_hash": my_avatar_hash,
    });

    crate::network::transport::write_json_packet(&mut writer, &ping).await?;
    let resp: serde_json::Value = crate::network::transport::read_json_packet(&mut reader).await?;

    let peer_dev_id = resp.get("device_id").and_then(|v| v.as_str()).ok_or("Invalid response from peer")?.to_string();
    let peer_alias = resp.get("alias_name").and_then(|v| v.as_str()).unwrap_or("Raydrop").to_string();
    let peer_os = resp.get("os_type").and_then(|v| v.as_str()).map(|s| s.to_string());
    let peer_listen_port = resp.get("listen_port").and_then(|v| v.as_u64()).map(|p| p as u16).unwrap_or(target_port);
    let peer_avatar_hash = resp.get("avatar_hash").and_then(|v| v.as_str()).map(|s| s.to_string());

    let existing = state.db.get_contact(&peer_dev_id).ok().flatten();
    let contact = Contact {
        device_id: peer_dev_id,
        alias_name: peer_alias,
        custom_name: existing.as_ref().and_then(|e| e.custom_name.clone()),
        user_email: existing.as_ref().and_then(|e| e.user_email.clone()),
        user_name: existing.as_ref().and_then(|e| e.user_name.clone()),
        os_type: peer_os,
        is_own_device: false,
        ed25519_pubkey: None,
        network_type: "lan".to_string(),
        last_known_ip: Some(ip.trim().to_string()),
        last_known_port: Some(peer_listen_port),
        trust_level: existing.as_ref().map(|e| e.trust_level.clone()).unwrap_or_else(|| "ask".to_string()),
        is_online: true,
        last_seen_at: crate::db::storage::now_unix_secs(),
        created_at: crate::db::storage::now_unix_secs(),
        avatar_hash: peer_avatar_hash.clone().or_else(|| existing.as_ref().and_then(|e| e.avatar_hash.clone())),
    };

    state.db.upsert_contact(&contact)?;
    if peer_avatar_hash.is_some() {
        request_peer_avatar_internal(&state, &contact.device_id);
    }
    let _ = app.emit("contacts_updated", ());
    let _ = app.emit("lan_contacts_updated", ());

    Ok(contact)
}

// ----------------- Remote Desktop Control Commands -----------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoteDesktopSettings {
    pub enabled: bool,
    pub pin: String,
    pub unattended: bool,
}

#[tauri::command]
pub async fn get_remote_desktop_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<RemoteDesktopSettings, String> {
    let enabled_str = state.db.get_setting("remote_desktop_enabled").unwrap_or(None).unwrap_or_else(|| "true".to_string());
    let pin_opt = state.db.get_setting("remote_desktop_pin").unwrap_or(None);
    let unattended_str = state.db.get_setting("remote_desktop_unattended").unwrap_or(None).unwrap_or_else(|| "true".to_string());

    let pin = match pin_opt {
        Some(p) if !p.trim().is_empty() => p.trim().to_string(),
        _ => {
            // Generate a friendly 6-digit default PIN and persist it
            let gen_pin = format!("{:06}", (rand::random::<u32>() % 900000) + 100000);
            let _ = state.db.set_setting("remote_desktop_pin", &gen_pin);
            gen_pin
        }
    };

    Ok(RemoteDesktopSettings {
        enabled: enabled_str == "true" || enabled_str == "1",
        pin,
        unattended: unattended_str == "true" || unattended_str == "1",
    })
}

#[tauri::command]
pub async fn save_remote_desktop_settings(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
    pin: String,
    unattended: bool,
) -> Result<(), String> {
    let clean_pin = pin.trim();
    if clean_pin.is_empty() {
        return Err("PIN Remote Desktop tidak boleh kosong".to_string());
    }
    state.db.set_setting("remote_desktop_enabled", if enabled { "true" } else { "false" })?;
    state.db.set_setting("remote_desktop_pin", clean_pin)?;
    state.db.set_setting("remote_desktop_unattended", if unattended { "true" } else { "false" })?;
    Ok(())
}

#[tauri::command]
pub async fn verify_remote_desktop_pin(
    state: State<'_, Arc<AppState>>,
    pin: String,
) -> Result<bool, String> {
    let current = state.db.get_setting("remote_desktop_pin").unwrap_or(None).unwrap_or_default();
    Ok(pin.trim() == current.trim() && !current.trim().is_empty())
}

#[tauri::command]
pub async fn simulate_remote_mouse_move(
    norm_x: f32,
    norm_y: f32,
) -> Result<(), String> {
    crate::remote_input::simulate_mouse_move(norm_x, norm_y)
}

#[tauri::command]
pub async fn simulate_remote_mouse_click(
    button: String,
    action: String,
    alt: Option<bool>,
    ctrl: Option<bool>,
    shift: Option<bool>,
    meta: Option<bool>,
) -> Result<(), String> {
    crate::remote_input::simulate_mouse_click(&button, &action, alt, ctrl, shift, meta)
}

#[tauri::command]
pub async fn release_remote_modifiers() -> Result<(), String> {
    crate::remote_input::release_all_modifiers();
    Ok(())
}

#[tauri::command]
pub async fn simulate_remote_mouse_wheel(
    delta_y: i32,
) -> Result<(), String> {
    crate::remote_input::simulate_mouse_wheel(delta_y)
}

#[tauri::command]
pub async fn simulate_remote_keyboard_key(
    key: String,
    code: String,
    action: String,
) -> Result<(), String> {
    crate::remote_input::simulate_keyboard_key(&key, &code, &action)
}

#[tauri::command]
pub async fn simulate_remote_shortcut(
    shortcut: String,
) -> Result<(), String> {
    crate::remote_input::simulate_shortcut(&shortcut)
}

#[tauri::command]
pub async fn get_host_screen_size() -> Result<(u32, u32), String> {
    Ok(crate::remote_input::get_screen_size())
}

#[tauri::command]
pub async fn set_host_cursor_hidden(hidden: bool) -> Result<(), String> {
    crate::remote_input::set_host_cursor_hidden(hidden);
    Ok(())
}

#[tauri::command]
pub async fn get_host_monitors() -> Result<Vec<crate::remote_input::MonitorInfo>, String> {
    Ok(crate::remote_input::list_monitors())
}

#[tauri::command]
pub async fn set_host_target_monitor(monitor_index: Option<i32>) -> Result<(), String> {
    crate::remote_input::set_target_monitor(monitor_index)
}

#[tauri::command]
pub async fn execute_remote_power_action(action: String) -> Result<(), String> {
    crate::remote_input::execute_power_action(&action)
}

#[tauri::command]
pub async fn send_wake_on_lan(mac_address: String) -> Result<(), String> {
    let clean: String = mac_address
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();
    if clean.len() != 12 {
        return Err("Invalid MAC address. Expected 12 hex characters (e.g. AA:BB:CC:DD:EE:FF).".to_string());
    }
    let mut mac_bytes = [0u8; 6];
    for i in 0..6 {
        mac_bytes[i] = u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16)
            .map_err(|_| "Invalid hex byte in MAC address".to_string())?;
    }

    let mut packet = Vec::with_capacity(102);
    packet.extend_from_slice(&[0xFFu8; 6]);
    for _ in 0..16 {
        packet.extend_from_slice(&mac_bytes);
    }

    let socket = std::net::UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("Failed to bind UDP socket for WoL: {e}"))?;
    socket
        .set_broadcast(true)
        .map_err(|e| format!("Failed to enable UDP broadcast: {e}"))?;

    let _ = socket.send_to(&packet, "255.255.255.255:9");
    let _ = socket.send_to(&packet, "255.255.255.255:7");
    Ok(())
}

#[tauri::command]
pub async fn start_host_audio_loopback(app: AppHandle) -> Result<(), String> {
    crate::remote_audio::start_host_audio_loopback(app)
}

#[tauri::command]
pub async fn stop_host_audio_loopback() -> Result<(), String> {
    crate::remote_audio::stop_host_audio_loopback();
    Ok(())
}

#[tauri::command]
pub async fn start_second_screen_server(
    state: State<'_, Arc<AppState>>,
) -> Result<crate::network::second_screen::SecondScreenInfo, String> {
    crate::network::second_screen::start_second_screen_server(state.second_screen.clone()).await
}

#[tauri::command]
pub async fn stop_second_screen_server(
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.second_screen.stop();
    Ok(())
}

#[tauri::command]
pub async fn push_second_screen_frame(
    frame_base64: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.second_screen.push_frame_b64(&frame_base64);
    Ok(())
}

#[tauri::command]
pub async fn open_system_display_settings() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "ms-settings:display"])
            .spawn()
            .map_err(|e| format!("Failed to open Display Settings: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("/System/Library/PreferencePanes/Displays.prefPane")
            .spawn()
            .map_err(|e| format!("Failed to open Display Settings: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn start_native_screen_capture(app: AppHandle) -> Result<(), String> {
    crate::remote_capture::start_native_screen_capture(app)
}

#[tauri::command]
pub async fn stop_native_screen_capture() -> Result<(), String> {
    crate::remote_capture::stop_native_screen_capture();
    Ok(())
}

#[tauri::command]
pub async fn ack_native_screen_frame() -> Result<(), String> {
    crate::remote_capture::ack_native_screen_frame();
    Ok(())
}

#[tauri::command]
pub async fn is_secure_desktop_active() -> Result<bool, String> {
    Ok(crate::win_service::is_winlogon_or_system_agent())
}

#[tauri::command]
pub async fn check_unattended_boot_service() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let out = std::process::Command::new("schtasks")
            .args(["/Query", "/TN", "RaydropUnattendedHost"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        return Ok(out.map(|o| o.status.success()).unwrap_or(false));
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

#[tauri::command]
pub async fn install_unattended_boot_service() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe.to_string_lossy().replace('\'', "''");
        let task_cmd = format!(
            "/Create /TN \"RaydropUnattendedHost\" /TR \"\\\"{}\\\" --minimized\" /SC ONLOGON /RL HIGHEST /F",
            exe_str
        );
        let ps_cmd = format!(
            "Start-Process schtasks -ArgumentList '{}' -Verb RunAs -Wait",
            task_cmd.replace('\'', "''")
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps_cmd])
            .status()
            .map_err(|e| format!("Failed to elevate task scheduler: {e}"))?;
        if status.success() {
            Ok("Elevated Boot Service registered! Raydrop will auto-start with Admin/UIPI remote control privileges on logon.".to_string())
        } else {
            Err("Administrator permission was cancelled or failed.".to_string())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Unattended boot service registration is currently supported on Windows.".to_string())
    }
}

#[tauri::command]
pub async fn check_windows_service_status() -> Result<String, String> {
    Ok(crate::win_service::check_service_status())
}

#[tauri::command]
pub async fn install_windows_service() -> Result<String, String> {
    crate::win_service::install_service_elevated()
}

#[tauri::command]
pub async fn uninstall_windows_service() -> Result<String, String> {
    crate::win_service::uninstall_service_elevated()
}

