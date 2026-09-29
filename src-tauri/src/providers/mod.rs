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
mod posthog;
mod prisma;
mod supabase;
mod nile;
mod railway;
mod tidb;
mod turso;
mod upstash;

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
    TiDB,
    Turso,
    Railway,
    Nile,
    Upstash,
    PostHog,
}

/// How a provider signs the user in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignIn {
    /// Browser authorization code + localhost redirect (Neon, Supabase, …).
    AuthCode,
    /// OAuth 2.0 device code grant, RFC 8628 (TiDB Cloud): the browser confirms
    /// a code and the app polls for the token, so no callback port is involved.
    DeviceCode,
    /// A CLI login page that redirects to localhost with the API token itself in
    /// `?jwt=` (Turso). No code exchange and no refresh token.
    TokenRedirect,
}

impl Provider {
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "neon" => Ok(Self::Neon),
            "supabase" => Ok(Self::Supabase),
            "planetscale" => Ok(Self::PlanetScale),
            "prisma" => Ok(Self::Prisma),
            "tidb" => Ok(Self::TiDB),
            "turso" => Ok(Self::Turso),
            "railway" => Ok(Self::Railway),
            "nile" => Ok(Self::Nile),
            "upstash" => Ok(Self::Upstash),
            "posthog" => Ok(Self::PostHog),
            other => Err(format!("Unknown provider: {other}")),
        }
    }

    const ALL: [Provider; 10] = [
        Self::Neon, Self::Supabase, Self::PlanetScale, Self::Prisma, Self::TiDB,
        Self::Turso, Self::Railway, Self::Nile, Self::Upstash, Self::PostHog,
    ];

    /// The API host each listing and connect talks to, for `provider_warm`.
    /// None for PostHog, whose host is the user's own instance.
    fn api_origin(&self) -> Option<&'static str> {
        Some(match self {
            Self::Neon => "https://console.neon.tech",
            Self::Supabase => "https://api.supabase.com",
            Self::PlanetScale => "https://api.planetscale.com",
            Self::Prisma => "https://api.prisma.io",
            Self::TiDB => "https://serverless.tidbapi.com",
            Self::Turso => "https://api.turso.tech",
            Self::Railway => "https://backboard.railway.com",
            Self::Nile => "https://global.thenile.dev",
            Self::Upstash => "https://api.upstash.com",
            Self::PostHog => return None,
        })
    }

    /// Stable key used to namespace stored tokens (`__{key}_refresh__`, …).
    fn key(&self) -> &'static str {
        match self {
            Self::Neon => "neon",
            Self::Supabase => "supabase",
            Self::PlanetScale => "planetscale",
            Self::Prisma => "prisma",
            Self::TiDB => "tidb",
            Self::Turso => "turso",
            Self::Railway => "railway",
            Self::Nile => "nile",
            Self::Upstash => "upstash",
            Self::PostHog => "posthog",
        }
    }

    /// Display name, for the page the browser lands on after sign-in.
    fn label(&self) -> &'static str {
        match self {
            Self::Neon => "Neon",
            Self::Supabase => "Supabase",
            Self::PlanetScale => "PlanetScale",
            Self::Prisma => "Prisma",
            Self::TiDB => "TiDB Cloud",
            Self::Turso => "Turso",
            Self::Railway => "Railway",
            Self::Nile => "Nile",
            Self::Upstash => "Upstash",
            Self::PostHog => "PostHog",
        }
    }

    fn sign_in(&self) -> SignIn {
        match self {
            Self::TiDB => SignIn::DeviceCode,
            Self::Turso => SignIn::TokenRedirect,
            _ => SignIn::AuthCode,
        }
    }

    fn oauth(&self) -> OAuthConfig {
        match self {
            Self::Neon => neon::OAUTH,
            Self::Supabase => supabase::OAUTH,
            Self::PlanetScale => planetscale::OAUTH,
            Self::Prisma => prisma::OAUTH,
            Self::TiDB => tidb::OAUTH,
            Self::Turso => turso::OAUTH,
            Self::Railway => railway::OAUTH,
            Self::Nile => nile::OAUTH,
            Self::Upstash => upstash::OAUTH,
            Self::PostHog => posthog::OAUTH,
        }
    }

    /// Whether a provider uses a pasted API credential instead of the browser
    /// OAuth dance: Upstash, which offers no OAuth to third-party apps.
    fn is_token_based(&self) -> bool {
        matches!(self, Self::Upstash | Self::PostHog)
    }

    /// Localhost callback ports to try, in order. PlanetScale accepts only ONE
    /// registered redirect URI, so it must always land on a fixed port (8989) or
    /// the redirect_uri won't match. Providers that allow multiple registered
    /// redirects use the full range so a busy port can fall back.
    fn callback_ports(&self) -> &'static [u16] {
        match self {
            // Railway, like PlanetScale, matches the redirect URI exactly and
            // the app registers one: http://127.0.0.1:8989/oauth/callback.
            Self::PlanetScale | Self::Railway => &[8989],
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
        // Neon and Nile reuse their CLIs' public clients; Railway is registered
        // as a native (public) app. None of them has a secret to inject.
        matches!(self, Self::Neon | Self::Nile | Self::Railway)
    }

    /// The loopback redirect URI. Most providers registered
    /// `http://localhost:{port}/oauth/callback`; Neon reuses neonctl's client,
    /// which registered `http://127.0.0.1:{port}/callback`.
    fn redirect_uri(&self, port: u16) -> String {
        match self {
            Self::Neon => format!("http://127.0.0.1:{port}/callback"),
            // nilecli's client allows any localhost port on /callback.
            Self::Nile => format!("http://localhost:{port}/callback"),
            // Stroke's Railway app is registered with the loopback IP, and
            // Railway matches the redirect exactly: `localhost` here was
            // rejected with invalid_redirect_uri.
            Self::Railway => format!("http://127.0.0.1:{port}/oauth/callback"),
            _ => format!("http://localhost:{port}/oauth/callback"),
        }
    }

    async fn list_databases(&self, token: &str) -> Result<Vec<ProviderDatabase>, String> {
        match self {
            Self::Neon => neon::list_databases(token).await,
            Self::Supabase => supabase::list_databases(token).await,
            Self::PlanetScale => planetscale::list_databases(token).await,
            Self::Prisma => prisma::list_databases(token).await,
            Self::TiDB => tidb::list_databases(token).await,
            Self::Turso => turso::list_databases(token).await,
            Self::Railway => railway::list_databases(token).await,
            Self::Nile => nile::list_databases(token).await,
            Self::Upstash => upstash::list_databases(token).await,
            Self::PostHog => posthog::list_databases(token).await,
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
            Self::TiDB => tidb::build_connection(token, db_ref).await,
            Self::Turso => turso::build_connection(token, db_ref).await,
            Self::Railway => railway::build_connection(token, db_ref).await,
            Self::Nile => nile::build_connection(token, db_ref).await,
            Self::Upstash => upstash::build_connection(token, db_ref).await,
            Self::PostHog => posthog::build_connection(token, db_ref).await,
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
    pub db_type: String, // "postgres" | "mysql" | "libsql"
    /// For libsql, the full `libsql://` URL (there is no host/port split).
    pub host: String,
    pub port: u16,
    pub username: String,
    /// For libsql, the database auth token.
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
            // Drop idle sockets before the far end does. reqwest keeps them 90s
            // by default, while the AWS load balancers in front of TiDB Cloud's
            // API (and most others here) close idle connections at 60s: the
            // next request went out on a dead socket and failed with a bare
            // "error sending request", sometimes twice in a row.
            .pool_idle_timeout(std::time::Duration::from_secs(20))
            // Same reason as the Cloudflare client: Neon, Supabase, PlanetScale
            // and Prisma discovery all run through here, and an unbounded call
            // shows as a provider panel that never finishes loading.
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build provider HTTP client")
    })
}

/// Merge per-organization (or per-workspace) results where some may fail.
///
/// A token often covers only some of the orgs a user belongs to: PlanetScale
/// answers 403 for an org not picked on the consent screen, Railway errors for a
/// workspace the token wasn't shared. One of those must not fail the whole list.
/// Rules: an ended session (401) always wins, since nothing else will work
/// either; otherwise skip failures, and only fail when every page did, with the
/// first error plus `hint`.
pub(crate) fn merge_partial<T>(pages: Vec<Result<T, String>>, hint: &str) -> Result<Vec<T>, String> {
    if pages.iter().any(|r| matches!(r, Err(e) if e == UNAUTHORIZED)) {
        return Err(UNAUTHORIZED.into());
    }
    if !pages.is_empty() && pages.iter().all(Result::is_err) {
        let first = pages.into_iter().find_map(Result::err).unwrap_or_default();
        return Err(if hint.is_empty() { first } else { format!("{first}. {hint}") });
    }
    Ok(pages.into_iter().filter_map(Result::ok).collect())
}

/// A reqwest error with its causes. reqwest's own message stops at "error
/// sending request for url (…)", which hides whether it was DNS, a refused
/// connection, TLS, or a reset socket.
pub(crate) fn describe(e: &reqwest::Error) -> String {
    let mut out = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(cause) = src {
        let c = cause.to_string();
        if !out.contains(&c) {
            out.push_str(": ");
            out.push_str(&c);
        }
        src = cause.source();
    }
    out
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

/// The local end of an OAuth redirect, on both loopback addresses.
///
/// A redirect to `http://localhost:…` is resolved by the browser, and some
/// setups (IPv6-first resolvers, a proxy, some Linux configs) send it to `::1`
/// before `127.0.0.1`. Listening on IPv4 alone left those sign-ins waiting on a
/// socket nothing connected to until the 5-minute timeout. The IPv6 listener is
/// best effort: a machine without IPv6 loopback just doesn't get one.
pub(crate) struct CallbackListener {
    v4: TcpListener,
    v6: Option<TcpListener>,
}

impl CallbackListener {
    async fn accept(&self) -> std::io::Result<(tokio::net::TcpStream, std::net::SocketAddr)> {
        match &self.v6 {
            Some(v6) => tokio::select! {
                r = self.v4.accept() => r,
                r = v6.accept() => r,
            },
            None => self.v4.accept().await,
        }
    }
}

pub(crate) async fn bind_callback_listener(ports: &[u16]) -> Result<(CallbackListener, u16), String> {
    for &port in ports {
        if let Ok(v4) = TcpListener::bind(format!("127.0.0.1:{port}")).await {
            // The port IPv4 actually got (differs from `port` only when it is 0),
            // so both addresses answer on the same one.
            let port = v4.local_addr().map(|a| a.port()).unwrap_or(port);
            let v6 = TcpListener::bind(format!("[::1]:{port}")).await.ok();
            return Ok((CallbackListener { v4, v6 }, port));
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


/// Wait for one OAuth redirect and return the value of `value_key` from its
/// query: the authorization code (`code`), or for a token redirect the token
/// itself (`jwt`).
pub(crate) async fn await_oauth_callback(
    listener: CallbackListener,
    expected_state: &str,
    value_key: &str,
    provider_label: &str,
) -> Result<String, String> {
    let ok_page = crate::oauth_page::page(true, provider_label);
    let err_page = crate::oauth_page::page(false, provider_label);
    let send = |html: &str| {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(),
            html
        )
    };

    // Keep accepting until a request that is actually the redirect arrives. The
    // listener used to take exactly one connection, so anything that reached the
    // port first spent it: a browser's speculative preconnect (a socket that
    // never sends), a `/favicon.ico` fetch, or any other local process. Every
    // connection is read concurrently, so a silent socket can't hold up the real
    // redirect behind it; the rest get a 404. AUTH_TIMEOUT_SECS still bounds it.
    async fn read_request(mut stream: tokio::net::TcpStream, value_key: String) -> Option<(tokio::net::TcpStream, String)> {
        let mut buf = vec![0u8; 8192];
        let n = match tokio::time::timeout(std::time::Duration::from_secs(10), stream.read(&mut buf)).await {
            Ok(Ok(n)) if n > 0 => n,
            _ => return None,
        };
        let req = String::from_utf8_lossy(&buf[..n]);
        let target = req.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("").to_string();
        let query = target.split_once('?').map(|(_, q)| q.to_string()).unwrap_or_default();
        let has_answer = query.split('&').any(|kv| {
            let k = kv.split('=').next().unwrap_or("");
            k == value_key || k == "error" || k == "state"
        });
        if !has_answer {
            let _ = stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
            return None;
        }
        Some((stream, query))
    }
    let mut pending = futures::stream::FuturesUnordered::new();
    let (mut stream, query) = loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (conn, _) = accepted.map_err(|e| format!("Callback accept failed: {e}"))?;
                pending.push(read_request(conn, value_key.to_string()));
            }
            Some(done) = futures::StreamExt::next(&mut pending), if !pending.is_empty() => {
                if let Some(hit) = done {
                    break hit;
                }
            }
        }
    };
    let query = query.as_str();

    let (mut code, mut state, mut error) = (None, None, None);
    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        let key = kv.next().unwrap_or("");
        let val = kv
            .next()
            .map(|v| urlencoding::decode(v).unwrap_or_default().into_owned())
            .unwrap_or_default();
        match key {
            k if k == value_key => code = Some(val),
            "state" => state = Some(val),
            "error" => error = Some(val),
            "error_description" if error.is_none() => error = Some(val),
            _ => {}
        }
    }

    if let Some(err) = error {
        let _ = stream.write_all(send(&err_page).as_bytes()).await;
        return Err(format!("Provider denied authorization: {err}"));
    }
    let code = match code {
        Some(c) if !c.is_empty() => c,
        _ => {
            let _ = stream.write_all(send(&err_page).as_bytes()).await;
            return Err("No authorization code in callback".into());
        }
    };
    if state.as_deref() != Some(expected_state) {
        let _ = stream.write_all(send(&err_page).as_bytes()).await;
        return Err("OAuth state mismatch - possible CSRF".into());
    }

    let _ = stream.write_all(send(&ok_page).as_bytes()).await;
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

    match p.sign_in() {
        SignIn::AuthCode => {}
        SignIn::DeviceCode => {
            let t = device_code_sign_in(&app, &cfg).await?;
            // No refresh token is stored even if one comes back: the refresh
            // path posts form-encoded to the stroke.click proxy, which this
            // provider isn't behind. Without one, an expired token ends the
            // session cleanly ("Sign in again") instead of failing to renew.
            store_tokens(&app, p, &t.access_token, None, t.expires_in, None).await?;
            return Ok(ProviderOAuthStatus { connected: true, email: None });
        }
        SignIn::TokenRedirect => {
            let token = token_redirect_sign_in(&app, &cfg, &state).await?;
            store_tokens(&app, p, &token, None, None, None).await?;
            return Ok(ProviderOAuthStatus { connected: true, email: None });
        }
    }

    if cfg.client_id.is_empty() {
        return Err(format!(
            "{} sign-in isn't set up in this build yet: its OAuth app client id is missing.",
            p.label()
        ));
    }
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
    // OIDC only issues a refresh token for `offline_access` together with
    // `prompt=consent`; Railway's access tokens last an hour without one. Only
    // Railway: Neon and Prisma already return refresh tokens without it.
    let prompt_param = if p == Provider::Railway {
        "&prompt=consent"
    } else {
        ""
    };
    let auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}{}&state={}{}{}",
        cfg.auth_url,
        urlencoding::encode(cfg.client_id),
        urlencoding::encode(&redirect_uri),
        scope_param,
        urlencoding::encode(&state),
        pkce_param,
        prompt_param,
    );

    eprintln!("[provider oauth] {} authorize URL: {auth_url}", p.key());
    open_sign_in_page(&app, &auth_url);

    // Register the cancel waiter BEFORE awaiting so a Cancel click can't slip
    // through between opening the browser and starting to wait.
    let cancelled = oauth_cancel().notified();
    let code = tokio::select! {
        r = tokio::time::timeout(
            std::time::Duration::from_secs(AUTH_TIMEOUT_SECS),
            await_oauth_callback(listener, &state, "code", p.label()),
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

/// Hand the sign-in page to the UI, then try to open it in the browser.
///
/// The UI gets the URL first so the "Waiting for…" panel can offer "Open again"
/// and "Copy link". A failure to open the browser is no longer fatal: with no
/// default browser set (common on Linux), or a closed tab, the sign-in used to
/// die with "Could not open browser" or wait five minutes with no way back. Now
/// the flow keeps waiting and the link is on screen.
pub(crate) fn open_sign_in_page(app: &tauri::AppHandle, url: &str) {
    let _ = tauri::Emitter::emit(app, "provider-auth-url", serde_json::json!({ "url": url }));
    if let Err(e) = tauri_plugin_opener::OpenerExt::opener(app).open_url(url, None::<&str>) {
        eprintln!("[provider oauth] could not open the browser: {e}");
    }
}

/// OAuth 2.0 device code grant (RFC 8628). Opens the verification page with the
/// code prefilled, then polls the token endpoint until the user approves it,
/// declines it, the code expires, or they hit Cancel.
async fn device_code_sign_in(app: &tauri::AppHandle, cfg: &OAuthConfig) -> Result<TokenResponse, String> {
    let resp = http()
        .post(cfg.auth_url)
        .json(&serde_json::json!({ "client_id": cfg.client_id }))
        .send()
        .await
        .map_err(|e| format!("Device authorization failed: {e}"))?;
    let status = resp.status().as_u16();
    let auth: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Device authorization returned bad JSON: {e}"))?;
    if !(200..300).contains(&status) {
        let msg = auth["error_description"].as_str().or_else(|| auth["error"].as_str()).unwrap_or("request failed");
        return Err(format!("Device authorization failed ({status}): {msg}"));
    }
    let device_code = auth["device_code"].as_str().ok_or("Device authorization: missing device_code")?.to_string();
    let verify_url = auth["verification_uri_complete"]
        .as_str()
        .or_else(|| auth["verification_uri"].as_str())
        .ok_or("Device authorization: missing verification URL")?
        .to_string();
    let mut interval = auth["interval"].as_u64().unwrap_or(5).max(1);
    let expires = auth["expires_in"].as_u64().unwrap_or(AUTH_TIMEOUT_SECS).min(AUTH_TIMEOUT_SECS);

    // The confirmation code goes to the UI, not just a log line: the user checks
    // that the code on TiDB's page matches the one Stroke shows, which is the
    // whole point of the device grant, and it's the way back if the tab closes.
    let _ = tauri::Emitter::emit(
        app,
        "provider-device-code",
        serde_json::json!({
            "userCode": auth["user_code"].as_str().unwrap_or_default(),
            "verificationUri": auth["verification_uri"].as_str().unwrap_or(&verify_url),
            "verificationUriComplete": verify_url,
            "expiresIn": expires,
        }),
    );
    // The code and a reopen button are already on screen, so a browser that
    // won't open is not a reason to abandon the sign-in.
    if let Err(e) = tauri_plugin_opener::OpenerExt::opener(app).open_url(verify_url, None::<&str>) {
        eprintln!("[provider oauth] could not open the browser: {e}");
    }

    let started = std::time::Instant::now();
    let cancelled = oauth_cancel().notified();
    tokio::pin!(cancelled);
    // Counts a Cancel that lands between polls, not only one during a sleep.
    cancelled.as_mut().enable();
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(interval)) => {}
            _ = &mut cancelled => return Err("cancelled".into()),
        }
        if started.elapsed().as_secs() >= expires {
            return Err("Authorization timed out".into());
        }
        let resp = http()
            .post(cfg.token_url)
            .json(&serde_json::json!({
                "device_code": device_code,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "client_id": cfg.client_id,
            }))
            .send()
            .await
            .map_err(|e| format!("Token request failed: {e}"))?;
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        if let Some(access) = body["access_token"].as_str().filter(|t| !t.is_empty()) {
            return Ok(TokenResponse {
                access_token: access.to_string(),
                refresh_token: None,
                expires_in: body["expires_in"].as_u64(),
            });
        }
        match body["error"].as_str().unwrap_or("") {
            "authorization_pending" | "" => {}
            "slow_down" => interval += 5,
            "expired_token" => return Err("Authorization timed out".into()),
            "access_denied" => return Err("Provider denied authorization: access_denied".into()),
            other => return Err(format!("OAuth token error: {other}")),
        }
    }
}

/// Turso-style CLI login: the login page redirects to our localhost listener
/// with the API token in `?jwt=`.
async fn token_redirect_sign_in(
    app: &tauri::AppHandle,
    cfg: &OAuthConfig,
    state: &str,
) -> Result<String, String> {
    let (listener, port) = bind_callback_listener(CALLBACK_PORTS).await?;
    let url = format!(
        "{}/?port={port}&redirect=true&type=cli&state={}",
        cfg.auth_url.trim_end_matches('/'),
        urlencoding::encode(state),
    );
    open_sign_in_page(app, &url);
    let cancelled = oauth_cancel().notified();
    tokio::select! {
        r = tokio::time::timeout(
            std::time::Duration::from_secs(AUTH_TIMEOUT_SECS),
            await_oauth_callback(listener, state, "jwt", "Turso"),
        ) => r.map_err(|_| "Authorization timed out".to_string())?,
        _ = cancelled => Err("cancelled".to_string()),
    }
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

/// Open the HTTPS connection to every signed-in provider's API ahead of use.
///
/// The first call to an API pays DNS, TCP and TLS before the request itself:
/// three or four round trips, which to PlanetScale's or Neon's API from a far
/// region is most of a second on top of the request. Called when the connect
/// dialog opens; the shared client keeps the socket for the listing and the
/// connect that follow. Unauthenticated `HEAD /`: no token leaves the app, and
/// whatever it answers is thrown away.
#[tauri::command]
pub async fn provider_warm(app: tauri::AppHandle) {
    let map = crate::secrets::read_all_async(&app).await;
    for p in Provider::ALL {
        let Some(origin) = p.api_origin() else { continue };
        if !map.contains_key(&format!("__{}_access__", p.key())) {
            continue;
        }
        tokio::spawn(async move {
            let _ = http().head(origin).timeout(std::time::Duration::from_secs(5)).send().await;
        });
    }
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
    let t0 = std::time::Instant::now();
    let r = with_token(&app, p, |token| async move { p.list_databases(&token).await }).await;
    // "The provider panel is slow" is otherwise unattributable: this says which
    // provider, and whether it was the listing or the connect that took the time.
    log::info!("{} list_databases: {} in {}ms", p.key(), if r.is_ok() { "ok" } else { "failed" }, t0.elapsed().as_millis());
    r
}

#[tauri::command]
pub async fn provider_build_connection(
    app: tauri::AppHandle,
    provider: String,
    db_ref: String,
) -> Result<ProviderConnection, String> {
    let p = Provider::parse(&provider)?;
    let t0 = std::time::Instant::now();
    let r = with_token(&app, p, |token| {
        let db_ref = db_ref.clone();
        async move { p.build_connection(&token, &db_ref).await }
    })
    .await;
    log::info!("{} build_connection: {} in {}ms", p.key(), if r.is_ok() { "ok" } else { "failed" }, t0.elapsed().as_millis());
    r
}

#[cfg(test)]
mod callback_tests {
    use super::await_oauth_callback;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn hit(port: u16, path: &str) -> String {
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes()).await.unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out).await;
        out
    }

    /// A redirect that the browser sends to `::1` is answered too.
    #[tokio::test]
    async fn the_callback_answers_on_ipv6_loopback() {
        let Ok((listener, port)) = super::bind_callback_listener(&[0]).await else { return };
        if listener.v6.is_none() {
            return; // no IPv6 loopback on this machine
        }
        let waiter = tokio::spawn(async move { await_oauth_callback(listener, "s6", "code", "Neon").await });
        let mut s = tokio::net::TcpStream::connect(("::1", port)).await.unwrap();
        s.write_all(b"GET /oauth/callback?code=v6&state=s6 HTTP/1.1\r\nHost: localhost\r\n\r\n").await.unwrap();
        assert_eq!(waiter.await.unwrap().unwrap(), "v6");
    }

    /// A stray request (favicon, preconnect) used to spend the only accept and
    /// fail the sign-in. It must be answered 404 and the wait must go on.
    #[tokio::test]
    async fn stray_requests_do_not_consume_the_callback() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let listener = super::CallbackListener { v4: listener, v6: None };
        let waiter = tokio::spawn(async move { await_oauth_callback(listener, "s1", "code", "Neon").await });
        // A socket that connects and never sends, then a favicon fetch.
        let _silent = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let favicon = hit(port, "/favicon.ico").await;
        assert!(favicon.starts_with("HTTP/1.1 404"), "{favicon}");
        let ok = hit(port, "/oauth/callback?code=abc&state=s1").await;
        assert!(ok.contains("200 OK"));
        assert_eq!(waiter.await.unwrap().unwrap(), "abc");
    }
}

#[cfg(test)]
mod merge_tests {
    use super::{merge_partial, UNAUTHORIZED};

    #[test]
    fn one_ungranted_org_does_not_fail_the_list() {
        let pages = vec![Ok(1), Err("PlanetScale API error (403)".to_string()), Ok(3)];
        assert_eq!(merge_partial(pages, "hint").unwrap(), vec![1, 3]);
    }

    #[test]
    fn all_failing_reports_the_first_error_with_the_hint() {
        let pages: Vec<Result<u8, String>> = vec![Err("403 a".into()), Err("403 b".into())];
        assert_eq!(merge_partial(pages, "Sign in again.").unwrap_err(), "403 a. Sign in again.");
    }

    #[test]
    fn an_ended_session_always_wins() {
        let pages = vec![Ok(1), Err(UNAUTHORIZED.to_string())];
        assert_eq!(merge_partial(pages, "").unwrap_err(), UNAUTHORIZED);
    }
}
