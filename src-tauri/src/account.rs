//! Stroke account: sign in with GitHub or Google through the website, and take
//! the license from the account.
//!
//! Flow (docs/accounts-and-sync.md, Appendix A):
//! 1. Bind a one-shot listener on `127.0.0.1:0`, open
//!    `<base>/desktop/sign-in?port&state&code_challenge&device_id&device_name`.
//! 2. The site redirects to `http://127.0.0.1:<port>/callback?code&state`
//!    (or `error=cancelled`). We check `state` and answer a tiny HTML page.
//! 3. `POST /api/desktop/token` with the PKCE verifier returns a bearer token,
//!    which goes into the keychain vault (`secrets.rs`), never a file.
//! 4. `GET /api/me` returns the entitlement, signed with the license key. We
//!    verify the signature over the entitlement's JSON text exactly as sent,
//!    cache the whole response in `<app_data>/account.json`, and trust it offline
//!    until `valid_until`. Every read of the cache re-verifies the signature, so
//!    editing the file grants nothing.
//!
//! Existing customers: an activated `license.json` keeps working with or without
//! an account (see `combine`), and the device trial still applies to anyone who
//! is not signed in.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::Manager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::license::LicenseStatus;

const DEFAULT_BASE: &str = "https://stroke.click";
/// Vault entry holding the desktop session token.
const TOKEN_KEY: &str = "__stroke_account_token__";
const CACHE_FILE: &str = "account.json";
/// Present while a new install still has to sign in (or enter a key).
const ONBOARDING_FILE: &str = ".account-onboarding";
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// Base URL of the website/API. `STROKE_API_BASE` overrides it for local dev.
pub fn base_url() -> String {
    std::env::var("STROKE_API_BASE")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE.to_string())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ── PKCE ──────────────────────────────────────────────────────────────────────

fn random_b64(n: usize) -> String {
    let mut bytes = vec![0u8; n];
    getrandom::getrandom(&mut bytes).expect("getrandom failed");
    URL_SAFE_NO_PAD.encode(bytes)
}

/// S256 code challenge: base64url(SHA-256(verifier)), no padding.
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// A fresh (verifier, challenge) pair. 32 random bytes give a 43-char verifier,
/// the RFC 7636 minimum length.
pub fn pkce_pair() -> (String, String) {
    let verifier = random_b64(32);
    let challenge = pkce_challenge(&verifier);
    (verifier, challenge)
}

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct User {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EntLicense {
    #[serde(default)]
    pub plan: String,
    /// 0 = lifetime.
    #[serde(default)]
    pub expires_at: u64,
    #[serde(default)]
    pub key_hint: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EntTrial {
    pub started_at: u64,
    pub ends_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Entitlement {
    pub plan: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub license: Option<EntLicense>,
    #[serde(default)]
    pub trial: Option<EntTrial>,
    pub issued_at: u64,
    pub valid_until: u64,
    /// The device this entitlement was issued to, when the server includes it.
    /// It is inside the signed text, so it can't be edited: a cache copied to
    /// another machine stops counting there.
    #[serde(default)]
    pub device_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Device {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub last_seen_at: u64,
    #[serde(default)]
    pub current: bool,
}

/// `/api/me` as received. `entitlement` stays raw: the signature covers its
/// exact text, and re-serialising could change it.
#[derive(Deserialize)]
struct MeWire {
    user: User,
    entitlement: Box<RawValue>,
    entitlement_sig: String,
    #[serde(default)]
    devices: Vec<Device>,
}

/// A signature-checked `/api/me`.
#[derive(Debug, Clone, PartialEq)]
pub struct Me {
    pub user: User,
    pub entitlement: Entitlement,
    pub devices: Vec<Device>,
}

fn public_key() -> Result<VerifyingKey, String> {
    // Development builds can trust a local stroke-web's key (its .dev.vars
    // signs with a dev key). Release builds always use the embedded key.
    #[cfg(debug_assertions)]
    let hex_key = std::env::var("STROKE_ACCOUNT_PUBKEY_HEX").unwrap_or_else(|_| crate::license::PUBLIC_KEY_HEX.to_string());
    #[cfg(not(debug_assertions))]
    let hex_key = crate::license::PUBLIC_KEY_HEX.to_string();
    let bytes: [u8; 32] = hex::decode(hex_key)
        .map_err(|_| "invalid public key constant".to_string())?
        .try_into()
        .map_err(|_| "public key must be 32 bytes".to_string())?;
    VerifyingKey::from_bytes(&bytes).map_err(|e| format!("bad public key: {e}"))
}

fn verify_sig(key: &VerifyingKey, msg: &[u8], sig_b64: &str) -> Result<(), String> {
    let raw = URL_SAFE_NO_PAD
        .decode(sig_b64.trim().trim_end_matches('='))
        .map_err(|_| "entitlement signature is not base64url".to_string())?;
    let arr: [u8; 64] = raw
        .try_into()
        .map_err(|_| "entitlement signature must be 64 bytes".to_string())?;
    key.verify(msg, &Signature::from_bytes(&arr))
        .map_err(|_| "entitlement signature does not verify".to_string())
}

/// Parse `/api/me` (or `/api/license/link`) and verify `entitlement_sig` over the
/// entitlement's JSON text exactly as it appears in `raw`.
pub fn parse_me_with_key(raw: &str, key: &VerifyingKey) -> Result<Me, String> {
    let wire: MeWire = serde_json::from_str(raw).map_err(|e| format!("unexpected /api/me reply: {e}"))?;
    verify_sig(key, wire.entitlement.get().as_bytes(), &wire.entitlement_sig)?;
    let entitlement: Entitlement = serde_json::from_str(wire.entitlement.get())
        .map_err(|e| format!("unexpected entitlement: {e}"))?;
    Ok(Me { user: wire.user, entitlement, devices: wire.devices })
}

pub fn parse_me(raw: &str) -> Result<Me, String> {
    parse_me_with_key(raw, &public_key()?)
}

/// The cached entitlement may be trusted while `now < valid_until`.
pub fn entitlement_fresh(ent: &Entitlement, now: u64) -> bool {
    now < ent.valid_until
}

// ── Cache file ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
struct CacheFile {
    /// The `/api/me` body, verbatim.
    me: String,
    fetched_at: u64,
    /// SHA-256 of the license key already sent to `/api/license/link`.
    #[serde(default)]
    linked_key_hash: Option<String>,
    /// Why linking failed in a way retrying won't fix (`key_owned_elsewhere`).
    #[serde(default)]
    link_error: Option<String>,
    /// The device that wrote this cache. A copy taken to another machine is
    /// ignored there, which has to sign in for itself.
    #[serde(default)]
    device_id: Option<String>,
}

fn cache_path(dir: &Path) -> PathBuf {
    dir.join(CACHE_FILE)
}

fn read_cache(dir: &Path) -> Option<CacheFile> {
    serde_json::from_str(&std::fs::read_to_string(cache_path(dir)).ok()?).ok()
}

fn write_cache(dir: &Path, c: &CacheFile) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(c).map_err(|e| e.to_string())?;
    std::fs::write(cache_path(dir), json).map_err(|e| e.to_string())
}

fn delete_cache(dir: &Path) {
    let _ = std::fs::remove_file(cache_path(dir));
}

/// The cached, re-verified `/api/me`, regardless of freshness.
fn cached_me(dir: &Path) -> Option<(Me, CacheFile)> {
    let c = read_cache(dir)?;
    let me = parse_me(&c.me).ok()?;
    // Bound to this machine: by the signed entitlement when the server names a
    // device, and by the cache's own record of where it was written.
    let here = crate::license::device_id();
    if me.entitlement.device_id.as_deref().is_some_and(|d| d != here) {
        return None;
    }
    if c.device_id.as_deref().is_some_and(|d| d != here) {
        return None;
    }
    Some((me, c))
}

// ── Licensing integration ─────────────────────────────────────────────────────

/// What a fresh entitlement means in `LicenseStatus` terms.
pub fn entitlement_status(ent: &Entitlement, email: &str, now: u64) -> LicenseStatus {
    if ent.plan == "pro" {
        if ent.source == "trial" {
            if let Some(t) = &ent.trial {
                if t.ends_at <= now {
                    return LicenseStatus::TrialExpired;
                }
                let days = t.ends_at.saturating_sub(now).div_ceil(86_400) as i64;
                return LicenseStatus::Trial { days_remaining: days.max(1) };
            }
        }
        let lic = ent.license.as_ref();
        return LicenseStatus::Valid {
            email: email.to_string(),
            plan: lic.map(|l| l.plan.clone()).filter(|p| !p.is_empty()).unwrap_or_else(|| "pro".into()),
            issued_at: ent.issued_at,
            expires_at: lic.map(|l| l.expires_at).filter(|&e| e > 0),
        };
    }
    LicenseStatus::TrialExpired
}

/// Merge the account's verdict into the local (key / device-trial) one.
///
/// - No fresh account entitlement: the local status, unchanged.
/// - Account grants Pro via a license: that wins.
/// - A valid local `license.json` is never downgraded by the account: Pro.
/// - Otherwise the account decides (its trial, or no Pro), replacing the
///   per-device trial for signed-in users.
pub fn combine(local: LicenseStatus, account: Option<LicenseStatus>) -> LicenseStatus {
    match account {
        None => local,
        Some(acc @ LicenseStatus::Valid { .. }) => acc,
        Some(acc) => match local {
            LicenseStatus::Valid { .. } => local,
            _ => acc,
        },
    }
}

/// The account's status from the cache only: no network, no keychain. Used by
/// the license commands so existing Pro gates see account entitlements.
pub fn account_license_status(dir: &Path) -> Option<LicenseStatus> {
    let (me, _) = cached_me(dir)?;
    // Clock-guarded: setting the clock back can't stretch a cached entitlement.
    let now = crate::license::trusted_now(dir);
    if !entitlement_fresh(&me.entitlement, now) {
        return None;
    }
    Some(entitlement_status(&me.entitlement, &me.user.email, now))
}

/// A debug build pointed at a local stroke-web (`STROKE_API_BASE=http://localhost:…`)
/// is a developer's own setup: never gate it behind the trial.
fn local_dev_pro() -> Option<LicenseStatus> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let base = base_url();
    let host = base.split("://").nth(1)?.split(['/', ':']).next()?;
    matches!(host, "localhost" | "127.0.0.1").then(|| LicenseStatus::Valid {
        email: "dev@localhost".into(),
        plan: "pro".into(),
        issued_at: 0,
        expires_at: None,
    })
}

pub fn apply_account(dir: &Path, local: LicenseStatus) -> LicenseStatus {
    if let Some(dev) = local_dev_pro() {
        return dev;
    }
    if matches!(local, LicenseStatus::Error { .. }) {
        return local;
    }
    combine(local, account_license_status(dir))
}

// ── New-install detection ─────────────────────────────────────────────────────

/// Record, once per run and before anything writes the trial files, whether
/// this is a brand-new install (no license, no trial ever started). A new
/// install gets a marker that keeps the sign-in screen up across relaunches
/// until the user signs in or enters a key. Existing installs never get one.
pub fn note_install_state(dir: &Path) {
    static NOTED: OnceLock<()> = OnceLock::new();
    NOTED.get_or_init(|| {
        let fresh = crate::license::load_license(dir).is_none()
            && crate::license::local_trial_start(dir).is_none()
            && read_cache(dir).is_none();
        if fresh {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(dir.join(ONBOARDING_FILE), b"1");
        }
    });
}

fn onboarding_pending(dir: &Path) -> bool {
    dir.join(ONBOARDING_FILE).exists()
}

fn clear_onboarding(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(ONBOARDING_FILE));
}

// ── HTTP ──────────────────────────────────────────────────────────────────────

pub(crate) fn http() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default()
    })
}

#[derive(Debug, PartialEq)]
pub enum ApiError {
    /// 401: the token is unknown or revoked.
    Unauthorized,
    /// No answer (offline, DNS, timeout).
    Network(String),
    /// The server answered with an error: (code, message).
    Server(String, String),
}

impl ApiError {
    pub(crate) fn user_message(&self) -> String {
        match self {
            ApiError::Unauthorized => "Your session has ended. Sign in again.".into(),
            ApiError::Network(_) => "Couldn't reach stroke.click. Check your connection and try again.".into(),
            ApiError::Server(code, msg) => match code.as_str() {
                "invalid_code" => "The sign-in link was already used. Start again.".into(),
                "expired_code" => "The sign-in took too long. Start again.".into(),
                "pkce_mismatch" => "The sign-in didn't match this app. Start again.".into(),
                "key_owned_elsewhere" => "This license key belongs to another account.".into(),
                "rate_limited" => "Too many attempts. Wait a minute and try again.".into(),
                _ if !msg.is_empty() => msg.clone(),
                _ => format!("Server error ({code})"),
            },
        }
    }
}

pub(crate) async fn read_error(resp: reqwest::Response) -> ApiError {
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return ApiError::Unauthorized;
    }
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    ApiError::Server(
        body["error"].as_str().unwrap_or(status.as_str()).to_string(),
        body["message"].as_str().unwrap_or("").to_string(),
    )
}

#[derive(Deserialize, Debug)]
pub struct TokenResponse {
    pub token: String,
    /// Unused by the app (`/api/me` follows at once), kept for the contract tests.
    #[serde(default)]
    #[allow(dead_code)]
    pub user: Option<User>,
}

pub async fn exchange_code(
    base: &str,
    code: &str,
    verifier: &str,
    device_id: &str,
    device_name: &str,
    version: &str,
) -> Result<TokenResponse, ApiError> {
    let resp = http()
        .post(format!("{base}/api/desktop/token"))
        .json(&serde_json::json!({
            "code": code,
            "code_verifier": verifier,
            "device_id": device_id,
            "device_name": device_name,
            "os": std::env::consts::OS,
            "version": version,
        }))
        .send()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(read_error(resp).await);
    }
    let t: TokenResponse = resp
        .json()
        .await
        .map_err(|e| ApiError::Server("bad_response".into(), e.to_string()))?;
    if t.token.is_empty() {
        return Err(ApiError::Server("bad_response".into(), "No token in the reply".into()));
    }
    Ok(t)
}

/// Raw text of a `/api/me`-shaped reply (`GET /api/me` or `POST /api/license/link`).
async fn me_text(req: reqwest::RequestBuilder) -> Result<String, ApiError> {
    let resp = req.send().await.map_err(|e| ApiError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(read_error(resp).await);
    }
    resp.text().await.map_err(|e| ApiError::Network(e.to_string()))
}

pub async fn fetch_me(base: &str, token: &str) -> Result<String, ApiError> {
    me_text(http().get(format!("{base}/api/me")).bearer_auth(token)).await
}

pub async fn link_license(base: &str, token: &str, key: &str) -> Result<String, ApiError> {
    me_text(
        http()
            .post(format!("{base}/api/license/link"))
            .bearer_auth(token)
            .json(&serde_json::json!({ "key": key })),
    )
    .await
}

pub async fn remote_sign_out(base: &str, token: &str) {
    let _ = http()
        .post(format!("{base}/api/desktop/sign-out"))
        .bearer_auth(token)
        .send()
        .await;
}

// ── Loopback callback ─────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum Callback {
    Code(String),
    Cancelled,
    Error(String),
}

/// Parse the request target of a callback hit. `None` = not our redirect
/// (favicon, a preconnect, any other path): answer 404 and keep waiting.
pub fn parse_callback(target: &str, expected_state: &str) -> Option<Callback> {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != "/callback" {
        return None;
    }
    let (mut code, mut state, mut error) = (None, None, None);
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v = urlencoding::decode(&v.replace('+', " ")).map(|s| s.into_owned()).unwrap_or_default();
        match k {
            "code" => code = Some(v),
            "state" => state = Some(v),
            "error" => error = Some(v),
            _ => {}
        }
    }
    if state.is_none() && code.is_none() && error.is_none() {
        return None;
    }
    if state.as_deref() != Some(expected_state) {
        return Some(Callback::Error("The sign-in reply didn't match this request. Start again.".into()));
    }
    if let Some(e) = error {
        return Some(if e == "cancelled" { Callback::Cancelled } else { Callback::Error(format!("Sign-in failed: {e}")) });
    }
    match code {
        Some(c) if !c.is_empty() => Some(Callback::Code(c)),
        _ => Some(Callback::Error("No sign-in code in the reply.".into())),
    }
}

fn callback_page(heading: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="dark light"><title>Stroke</title>
<style>body{{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;font:15px/1.5 ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;background:#0b0b0c;color:#ededee;padding:16px}}main{{max-width:380px}}h1{{font-size:19px;font-weight:600;margin:0 0 6px}}p{{margin:0;color:#a1a1a8}}@media (prefers-color-scheme: light){{body{{background:#f6f6f7;color:#18181b}}p{{color:#5b5b63}}}}</style></head>
<body><main><h1>{heading}</h1><p>{body}</p></main></body></html>"#
    )
}

async fn respond(stream: &mut TcpStream, status: &str, html: &str) {
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = stream.write_all(resp.as_bytes()).await;
    let _ = stream.flush().await;
}

/// Read one request's target. `None` for a silent or unreadable socket.
async fn read_target(mut stream: TcpStream) -> Option<(TcpStream, String)> {
    let mut buf = vec![0u8; 8192];
    let n = match tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buf)).await {
        Ok(Ok(n)) if n > 0 => n,
        _ => return None,
    };
    let req = String::from_utf8_lossy(&buf[..n]);
    let mut first = req.lines().next().unwrap_or("").split_whitespace();
    let method = first.next().unwrap_or("");
    let target = first.next().unwrap_or("").to_string();
    if method != "GET" {
        return None;
    }
    Some((stream, target))
}

/// Wait for the redirect. Every connection is read concurrently, so a
/// browser's speculative preconnect can't hold up the real request; anything
/// that isn't `/callback` gets a 404.
pub async fn wait_for_callback(listener: TcpListener, expected_state: &str) -> Result<String, String> {
    let mut pending = futures::stream::FuturesUnordered::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (conn, _) = accepted.map_err(|e| format!("Callback accept failed: {e}"))?;
                pending.push(read_target(conn));
            }
            Some(done) = futures::StreamExt::next(&mut pending), if !pending.is_empty() => {
                let Some((mut stream, target)) = done else { continue };
                match parse_callback(&target, expected_state) {
                    None => {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                    }
                    Some(Callback::Code(code)) => {
                        respond(&mut stream, "200 OK", &callback_page(
                            "You're signed in",
                            "You can close this tab and return to Stroke.",
                        )).await;
                        return Ok(code);
                    }
                    Some(Callback::Cancelled) => {
                        respond(&mut stream, "200 OK", &callback_page(
                            "Sign-in cancelled",
                            "You can close this tab and return to Stroke.",
                        )).await;
                        return Err("cancelled".into());
                    }
                    Some(Callback::Error(msg)) => {
                        respond(&mut stream, "400 Bad Request", &callback_page(
                            "Sign-in didn't finish",
                            "Close this tab and start the sign-in again from Stroke.",
                        )).await;
                        return Err(msg);
                    }
                }
            }
        }
    }
}

pub fn sign_in_url(base: &str, port: u16, state: &str, challenge: &str, device_id: &str, device_name: &str, provider: Option<&str>) -> String {
    let mut url = format!(
        "{base}/desktop/sign-in?port={port}&state={}&code_challenge={}&device_id={}&device_name={}",
        urlencoding::encode(state),
        urlencoding::encode(challenge),
        urlencoding::encode(device_id),
        urlencoding::encode(device_name),
    );
    if let Some(p) = provider.filter(|p| matches!(*p, "github" | "google")) {
        url.push_str("&provider=");
        url.push_str(p);
    }
    url
}

// ── Device name ───────────────────────────────────────────────────────────────

/// 1-80 chars, no control characters (the sign-in page rejects anything else).
fn clean_device_name(raw: &str) -> String {
    let s: String = raw.chars().filter(|c| !c.is_control()).take(80).collect();
    let s = s.trim().to_string();
    if s.is_empty() { "This computer".to_string() } else { s }
}

async fn device_name() -> String {
    clean_device_name(&raw_device_name().await)
}

async fn raw_device_name() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = tokio::process::Command::new("scutil").args(["--get", "ComputerName"]).output().await {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if out.status.success() && !s.is_empty() {
                return s.chars().take(80).collect();
            }
        }
    }
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .or_else(|_| std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()))
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "This computer".to_string())
        .chars()
        .take(80)
        .collect()
}

async fn this_device_id() -> String {
    tauri::async_runtime::spawn_blocking(crate::license::device_id)
        .await
        .unwrap_or_default()
}

// ── Token in the vault ────────────────────────────────────────────────────────

pub(crate) async fn load_token(app: &tauri::AppHandle) -> Option<String> {
    crate::secrets::read_all_async(app)
        .await
        .get(TOKEN_KEY)
        .cloned()
        .filter(|t| !t.is_empty())
}

async fn store_token(app: &tauri::AppHandle, token: String) -> Result<(), String> {
    crate::secrets::update_async(app, move |m| {
        m.insert(TOKEN_KEY.to_string(), token);
    })
    .await
}

/// Signing out (or a revoked token) also drops the sync key: the next account
/// on this device must unlock sync with its own passphrase.
async fn delete_token(app: &tauri::AppHandle) {
    let _ = crate::secrets::update_async(app, |m| {
        m.remove(TOKEN_KEY);
        m.remove(crate::sync::SYNC_KEY_VAULT);
    })
    .await;
}

// ── Status for the UI ─────────────────────────────────────────────────────────

#[derive(Serialize, Debug, Clone, Default)]
pub struct AccountStatus {
    pub signed_in: bool,
    pub user: Option<User>,
    pub entitlement: Option<Entitlement>,
    /// The cached entitlement verified and is inside its validity window.
    pub entitlement_fresh: bool,
    pub devices: Vec<Device>,
    pub fetched_at: u64,
    /// The last refresh couldn't reach the server; showing the cache.
    pub offline: bool,
    /// New install that hasn't signed in or entered a key yet.
    pub needs_onboarding: bool,
    /// A license.json is present on this device.
    pub has_license_key: bool,
    pub link_error: Option<String>,
    pub error: Option<String>,
    pub manage_url: String,
}

fn build_status(dir: &Path, signed_in: bool) -> AccountStatus {
    let has_license_key = crate::license::load_license(dir).is_some();
    let mut s = AccountStatus {
        signed_in,
        has_license_key,
        needs_onboarding: !signed_in && !has_license_key && onboarding_pending(dir),
        manage_url: format!("{}/app", base_url()),
        ..Default::default()
    };
    if signed_in {
        if let Some((me, c)) = cached_me(dir) {
            s.entitlement_fresh = entitlement_fresh(&me.entitlement, crate::license::trusted_now(dir));
            s.user = Some(me.user);
            s.entitlement = Some(me.entitlement);
            s.devices = me.devices;
            s.fetched_at = c.fetched_at;
            s.link_error = c.link_error;
        }
    }
    s
}

/// Save a verified `/api/me` reply, keeping the link bookkeeping.
fn save_me(dir: &Path, raw: String) -> Result<Me, String> {
    let me = parse_me(&raw)?;
    let prev = read_cache(dir).unwrap_or_default();
    write_cache(
        dir,
        &CacheFile {
            me: raw,
            fetched_at: now_secs(),
            linked_key_hash: prev.linked_key_hash,
            link_error: prev.link_error,
            device_id: Some(crate::license::device_id()),
        },
    )?;
    // The server just answered: its issue time is real time. Reset the clock
    // guard to it, so a clock that was set wrong heals the moment we're online.
    crate::license::reset_clock_mark(dir, me.entitlement.issued_at);
    Ok(me)
}

/// Link the held license key to the account, once per key.
async fn link_held_key(dir: &Path, base: &str, token: &str) {
    let Some(lic) = crate::license::load_license(dir) else { return };
    let hash = crate::license::key_hash(&lic.key);
    let Some(cache) = read_cache(dir) else { return };
    if cache.linked_key_hash.as_deref() == Some(hash.as_str()) {
        return;
    }
    let result = link_license(base, token, &lic.key).await;
    let mut c = read_cache(dir).unwrap_or(cache);
    match result {
        Ok(raw) if parse_me(&raw).is_ok() => {
            c.me = raw;
            c.fetched_at = now_secs();
            c.linked_key_hash = Some(hash);
            c.link_error = None;
        }
        // Retrying won't change these: remember and stop asking.
        Err(ApiError::Server(code, _))
            if matches!(code.as_str(), "key_owned_elsewhere" | "not_found" | "invalid_key" | "revoked") =>
        {
            c.linked_key_hash = Some(hash);
            c.link_error = Some(code);
        }
        // Offline, 5xx, a bad reply: try again on the next refresh.
        _ => return,
    }
    let _ = write_cache(dir, &c);
}

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Refresh from the server when signed in. 401 signs out locally; a network
/// failure keeps the cache (trusted until `valid_until`).
async fn refresh(app: &tauri::AppHandle, dir: &Path, token: &str) -> AccountStatus {
    let base = base_url();
    match fetch_me(&base, token).await {
        Ok(raw) => match save_me(dir, raw) {
            Ok(_) => {
                link_held_key(dir, &base, token).await;
                build_status(dir, true)
            }
            Err(e) => AccountStatus { error: Some(e), ..build_status(dir, true) },
        },
        Err(ApiError::Unauthorized) => {
            delete_token(app).await;
            delete_cache(dir);
            AccountStatus { error: Some(ApiError::Unauthorized.user_message()), ..build_status(dir, false) }
        }
        Err(ApiError::Network(_)) => AccountStatus { offline: true, ..build_status(dir, true) },
        Err(e) => AccountStatus { error: Some(e.user_message()), ..build_status(dir, true) },
    }
}

// ── Sign-in flow state ────────────────────────────────────────────────────────

fn cancel_notify() -> &'static tokio::sync::Notify {
    static N: OnceLock<tokio::sync::Notify> = OnceLock::new();
    N.get_or_init(tokio::sync::Notify::new)
}

/// Generation of the newest sign-in, so a superseded one stops quietly.
static SIGN_IN_GEN: AtomicU64 = AtomicU64::new(0);

/// What the paste-a-code fallback needs from the sign-in in flight: the PKCE
/// verifier and device it was started with (Appendix B, last section).
#[derive(Clone)]
struct PendingSignIn {
    gen: u64,
    verifier: String,
    device_id: String,
    device_name: String,
}

fn pending_sign_in() -> &'static std::sync::Mutex<Option<PendingSignIn>> {
    static P: OnceLock<std::sync::Mutex<Option<PendingSignIn>>> = OnceLock::new();
    P.get_or_init(|| std::sync::Mutex::new(None))
}

fn set_pending(p: Option<PendingSignIn>) {
    *pending_sign_in().lock().unwrap_or_else(|e| e.into_inner()) = p;
}

/// Clear the pending sign-in only if it is still generation `gen`.
fn clear_pending(gen: u64) {
    let mut g = pending_sign_in().lock().unwrap_or_else(|e| e.into_inner());
    if g.as_ref().is_some_and(|p| p.gen == gen) {
        *g = None;
    }
}

/// Fires when a pasted code finished the sign-in, so the loopback wait ends.
fn pasted_notify() -> &'static tokio::sync::Notify {
    static N: OnceLock<tokio::sync::Notify> = OnceLock::new();
    N.get_or_init(tokio::sync::Notify::new)
}

/// The code inside `stroke-code:<code>` or a bare code. Codes are base64url
/// (32 random bytes on the server); anything else is a paste mistake.
pub fn parse_pasted_code(text: &str) -> Option<String> {
    let t = text.trim().trim_matches(|c| c == '"' || c == '\'' || c == '`').trim();
    let code = match t.get(..12) {
        Some(p) if p.eq_ignore_ascii_case("stroke-code:") => t[12..].trim(),
        _ => t,
    };
    let ok = (16..=256).contains(&code.len())
        && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    ok.then(|| code.to_string())
}

async fn run_sign_in(app: &tauri::AppHandle, provider: Option<String>) -> Result<AccountStatus, String> {
    // Starting again (e.g. GitHub, then Google) replaces the flow in progress.
    cancel_notify().notify_waiters();
    let gen = SIGN_IN_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let cancelled = cancel_notify().notified();
    tokio::pin!(cancelled);
    cancelled.as_mut().enable();
    let pasted = pasted_notify().notified();
    tokio::pin!(pasted);
    pasted.as_mut().enable();

    let dir = data_dir(app)?;
    let base = base_url();
    let (verifier, challenge) = pkce_pair();
    let device_id = this_device_id().await;
    let name = device_name().await;
    set_pending(Some(PendingSignIn { gen, verifier: verifier.clone(), device_id: device_id.clone(), device_name: name.clone() }));
    let result = sign_in_loopback(app, &dir, &base, provider, gen, (&verifier, &challenge), &device_id, &name, cancelled, pasted).await;
    clear_pending(gen);
    result
}

#[allow(clippy::too_many_arguments)]
async fn sign_in_loopback(
    app: &tauri::AppHandle,
    dir: &Path,
    base: &str,
    provider: Option<String>,
    gen: u64,
    (verifier, challenge): (&str, &str),
    device_id: &str,
    name: &str,
    mut cancelled: std::pin::Pin<&mut tokio::sync::futures::Notified<'_>>,
    mut pasted: std::pin::Pin<&mut tokio::sync::futures::Notified<'_>>,
) -> Result<AccountStatus, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Couldn't start the sign-in listener: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let state = random_b64(24);
    let url = sign_in_url(base, port, &state, challenge, device_id, name, provider.as_deref());

    let _ = tauri::Emitter::emit(app, "account-sign-in-url", serde_json::json!({ "url": url }));
    if let Err(e) = tauri_plugin_opener::OpenerExt::opener(app).open_url(&url, None::<&str>) {
        eprintln!("[account] could not open the browser: {e}");
    }

    let code = tokio::select! {
        r = tokio::time::timeout(SIGN_IN_TIMEOUT, wait_for_callback(listener, &state)) => match r {
            Ok(r) => r?,
            Err(_) => return Err("timeout".into()),
        },
        _ = &mut cancelled => return Err("cancelled".into()),
        // `account_paste_code` exchanged the code and stored the token.
        _ = &mut pasted => return Ok(build_status(dir, true)),
    };
    if SIGN_IN_GEN.load(Ordering::SeqCst) != gen {
        return Err("cancelled".into());
    }

    let version = app.package_info().version.to_string();
    let tok = exchange_code(base, &code, verifier, device_id, name, &version)
        .await
        .map_err(|e| e.user_message())?;
    store_token(app, tok.token.clone()).await?;
    clear_onboarding(dir);
    Ok(refresh(app, dir, &tok.token).await)
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Start the browser sign-in and resolve when it finishes. Errors are
/// `"cancelled"`, `"timeout"`, or a message the UI can show as is.
#[tauri::command]
pub async fn account_sign_in(app: tauri::AppHandle, provider: Option<String>) -> Result<AccountStatus, String> {
    run_sign_in(&app, provider).await
}

/// Finish the sign-in in flight with a code copied from the website
/// (`stroke-code:<code>` or the bare code), exactly as the loopback path does.
/// A bad code leaves the loopback wait running, so the user can try again.
#[tauri::command]
pub async fn account_paste_code(app: tauri::AppHandle, text: String) -> Result<AccountStatus, String> {
    let code = parse_pasted_code(&text).ok_or_else(|| "That doesn't look like a Stroke sign-in code.".to_string())?;
    let p = pending_sign_in()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| "Start signing in first, then paste the code.".to_string())?;
    let dir = data_dir(&app)?;
    let base = base_url();
    let version = app.package_info().version.to_string();
    let tok = exchange_code(&base, &code, &p.verifier, &p.device_id, &p.device_name, &version)
        .await
        .map_err(|e| e.user_message())?;
    store_token(&app, tok.token.clone()).await?;
    clear_onboarding(&dir);
    let status = refresh(&app, &dir, &tok.token).await;
    clear_pending(p.gen);
    pasted_notify().notify_waiters();
    Ok(status)
}

#[tauri::command]
pub fn account_cancel_sign_in() {
    SIGN_IN_GEN.fetch_add(1, Ordering::SeqCst);
    cancel_notify().notify_waiters();
}

/// The account as cached; with `refresh`, ask the server first (when signed in).
#[tauri::command]
pub async fn account_status(app: tauri::AppHandle, refresh: Option<bool>) -> Result<AccountStatus, String> {
    let dir = data_dir(&app)?;
    note_install_state(&dir);
    let token = load_token(&app).await;
    Ok(match token {
        Some(t) if refresh.unwrap_or(false) => self::refresh(&app, &dir, &t).await,
        Some(_) => build_status(&dir, true),
        // A failed keychain read also lands here, so the cache is left alone:
        // sign-out and a 401 are what delete it.
        None => build_status(&dir, false),
    })
}

/// Revoke the token on the server (best effort) and forget it locally.
/// `license.json` and all local data stay.
#[tauri::command]
pub async fn account_sign_out(app: tauri::AppHandle) -> Result<AccountStatus, String> {
    let dir = data_dir(&app)?;
    if let Some(t) = load_token(&app).await {
        remote_sign_out(&base_url(), &t).await;
    }
    delete_token(&app).await;
    delete_cache(&dir);
    Ok(build_status(&dir, false))
}

/// The new-install sign-in screen was skipped with a license key.
#[tauri::command]
pub fn account_skip_onboarding(app: tauri::AppHandle) -> Result<(), String> {
    clear_onboarding(&data_dir(&app)?);
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn sign(sk: &SigningKey, msg: &str) -> String {
        URL_SAFE_NO_PAD.encode(sk.sign(msg.as_bytes()).to_bytes())
    }

    /// An `/api/me` body with the entitlement written in a deliberately odd
    /// layout, to prove the signature is checked over the verbatim text.
    fn me_body(sk: &SigningKey, ent: &str) -> String {
        format!(
            r#"{{"user":{{"id":"u1","email":"a@b.co","name":"A","image":null}},"entitlement":{ent},"entitlement_sig":"{}","devices":[{{"device_id":"d","name":"Mac","os":"macos","last_seen_at":1,"current":true}}]}}"#,
            sign(sk, ent)
        )
    }

    const ENT: &str = r#"{ "plan":"pro","source":"license",
        "license":{"plan":"pro","expires_at":0,"key_hint":"abc123"},"trial":null,"issued_at":1700000000,"valid_until":1700604800 }"#;

    #[test]
    fn entitlement_with_device_id_parses_and_old_shape_still_does() {
        let sk = test_key();
        let vk = sk.verifying_key();
        // The server's new shape: device_id inside the signed text.
        let with = r#"{ "plan":"pro","source":"license","license":null,"trial":null,"issued_at":1,"valid_until":2,"device_id":"abc" }"#;
        let me = parse_me_with_key(&me_body(&sk, with), &vk).expect("new shape verifies");
        assert_eq!(me.entitlement.device_id.as_deref(), Some("abc"));
        // The old shape (no device_id) still verifies, as served before the update.
        let me = parse_me_with_key(&me_body(&sk, ENT), &vk).expect("old shape verifies");
        assert_eq!(me.entitlement.device_id, None);
    }

    #[test]
    fn pkce_challenge_vector() {
        // SHA-256 → base64url, cross-checked with
        // `printf %s <verifier> | openssl dgst -sha256 -binary | base64 | tr '+/' '-_' | tr -d =`.
        assert_eq!(
            pkce_challenge("stroke-pkce-test-verifier-0123456789-abcdefghijklmnop"),
            "iQ7FbTCljUrCbFJ3W3uPEYcLBitdMSdbrasTjBOVYY8"
        );
        let (v, c) = pkce_pair();
        assert_eq!(v.len(), 43);
        assert!(v.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'));
        assert_eq!(c, pkce_challenge(&v));
    }

    #[test]
    fn pasted_code_parsing() {
        let code = "AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_abcde";
        assert_eq!(parse_pasted_code(code).as_deref(), Some(code));
        assert_eq!(parse_pasted_code(&format!("stroke-code:{code}")).as_deref(), Some(code));
        assert_eq!(parse_pasted_code(&format!("  STROKE-CODE: {code}\n")).as_deref(), Some(code));
        assert_eq!(parse_pasted_code(&format!("\"stroke-code:{code}\"")).as_deref(), Some(code));
        assert_eq!(parse_pasted_code(""), None);
        assert_eq!(parse_pasted_code("stroke-code:"), None);
        assert_eq!(parse_pasted_code("short"), None);
        assert_eq!(parse_pasted_code("stroke-code:has spaces in the middle of it"), None);
        assert_eq!(parse_pasted_code("https://stroke.click/x?code=abcdefghijklmnopqrstu"), None);
        assert_eq!(parse_pasted_code(&"a".repeat(257)), None);
    }

    #[test]
    fn callback_parsing() {
        assert_eq!(parse_callback("/callback?code=abc%2Bx&state=s1", "s1"), Some(Callback::Code("abc+x".into())));
        assert_eq!(parse_callback("/callback?error=cancelled&state=s1", "s1"), Some(Callback::Cancelled));
        assert!(matches!(parse_callback("/callback?code=abc&state=other", "s1"), Some(Callback::Error(_))));
        assert!(matches!(parse_callback("/callback?code=abc", "s1"), Some(Callback::Error(_))));
        // A cancel with the wrong state is still rejected as a mismatch.
        assert!(matches!(parse_callback("/callback?error=cancelled&state=x", "s1"), Some(Callback::Error(_))));
        assert_eq!(parse_callback("/favicon.ico", "s1"), None);
        assert_eq!(parse_callback("/callback", "s1"), None);
        assert_eq!(parse_callback("/other?code=a&state=s1", "s1"), None);
    }

    #[test]
    fn signature_over_verbatim_entitlement() {
        let sk = test_key();
        let vk = sk.verifying_key();
        let me = parse_me_with_key(&me_body(&sk, ENT), &vk).expect("valid");
        assert_eq!(me.entitlement.plan, "pro");
        assert_eq!(me.user.email, "a@b.co");
        assert_eq!(me.devices.len(), 1);

        // Same JSON value, different text: must not verify.
        let compact: serde_json::Value = serde_json::from_str(ENT).unwrap();
        let body = me_body(&sk, ENT).replace(ENT, &compact.to_string());
        assert!(parse_me_with_key(&body, &vk).is_err());

        // Tampered plan.
        let body = me_body(&sk, ENT).replacen(r#""plan":"pro","source""#, r#""plan":"pro" ,"source""#, 1);
        assert!(parse_me_with_key(&body, &vk).is_err());

        // Another key.
        let other = SigningKey::from_bytes(&[9u8; 32]);
        assert!(parse_me_with_key(&me_body(&other, ENT), &vk).is_err());

        // Not signed with the production key either.
        assert!(parse_me(&me_body(&sk, ENT)).is_err());
    }

    fn ent(plan: &str, source: &str, valid_until: u64) -> Entitlement {
        Entitlement {
            plan: plan.into(),
            source: source.into(),
            license: None,
            trial: Some(EntTrial { started_at: 0, ends_at: 1_000 + 3 * 86_400 }),
            issued_at: 1_000,
            valid_until,
            device_id: None,
        }
    }

    #[test]
    fn cache_validity_window() {
        let e = ent("pro", "license", 2_000);
        assert!(entitlement_fresh(&e, 1_000));
        assert!(entitlement_fresh(&e, 1_999));
        assert!(!entitlement_fresh(&e, 2_000));
        assert!(!entitlement_fresh(&e, 10_000));
    }

    #[test]
    fn entitlement_maps_to_license_status() {
        let now = 1_000;
        assert!(matches!(entitlement_status(&ent("pro", "license", 9e9 as u64), "a", now), LicenseStatus::Valid { .. }));
        match entitlement_status(&ent("pro", "trial", 9e9 as u64), "a", now) {
            LicenseStatus::Trial { days_remaining } => assert_eq!(days_remaining, 3),
            s => panic!("{s:?}"),
        }
        assert!(matches!(entitlement_status(&ent("pro", "trial", 9e9 as u64), "a", 1_000 + 4 * 86_400), LicenseStatus::TrialExpired));
        assert!(matches!(entitlement_status(&ent("free", "none", 9e9 as u64), "a", now), LicenseStatus::TrialExpired));
    }

    #[test]
    fn existing_keys_are_never_downgraded() {
        let key = LicenseStatus::Valid { email: "k@x".into(), plan: "pro".into(), issued_at: 1, expires_at: None };
        let device_trial = LicenseStatus::Trial { days_remaining: 5 };
        // No account: unchanged.
        assert!(matches!(combine(key.clone(), None), LicenseStatus::Valid { .. }));
        assert!(matches!(combine(device_trial.clone(), None), LicenseStatus::Trial { days_remaining: 5 }));
        // Account says free: the key still grants Pro.
        assert!(matches!(combine(key.clone(), Some(LicenseStatus::TrialExpired)), LicenseStatus::Valid { .. }));
        assert!(matches!(combine(key, Some(LicenseStatus::Trial { days_remaining: 2 })), LicenseStatus::Valid { .. }));
        // Account Pro lifts an expired device trial.
        let acc = LicenseStatus::Valid { email: "a".into(), plan: "pro".into(), issued_at: 1, expires_at: None };
        assert!(matches!(combine(LicenseStatus::TrialExpired, Some(acc)), LicenseStatus::Valid { .. }));
        // Signed in: the account's trial replaces the device trial.
        assert!(matches!(combine(device_trial, Some(LicenseStatus::Trial { days_remaining: 9 })), LicenseStatus::Trial { days_remaining: 9 }));
    }

    #[test]
    fn sign_in_params_match_server_rules() {
        let (_, c) = pkce_pair();
        assert_eq!(c.len(), 43);
        let st = random_b64(24);
        assert!((1..=128).contains(&st.len()));
        assert!(st.chars().all(|ch| ch.is_ascii_alphanumeric() || "._~-".contains(ch)));
        assert_eq!(clean_device_name("Mac\u{7}\nBook"), "MacBook");
        assert_eq!(clean_device_name(" \t "), "This computer");
        assert_eq!(clean_device_name(&"x".repeat(200)).len(), 80);
    }

    #[test]
    fn sign_in_url_shape() {
        let u = sign_in_url("https://x.test", 5555, "st", "ch", "dev", "My Mac", Some("github"));
        assert_eq!(u, "https://x.test/desktop/sign-in?port=5555&state=st&code_challenge=ch&device_id=dev&device_name=My%20Mac&provider=github");
        assert!(!sign_in_url("https://x.test", 1, "s", "c", "d", "n", Some("evil&x=1")).contains("provider"));
    }

    #[tokio::test]
    async fn loopback_callback_end_to_end() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let waiter = tokio::spawn(async move { wait_for_callback(listener, "st8").await });

        // A silent preconnect and a favicon fetch must not spend the listener.
        let _idle = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let fav = http().get(format!("http://127.0.0.1:{port}/favicon.ico")).send().await.unwrap();
        assert_eq!(fav.status(), 404);

        let page = http()
            .get(format!("http://127.0.0.1:{port}/callback?code=c0de&state=st8"))
            .send()
            .await
            .unwrap();
        assert_eq!(page.status(), 200);
        assert!(page.text().await.unwrap().contains("return to Stroke"));
        assert_eq!(waiter.await.unwrap(), Ok("c0de".to_string()));

        // Cancel from the website.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let waiter = tokio::spawn(async move { wait_for_callback(listener, "s").await });
        let _ = http().get(format!("http://127.0.0.1:{port}/callback?error=cancelled&state=s")).send().await.unwrap();
        assert_eq!(waiter.await.unwrap(), Err("cancelled".to_string()));
    }

    /// Minimal HTTP/1.1 server: answers each request with the route's (status, body).
    async fn mock_server(routes: Vec<(&'static str, u16, String)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else { return };
                let routes = routes.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 16384];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let line = req.lines().next().unwrap_or("").to_string();
                    let (status, body) = routes
                        .iter()
                        .find(|(r, _, _)| line.starts_with(r))
                        .map(|(_, st, b)| (*st, b.clone()))
                        .unwrap_or((404, "{}".into()));
                    let authed = req.to_ascii_lowercase().contains("authorization: bearer tok");
                    let (status, body) = if line.contains("/api/me") && !authed {
                        (401, r#"{"error":"unauthorized","message":"no"}"#.to_string())
                    } else {
                        (status, body)
                    };
                    let resp = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = s.write_all(resp.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn http_against_mock_server() {
        let sk = test_key();
        let me = me_body(&sk, ENT);
        let base = mock_server(vec![
            ("POST /api/desktop/token", 200, r#"{"token":"tok","user":{"id":"u1","email":"a@b.co","name":"A","image":null}}"#.into()),
            ("GET /api/me", 200, me.clone()),
            ("POST /api/license/link", 409, r#"{"error":"key_owned_elsewhere","message":"Owned by another account"}"#.into()),
        ])
        .await;

        let t = exchange_code(&base, "c", "v", "d", "n", "1.0").await.unwrap();
        assert_eq!(t.token, "tok");
        assert_eq!(t.user.unwrap().email, "a@b.co");

        let raw = fetch_me(&base, "tok").await.unwrap();
        assert_eq!(raw, me, "the body is kept verbatim");
        assert!(parse_me_with_key(&raw, &sk.verifying_key()).is_ok());

        assert_eq!(fetch_me(&base, "wrong").await, Err(ApiError::Unauthorized));
        assert_eq!(
            link_license(&base, "tok", "k").await,
            Err(ApiError::Server("key_owned_elsewhere".into(), "Owned by another account".into()))
        );

        let bad = mock_server(vec![("POST /api/desktop/token", 400, r#"{"error":"expired_code","message":"x"}"#.into())]).await;
        let e = exchange_code(&bad, "c", "v", "d", "n", "1.0").await.unwrap_err();
        assert_eq!(e.user_message(), "The sign-in took too long. Start again.");

        assert!(matches!(fetch_me("http://127.0.0.1:1", "tok").await, Err(ApiError::Network(_))));
    }
}

