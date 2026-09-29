/*!
 * Cloudflare OAuth 2.0 + PKCE flow for Stroke.
 *
 * Uses the same public client_id as the Wrangler CLI - Cloudflare's official
 * developer tool - which accepts localhost redirect URIs for desktop apps.
 *
 * Flow:
 *   1. Generate PKCE verifier/challenge + random state
 *   2. Spin up a temporary local HTTP server on a random port
 *   3. Open the Cloudflare auth page in the system browser
 *   4. Cloudflare redirects to http://localhost:{port}/oauth/callback?code=...
 *   5. Exchange the code + verifier for access + refresh tokens
 *   6. Store tokens in the app keychain (secrets module)
 */

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

// ── Cloudflare OAuth constants ────────────────────────────────────────────────

// Public client_id used by Wrangler CLI and other official Cloudflare tooling.
const CF_CLIENT_ID: &str = "54d11594-84e4-41aa-b438-e81b8fa78ee7";
const CF_AUTH_URL: &str = "https://dash.cloudflare.com/oauth2/auth";
const CF_TOKEN_URL: &str = "https://dash.cloudflare.com/oauth2/token";
const CF_REVOKE_URL: &str = "https://dash.cloudflare.com/oauth2/revoke";
// Scopes: account listing + D1 read/write + offline refresh tokens
const CF_SCOPES: &str = "account:read user:read d1:write offline_access";
// How long to wait for the user to authorize before giving up
const AUTH_TIMEOUT_SECS: u64 = 300;
// Cloudflare has pre-registered http://localhost:{PORT}/oauth/callback for this
// client ID - the exact same ports Wrangler CLI uses. A random port is rejected.
const CF_CALLBACK_PORTS: &[u16] = &[8976, 8977, 8978, 8979, 8980];

// ── Keychain keys ─────────────────────────────────────────────────────────────

const KEY_ACCESS: &str = "__cf_access_token__";
const KEY_REFRESH: &str = "__cf_refresh_token__";
const KEY_EXPIRES: &str = "__cf_token_expires__"; // unix seconds as string
const KEY_EMAIL: &str = "__cf_user_email__";

// ── Shared HTTP client ────────────────────────────────────────────────────────

static CF_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn http() -> &'static reqwest::Client {
    CF_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("stroke/1.0")
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .pool_max_idle_per_host(4)
            // Idle sockets go before an upstream load balancer's 60s cutoff, so a
            // request after a pause doesn't go out on a connection already closed
            // (same fix as the provider client in providers/mod.rs).
            .pool_idle_timeout(std::time::Duration::from_secs(20))
            // Bounded on purpose. `reqwest` has no default timeout, so a request
            // that never answers - captive portal, dropped route, a stalled edge -
            // leaves the command awaiting forever and the UI on its spinner with no
            // way out. "Loading your Cloudflare accounts…" sat there permanently.
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build CF HTTP client")
    })
}

// ── Public API structs ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CfAccount {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CfD1Database {
    pub uuid: String,
    pub name: String,
    pub created_at: Option<String>,
    pub num_tables: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CfOAuthStatus {
    pub connected: bool,
    pub email: Option<String>,
}

// ── PKCE helpers ──────────────────────────────────────────────────────────────

fn random_base64url(n: usize) -> String {
    let mut bytes = vec![0u8; n];
    getrandom::getrandom(&mut bytes).expect("getrandom failed");
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// Returns (verifier, challenge) where challenge = BASE64URL(SHA256(verifier)).
fn pkce_pair() -> (String, String) {
    let verifier = random_base64url(32);
    let hash = Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hash);
    (verifier, challenge)
}

// ── Local callback server ─────────────────────────────────────────────────────



// ── Token exchange ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

async fn exchange_code(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<TokenResponse, String> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", CF_CLIENT_ID),
        ("code_verifier", verifier),
    ];

    let resp = http()
        .post(CF_TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token exchange request failed: {e}"))?;

    let status = resp.status().as_u16();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse token response: {e}"))?;

    if status != 200 {
        let msg = body["error_description"]
            .as_str()
            .or_else(|| body["error"].as_str())
            .unwrap_or("Token exchange failed");
        return Err(format!("Cloudflare token error ({status}): {msg}"));
    }

    let access_token = body["access_token"]
        .as_str()
        .ok_or("Missing access_token")?
        .to_string();

    Ok(TokenResponse {
        access_token,
        refresh_token: body["refresh_token"].as_str().map(|s| s.to_string()),
        expires_in: body["expires_in"].as_u64(),
    })
}

async fn refresh_access_token(refresh_token: &str) -> Result<TokenResponse, String> {
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CF_CLIENT_ID),
    ];

    let resp = http()
        .post(CF_TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Refresh request failed: {e}"))?;

    let status = resp.status().as_u16();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse refresh response: {e}"))?;

    if status != 200 {
        let msg = body["error_description"]
            .as_str()
            .or_else(|| body["error"].as_str())
            .unwrap_or("Refresh failed");
        return Err(format!("Token refresh error ({status}): {msg}"));
    }

    let access_token = body["access_token"]
        .as_str()
        .ok_or("Missing access_token in refresh response")?
        .to_string();

    Ok(TokenResponse {
        access_token,
        refresh_token: body["refresh_token"].as_str().map(|s| s.to_string()),
        expires_in: body["expires_in"].as_u64(),
    })
}

// ── Fetch user email ──────────────────────────────────────────────────────────

async fn fetch_user_email(access_token: &str) -> Option<String> {
    let resp = http()
        .get("https://api.cloudflare.com/client/v4/user")
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    body["result"]["email"].as_str().map(|s| s.to_string())
}

// ── Token storage helpers ─────────────────────────────────────────────────────

// `async` because the keychain is only ever touched off the caller's thread -
// see the module comment in secrets.rs.
async fn store_tokens(
    app: &tauri::AppHandle,
    access: &str,
    refresh: Option<&str>,
    expires_in: Option<u64>,
    email: Option<&str>,
) -> Result<(), String> {
    let access = access.to_string();
    let refresh = refresh.map(str::to_string);
    let email = email.map(str::to_string);
    crate::secrets::update_async(app, move |map| {
        map.insert(KEY_ACCESS.to_string(), access);
        if let Some(r) = refresh {
            map.insert(KEY_REFRESH.to_string(), r);
        }
        if let Some(exp) = expires_in {
            let expires_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                + exp.saturating_sub(30); // 30s buffer
            map.insert(KEY_EXPIRES.to_string(), expires_at.to_string());
        }
        if let Some(e) = email {
            map.insert(KEY_EMAIL.to_string(), e);
        }
    })
    .await
}

async fn clear_tokens(app: &tauri::AppHandle) -> Result<(), String> {
    crate::secrets::update_async(app, |map| {
        map.remove(KEY_ACCESS);
        map.remove(KEY_REFRESH);
        map.remove(KEY_EXPIRES);
        map.remove(KEY_EMAIL);
    })
    .await
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// Start the OAuth flow: open browser, wait for callback, exchange code, store tokens.
/// Returns the user's email address on success.
#[tauri::command]
pub async fn cloudflare_start_oauth(app: tauri::AppHandle) -> Result<CfOAuthStatus, String> {
    let (verifier, challenge) = pkce_pair();
    let state = random_base64url(16);

    // The shared callback in providers/mod.rs: both loopback addresses, stray
    // requests ignored, and the branded page. Cloudflare's own copy took one
    // connection on IPv4 only, so a favicon fetch or an IPv6-first `localhost`
    // could break the sign-in.
    let (listener, port) = crate::providers::bind_callback_listener(CF_CALLBACK_PORTS)
        .await
        .map_err(|_| {
            format!(
                "Could not bind to any of the pre-registered callback ports ({}-{}). \
                 Close other Wrangler or Stroke processes and try again.",
                CF_CALLBACK_PORTS[0],
                CF_CALLBACK_PORTS[CF_CALLBACK_PORTS.len() - 1]
            )
        })?;
    let redirect_uri = format!("http://localhost:{port}/oauth/callback");

    let auth_url = format!(
        "{CF_AUTH_URL}?response_type=code&client_id={CF_CLIENT_ID}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(CF_SCOPES),
        state,
        challenge,
    );

    crate::providers::open_sign_in_page(&app, &auth_url);

    let code = tokio::time::timeout(
        std::time::Duration::from_secs(AUTH_TIMEOUT_SECS),
        crate::providers::await_oauth_callback(listener, &state, "code", "Cloudflare"),
    )
    .await
    .map_err(|_| "Authorization timed out - please try again.".to_string())??;

    let token = exchange_code(&code, &verifier, &redirect_uri).await?;

    let email = fetch_user_email(&token.access_token).await;

    store_tokens(
        &app,
        &token.access_token,
        token.refresh_token.as_deref(),
        token.expires_in,
        email.as_deref(),
    )
    .await?;

    Ok(CfOAuthStatus {
        connected: true,
        email,
    })
}

/// Returns the current OAuth status (connected + email) without side effects.
#[tauri::command]
pub async fn cloudflare_oauth_status(app: tauri::AppHandle) -> CfOAuthStatus {
    let map = crate::secrets::read_all_async(&app).await;
    let access = map.get(KEY_ACCESS).cloned().unwrap_or_default();
    if access.is_empty() {
        return CfOAuthStatus { connected: false, email: None };
    }
    CfOAuthStatus {
        connected: true,
        email: map.get(KEY_EMAIL).cloned(),
    }
}

/// Process-wide handle, set once at startup.
///
/// The D1 driver needs to reach the token store when a request comes back 401,
/// but it is handed only a `D1Config` - the whole `db` layer is deliberately
/// free of Tauri types. Rather than thread an `AppHandle` through every driver
/// signature for this one case, the handle is parked here.
static APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// Called once from `setup()`. Later calls are ignored.
pub fn set_app_handle(app: tauri::AppHandle) {
    let _ = APP.set(app);
}

/// A freshly-minted access token, or None when this install has no Cloudflare
/// OAuth session to refresh from (a hand-pasted API token, or not signed in).
///
/// Used by the D1 driver to recover from a mid-session expiry. Unconditionally
/// exchanges the refresh token rather than going through
/// `cloudflare_get_valid_token`: that one trusts the stored expiry and returns
/// the cached access token early, which is exactly the token the server just
/// rejected. A 401 is the authoritative answer about validity, whatever the
/// bookkeeping says.
///
/// Returns None rather than an error because the caller's job is to report the
/// *original* failure when no refresh is possible - "session expired" would be
/// a misleading thing to show someone using a manual API token.
// One refresh at a time. Cloudflare rotates refresh tokens, so two concurrent
// refreshes (the D1 driver's 401 recovery and a panel's token check) with the
// same one left the loser rejected and the session looking signed out.
static REFRESH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn refreshed_token() -> Option<String> {
    let app = APP.get()?.clone();
    let before = crate::secrets::read_all_async(&app).await.get(KEY_REFRESH).cloned();
    let _guard = REFRESH_LOCK.lock().await;
    let map = crate::secrets::read_all_async(&app).await;
    let refresh = map.get(KEY_REFRESH).cloned().unwrap_or_default();
    if refresh.is_empty() {
        return None;
    }
    // Refreshed by someone else while we waited: that token is the fresh one,
    // and reusing the rotated-out refresh token would be rejected.
    if before.as_deref() != Some(refresh.as_str()) {
        return map.get(KEY_ACCESS).cloned();
    }
    let new_token = refresh_access_token(&refresh).await.ok()?;
    let email = map.get(KEY_EMAIL).cloned();
    store_tokens(
        &app,
        &new_token.access_token,
        new_token.refresh_token.as_deref().or(Some(&refresh)),
        new_token.expires_in,
        email.as_deref(),
    )
    .await
    .ok()?;
    Some(new_token.access_token)
}

/// Return a valid access token, transparently refreshing via the refresh token if needed.
#[tauri::command]
pub async fn cloudflare_get_valid_token(app: tauri::AppHandle) -> Result<String, String> {
    let map = crate::secrets::read_all_async(&app).await;
    let access = map.get(KEY_ACCESS).cloned().unwrap_or_default();

    if access.is_empty() {
        return Err("Not connected to Cloudflare - please authorize first.".to_string());
    }

    let expires_at = map
        .get(KEY_EXPIRES)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(u64::MAX);

    if now_secs() < expires_at {
        return Ok(access);
    }

    let _guard = REFRESH_LOCK.lock().await;
    // Whoever held the lock may already have refreshed.
    let map = crate::secrets::read_all_async(&app).await;
    let expires_at = map
        .get(KEY_EXPIRES)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(u64::MAX);
    if let Some(access) = map.get(KEY_ACCESS).filter(|a| !a.is_empty()) {
        if now_secs() < expires_at {
            return Ok(access.clone());
        }
    }

    let refresh = map.get(KEY_REFRESH).cloned().unwrap_or_default();
    if refresh.is_empty() {
        return Err(
            "Cloudflare session expired. Please re-authorize in the connection dialog.".to_string(),
        );
    }

    let new_token = refresh_access_token(&refresh).await?;
    let email = map.get(KEY_EMAIL).cloned();

    store_tokens(
        &app,
        &new_token.access_token,
        new_token.refresh_token.as_deref().or(Some(&refresh)),
        new_token.expires_in,
        email.as_deref(),
    )
    .await?;

    Ok(new_token.access_token)
}

/// Revoke the stored tokens and clear them from the keychain.
#[tauri::command]
pub async fn cloudflare_logout(app: tauri::AppHandle) -> Result<(), String> {
    let map = crate::secrets::read_all_async(&app).await;
    let token = map.get(KEY_ACCESS).cloned().unwrap_or_default();

    if !token.is_empty() {
        let _ = http()
            .post(CF_REVOKE_URL)
            .form(&[("client_id", CF_CLIENT_ID), ("token", &token)])
            .send()
            .await;
    }

    clear_tokens(&app).await
}

// ── Discovery commands ────────────────────────────────────────────────────────

/// List all Cloudflare accounts accessible with the given API token.
#[tauri::command]
pub async fn cloudflare_list_accounts(api_token: String) -> Result<Vec<CfAccount>, String> {
    let resp = http()
        .get("https://api.cloudflare.com/client/v4/accounts?per_page=50")
        .header("Authorization", format!("Bearer {}", api_token.trim()))
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    let status = resp.status().as_u16();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {e}"))?;

    if status == 401 || status == 403 {
        return Err("Invalid or expired token.".to_string());
    }
    if status != 200 {
        let msg = body["errors"]
            .as_array()
            .and_then(|e| e.first())
            .and_then(|e| e["message"].as_str())
            .unwrap_or("Unknown error");
        return Err(format!("Cloudflare error {status}: {msg}"));
    }

    let accounts: Vec<CfAccount> = body["result"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            Some(CfAccount {
                id: a["id"].as_str()?.to_string(),
                name: a["name"].as_str().unwrap_or("Unnamed").to_string(),
            })
        })
        .collect();

    if accounts.is_empty() {
        return Err("No Cloudflare accounts found for this token.".to_string());
    }
    Ok(accounts)
}

/// List all D1 databases for a given Cloudflare account.
#[tauri::command]
pub async fn cloudflare_list_d1_databases(
    api_token: String,
    account_id: String,
) -> Result<Vec<CfD1Database>, String> {
    let url = format!(
        "https://api.cloudflare.com/client/v4/accounts/{}/d1/database?per_page=100",
        account_id.trim()
    );

    let resp = http()
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_token.trim()))
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    let status = resp.status().as_u16();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {e}"))?;

    if status == 401 || status == 403 {
        return Err("Token lacks D1:Read permission for this account.".to_string());
    }
    if status != 200 {
        let msg = body["errors"]
            .as_array()
            .and_then(|e| e.first())
            .and_then(|e| e["message"].as_str())
            .unwrap_or("Unknown error");
        return Err(format!("Cloudflare error {status}: {msg}"));
    }

    Ok(body["result"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            Some(CfD1Database {
                uuid: d["uuid"].as_str()?.to_string(),
                name: d["name"].as_str().unwrap_or("Unnamed").to_string(),
                created_at: d["created_at"].as_str().map(|s| s.to_string()),
                num_tables: d["num_tables"].as_u64(),
            })
        })
        .collect())
}
