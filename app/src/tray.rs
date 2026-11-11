use crate::state::SessionState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Manager,
};

pub const TRAY_ID: &str = "main-tray";

pub fn create_tray(app: &AppHandle) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let open_item = MenuItem::with_id(app, "open", "Open Leeep Connect", true, None::<&str>)?;
    let stop_item = MenuItem::with_id(app, "stop", "Stop Active Session", true, None::<&str>)?;
    let exit_item = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open_item, &stop_item, &exit_item])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID);
    // The app icon from tauri.conf.json → bundle.icon
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder
        .tooltip("Leeep Connect: Ready")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "stop" => {
                let app_clone = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::stop_session_internal(&app_clone, Some("Stopped from tray menu")).await;
                });
            }
            "exit" => {
                // Never leave a live session running without the app (and its Stop button)
                let app_clone = app.clone();
                tauri::async_runtime::spawn(async move {
                    let active = {
                        let state = app_clone.state::<std::sync::Arc<crate::state::AppState>>();
                        let req = state.active_request_id.read().await;
                        req.is_some()
                    };
                    if active {
                        let _ = crate::stop_session_internal(&app_clone, Some("Leeep Connect was closed")).await;
                    }
                    app_clone.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    let is_visible = win.is_visible().unwrap_or(false);
                    if is_visible {
                        let _ = win.hide();
                    } else {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    Ok(tray)
}

pub fn update_tray_status(app: &AppHandle, state: &SessionState) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let tooltip = match state {
            SessionState::Idle => "Leeep Connect: Ready".to_string(),
            SessionState::Requesting => "Leeep Connect: Connecting to server...".to_string(),
            SessionState::Waiting { queue_position, .. } => {
                if let Some(pos) = queue_position {
                    format!("Leeep Connect: Waiting in queue (Pos #{})", pos)
                } else {
                    "Leeep Connect: Waiting for support agent...".to_string()
                }
            }
            SessionState::AgentJoining { agent_name, .. } => {
                format!("Leeep Connect: Agent {} is joining...", agent_name)
            }
            SessionState::InSession { agent_name, .. } => {
                format!("Leeep Connect: In Session with {}", agent_name)
            }
            SessionState::Ended { .. } => "Leeep Connect: Session Ended".to_string(),
        };
        let _ = tray.set_tooltip(Some(tooltip));
    }
}
