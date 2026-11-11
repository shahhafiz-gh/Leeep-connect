pub mod api;
pub mod logging;
pub mod state;
pub mod tray;

use api::{api_claim, api_end, api_status, get_mesh_node_id, parse_deep_link};
use state::{AppState, SessionState};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::watch;

/// The IPC handler for the window's commands, registered by main.rs.
/// Tauri commands defined in lib.rs can't be `pub` (the macro's generated glue
/// would be defined twice, error E0255), so they stay private and are exposed
/// through this one function instead.
pub fn invoke_handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![get_state, start_claim, stop_session, minimize_to_tray]
}

/// Get current session state
#[tauri::command]
async fn get_state(state: State<'_, Arc<AppState>>) -> Result<SessionState, String> {
    let s = state.state.read().await;
    Ok(s.clone())
}

/// Triggered by frontend or deep link to claim a session token
#[tauri::command]
async fn start_claim(token: String, app: AppHandle) -> Result<(), String> {
    let app_state = app.state::<Arc<AppState>>().inner().clone();
    
    // Set requesting state
    {
        let mut s = app_state.state.write().await;
        *s = SessionState::Requesting;
    }
    emit_state_change(&app, &SessionState::Requesting);
    tray::update_tray_status(&app, &SessionState::Requesting);

    let node_id = get_mesh_node_id();
    match api_claim(&token, node_id).await {
        Ok(claim_res) => {
            tracing::info!(request_id = %claim_res.request_id, "Session claimed successfully");
            
            let request_id = claim_res.request_id.clone();
            let device_secret = claim_res.device_secret.clone();

            // Store in memory only!
            {
                let mut req_lock = app_state.active_request_id.write().await;
                *req_lock = Some(request_id.clone());
                let mut sec_lock = app_state.device_secret.write().await;
                *sec_lock = Some(device_secret.clone());
            }

            let initial_waiting = SessionState::Waiting {
                request_id: request_id.clone(),
                queue_position: None,
                wait_seconds: 0,
            };

            {
                let mut s = app_state.state.write().await;
                *s = initial_waiting.clone();
            }
            emit_state_change(&app, &initial_waiting);
            tray::update_tray_status(&app, &initial_waiting);

            // Spawn polling loop
            let (tx, rx) = watch::channel(false);
            {
                let mut tx_guard = app_state.stop_tx.write().await;
                *tx_guard = Some(tx);
            }

            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                run_polling_loop(app_clone, request_id, device_secret, rx).await;
            });

            Ok(())
        }
        Err(e) => {
            tracing::error!(error = %e, "Claim failed");
            let ended = SessionState::Ended {
                request_id: String::new(),
                reason: format!("Failed to claim session: {}", e),
                duration_seconds: None,
            };
            {
                let mut s = app_state.state.write().await;
                *s = ended.clone();
            }
            emit_state_change(&app, &ended);
            tray::update_tray_status(&app, &ended);
            shutdown_after_session(&app);
            Err(e)
        }
    }
}

/// Stop the active support session
#[tauri::command]
async fn stop_session(app: AppHandle) -> Result<(), String> {
    stop_session_internal(&app, Some("Stopped by user via desktop app")).await
}

/// Minimize the main window to the system tray
#[tauri::command]
async fn minimize_to_tray(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
    Ok(())
}

/// Shared internal session termination logic
pub async fn stop_session_internal(app: &AppHandle, reason: Option<&str>) -> Result<(), String> {
    let app_state = app.state::<Arc<AppState>>().inner().clone();
    
    // Stop background polling loop
    app_state.signal_stop().await;

    let req_id_opt = { app_state.active_request_id.read().await.clone() };
    let secret_opt = { app_state.device_secret.read().await.clone() };

    if let (Some(req_id), Some(secret)) = (req_id_opt.as_ref(), secret_opt.as_ref()) {
        tracing::info!(request_id = %req_id, "Stopping session via central app_end");
        let _ = api_end(req_id, secret, reason).await;
    }

    // Securely wipe secret from memory
    app_state.clear_session_secret().await;

    let ended = SessionState::Ended {
        request_id: req_id_opt.unwrap_or_default(),
        reason: reason.unwrap_or("Session stopped").to_string(),
        duration_seconds: None,
    };

    {
        let mut s = app_state.state.write().await;
        *s = ended.clone();
    }

    emit_state_change(app, &ended);
    tray::update_tray_status(app, &ended);
    Ok(())
}

/// Close Leeep Connect once a support session has finished.
///
/// In the approval-gated model the app only needs to run for the duration of an
/// approved session. When the session ends (the operator finished it, or it was
/// cancelled / declined / expired) the app quits so nothing of Leeep's stays
/// resident on the client between sessions — a later approved request re-opens
/// it through the `leeep-connect://` handler.
///
/// NOTE: this quits the *app* only. The MeshAgent (`LeeepConnect` service) is a
/// separate, always-on Windows service, so "no access between sessions" must
/// also be enforced server-side: the engine must refuse a session unless there
/// is an approved request. Quitting the app alone does not sever the agent.
fn shutdown_after_session(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Brief grace so the final state emit and log flush complete first.
        tokio::time::sleep(Duration::from_secs(2)).await;
        tracing::info!("Support session finished; closing Leeep Connect");
        app.exit(0);
    });
}

/// Polling loop running every 3 seconds to monitor central assignment and status
async fn run_polling_loop(
    app: AppHandle,
    request_id: String,
    device_secret: String,
    mut stop_rx: watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(3));
    let mut elapsed_wait: u64 = 0;
    let mut session_elapsed: u64 = 0;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if *stop_rx.borrow() {
                    break;
                }

                match api_status(&request_id, &device_secret).await {
                    Ok(status_res) => {
                        let app_state = app.state::<Arc<AppState>>().inner().clone();
                        match status_res.status.as_str() {
                            "Waiting" => {
                                elapsed_wait += 3;
                                let new_state = SessionState::Waiting {
                                    request_id: request_id.clone(),
                                    queue_position: status_res.queue_position,
                                    wait_seconds: elapsed_wait,
                                };
                                update_state(&app, &app_state, new_state).await;
                            }
                            "Taken" => {
                                let new_state = SessionState::AgentJoining {
                                    request_id: request_id.clone(),
                                    agent_name: status_res.agent_name.unwrap_or_else(|| "Agent".to_string()),
                                };
                                update_state(&app, &app_state, new_state).await;
                            }
                            "In Session" => {
                                session_elapsed += 3;
                                let new_state = SessionState::InSession {
                                    request_id: request_id.clone(),
                                    session_id: status_res.session_id,
                                    agent_name: status_res.agent_name.unwrap_or_else(|| "Agent".to_string()),
                                    duration_seconds: session_elapsed,
                                };
                                update_state(&app, &app_state, new_state).await;
                            }
                            "Ended" | "Cancelled" | "Expired" | "Declined" => {
                                tracing::info!(final_status = %status_res.status, "Session reached terminal state");
                                app_state.clear_session_secret().await;
                                let new_state = SessionState::Ended {
                                    request_id: request_id.clone(),
                                    reason: format!("Session {}", status_res.status),
                                    duration_seconds: if session_elapsed > 0 { Some(session_elapsed) } else { None },
                                };
                                update_state(&app, &app_state, new_state).await;
                                shutdown_after_session(&app);
                                break;
                            }
                            _ => {
                                tracing::warn!(status = %status_res.status, "Unknown session status received");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Status polling attempt failed, retrying in 3s");
                    }
                }
            }
            _ = stop_rx.changed() => {
                if *stop_rx.borrow() {
                    tracing::info!("Polling loop received stop notification");
                    break;
                }
            }
        }
    }
}

async fn update_state(app: &AppHandle, app_state: &Arc<AppState>, new_state: SessionState) {
    {
        let mut s = app_state.state.write().await;
        *s = new_state.clone();
    }
    emit_state_change(app, &new_state);
    tray::update_tray_status(app, &new_state);
}

fn emit_state_change(app: &AppHandle, state: &SessionState) {
    let _ = app.emit("state-changed", state);
}

/// True when tauri.conf.json carries a real updater public key (not the placeholder).
pub fn updater_configured(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|p| p.as_str())
        .map(|k| !k.trim().is_empty() && !k.starts_with("REPLACE"))
        .unwrap_or(false)
}

/// Silent self-update. Runs only while no support request is active, so a
/// session is never interrupted. The installer (passive mode) restarts the app.
pub async fn check_for_updates(app: AppHandle) {
    use tauri_plugin_updater::UpdaterExt;

    let app_state = app.state::<Arc<AppState>>().inner().clone();
    {
        let s = app_state.state.read().await;
        if !matches!(*s, SessionState::Idle | SessionState::Ended { .. }) {
            tracing::info!("Skipping update check: a support request is active");
            return;
        }
    }

    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(error = %e, "Updater unavailable");
            return;
        }
    };
    match updater.check().await {
        Ok(Some(update)) => {
            tracing::info!(version = %update.version, "Installing Leeep Connect update");
            if let Err(e) = update.download_and_install(|_, _| {}, || {}).await {
                tracing::warn!(error = %e, "Update install failed; will retry later");
            }
        }
        Ok(None) => tracing::debug!("Leeep Connect is up to date"),
        Err(e) => tracing::warn!(error = %e, "Update check failed"),
    }
}

/// Handle incoming deep link URI (either on startup or via single-instance forwarder)
pub fn handle_deep_link(app: &AppHandle, raw_url: &str) {
    let safe_log = logging::sanitize_for_log(raw_url);
    tracing::info!(deep_link = %safe_log, "Processing incoming deep link");

    match parse_deep_link(raw_url) {
        Ok(token) => {
            // Headless: no window is shown. The claim runs in the background and
            // the session is driven from the operator console.
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = start_claim(token, app_clone).await;
            });
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to parse deep link");
        }
    }
}
