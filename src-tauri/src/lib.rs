pub mod ai_agent;
pub mod browser_bridge;
pub mod commands;
pub mod crypto;
pub mod db;
pub mod folder_sync;
pub mod http_downloader;
pub mod image_tools;
pub mod llm_bridge;
pub mod mcp_library;
pub mod mcp_server;
pub mod media_converter;
pub mod media_player;
pub mod network;
pub mod os;
pub mod remote_audio;
pub mod remote_capture;
pub mod remote_fs;
pub mod remote_input;
pub mod skills;
pub mod state;
pub mod system_monitor;
pub mod telegram_bot;
pub mod torrent;
pub mod tray_badge;
pub mod win_service;

use crate::db::storage::Database;
use crate::network::{
    mdns::start_mdns_daemon, signaling::start_signaling_daemon, transport::start_transport_listeners,
};
use crate::state::AppState;
use std::collections::HashMap;
use std::net::TcpListener as StdTcpListener;
use std::sync::{atomic::AtomicBool, Arc, Mutex};
#[cfg(desktop)]
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
#[allow(unused_imports)]
use tauri::{Emitter, Manager, WindowEvent};
#[allow(unused_imports)]
use tracing::info;

fn extract_send_paths(args: &[String]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut after_send = false;
    for arg in args {
        if after_send {
            if !arg.starts_with("--") {
                paths.push(arg.clone());
            }
        } else if arg == "--send" {
            after_send = true;
        }
    }
    paths
}

fn extract_convert_paths(args: &[String]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut after_convert = false;
    for arg in args {
        if after_convert {
            if !arg.starts_with("--") {
                paths.push(arg.clone());
            }
        } else if arg == "--convert" {
            after_convert = true;
        }
    }
    paths
}

fn extract_magnet_url(args: &[String]) -> Option<String> {
    for arg in args {
        let trimmed = arg.trim();
        if trimmed.starts_with("magnet:?") {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn allocate_dynamic_port() -> u16 {
    StdTcpListener::bind("0.0.0.0:0")
        .and_then(|s| s.local_addr())
        .map(|a| a.port())
        .unwrap_or(53317)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "windows")]
    {
        let current_args = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default();
        let background_and_capture_switches = [
            "--enable-usermedia-screen-capturing",
            "--auto-select-desktop-capture-source=\"Entire screen\"",
            "--use-fake-ui-for-media-stream",
            "--autoplay-policy=no-user-gesture-required",
            "--disable-background-timer-throttling",
            "--disable-backgrounding-occluded-windows",
            "--disable-renderer-backgrounding",
            "--disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling,ThrottleDisplayNoneAndVisibilityHiddenCrossOriginIframes,WebRtcHideLocalIpsWithMdns",
        ];
        let mut combined = current_args;
        for switch in &background_and_capture_switches {
            let key = switch.split('=').next().unwrap_or(switch);
            if !combined.contains(key) {
                if !combined.is_empty() {
                    combined.push(' ');
                }
                combined.push_str(switch);
            }
        }
        std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", combined);
    }

    tracing_subscriber::fmt::init();

    let initial_args: Vec<String> = std::env::args().collect();
    let initial_send_paths = extract_send_paths(&initial_args);
    let initial_convert_paths = extract_convert_paths(&initial_args);
    let initial_magnet = extract_magnet_url(&initial_args);
    let is_system_agent = initial_args.iter().any(|a| a == "--system-agent");
    let start_minimized = is_system_agent
        || (initial_args.iter().any(|a| a == "--minimized")
            && initial_send_paths.is_empty()
            && initial_convert_paths.is_empty());

    let builder = tauri::Builder::default();

    #[cfg(desktop)]
    let builder = if !is_system_agent {
        builder.plugin(tauri_plugin_single_instance::init(move |app, argv, _cwd| {
        let paths = extract_send_paths(&argv);
        let convert_paths = extract_convert_paths(&argv);
        let magnet = extract_magnet_url(&argv);

        crate::network::transport::popup_and_focus_main_window(app);

        if let Some(state_ref) = app.try_state::<Arc<AppState>>() {
            if let Some(m) = magnet {
                info!("Received magnet URL from secondary instance: {}", m);
                if let Ok(mut pending) = state_ref.pending_magnet_link.lock() {
                    *pending = Some(m.clone());
                }
                let _ = app.emit("open_magnet_link", m);
            }

            if !paths.is_empty() {
                info!("Received CLI --send paths from secondary instance: {:?}", paths);
                if let Ok(mut q) = state_ref.queued_cli_files.lock() {
                    q.extend(paths.clone());
                }
                let _ = app.emit("cli_send_files", paths);
            }

            if !convert_paths.is_empty() {
                info!("Received CLI --convert paths from secondary instance: {:?}", convert_paths);
                if let Ok(mut q) = state_ref.queued_convert_files.lock() {
                    q.extend(convert_paths.clone());
                }
                let _ = app.emit("cli_convert_files", convert_paths);
            }
        }
    }))
    } else {
        builder
    };

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            let handle = app.handle().clone();

            let (db, identity, settings) =
                Database::init().expect("Failed to initialize Raydrop SQLite database");

            let local_ip = local_ip_address::local_ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|_| "127.0.0.1".to_string());
            let listen_port = allocate_dynamic_port();

            let torrent_mgr = Arc::new(crate::torrent::TorrentManager::new(
                std::path::PathBuf::from(&settings.download_dir),
            ));

            let http_mgr = Arc::new(crate::http_downloader::HttpDownloadManager::new(
                std::path::PathBuf::from(&settings.download_dir),
                settings.auto_categorize,
                settings.max_concurrent_downloads,
            ));

            let folder_sync_mgr = Arc::new(crate::folder_sync::FolderSyncManager::new());

            let persisted_media_player: crate::media_player::MediaPlayerState = db
                .get_setting("rayamp_persisted_state")
                .ok()
                .flatten()
                .and_then(|s| serde_json::from_str::<crate::media_player::MediaPlayerState>(&s).ok())
                .map(|mut m| {
                    m.is_playing = false;
                    m
                })
                .unwrap_or_default();

            let app_state = Arc::new(AppState {
                db: Arc::new(db),
                identity,
                settings: Mutex::new(settings),
                local_ip,
                listen_port,
                wan_reflexive_addr: Mutex::new(None),
                signaling_connected: AtomicBool::new(false),
                signaling_tx: Mutex::new(None),
                quic_endpoint: Mutex::new(None),
                mesh_connections: Mutex::new(HashMap::new()),
                relay_streams: Mutex::new(HashMap::new()),
                pending_requests: Mutex::new(HashMap::new()),
                active_cancellations: Mutex::new(HashMap::new()),
                pending_wan_candidates: Mutex::new(HashMap::new()),
                queued_cli_files: Mutex::new(initial_send_paths.clone()),
                queued_convert_files: Mutex::new(initial_convert_paths.clone()),
                active_web_share: Mutex::new(None),
                torrent_manager: torrent_mgr,
                pending_magnet_link: Mutex::new(initial_magnet.clone()),
                http_downloader: http_mgr,
                folder_sync_watcher: folder_sync_mgr.clone(),
                pending_fs_requests: Mutex::new(HashMap::new()),
                second_screen: Arc::new(crate::network::second_screen::SecondScreenState::new()),
                media_player: Arc::new(Mutex::new(persisted_media_player)),
                mcp_server: Arc::new(crate::mcp_server::McpServerManager::default()),
                telegram_bot: Arc::new(crate::telegram_bot::TelegramBotManager::default()),
                llm_bridge: Arc::new(crate::llm_bridge::LlmBridgeManager::default()),
            });

            let state_for_setup = app_state.clone();
            app.manage(app_state);

            #[cfg(desktop)]
            if !is_system_agent {
                // Build System Tray Menu (user desktop instance only, never for headless --system-agent)
                let is_disc = state_for_setup.is_discoverable();
                let is_autostart = state_for_setup
                    .settings
                    .lock()
                    .map(|s| s.autostart)
                    .unwrap_or(true);
                let toggle_label = if is_disc {
                    "Status: Visible (Online)"
                } else {
                    "Status: Hidden (Offline)"
                };
                let autostart_label = if is_autostart {
                    "Auto-Start at Boot: ON"
                } else {
                    "Auto-Start at Boot: OFF"
                };
                let toggle_item =
                    MenuItem::with_id(app, "toggle_disc", toggle_label, true, None::<&str>)?;
                let autostart_item =
                    MenuItem::with_id(app, "toggle_autostart", autostart_label, true, None::<&str>)?;
                let open_item =
                    MenuItem::with_id(app, "open_dashboard", "Open Dashboard", true, None::<&str>)?;
                let quit_item = MenuItem::with_id(app, "quit", "Quit Raydrop", true, None::<&str>)?;
                let menu = Menu::with_items(
                    app,
                    &[&toggle_item, &autostart_item, &open_item, &quit_item],
                )?;

                let state_tray = state_for_setup.clone();
                let toggle_item_clone = toggle_item.clone();
                let autostart_item_clone = autostart_item.clone();
                let state_menu = state_tray.clone();

                let mut tray_builder = TrayIconBuilder::with_id("main_tray")
                    .menu(&menu)
                    .tooltip("Raydrop — Universal P2P File Transfer")
                    .on_menu_event(move |app_handle, event| match event.id.as_ref() {
                        "toggle_disc" => {
                            let next = !state_menu.is_discoverable();
                            let _ = state_menu
                                .db
                                .set_setting("discoverable", if next { "true" } else { "false" });
                            if let Ok(mut s) = state_menu.settings.lock() {
                                s.discoverable = next;
                            }
                            if !next {
                                *state_menu.signaling_tx.lock().unwrap() = None;
                                state_menu
                                    .signaling_connected
                                    .store(false, std::sync::atomic::Ordering::Relaxed);
                                let _ = app_handle.emit("signaling_status_changed", false);
                            }
                            let new_label = if next {
                                "Status: Visible (Online)"
                            } else {
                                "Status: Hidden (Offline)"
                            };
                            let _ = toggle_item_clone.set_text(new_label);
                            let _ = app_handle.emit("discoverability_changed", next);
                        }
                        "toggle_autostart" => {
                            let current = state_menu
                                .settings
                                .lock()
                                .map(|s| s.autostart)
                                .unwrap_or(true);
                            let next = !current;
                            let _ = crate::os::set_autostart(next);
                            let _ = state_menu
                                .db
                                .set_setting("autostart", if next { "true" } else { "false" });
                            if let Ok(mut s) = state_menu.settings.lock() {
                                s.autostart = next;
                            }
                            let new_label = if next {
                                "Auto-Start at Boot: ON"
                            } else {
                                "Auto-Start at Boot: OFF"
                            };
                            let _ = autostart_item_clone.set_text(new_label);
                            let _ = app_handle.emit("autostart_changed", next);
                        }
                        "open_dashboard" => {
                            if let Some(w) = app_handle.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => {
                            if let Some(w) = app_handle.get_webview_window("main") {
                                let _ = w.destroy();
                            }
                            crate::remote_input::set_host_cursor_hidden(false);
                            crate::remote_audio::stop_host_audio_loopback();
                            crate::remote_capture::stop_native_screen_capture();
                            if let Some(state_ref) = app_handle.try_state::<Arc<AppState>>() {
                                state_ref.db.checkpoint_wal();
                            }
                            app_handle.exit(0);
                            std::process::exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app_handle = tray.app_handle();
                            if let Some(w) = app_handle.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    });

                if let Ok(tray_img) =
                    tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))
                {
                    tray_builder = tray_builder.icon(tray_img);
                } else if let Some(icon) = app.default_window_icon() {
                    tray_builder = tray_builder.icon(icon.clone());
                }
                let _ = tray_builder.build(app)?;

                // Run tray badge sync & OS shell/registry registrations off the main UI thread
                // so the window message loop never shows "Not Responding" on startup.
                let handle_os_init = handle.clone();
                let state_os_init = state_tray.clone();
                std::thread::spawn(move || {
                    if let Ok(counts) = state_os_init.db.get_unread_chat_counts() {
                        let total: u32 = counts.values().sum();
                        crate::tray_badge::update_tray_unread_badge(&handle_os_init, total);
                    }

                    match crate::os::register_context_menu() {
                        Ok(msg) => info!("{msg}"),
                        Err(e) => info!("OS Context Menu registration skipped: {e}"),
                    }

                    match crate::os::register_magnet_protocol() {
                        Ok(msg) => info!("{msg}"),
                        Err(e) => info!("Magnet protocol registration skipped: {e}"),
                    }

                    if is_autostart {
                        match crate::os::set_autostart(true) {
                            Ok(msg) => info!("{msg}"),
                            Err(e) => info!("OS Autostart registration skipped: {e}"),
                        }
                    }
                });

                // Background Cross-Platform Clipboard History Watcher (Windows, macOS, Linux)
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                {
                    let app_cb = handle.clone();
                    let state_cb = state_for_setup.clone();
                    std::thread::spawn(move || {
                        let mut last_seq: u64 = 0;
                        let mut last_hash = String::new();
                        loop {
                            std::thread::sleep(std::time::Duration::from_millis(900));
                            let seq = commands::get_clipboard_sequence_number();
                            if seq != 0 && seq == last_seq {
                                continue;
                            }
                            if seq != 0 {
                                last_seq = seq;
                            }
                            if let Ok(content) = commands::read_clipboard_content() {
                                if content.trim().is_empty() || content.len() > 6_000_000 {
                                    continue;
                                }
                                let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
                                if hash == last_hash {
                                    continue;
                                }
                                last_hash = hash;
                                if let Ok(true) = state_cb.db.record_clipboard_entry(&content) {
                                    let _ = app_cb.emit("clipboard_history_updated", ());
                                }
                            }
                        }
                    });
                }
            }

            // Show & focus main window on startup (desktop only)
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            if let Some(main_win) = app.get_webview_window("main") {
                if let Ok(win_img) =
                    tauri::image::Image::from_bytes(include_bytes!("../icons/128x128.png"))
                {
                    let _ = main_win.set_icon(win_img);
                }
                if start_minimized {
                    let _ = main_win.hide();
                } else {
                    let _ = main_win.show();
                    let _ = main_win.unminimize();
                    let _ = main_win.set_focus();
                    if !initial_send_paths.is_empty() || initial_magnet.is_some() {
                        crate::network::transport::popup_and_focus_main_window(&handle);
                    }
                }
                let win_clone = main_win.clone();
                main_win.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win_clone.hide();
                    }
                });
            }

            // Background Periodic Torrent Stats Broadcast
            let handle_for_torrents = handle.clone();
            let state_for_torrents = state_for_setup.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(1200));
                let mut recorded_completed = std::collections::HashSet::<String>::new();
                let mut initialized = false;
                loop {
                    interval.tick().await;
                    let tasks = state_for_torrents.torrent_manager.list_torrents().await;
                    if !initialized {
                        for task in &tasks {
                            if task.status == "completed" {
                                recorded_completed.insert(task.id.clone());
                            }
                        }
                        initialized = true;
                    }
                    if !tasks.is_empty() {
                        for task in &tasks {
                            if task.status == "completed" && !recorded_completed.contains(&task.id) {
                                recorded_completed.insert(task.id.clone());
                                crate::network::transport::notify_os(
                                    &handle_for_torrents,
                                    "Torrent Download Complete",
                                    &format!(
                                        "{} ({}) has finished downloading.",
                                        task.name,
                                        crate::network::transport::format_bytes_human(task.total_bytes)
                                    ),
                                );
                            }
                        }
                        let _ = handle_for_torrents.emit("torrent_stats_update", &tasks);
                    }
                }
            });

            // Background Periodic HTTP Download Stats Broadcast
            let handle_for_http = handle.clone();
            let state_for_http = state_for_setup.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
                let mut recorded_completed = std::collections::HashSet::<String>::new();
                loop {
                    interval.tick().await;
                    let tasks = state_for_http.http_downloader.list_tasks().await;
                    if !tasks.is_empty() {
                        for task in &tasks {
                            if task.status == "completed" && !recorded_completed.contains(&task.id) {
                                recorded_completed.insert(task.id.clone());
                                let rec = crate::db::storage::TransferRecord {
                                    id: format!("http_{}", task.id),
                                    peer_device_id: "http_accelerator".to_string(),
                                    peer_alias: "HTTP Accelerator".to_string(),
                                    direction: "incoming".to_string(),
                                    network_route: "wan_p2p".to_string(),
                                    total_files: 1,
                                    total_bytes: task.total_bytes,
                                    transferred_bytes: task.downloaded_bytes,
                                    status: "completed".to_string(),
                                    target_path: Some(task.save_path.clone()),
                                    source_paths: Some(
                                        serde_json::to_string(&vec![&task.save_path])
                                            .unwrap_or_default(),
                                    ),
                                    error_reason: None,
                                    created_at: crate::db::storage::now_unix_secs(),
                                };
                                let _ = state_for_http.db.upsert_transfer(&rec);
                                let _ = handle_for_http.emit("transfer_state_changed", &rec);
                                crate::network::transport::notify_os(
                                    &handle_for_http,
                                    "Download Complete",
                                    &format!(
                                        "{} ({}) has finished downloading.",
                                        task.filename,
                                        crate::network::transport::format_bytes_human(
                                            task.total_bytes.max(task.downloaded_bytes)
                                        )
                                    ),
                                );
                            }
                        }
                        let _ = handle_for_http.emit("http_download_stats_update", &tasks);
                    }
                }
            });

            // Start Background Networking Daemons (TCP + QUIC + mDNS + WAN Signaling/STUN)
            start_transport_listeners(handle.clone(), state_for_setup.clone());
            start_mdns_daemon(handle.clone(), state_for_setup.clone());
            start_signaling_daemon(handle.clone(), state_for_setup.clone());

            // Initialize Continuous Real-Time Folder Mirroring (Background Sync) off main thread
            let handle_sync = handle.clone();
            let state_sync = state_for_setup.clone();
            let fsm = folder_sync_mgr.clone();
            std::thread::spawn(move || {
                fsm.init_all_auto_tasks(handle_sync, state_sync);
            });

            // Start Local Loopback Server for Chrome/Edge Browser Extension (IDM mode, user session only)
            if !is_system_agent {
                let ai_cfg = crate::ai_agent::load_ai_mcp_config(&state_for_setup.db);
                crate::mcp_server::restart_mcp_server(
                    handle.clone(),
                    state_for_setup.clone(),
                    ai_cfg.mcp_enabled,
                    ai_cfg.mcp_port,
                    ai_cfg.mcp_auth_token.clone(),
                );
                crate::telegram_bot::restart_telegram_bot(handle.clone(), state_for_setup.clone());
                #[cfg(desktop)]
                {
                    let handle_ytdlp = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        let _ = crate::media_player::ensure_ytdlp_installed(Some(&handle_ytdlp)).await;
                    });
                }
                let handle_llm = handle.clone();
                let state_llm = state_for_setup.clone();
                tauri::async_runtime::spawn(async move {
                    state_llm.llm_bridge.init_from_db(&handle_llm, &state_llm.db);
                });
                browser_bridge::start_browser_bridge_daemon(handle, state_for_setup);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            browser_bridge::run_web_speedtest,
            llm_bridge::get_llm_bridge_status,
            llm_bridge::update_llm_bridge_provider,
            llm_bridge::refresh_llm_bridge_models,
            llm_bridge::test_llm_bridge_provider,
            llm_bridge::get_llm_bridge_network_info,
            llm_bridge::discover_peer_llm_bridges,
            llm_bridge::get_peer_llm_proxies,
            llm_bridge::start_peer_llm_proxy,
            llm_bridge::stop_peer_llm_proxy,
            mcp_library::mcp_library_list,
            mcp_library::mcp_library_supported_agents,
            mcp_library::mcp_library_inspect,
            mcp_library::mcp_library_save,
            mcp_library::mcp_library_delete,
            mcp_library::mcp_library_test,
            mcp_library::mcp_library_toggle_bot,
            mcp_library::mcp_library_install_agent,
            mcp_library::mcp_library_uninstall_agent,
            mcp_library::mcp_library_popular_presets,
            commands::get_app_state,
            commands::save_cloud_session,
            commands::logout_cloud_session,
            commands::sync_cloud_data_to_local,
            commands::delete_local_contact,
            commands::toggle_discoverability,
            commands::toggle_autostart,
            commands::update_settings,
            commands::set_ui_theme,
            commands::get_contacts,
            commands::add_wan_contact,
            commands::update_contact_trust,
            commands::initiate_transfer,
            commands::resume_transfer,
            commands::pause_transfer,
            commands::respond_transfer_request,
            commands::cancel_transfer,
            commands::get_transfer_history,
            commands::delete_transfer_record,
            commands::clear_transfer_history,
            commands::register_os_context_menu,
            commands::open_file_path,
            commands::open_folder_path,
            commands::get_win11_direct_menu,
            commands::toggle_win11_direct_menu,
            commands::check_app_update,
            commands::download_and_install_update,
            commands::send_chat_message,
            commands::get_chat_messages,
            commands::mark_chat_messages_read,
            commands::clear_chat_messages,
            commands::get_chat_thread_peers,
            commands::get_unread_chat_counts,
            commands::create_chat_group,
            commands::get_chat_groups,
            commands::get_chat_group,
            commands::update_chat_group_details,
            commands::invite_group_members,
            commands::remove_group_member,
            commands::set_member_role,
            commands::set_group_send_permission,
            commands::leave_chat_group,
            commands::send_group_chat_message,
            commands::update_tray_badge,
            commands::open_external_url,
            commands::get_file_thumbnail,
            commands::start_web_share,
            commands::stop_web_share,
            commands::read_system_clipboard,
            commands::read_system_clipboard_if_changed,
            commands::get_clipboard_sequence_number,
            commands::list_clipboard_history,
            commands::copy_clipboard_history_item,
            commands::toggle_pin_clipboard_item,
            commands::delete_clipboard_history_item,
            commands::clear_clipboard_history,
            commands::list_notes,
            commands::save_note,
            commands::toggle_pin_note,
            commands::toggle_archive_note,
            commands::delete_note,
            commands::write_system_clipboard,
            commands::save_base64_image,
            commands::broadcast_clipboard,
            commands::set_speed_limit,
            commands::list_folder_sync_tasks,
            commands::add_folder_sync_task,
            commands::toggle_folder_sync_auto,
            commands::delete_folder_sync_task,
            commands::trigger_folder_sync,
            commands::add_torrent_magnet,
            commands::list_torrent_tasks,
            commands::pause_torrent_task,
            commands::resume_torrent_task,
            commands::delete_torrent_task,
            commands::get_pending_magnet_link,
            commands::add_http_download,
            commands::list_http_downloads,
            commands::pause_http_download,
            commands::resume_http_download,
            commands::update_http_download_url,
            commands::delete_http_download,
            commands::open_browser_extension_dir,
            commands::open_browser_extension_url,
            commands::set_my_avatar,
            commands::get_my_avatar,
            commands::get_peer_avatar,
            commands::set_contact_custom_name,
            commands::start_http_download_now,
            commands::execute_system_power_action,
            commands::calculate_file_hash,
            commands::get_direct_listen_address,
            commands::connect_peer_by_ip,
            commands::list_shared_folders,
            commands::add_shared_folder,
            commands::delete_shared_folder,
            commands::browse_remote_shares,
            commands::browse_remote_dir,
            commands::pull_remote_file,
            commands::send_meeting_signal,
            commands::request_peer_avatar,
            commands::get_remote_desktop_settings,
            commands::save_remote_desktop_settings,
            commands::verify_remote_desktop_pin,
            commands::simulate_remote_mouse_move,
            commands::simulate_remote_mouse_move_relative,
            commands::simulate_remote_gamepad,
            commands::request_direct_mesh,
            commands::respond_direct_mesh,
            commands::revoke_direct_mesh,
            commands::get_lan_connection_statuses,
            commands::get_virtual_display_status,
            commands::set_virtual_display_enabled,
            commands::set_client_keyboard_lock,
            commands::simulate_remote_mouse_click,
            commands::simulate_remote_mouse_wheel,
            commands::simulate_remote_keyboard_key,
            commands::simulate_remote_shortcut,
            commands::release_remote_modifiers,
            commands::get_host_screen_size,
            commands::set_host_cursor_hidden,
            commands::get_host_monitors,
            commands::set_host_target_monitor,
            commands::get_monitor_resolutions,
            commands::set_monitor_resolution,
            commands::get_extend_platform_info,
            commands::enable_linux_virtual_display,
            commands::disable_linux_virtual_display,
            commands::execute_remote_power_action,
            commands::send_wake_on_lan,
            commands::send_wake_on_lan_auto,
            commands::start_host_audio_loopback,
            commands::stop_host_audio_loopback,
            commands::start_second_screen_server,
            commands::set_second_screen_source,
            commands::stop_second_screen_server,
            commands::push_second_screen_frame,
            commands::record_activity_tick,
            commands::open_system_display_settings,
            commands::start_native_screen_capture,
            commands::stop_native_screen_capture,
            commands::ack_native_screen_frame,
            commands::is_secure_desktop_active,
            commands::check_unattended_boot_service,
            commands::install_unattended_boot_service,
            commands::check_windows_service_status,
            commands::install_windows_service,
            commands::uninstall_windows_service,
            commands::get_system_telemetry,
            commands::get_transfer_analytics,
            commands::get_image_metadata,
            commands::convert_and_process_images,
            commands::generate_mobile_icons_zip,
            commands::detect_document_engines,
            commands::convert_documents,
            commands::media_search_ytmusic,
            commands::media_resolve_stream,
            commands::media_download_ytmusic,
            commands::media_get_state,
            commands::media_sync_frontend_state,
            commands::media_dispatch_command,
            commands::media_list_playlists,
            commands::media_create_playlist,
            commands::media_rename_playlist,
            commands::media_delete_playlist,
            commands::media_list_playlist_items,
            commands::media_add_playlist_item,
            commands::media_remove_playlist_item,
            commands::media_reorder_playlist_items,
            commands::get_ai_mcp_config,
            commands::save_ai_mcp_config,
            commands::get_ai_mcp_runtime_status,
            commands::test_llm_connection_cmd,
            commands::verify_telegram_token_cmd,
            commands::detect_media_converter_engine,
            commands::convert_media_files,
            skills::commands::skills_init_library,
            skills::commands::skills_search,
            skills::commands::skills_get_detail,
            skills::commands::skills_get_files,
            skills::commands::skills_preview_file,
            skills::commands::skills_import_local_folder,
            skills::commands::skills_import_zip,
            skills::commands::skills_export_zip,
            skills::commands::skills_discover_github,
            skills::commands::skills_import_github,
            skills::commands::skills_delete,
            skills::commands::skills_toggle_favorite,
            skills::commands::skills_list_sources,
            skills::commands::skills_add_source,
            skills::commands::skills_toggle_source,
            skills::commands::skills_refresh_source,
            skills::commands::skills_remove_source,
            skills::commands::skills_detect_agents,
            skills::commands::skills_get_agents,
            skills::commands::skills_configure_agent_path,
            skills::commands::skills_validate_agent_path,
            skills::commands::skills_get_compatibility,
            skills::commands::skills_preview_install,
            skills::commands::skills_install,
            skills::commands::skills_list_installed,
            skills::commands::skills_scan_project_folder,
            skills::commands::skills_list_scanned_projects,
            skills::commands::skills_remove_scanned_project,
            skills::commands::skills_uninstall,
            skills::commands::skills_update_check,
            skills::commands::skills_rollback,
            skills::commands::skills_get_install_history,
            skills::commands::skills_list_presets,
            skills::commands::skills_save_preset,
            skills::commands::skills_delete_preset,
            skills::commands::skills_get_settings,
            skills::commands::skills_update_settings,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Raydrop Tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                crate::remote_input::set_host_cursor_hidden(false);
                crate::remote_input::restore_client_gamebar_setting();
                crate::remote_audio::stop_host_audio_loopback();
                crate::remote_capture::stop_native_screen_capture();
                if let Some(state_ref) = app_handle.try_state::<Arc<AppState>>() {
                    state_ref.db.checkpoint_wal();
                }
            }
        });
}

