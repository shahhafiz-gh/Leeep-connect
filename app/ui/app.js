(function () {
  // Tauri API bridge helper
  const isTauri = typeof window.__TAURI__ !== "undefined";
  const invoke = isTauri
    ? (window.__TAURI__.core?.invoke || window.__TAURI__.invoke || window.__TAURI__.tauri?.invoke)
    : async (cmd, args) => { console.log(`[Mock Tauri] invoke ${cmd}`, args); return null; };
  const listen = isTauri
    ? (window.__TAURI__.event?.listen)
    : async (event, cb) => { console.log(`[Mock Tauri] listen ${event}`); };

  const stateCards = {
    idle: document.getElementById("state-idle"),
    requesting: document.getElementById("state-requesting"),
    waiting: document.getElementById("state-waiting"),
    agentjoining: document.getElementById("state-agentjoining"),
    insession: document.getElementById("state-insession"),
    ended: document.getElementById("state-ended"),
  };

  const queuePosEl = document.getElementById("queue-pos");
  const waitTimeEl = document.getElementById("wait-time");
  const joiningAgentEl = document.getElementById("joining-agent-name");
  const activeAgentEl = document.getElementById("active-agent-name");
  const sessionTimeEl = document.getElementById("session-time");
  const endedReasonEl = document.getElementById("ended-reason");
  const endedDurationBox = document.getElementById("ended-duration-box");
  const endedDurationEl = document.getElementById("ended-duration");
  const footerStatusEl = document.getElementById("footer-status-text");

  function formatTime(totalSeconds) {
    const mins = Math.floor(totalSeconds / 60);
    const secs = totalSeconds % 60;
    return `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  }

  function formatDurationText(totalSeconds) {
    const mins = Math.floor(totalSeconds / 60);
    const secs = totalSeconds % 60;
    if (mins === 0) return `${secs}s`;
    return `${mins}m ${secs}s`;
  }

  function setActiveCard(stateKey) {
    Object.keys(stateCards).forEach((key) => {
      if (stateCards[key]) {
        stateCards[key].classList.toggle("active", key === stateKey);
      }
    });
  }

  function renderState(sessionState) {
    if (!sessionState) return;
    const type = sessionState.type || "idle";
    const data = sessionState.data || {};

    switch (type) {
      case "idle":
        setActiveCard("idle");
        footerStatusEl.textContent = "Ready for incoming connection";
        break;

      case "requesting":
        setActiveCard("requesting");
        footerStatusEl.textContent = "Connecting to Central...";
        break;

      case "waiting":
        setActiveCard("waiting");
        queuePosEl.textContent = data.queue_position ? `#${data.queue_position}` : "Next";
        waitTimeEl.textContent = formatTime(data.wait_seconds || 0);
        footerStatusEl.textContent = `In queue (${data.request_id || "Waiting"})`;
        break;

      case "agent_joining":
        setActiveCard("agentjoining");
        joiningAgentEl.textContent = data.agent_name || "Support Agent";
        footerStatusEl.textContent = `Agent ${data.agent_name || "Agent"} is connecting...`;
        break;

      case "in_session":
        setActiveCard("insession");
        activeAgentEl.textContent = data.agent_name || "Support Agent";
        sessionTimeEl.textContent = formatTime(data.duration_seconds || 0);
        footerStatusEl.textContent = `Live session: ${data.session_id || "Active"}`;
        break;

      case "ended":
        setActiveCard("ended");
        endedReasonEl.textContent = data.reason || "The support session has finished.";
        if (data.duration_seconds) {
          endedDurationBox.style.display = "block";
          endedDurationEl.textContent = formatDurationText(data.duration_seconds);
        } else {
          endedDurationBox.style.display = "none";
        }
        footerStatusEl.textContent = "Session ended";
        break;

      default:
        console.warn("Unknown state:", sessionState);
    }
  }

  // Bind Buttons
  document.getElementById("minimize-btn")?.addEventListener("click", () => {
    invoke("minimize_to_tray");
  });

  document.getElementById("stop-waiting-btn")?.addEventListener("click", () => {
    invoke("stop_session");
  });

  // Two-step Stop instead of window.confirm (native dialogs aren't reliable in
  // the Windows webview and would block the window).
  const stopBtn = document.getElementById("stop-session-btn");
  let stopArmedTimer = null;
  stopBtn?.addEventListener("click", () => {
    if (stopArmedTimer) {
      clearTimeout(stopArmedTimer);
      stopArmedTimer = null;
      stopBtn.textContent = "Stopping…";
      stopBtn.disabled = true;
      invoke("stop_session").finally(() => {
        stopBtn.textContent = "Stop Session";
        stopBtn.disabled = false;
      });
      return;
    }
    stopBtn.textContent = "Click again to stop";
    stopArmedTimer = setTimeout(() => {
      stopArmedTimer = null;
      stopBtn.textContent = "Stop Session";
    }, 4000);
  });

  document.getElementById("dismiss-ended-btn")?.addEventListener("click", () => {
    setActiveCard("idle");
    footerStatusEl.textContent = "Ready for incoming connection";
  });

  // Init listener
  if (listen) {
    listen("state-changed", (event) => {
      renderState(event.payload);
    });
  }

  // Version from Cargo.toml (via tauri.conf.json), not hard-coded in the page
  window.__TAURI__?.app?.getVersion?.()
    .then((v) => {
      const el = document.getElementById("app-version");
      if (el) el.textContent = `v${v}`;
    })
    .catch(() => {});

  // Fetch initial state
  invoke("get_state")
    .then((initialState) => {
      if (initialState) renderState(initialState);
    })
    .catch((err) => {
      console.warn("Could not retrieve initial state:", err);
    });
})();
