use std::path::PathBuf;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize a rotating file logger and stdout logger.
/// The log file rotates daily and is stored in %LOCALAPPDATA%\LeeepConnect\logs (or equivalent).
/// IMPORTANT: Log lines MUST NEVER contain session tokens or device secrets.
pub fn setup_logging() -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = get_log_dir();
    std::fs::create_dir_all(&log_dir)?;

    // Daily rotating log file
    let file_appender = tracing_appender::rolling::daily(&log_dir, "leeep-connect.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Box leak guard so the non-blocking worker thread persists for the app lifetime
    Box::leak(Box::new(_guard));

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,leeep_connect=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stdout))
        .with(
            fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true),
        )
        .try_init()?;

    tracing::info!(log_directory = ?log_dir, "Logger initialized (rotating daily)");
    Ok(())
}

/// Locate app log directory across platforms
fn get_log_dir() -> PathBuf {
    if let Some(mut data_dir) = dirs::data_local_dir() {
        data_dir.push("LeeepConnect");
        data_dir.push("logs");
        data_dir
    } else {
        PathBuf::from(".logs")
    }
}

/// Redact sensitive tokens from any arbitrary string for safe logging.
/// Never log raw tokens or secrets.
pub fn sanitize_for_log(input: &str) -> String {
    // Redact query parameter token=...
    if input.contains("token=") {
        let re = regex_replace_token(input);
        return re;
    }
    input.to_string()
}

fn regex_replace_token(input: &str) -> String {
    let parts: Vec<&str> = input.split("token=").collect();
    if parts.len() < 2 {
        return input.to_string();
    }
    let mut out = parts[0].to_string();
    out.push_str("token=[REDACTED]");
    if let Some(idx) = parts[1].find('&') {
        out.push_str(&parts[1][idx..]);
    }
    out
}
