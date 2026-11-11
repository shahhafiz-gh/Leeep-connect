use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

/// Central host, fixed at BUILD time. Production builds use admin.leeep.in.
/// A test build can bake in another host with the LEEEP_CONNECT_CENTRAL_URL
/// environment variable when compiling (e.g. http://192.168.1.7:8000).
/// CRITICAL: the host is NEVER read from deep link URLs or user input.
pub const CENTRAL_BASE_URL: &str = match option_env!("LEEEP_CONNECT_CENTRAL_URL") {
    Some(url) => url,
    None => "https://admin.leeep.in",
};

/// Optional Frappe site name (X-Frappe-Site-Name) for a test server that hosts
/// several sites behind one address. Also build-time only.
pub const CENTRAL_SITE_NAME: Option<&str> = option_env!("LEEEP_CONNECT_SITE_NAME");

/// App version (from Cargo.toml), sent with every claim for the minimum-version policy
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// HTTP client for central: timeout plus the optional site-name header.
fn central_client(timeout_secs: u64) -> Result<Client, String> {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(site) = CENTRAL_SITE_NAME {
        if let Ok(v) = reqwest::header::HeaderValue::from_str(site) {
            headers.insert("X-Frappe-Site-Name", v);
        }
    }
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .default_headers(headers)
        .user_agent(format!("LeeepConnect/{}", APP_VERSION))
        .build()
        .map_err(|e| e.to_string())
}

/// Readable message from a Frappe error body (`_server_messages`), else the raw text.
fn frappe_error(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(raw) = v.get("_server_messages").and_then(|m| m.as_str()) {
            if let Ok(list) = serde_json::from_str::<Vec<String>>(raw) {
                let msgs: Vec<String> = list
                    .iter()
                    .filter_map(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                    .filter_map(|m| m.get("message").and_then(|s| s.as_str()).map(str::to_string))
                    .collect();
                if !msgs.is_empty() {
                    return msgs.join(" ");
                }
            }
        }
    }
    body.chars().take(300).collect()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FrappeResponse<T> {
    pub message: Option<T>,
    #[serde(default)]
    pub exc: Option<String>,
    #[serde(default)]
    pub _server_messages: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaimPayload {
    pub token: String,
    pub engine_node_id: Option<String>,
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub app_version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaimResult {
    pub success: bool,
    pub request_id: String,
    pub device_secret: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StatusResult {
    pub success: bool,
    pub request_id: String,
    pub status: String,
    pub agent_name: Option<String>,
    pub session_id: Option<String>,
    pub queue_position: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EndResult {
    pub success: bool,
    pub status: String,
}

/// Parse deep link URL (e.g. `leeep-connect://join?token=xyz123`)
/// Strictly extracts token parameter; all host, path, or network targets in the URL are ignored.
pub fn parse_deep_link(raw_url: &str) -> Result<String, String> {
    let parsed = Url::parse(raw_url).map_err(|e| format!("Invalid deep link URL: {}", e))?;
    
    if parsed.scheme() != "leeep-connect" {
        return Err(format!("Unsupported URI scheme: {}", parsed.scheme()));
    }

    for (key, val) in parsed.query_pairs() {
        if key == "token" && !val.trim().is_empty() {
            return Ok(val.trim().to_string());
        }
    }

    Err("Missing required 'token' parameter in deep link URL".to_string())
}

/// Dispatches claim call to hardcoded central server
pub async fn api_claim(
    token: &str,
    engine_node_id: Option<String>,
) -> Result<ClaimResult, String> {
    let client = central_client(10)?;

    let url = format!("{}/api/method/leeep.leeep.connect.claim", CENTRAL_BASE_URL);

    let payload = ClaimPayload {
        token: token.to_string(),
        engine_node_id,
        hostname: Some(get_hostname()),
        os: Some(get_os_info()),
        app_version: Some(APP_VERSION.to_string()),
    };

    tracing::info!(endpoint = %url, "Sending session claim request to Central");

    let resp = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Network error during claim: {}", e))?;

    if !resp.status().is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        return Err(frappe_error(&err_body));
    }

    let result: FrappeResponse<ClaimResult> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse claim response: {}", e))?;

    result.message.ok_or_else(|| "Empty message in claim response".to_string())
}

/// Dispatches app_status call to hardcoded central server
pub async fn api_status(
    request_id: &str,
    device_secret: &str,
) -> Result<StatusResult, String> {
    let client = central_client(8)?;

    let url = format!("{}/api/method/leeep.leeep.connect.app_status", CENTRAL_BASE_URL);

    let resp = client
        .post(&url)
        .form(&[("request_id", request_id), ("device_secret", device_secret)])
        .send()
        .await
        .map_err(|e| format!("Network error during status poll: {}", e))?;

    if !resp.status().is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        return Err(frappe_error(&err_body));
    }

    let result: FrappeResponse<StatusResult> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse status response: {}", e))?;

    result.message.ok_or_else(|| "Empty status message".to_string())
}

/// Dispatches app_end call to hardcoded central server
pub async fn api_end(
    request_id: &str,
    device_secret: &str,
    reason: Option<&str>,
) -> Result<EndResult, String> {
    let client = central_client(10)?;

    let url = format!("{}/api/method/leeep.leeep.connect.app_end", CENTRAL_BASE_URL);

    let reason_val = reason.unwrap_or("Stopped by client");
    let resp = client
        .post(&url)
        .form(&[
            ("request_id", request_id),
            ("device_secret", device_secret),
            ("reason", reason_val),
        ])
        .send()
        .await
        .map_err(|e| format!("Network error during session end: {}", e))?;

    if !resp.status().is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        return Err(frappe_error(&err_body));
    }

    let result: FrappeResponse<EndResult> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse end response: {}", e))?;

    result.message.ok_or_else(|| "Empty end message".to_string())
}

/// System hostname lookup
pub fn get_hostname() -> String {
    sysinfo::System::host_name().unwrap_or_else(|| "Unknown-Host".to_string())
}

/// System OS lookup
pub fn get_os_info() -> String {
    let os_name = sysinfo::System::name().unwrap_or_else(|| "Windows".to_string());
    let os_version = sysinfo::System::os_version().unwrap_or_else(|| "10/11".to_string());
    format!("{} {}", os_name, os_version)
}

/// The local engine agent's node id, if it can be read; None otherwise.
///
/// The agent records its NodeId under HKLM\SOFTWARE\Open Source\<service name>.
/// Ours is branded "LeeepConnect"; the stock names are tried as a fallback.
/// Returning None is fine: central then finds this PC in the engine by its
/// computer name, and it also double-checks any id we send.
/// VERIFY (spike): registry location/value name and its encoding on a real install.
pub fn get_mesh_node_id() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
        use winreg::RegKey;

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        for service in ["LeeepConnect", "Mesh Agent", "MeshAgent"] {
            let path = format!(r"SOFTWARE\Open Source\{}", service);
            let Ok(key) = hklm.open_subkey_with_flags(&path, KEY_READ | KEY_WOW64_64KEY) else {
                continue;
            };
            if let Ok(raw) = key.get_value::<String, _>("NodeId") {
                if let Some(id) = normalize_node_id(&raw) {
                    tracing::info!(service = %service, "Found local engine agent id");
                    return Some(id);
                }
            }
        }
        tracing::info!("Engine agent id not found in registry; central will match by computer name");
    }
    None
}

/// MeshCentral ids look like "node//<base64 with + -> @ and / -> $>".
/// Accept that form as-is, or a bare base64 id, and reject anything else.
fn normalize_node_id(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.starts_with("node/") {
        return Some(raw.to_string());
    }
    let looks_b64 = !raw.is_empty()
        && raw.len() <= 128
        && raw.chars().all(|c| c.is_ascii_alphanumeric() || "+/=@$".contains(c));
    looks_b64.then(|| format!("node//{}", raw.replace('+', "@").replace('/', "$")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_link_takes_only_the_token() {
        assert_eq!(parse_deep_link("leeep-connect://join?token=abc").unwrap(), "abc");
        // A host in the link is ignored: the central host is compiled in
        assert_eq!(parse_deep_link("leeep-connect://evil.example/join?token=abc&host=x").unwrap(), "abc");
        assert!(parse_deep_link("https://join?token=abc").is_err());
        assert!(parse_deep_link("leeep-connect://join").is_err());
    }

    #[test]
    fn node_ids_are_normalised() {
        assert_eq!(normalize_node_id("node//abc@$").unwrap(), "node//abc@$");
        assert_eq!(normalize_node_id("ab+c/d=").unwrap(), "node//ab@c$d=");
        assert!(normalize_node_id("not an id!").is_none());
    }

    #[test]
    fn frappe_errors_are_readable() {
        let body = r#"{"_server_messages":"[\"{\\\"message\\\": \\\"This token has already been claimed.\\\"}\"]"}"#;
        assert_eq!(frappe_error(body), "This token has already been claimed.");
    }
}
