// Prevents additional console window on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use leeep_connect::{
    handle_deep_link, logging, state::AppState, tray,
};
use std::env;
use tauri::WindowEvent;

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
            // Background agent: a second launch with no deep link does nothing
            // visible. There is no client-facing window to raise.
        }))
        .invoke_handler(leeep_connect::invoke_handler())
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

            // 4. Handle an initial deep link if launched directly via the protocol.
            // This runs headless: the window is never shown. Leeep Connect is a
            // silent background agent; a support session is started and driven
            // entirely from the operator console, not from any client-side UI.
            let args: Vec<String> = env::args().collect();
            for arg in &args[1..] {
                if arg.starts_with("leeep-connect://") {
                    handle_deep_link(handle, arg);
                    break;
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
