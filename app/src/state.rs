use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{watch, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum SessionState {
    /// Initial idle state awaiting user action or incoming deep link
    Idle,

    /// Received token via deep link, sending claim request to central
    Requesting,

    /// Claimed successfully, waiting in queue for an available agent
    Waiting {
        request_id: String,
        queue_position: Option<u32>,
        wait_seconds: u64,
    },

    /// Agent has taken the ticket and is connecting to desktop relay
    AgentJoining {
        request_id: String,
        agent_name: String,
    },

    /// Remote support session is active
    InSession {
        request_id: String,
        session_id: Option<String>,
        agent_name: String,
        duration_seconds: u64,
    },

    /// Session ended (completed, stopped by client, cancelled, or expired)
    Ended {
        request_id: String,
        reason: String,
        duration_seconds: Option<u64>,
    },
}

impl Default for SessionState {
    fn default() -> Self {
        SessionState::Idle
    }
}

pub struct AppState {
    /// Active state exposed to UI and tray
    pub state: RwLock<SessionState>,

    /// Request ID of the currently tracked support request
    pub active_request_id: RwLock<Option<String>>,

    /// Per-request device secret issued by central during `claim()`.
    /// IN-MEMORY ONLY: Never written to disk, registry, or rotating log files!
    /// Cleared immediately upon session termination or transition to Idle/Ended.
    pub device_secret: RwLock<Option<String>>,

    /// Cancellation signal sender for the 3-second background polling loop
    pub stop_tx: RwLock<Option<watch::Sender<bool>>>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: RwLock::new(SessionState::Idle),
            active_request_id: RwLock::new(None),
            device_secret: RwLock::new(None),
            stop_tx: RwLock::new(None),
        })
    }

    /// Reset session credentials from memory
    pub async fn clear_session_secret(&self) {
        let mut secret = self.device_secret.write().await;
        *secret = None;
    }

    /// Send stop signal to active polling worker
    pub async fn signal_stop(&self) {
        let mut tx_guard = self.stop_tx.write().await;
        if let Some(tx) = tx_guard.take() {
            let _ = tx.send(true);
        }
    }
}
