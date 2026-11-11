use crate::state::SessionState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Manager,
};

pub const TRAY_ID: &str = "main-tray";

pub fn create_tray(app: &AppHandle) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    // Minimal presence menu. Leeep Connect has no client-facing window and no
    // client session controls; a session is driven from the operator console.
    // "Exit" stays so the app can always be quit from the machine it runs on.
    let exit_item = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&exit_item])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID);
    // The app icon from tauri.conf.json → bundle.icon
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder
        .tooltip("Leeep Connect: Ready")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "exit" => {
                // Ending the central request on exit keeps central's view tidy;
                // it does not touch the remote-control agent, which is a
                // separate Windows service.
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
