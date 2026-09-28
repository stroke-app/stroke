/*!
 * Database provider adapters - sign in to a hosting provider (Neon, Supabase,
 * PlanetScale, Prisma Postgres) with OAuth, list every database on the account,
 * and connect in one click without hunting for connection strings.
 *
 * Design: this module owns the shared OAuth 2.0 + PKCE machinery (mirrors
 * `cloudflare.rs`, kept separate so the working D1 flow is untouched) and
 * dispatches provider-specific work - listing databases, building a connectable
 * spec - to the per-provider submodules via a `Provider` enum. Every provider is
 * a *public* PKCE client: no client secret is shipped in the binary.
 *
 * Adding a provider = add an enum variant, its `OAuthConfig`, and a submodule
 * with `list_databases()` + `build_connection()`.
 */

mod neon;
mod planetscale;
mod prisma;
mod supabase;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// ── How long to wait for the user to authorize before giving up ────────────────
const AUTH_TIMEOUT_SECS: u64 = 300;

// Localhost callback ports Stroke registers as redirect URIs on each provider's
// OAuth app: http://localhost:{port}/oauth/callback. Any one that is free is used.
const CALLBACK_PORTS: &[u16] = &[8989, 8990, 8991, 8992, 8993];

// Token-exchange proxy. Neon/Supabase/PlanetScale are confidential OAuth clients
// whose token endpoint requires a client_secret - which must never ship in a
// desktop binary. The stroke-web app (TanStack Start on Cloudflare Workers)
// hosts a server route that holds the secrets, injects them server-side, and
// forwards to the real provider token endpoint. The app sends only the public
// client_id + PKCE verifier. See stroke-web: src/routes/api/oauth/token.ts.
const TOKEN_PROXY: &str = "https://stroke.click/api/oauth/token";

// Lets the frontend abort an in-flight OAuth wait (Cancel button). Notifying it
// drops the callback listener future, which frees the localhost port instead of
// holding it until the 5-minute timeout.
static OAUTH_CANCEL: OnceLock<tokio::sync::Notify> = OnceLock::new();
fn oauth_cancel() -> &'static tokio::sync::Notify {
    OAUTH_CANCEL.get_or_init(tokio::sync::Notify::new)
}

// ── Provider registry ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Neon,
    Supabase,
    PlanetScale,
    Prisma,
}

impl Provider {
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "neon" => Ok(Self::Neon),
            "supabase" => Ok(Self::Supabase),
            "planetscale" => Ok(Self::PlanetScale),
            "prisma" => Ok(Self::Prisma),
            other => Err(format!("Unknown provider: {other}")),
        }
    }

    /// Stable key used to namespace stored tokens (`__{key}_refresh__`, …).
    fn key(&self) -> &'static str {
        match self {
            Self::Neon => "neon",
            Self::Supabase => "supabase",
            Self::PlanetScale => "planetscale",
            Self::Prisma => "prisma",
        }
    }

    fn oauth(&self) -> OAuthConfig {
        match self {
            Self::Neon => neon::OAUTH,
            Self::Supabase => supabase::OAUTH,
            Self::PlanetScale => planetscale::OAUTH,
            Self::Prisma => prisma::OAUTH,
        }
    }

    /// Whether a provider uses a pasted API token instead of the browser OAuth
    /// dance. None currently do (Prisma moved to Management-API OAuth), but the
    /// hook stays so a future token-only provider can opt in.
    fn is_token_based(&self) -> bool {
        false
    }

    /// Localhost callback ports to try, in order. PlanetScale accepts only ONE
    /// registered redirect URI, so it must always land on a fixed port (8989) or
    /// the redirect_uri won't match. Providers that allow multiple registered
    /// redirects use the full range so a busy port can fall back.
    fn callback_ports(&self) -> &'static [u16] {
        match self {
            Self::PlanetScale => &[8989],
            _ => CALLBACK_PORTS,
        }
    }

    /// Whether the provider's authorize/token endpoints support PKCE. PlanetScale
    /// does NOT - sending code_challenge makes it reject the request - so it uses
    /// a plain confidential authorization-code flow (secret only, via the proxy).
    fn uses_pkce(&self) -> bool {
        !matches!(self, Self::PlanetScale)
    }

    /// Public PKCE clients ship no secret, so the token exchange goes DIRECTLY to
    /// the provider (no stroke.click proxy). Neon reuses neonctl's public client.
    fn is_public_client(&self) -> bool {
        matches!(self, Self::Neon)
    }

    /// The loopback redirect URI. Most providers registered
    /// `http://localhost:{port}/oauth/callback`; Neon reuses neonctl's client,
    /// which registered `http://127.0.0.1:{port}/callback`.
    fn redirect_uri(&self, port: u16) -> String {
        match self {
            Self::Neon => format!("http://127.0.0.1:{port}/callback"),
            _ => format!("http://localhost:{port}/oauth/callback"),
        }
    }

    async fn list_databases(&self, token: &str) -> Result<Vec<ProviderDatabase>, String> {
        match self {
            Self::Neon => neon::list_databases(token).await,
            Self::Supabase => supabase::list_databases(token).await,
            Self::PlanetScale => planetscale::list_databases(token).await,
            Self::Prisma => prisma::list_databases(token).await,
        }
    }

    async fn build_connection(
        &self,
        token: &str,
        db_ref: &str,
    ) -> Result<ProviderConnection, String> {
        match self {
            Self::Neon => neon::build_connection(token, db_ref).await,
            Self::Supabase => supabase::build_connection(token, db_ref).await,
            Self::PlanetScale => planetscale::build_connection(token, db_ref).await,
            Self::Prisma => prisma::build_connection(token, db_ref).await,
        }
    }
}

/// Per-provider OAuth endpoints. `client_id` is a public identifier registered
/// with the provider; there is intentionally no secret (public PKCE client).
#[derive(Clone, Copy)]
pub struct OAuthConfig {
    pub client_id: &'static str,
    pub auth_url: &'static str,
    // Kept for documentation/parity; the real exchange happens in the proxy.
    #[allow(dead_code)]
    pub token_url: &'static str,
    pub scopes: &'static str,
}

// ── Shared types returned to the frontend ───────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderDatabase {
    /// Opaque reference (often JSON) consumed by `build_connection`.
    pub db_ref: String,
    pub name: String,
    pub region: Option<String>,
    /// Kind label shown in the UI, e.g. "Project", "Branch", "Database".
    pub kind: Option<String>,
    /// Host preview for the list row, when cheaply available.
    pub host: Option<String>,
}

/// Everything the frontend needs to construct a `SavedConnection` and connect.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProviderConnection {
    pub db_type: String, // "postgres" | "mysql"
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub ssl: bool,
    /// True when the provider will not expose a password (Supabase). The UI must
    /// prompt the user for it; every other field is prefilled.
    pub needs_password: bool,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderOAuthStatus {
    pub connected: bool,
    pub email: Option<String>,
}

// ── Shared HTTP client ──────────────────────────────────────────────────────────

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

pub(crate) fn http() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("stroke/1.0")
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .pool_max_idle_per_host(4)
            // Same reason as the Cloudflare client: Neon, Supabase, PlanetScale
            // and Prisma discovery all run through here, and an unbounded call
            // shows as a provider panel that never finishes loading.
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build provider HTTP client")
    })
}

// ── PKCE helpers ─────────────────────────────────────────────────────────────────

fn random_base64url(n: usize) -> String {
    let mut bytes = vec![0u8; n];
    getrandom::getrandom(&mut bytes).expect("getrandom failed");
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// (verifier, challenge) where challenge = BASE64URL(SHA256(verifier)).
fn pkce_pair() -> (String, String) {
    let verifier = random_base64url(32);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── Local callback server ────────────────────────────────────────────────────────

async fn bind_callback_listener(ports: &[u16]) -> Result<(TcpListener, u16), String> {
    for &port in ports {
        if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{port}")).await {
            return Ok((listener, port));
        }
    }
    if ports.len() == 1 {
        return Err(format!(
            "Port {} is in use - this provider requires that exact port for its OAuth redirect. \
             Close whatever is using it (another Stroke window or dev server) and retry.",
            ports[0]
        ));
    }
    Err(format!(
        "Could not bind any callback port ({}-{}). Close other apps using them and retry.",
        ports[0],
        ports[ports.len() - 1]
    ))
}

const OK_HTML: &str = r#"<!DOCTYPE html><html lang="en"><head><meta charset="UTF-8"><title>Stroke - authorized</title>
<style>body{font-family:system-ui,sans-serif;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0;background:#0d0d0d;color:#eee}
.card{text-align:center;padding:48px;border-radius:16px;border:1px solid #333;background:#111}h2{color:#22c55e;margin-bottom:12px}p{color:#888;margin:0}</style></head>
<body><div class="card"><h2>Authorization successful</h2><p>You can close this tab and return to Stroke.</p></div></body></html>"#;

const ERR_HTML: &str = r#"<!DOCTYPE html><html lang="en"><head><meta charset="UTF-8"><title>Stroke - error</title>
<style>body{font-family:system-ui,sans-serif;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0;background:#0d0d0d;color:#eee}
.card{text-align:center;padding:48px;border-radius:16px;border:1px solid #4b1c1c;background:#1a0f0f}h2{color:#ef4444;margin-bottom:12px}p{color:#888;margin:0}</style></head>
<body><div class="card"><h2>Authorization failed</h2><p>You can close this tab and try again in Stroke.</p></div></body></html>"#;

/// Wait for one OAuth redirect and return the authorization code.
async fn await_oauth_callback(listener: TcpListener, expected_state: &str) -> Result<String, String> {
    let send = |html: &str| {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(),
            html
        )
    };

    let (mut stream, _) = listener
        .accept()
        .await
        .map_err(|e| format!("Callback accept failed: {e}"))?;

    let mut buf = vec![0u8; 8192];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("Callback read failed: {e}"))?;
    let req = String::from_utf8_lossy(&buf[..n]);

    let first_line = req.lines().next().unwrap_or("");
    let query = first_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .split('?')
        .nth(1)
        .unwrap_or("");

    let (mut code, mut state, mut error) = (None, None, None);
    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        let key = kv.next().unwrap_or("");
        let val = kv
            .next()
            .map(|v| urlencoding::decode(v).unwrap_or_default().into_owned())
            .unwrap_or_default();
        match key {
            "code" => code = Some(val),
            "state" => state = Some(val),
            "error" => error = Some(val),
            "error_description" if error.is_none() => error = Some(val),
            _ => {}
        }
    }

    if let Some(err) = error {
        let _ = stream.write_all(send(ERR_HTML).as_bytes()).await;
        return Err(format!("Provider denied authorization: {err}"));
    }
    let code = match code {
        Some(c) if !c.is_empty() => c,
        _ => {
            let _ = stream.write_all(send(ERR_HTML).as_bytes()).await;
            return Err("No authorization code in callback".into());
        }
    };
    if state.as_deref() != Some(expected_state) {
        let _ = stream.write_all(send(ERR_HTML).as_bytes()).await;
        return Err("OAuth state mismatch - possible CSRF".into());
    }

    let _ = stream.write_all(send(OK_HTML).as_bytes()).await;
    let _ = stream.flush().await;
    Ok(code)
}

// ── Token exchange / refresh ─────────────────────────────────────────────────────

struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

/// Why a token request failed. Only `Rejected` - the provider answered
/// `invalid_grant`, RFC 6749's "this refresh token is expired, revoked or
/// already used" - ends a session. Anything else (network, proxy down, a
/// misconfigured client) is not the user's sign-in going bad and must not
/// sign them out.
enum TokenError {
    Rejected(String),
    Other(String),
}

impl From<TokenError> for String {
    fn from(e: TokenError) -> String {
        match e {
            TokenError::Rejected(m) | TokenError::Other(m) => m,
        }
    }
}

/// Canonical error an adapter returns for an HTTP 401 from the provider API.
/// The command layer matches it to refresh once and retry.
pub(crate) const UNAUTHORIZED: &str = "provider API returned 401 Unauthorized";

/// Shown when the session can't be renewed. Starts with "Not signed in" so the
/// UI recognises it and offers "Sign in again".
const SESSION_EXPIRED: &str =
    "Not signed in to this provider: the session expired or was revoked. Sign in again to continue.";

/// POST a token request to `url` - either the real provider token endpoint
/// (public clients) or the stroke.click proxy (confidential clients, where the
/// proxy injects the secret and `params` includes `provider`).
async fn post_token(url: &str, params: &[(&str, &str)]) -> Result<TokenResponse, TokenError> {
    let resp = http()
        .post(url)
        .form(params)
        .send()
        .await
        .map_err(|e| TokenError::Other(format!("Token request failed: {e}")))?;
    let status = resp.status().as_u16();
    let text = resp
        .text()
        .await
        .map_err(|e| TokenError::Other(format!("Token request failed: {e}")))?;
    // A non-JSON body from the proxy usually means it isn't deployed (the request
    // hit the marketing site's HTML). Surface that clearly.
    let body: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
        let snippet: String = text.chars().take(160).collect();
        let hint = if url == TOKEN_PROXY {
            " (is the stroke.click/api/oauth/token proxy deployed?)"
        } else {
            ""
        };
        TokenError::Other(format!(
            "Token exchange returned a non-JSON response (HTTP {status}){hint}: {snippet}"
        ))
    })?;
    if status != 200 {
        let msg = body["error_description"]
            .as_str()
            .or_else(|| body["error"].as_str())
            .unwrap_or("Token request failed");
        let text = format!("OAuth token error ({status}): {msg}");
        return Err(if body["error"].as_str() == Some("invalid_grant") {
            TokenError::Rejected(text)
        } else {
            TokenError::Other(text)
        });
    }
    Ok(TokenResponse {
        access_token: body["access_token"]
            .as_str()
            .ok_or_else(|| TokenError::Other("Missing access_token".into()))?
            .to_string(),
        refresh_token: body["refresh_token"].as_str().map(String::from),
        expires_in: body["expires_in"].as_u64(),
    })
}

async fn exchange_code(
    cfg: &OAuthConfig,
    provider_key: &str,
    is_public: bool,
    code: &str,
    verifier: Option<&str>,
    redirect_uri: &str,
) -> Result<TokenResponse, TokenError> {
    let mut params = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", cfg.client_id),
    ];
    if let Some(v) = verifier {
        params.push(("code_verifier", v));
    }
    if is_public {
        // Direct to the provider - no secret, no proxy.
        post_token(cfg.token_url, &params).await
    } else {
        // Via the proxy, which injects the client_secret.
        params.insert(0, ("provider", provider_key));
        post_token(TOKEN_PROXY, &params).await
    }
}

async fn refresh_token(
    cfg: &OAuthConfig,
    provider_key: &str,
    is_public: bool,
    refresh: &str,
) -> Result<TokenResponse, TokenError> {
    let mut params = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh),
        ("client_id", cfg.client_id),
    ];
    if is_public {
        post_token(cfg.token_url, &params).await
    } else {
        params.insert(0, ("provider", provider_key));
        post_token(TOKEN_PROXY, &params).await
    }
}

// ── Token storage (namespaced per provider in the secrets store) ─────────────────

// `async` because the keychain is only ever touched off the caller's thread -
// see the module comment in secrets.rs.
async fn store_tokens(
    app: &tauri::AppHandle,
    p: Provider,
    access: &str,
    refresh: Option<&str>,
    expires_in: Option<u64>,
    email: Option<&str>,
) -> Result<(), String> {
    let k = p.key();
    let access = access.to_string();
    let refresh = refresh.map(str::to_string);
    let email = email.map(str::to_string);
    crate::secrets::update_async(app, move |map| {
        map.insert(format!("__{k}_access__"), access);
        if let Some(r) = refresh {
            map.insert(format!("__{k}_refresh__"), r);
        }
        match expires_in {
            // saturating: a provider answering expires_in < 30 used to underflow.
            Some(exp) => {
                map.insert(format!("__{k}_expires__"), (now_secs() + exp.saturating_sub(30)).to_string());
            }
            // A new token without an expiry must not inherit the old one's.
            None => {
                map.remove(&format!("__{k}_expires__"));
            }
        }
        if let Some(e) = email {
            map.insert(format!("__{k}_email__"), e);
        }
    })
    .await
}

async fn clear_tokens(app: &tauri::AppHandle, p: Provider) -> Result<(), String> {
    let k = p.key();
    crate::secrets::update_async(app, move |map| {
        for suffix in ["access", "refresh", "expires", "email"] {
            map.remove(&format!("__{k}_{suffix}__"));
        }
    })
    .await
}

// One refresh at a time. Prisma and Supabase rotate refresh tokens: the first
// refresh consumes the stored one, so a second concurrent refresh with the same
// token is rejected. Opening the panel fires the status check and the database
// list together, which is exactly that race.
static REFRESH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A valid access token, refreshing transparently if the stored one expired.
/// For token-based providers, the "access token" is the pasted API key.
///
/// `rejected` is an access token the provider API just answered 401 for: it is
/// refreshed even if its stored expiry says it is still good.
async fn valid_token(
    app: &tauri::AppHandle,
    p: Provider,
    rejected: Option<&str>,
) -> Result<String, String> {
    let k = p.key();
    let fresh = |map: &std::collections::HashMap<String, String>| -> Option<String> {
        let access = map.get(&format!("__{k}_access__"))?;
        // Missing expiry => assume the token is long-lived and valid, mirroring
        // the Cloudflare flow. A `0` default treated every such token as already
        // expired and forced a needless re-login on every call.
        let expires = map
            .get(&format!("__{k}_expires__"))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(u64::MAX);
        let usable = now_secs() < expires && rejected != Some(access.as_str());
        usable.then(|| access.clone())
    };

    let map = crate::secrets::read_all_async(app).await;
    if !map.contains_key(&format!("__{k}_access__")) {
        return Err(SESSION_EXPIRED.into());
    }
    if p.is_token_based() {
        return Ok(map[&format!("__{k}_access__")].clone());
    }
    if let Some(t) = fresh(&map) {
        return Ok(t);
    }

    let _guard = REFRESH_LOCK.lock().await;
    // Whoever held the lock may already have refreshed.
    let map = crate::secrets::read_all_async(app).await;
    let Some(access) = map.get(&format!("__{k}_access__")).cloned() else {
        return Err(SESSION_EXPIRED.into());
    };
    if let Some(t) = fresh(&map) {
        return Ok(t);
    }
    let Some(refresh) = map.get(&format!("__{k}_refresh__")).cloned() else {
        // Nothing to renew with. An expired-by-the-clock token may still work
        // (the API is the real arbiter); one the API already refused won't.
        if rejected.is_some() {
            clear_tokens(app, p).await?;
            return Err(SESSION_EXPIRED.into());
        }
        return Ok(access);
    };
    let cfg = p.oauth();
    match refresh_token(&cfg, p.key(), p.is_public_client(), &refresh).await {
        Ok(t) => {
            store_tokens(
                app,
                p,
                &t.access_token,
                t.refresh_token.as_deref().or(Some(&refresh)),
                t.expires_in,
                None,
            )
            .await?;
            Ok(t.access_token)
        }
        // The provider says this sign-in is over: clear it so the UI shows
        // "Sign in again" instead of failing on every call.
        Err(TokenError::Rejected(_)) => {
            clear_tokens(app, p).await?;
            Err(SESSION_EXPIRED.into())
        }
        // Network or proxy trouble. Keep the session: hand back the current
        // token unless the API already refused it, in which case say why.
        Err(TokenError::Other(e)) => {
            if rejected.is_some() {
                Err(format!("Could not renew the sign-in: {e}"))
            } else {
                Ok(access)
            }
        }
    }
}

/// Run a provider API call with a valid token. On a 401 the token is refreshed
/// once and the call retried; a second 401 ends the session.
async fn with_token<T, F, Fut>(app: &tauri::AppHandle, p: Provider, call: F) -> Result<T, String>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let token = valid_token(app, p, None).await?;
    match call(token.clone()).await {
        Err(e) if e == UNAUTHORIZED => {
            let renewed = valid_token(app, p, Some(&token)).await?;
            match call(renewed).await {
                Err(e) if e == UNAUTHORIZED => {
                    clear_tokens(app, p).await?;
                    Err(SESSION_EXPIRED.into())
                }
                r => r,
            }
        }
        r => r,
    }
}

// ── Tauri commands ───────────────────────────────────────────────────────────────

/// Start the browser OAuth flow for a provider and store the resulting tokens.
#[tauri::command]
pub async fn provider_start_oauth(
    app: tauri::AppHandle,
    provider: String,
) -> Result<ProviderOAuthStatus, String> {
    let p = Provider::parse(&provider)?;
    if p.is_token_based() {
        return Err("This provider uses an API token, not OAuth".into());
    }
    let cfg = p.oauth();
    let (verifier, challenge) = pkce_pair();
    let state = random_base64url(16);

    // Abort any prior in-flight OAuth wait (e.g. a sign-in the user abandoned in
    // the browser) so its callback listener is dropped and the port is freed
    // before we bind. Otherwise a stuck flow holding 8989 forces this one onto a
    // different port, which won't match the redirect URI registered with the
    // provider ("Invalid Redirect URI"). The short pause lets the listener drop.
    oauth_cancel().notify_waiters();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let (listener, port) = bind_callback_listener(p.callback_ports()).await?;
    let redirect_uri = p.redirect_uri(port);

    // Only include &scope= when the provider uses request-time scopes.
    let scope_param = if cfg.scopes.is_empty() {
        String::new()
    } else {
        format!("&scope={}", urlencoding::encode(cfg.scopes))
    };
    // Only include PKCE params for providers that support it (PlanetScale doesn't).
    let pkce_param = if p.uses_pkce() {
        format!("&code_challenge={}&code_challenge_method=S256", urlencoding::encode(&challenge))
    } else {
        String::new()
    };
    let auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}{}&state={}{}",
        cfg.auth_url,
        urlencoding::encode(cfg.client_id),
        urlencoding::encode(&redirect_uri),
        scope_param,
        urlencoding::encode(&state),
        pkce_param,
    );

    eprintln!("[provider oauth] {} authorize URL: {auth_url}", p.key());
    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_url(auth_url, None::<&str>)
        .map_err(|e| format!("Could not open browser: {e}"))?;

    // Register the cancel waiter BEFORE awaiting so a Cancel click can't slip
    // through between opening the browser and starting to wait.
    let cancelled = oauth_cancel().notified();
    let code = tokio::select! {
        r = tokio::time::timeout(
            std::time::Duration::from_secs(AUTH_TIMEOUT_SECS),
            await_oauth_callback(listener, &state),
        ) => r.map_err(|_| "Authorization timed out".to_string())??,
        // Dropping the other branch's future here drops `listener` → port freed.
        _ = cancelled => return Err("cancelled".to_string()),
    };

    let verifier_opt = if p.uses_pkce() { Some(verifier.as_str()) } else { None };
    let t = exchange_code(&cfg, p.key(), p.is_public_client(), &code, verifier_opt, &redirect_uri)
        .await
        .map_err(String::from)?;
    store_tokens(
        &app,
        p,
        &t.access_token,
        t.refresh_token.as_deref(),
        t.expires_in,
        None,
    )
    .await?;

    Ok(ProviderOAuthStatus {
        connected: true,
        email: None,
    })
}

/// Abort an in-flight OAuth wait (Cancel button) - frees the callback port.
#[tauri::command]
pub fn provider_cancel_oauth() {
    oauth_cancel().notify_waiters();
}

/// Store a pasted API token for a token-based provider (e.g. Prisma).
#[tauri::command]
pub async fn provider_store_token(
    app: tauri::AppHandle,
    provider: String,
    token: String,
) -> Result<(), String> {
    let p = Provider::parse(&provider)?;
    if token.trim().is_empty() {
        return clear_tokens(&app, p).await;
    }
    store_tokens(&app, p, token.trim(), None, None, None).await
}

#[tauri::command]
pub async fn provider_oauth_status(
    app: tauri::AppHandle,
    provider: String,
) -> Result<ProviderOAuthStatus, String> {
    let p = Provider::parse(&provider)?;
    let map = crate::secrets::read_all_async(&app).await;
    let k = p.key();
    Ok(ProviderOAuthStatus {
        connected: map.contains_key(&format!("__{k}_access__")),
        email: map.get(&format!("__{k}_email__")).cloned(),
    })
}

#[tauri::command]
pub async fn provider_logout(app: tauri::AppHandle, provider: String) -> Result<(), String> {
    clear_tokens(&app, Provider::parse(&provider)?).await
}

#[tauri::command]
pub async fn provider_list_databases(
    app: tauri::AppHandle,
    provider: String,
) -> Result<Vec<ProviderDatabase>, String> {
    let p = Provider::parse(&provider)?;
    with_token(&app, p, |token| async move { p.list_databases(&token).await }).await
}

#[tauri::command]
pub async fn provider_build_connection(
    app: tauri::AppHandle,
    provider: String,
    db_ref: String,
) -> Result<ProviderConnection, String> {
    let p = Provider::parse(&provider)?;
    with_token(&app, p, |token| {
        let db_ref = db_ref.clone();
        async move { p.build_connection(&token, &db_ref).await }
    })
    .await
}
