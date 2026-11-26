// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "windows")]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--service") {
            raydrop_lib::win_service::run_windows_service();
            return;
        }

        if args.iter().any(|a| a == "--system-agent") {
            std::env::set_var("RAYDROP_DATA_DIR", r"C:\ProgramData\Raydrop\Data");
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", r"C:\ProgramData\Raydrop\WebView2");
        }

        let current_args = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default();
        let capture_switches = "--enable-usermedia-screen-capturing --auto-select-desktop-capture-source=\"Entire screen\" --use-fake-ui-for-media-stream --autoplay-policy=no-user-gesture-required";
        if !current_args.contains("auto-select-desktop-capture-source") {
            let combined = if current_args.is_empty() {
                capture_switches.to_string()
            } else {
                format!("{} {}", current_args, capture_switches)
            };
            std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", combined);
        }
    }

    raydrop_lib::run()
}

