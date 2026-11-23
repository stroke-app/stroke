use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

// The OAuth app client ID used by GitHub Copilot CLI / VS Code extension.
// This is a public client_id; it does not grant access without user authorization.
const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

// ── Shared HTTP client ─────────────────────────────────────────────────────────
// One client for the whole process lifetime. Avoids reconstructing a TLS
// connection pool on every OAuth poll (called every 5 s during device flow).
static COPILOT_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn http_client() -> &'static reqwest::Client {
    COPILOT_CLIENT.get_or_init(|| {
        crate::proxy::http_builder(crate::proxy::Kind::Ai)
            .user_agent("stroke/1.0")
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .pool_max_idle_per_host(4)
            .build()
            .expect("failed to build Copilot HTTP client")
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceFlowStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PollResult {
    /// "authorized" | "pending" | "slow_down" | "expired" | "access_denied"
    pub status: String,
    pub token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CopilotToken {
    pub token: String,
    /// Unix timestamp (seconds) when the JWT expires (~30 min from issue).
    pub expires_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CopilotModel {
    pub id: String,
    pub name: String,
}

/// Step 1 of the GitHub Device OAuth flow: obtain a user_code and verification URL.
#[tauri::command]
pub async fn copilot_start_device_flow() -> Result<DeviceFlowStart, String> {
    let client = http_client();
    // Use raw body so we don't need the reqwest `form` feature.
    let body = format!("client_id={}&scope=read%3Auser", GITHUB_CLIENT_ID);

    let resp = client
        .post("https://github.com/login/device/code")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    let parsed: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GitHub response: {e}"))?;

    if let Some(err) = parsed["error"].as_str() {
        return Err(format!(
            "{}: {}",
            err,
            parsed["error_description"].as_str().unwrap_or("")
        ));
    }

    Ok(DeviceFlowStart {
        device_code: parsed["device_code"].as_str().unwrap_or("").to_string(),
        user_code: parsed["user_code"].as_str().unwrap_or("").to_string(),
        verification_uri: parsed["verification_uri"]
            .as_str()
            .unwrap_or("https://github.com/login/device")
            .to_string(),
        expires_in: parsed["expires_in"].as_u64().unwrap_or(900),
        interval: parsed["interval"].as_u64().unwrap_or(5),
    })
}

/// Step 2: poll GitHub until the user has authorized the device.
/// Call this every `interval` seconds (from the DeviceFlowStart response).
#[tauri::command]
pub async fn copilot_poll_oauth_token(device_code: String) -> Result<PollResult, String> {
    let client = http_client();
    let body = format!(
        "client_id={}&device_code={}&grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code",
        GITHUB_CLIENT_ID, device_code
    );

    let resp = client
        .post("https://github.com/login/oauth/access_token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    let parsed: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {e}"))?;

    if let Some(token) = parsed["access_token"].as_str() {
        if !token.is_empty() {
            return Ok(PollResult {
                status: "authorized".to_string(),
                token: Some(token.to_string()),
            });
        }
    }

    let error = parsed["error"].as_str().unwrap_or("authorization_pending");
    let status = match error {
        "authorization_pending" => "pending",
        "slow_down" => "slow_down",
        "expired_token" => "expired",
        "access_denied" => "access_denied",
        _ => "pending",
    }
    .to_string();

    Ok(PollResult { status, token: None })
}

/// Exchange a GitHub OAuth token for a short-lived Copilot JWT (~30 min).
/// The JWT is required for all Copilot API calls. Refresh it before it expires.
#[tauri::command]
pub async fn copilot_get_copilot_token(oauth_token: String) -> Result<CopilotToken, String> {
    let client = http_client();

    let resp = client
        .get("https://api.github.com/copilot_internal/v2/token")
        .header("Authorization", format!("Bearer {}", oauth_token))
        .header("Accept", "application/json")
        .header("User-Agent", "stroke/1.0")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "GitHub returned {status} - OAuth token may be expired or revoked. {body}"
        ));
    }

    let parsed: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Copilot token response: {e}"))?;

    let token = parsed["token"]
        .as_str()
        .ok_or_else(|| "No token field in Copilot response".to_string())?
        .to_string();

    let expires_at = parsed["expires_at"].as_u64().unwrap_or(0);

    Ok(CopilotToken { token, expires_at })
}

/// Fetch the list of models available to this Copilot subscription.
/// Falls back to a static list if the endpoint is unreachable.
#[tauri::command]
pub async fn copilot_fetch_models(copilot_token: String) -> Result<Vec<CopilotModel>, String> {
    let client = http_client();

    let resp = client
        .get("https://api.githubcopilot.com/models")
        .header("Authorization", format!("Bearer {}", copilot_token))
        .header("Content-Type", "application/json")
        .header("Copilot-Integration-Id", "vscode-chat")
        .header("Editor-Version", "vscode/1.95.0")
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        // Return the static fallback list so the UI stays functional.
        return Ok(fallback_models());
    }

    let parsed: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| "Failed to parse models response".to_string())?;

    // API may return { data: [...] } or just an array at the root.
    let arr = parsed["data"]
        .as_array()
        .or_else(|| parsed.as_array())
        .cloned()
        .unwrap_or_default();

    let models: Vec<CopilotModel> = arr
        .iter()
        .filter_map(|m| {
            let id = m["id"].as_str()?.to_string();
            // Skip embedding / non-chat models
            let caps_type = m["capabilities"]["type"].as_str().unwrap_or("chat");
            if caps_type == "embeddings" {
                return None;
            }
            let name = m["name"]
                .as_str()
                .or_else(|| m["display_name"].as_str())
                .unwrap_or(&id)
                .to_string();
            Some(CopilotModel { id, name })
        })
        .collect();

    if models.is_empty() {
        Ok(fallback_models())
    } else {
        Ok(models)
    }
}

fn fallback_models() -> Vec<CopilotModel> {
    vec![
        CopilotModel { id: "gpt-4o".into(), name: "GPT-4o".into() },
        CopilotModel { id: "gpt-4o-mini".into(), name: "GPT-4o mini".into() },
        CopilotModel { id: "o3-mini".into(), name: "o3-mini".into() },
        CopilotModel { id: "claude-3.5-sonnet".into(), name: "Claude 3.5 Sonnet".into() },
        CopilotModel { id: "claude-3.7-sonnet".into(), name: "Claude 3.7 Sonnet".into() },
        CopilotModel { id: "gemini-2.0-flash-001".into(), name: "Gemini 2.0 Flash".into() },
    ]
}
