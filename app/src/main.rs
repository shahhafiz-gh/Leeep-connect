// Prevents additional console window on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use leeep_connect::{
    handle_deep_link, logging, state::AppState, tray,
};
use std::env;
use tauri::{Manager, WindowEvent};

fn main() {
    // 1. Initialize safe rotating logging (daily rotation, token-redacted)
    if let Err(e) = logging::setup_logging() {
        eprintln!("Failed to initialize logging: {}", e);
    }

    tracing::info!("Starting Leeep Connect Client");

    let app_state = AppState::new();

    let builder = tauri::Builder::default()
        .manage(app_state.clone())
        // 2. Single-instance plugin: forwards secondary launches to active instance
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            tracing::info!(args_count = argv.len(), "Deep link forwarded from second instance");
            for arg in &argv {
                if arg.starts_with("leeep-connect://") {
                    handle_deep_link(app, arg);
                    return;
                }
            }
            // If launched without specific deep link, just bring window to focus
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![
            leeep_connect::get_state,
            leeep_connect::start_claim,
            leeep_connect::stop_session,
            leeep_connect::minimize_to_tray,
        ])
        .setup(|app| {
            let handle = app.handle();

            // 3. Setup system tray
            if let Err(e) = tray::create_tray(handle) {
                tracing::error!(error = ?e, "Failed to create tray icon");
            }

            // 3b. Auto-update (release builds with a real updater key only):
            // first check a minute after start, then every 6 hours, never mid-session.
            if !cfg!(debug_assertions) && leeep_connect::updater_configured(handle) {
                handle.plugin(tauri_plugin_updater::Builder::new().build())?;
                let updater_handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    loop {
                        leeep_connect::check_for_updates(updater_handle.clone()).await;
                        tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
                    }
                });
            }

            // 4. Handle initial command-line arguments (if launched directly via protocol)
            let args: Vec<String> = env::args().collect();
            let mut launched_via_deep_link = false;
            let mut start_minimized = false;

            for arg in &args[1..] {
                if arg.starts_with("leeep-connect://") {
                    handle_deep_link(handle, arg);
                    launched_via_deep_link = true;
                    break;
                } else if arg == "--minimized" || arg == "--tray" {
                    start_minimized = true;
                }
            }

            // If not launched minimized, or if deep-link was received, show main window
            if !start_minimized || launched_via_deep_link {
                if let Some(win) = handle.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // Minimize to tray on close button click instead of quitting
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        });

    builder
        .run(tauri::generate_context!())
        .expect("Error while running Leeep Connect application");
}
