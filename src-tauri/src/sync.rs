//! End-to-end encrypted sync of workspaces, connections and settings
//! (docs/accounts-and-sync.md, Appendix B).
//!
//! The server only ever sees opaque strings. Everything here follows the
//! contract byte for byte:
//!
//! - **Sync key:** 32 random bytes per account.
//! - **Record:** `base64url(nonce24 || XChaCha20-Poly1305(sync_key, nonce,
//!   plaintext, aad = "<kind>:<id>"))`.
//! - **Wrapped key:** `base64url(nonce24 || XChaCha20-Poly1305(kek, nonce,
//!   sync_key, aad = "stroke-sync-key-v1"))`, `kek = Argon2id(passphrase,
//!   salt16, m = 65536 KiB, t = 3, p = 1, len 32)`.
//!
//! base64url is unpadded (padding is accepted on input). The unwrapped key is
//! kept in the keychain vault (`secrets.rs`) together with the wrapped key it
//! came from, so a reset from another device is noticed and locks this one.
//!
//! The JS engine (`src/lib/sync/engine.js`) owns record bookkeeping; these
//! commands only do crypto and HTTP, handing plaintext JSON text back and forth.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};

use crate::account::{self, ApiError};

/// Error text when the server's key is gone or replaced (reset on another
/// device): this device has been locked and must not push with the old key.
pub const KEY_CHANGED: &str = "key_changed";
/// Vault entry holding `{ key, wrapped }` once unlocked.
pub(crate) const SYNC_KEY_VAULT: &str = "__stroke_sync_key__";
const WRAP_AAD: &[u8] = b"stroke-sync-key-v1";
pub const KDF_M_KIB: u32 = 65_536;
pub const KDF_T: u32 = 3;
pub const KDF_P: u32 = 1;
pub const KINDS: [&str; 3] = ["workspace", "connection", "settings"];
/// Server limit per push.
const PUSH_BATCH: usize = 200;
const MIN_PASSPHRASE: usize = 8;

// ── Crypto ────────────────────────────────────────────────────────────────────

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn unb64(s: &str) -> Result<Vec<u8>, String> {
    URL_SAFE_NO_PAD
        .decode(s.trim().trim_end_matches('='))
        .map_err(|_| "not base64url".to_string())
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("getrandom failed");
    b
}

/// `nonce24 || ciphertext+tag`, base64url.
pub fn seal_with_nonce(key: &[u8; 32], nonce: &[u8; 24], plaintext: &[u8], aad: &[u8]) -> String {
    let cipher = XChaCha20Poly1305::new(key.into());
    let ct = cipher
        .encrypt(XNonce::from_slice(nonce), Payload { msg: plaintext, aad })
        .expect("XChaCha20-Poly1305 encryption cannot fail for in-memory input");
    let mut out = Vec::with_capacity(24 + ct.len());
    out.extend_from_slice(nonce);
    out.extend_from_slice(&ct);
    b64(&out)
}

pub fn seal(key: &[u8; 32], plaintext: &[u8], aad: &[u8]) -> String {
    seal_with_nonce(key, &random::<24>(), plaintext, aad)
}

pub fn open(key: &[u8; 32], sealed: &str, aad: &[u8]) -> Result<Vec<u8>, String> {
    let raw = unb64(sealed)?;
    if raw.len() < 24 + 16 {
        return Err("ciphertext too short".into());
    }
    let (nonce, ct) = raw.split_at(24);
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad })
        .map_err(|_| "decryption failed".to_string())
}

pub fn record_aad(kind: &str, id: &str) -> String {
    format!("{kind}:{id}")
}

pub fn encrypt_record(key: &[u8; 32], kind: &str, id: &str, plaintext: &str) -> String {
    seal(key, plaintext.as_bytes(), record_aad(kind, id).as_bytes())
}

pub fn decrypt_record(key: &[u8; 32], kind: &str, id: &str, ciphertext: &str) -> Result<String, String> {
    let pt = open(key, ciphertext, record_aad(kind, id).as_bytes())?;
    String::from_utf8(pt).map_err(|_| "record is not UTF-8".to_string())
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Kdf {
    pub alg: String,
    /// base64url of the 16-byte salt.
    pub salt: String,
    pub m: u32,
    pub t: u32,
    pub p: u32,
}

impl Kdf {
    fn fresh() -> Self {
        Kdf { alg: "argon2id".into(), salt: b64(&random::<16>()), m: KDF_M_KIB, t: KDF_T, p: KDF_P }
    }
}

/// Argon2id → 32-byte KEK. Parameters from the server are bounded so a hostile
/// server can't make unlocking eat all memory.
pub fn derive_kek(passphrase: &str, kdf: &Kdf) -> Result<[u8; 32], String> {
    if kdf.alg != "argon2id" {
        return Err(format!("Unsupported key derivation: {}", kdf.alg));
    }
    if !(8..=1_048_576).contains(&kdf.m) || !(1..=10).contains(&kdf.t) || !(1..=8).contains(&kdf.p) {
        return Err("The server sent unusable key-derivation parameters.".into());
    }
    let salt = unb64(&kdf.salt).map_err(|_| "bad salt".to_string())?;
    let params = Params::new(kdf.m, kdf.t, kdf.p, Some(32)).map_err(|e| e.to_string())?;
    let mut out = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), &salt, &mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn wrap_key_with_nonce(kek: &[u8; 32], nonce: &[u8; 24], sync_key: &[u8; 32]) -> String {
    seal_with_nonce(kek, nonce, sync_key, WRAP_AAD)
}

/// `(wrapped_key, kdf)` for a sync key under a passphrase.
pub fn wrap_key(passphrase: &str, sync_key: &[u8; 32]) -> Result<(String, Kdf), String> {
    let kdf = Kdf::fresh();
    let kek = derive_kek(passphrase, &kdf)?;
    Ok((wrap_key_with_nonce(&kek, &random::<24>(), sync_key), kdf))
}

pub fn unwrap_key(passphrase: &str, wrapped: &str, kdf: &Kdf) -> Result<[u8; 32], String> {
    let kek = derive_kek(passphrase, kdf)?;
    let raw = open(&kek, wrapped, WRAP_AAD).map_err(|_| "That passphrase doesn't unlock sync.".to_string())?;
    raw.try_into().map_err(|_| "The sync key has the wrong length.".to_string())
}

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug, Clone)]
pub struct ServerKey {
    pub wrapped_key: String,
    pub kdf: Kdf,
    #[serde(default)]
    #[allow(dead_code)]
    pub created_at: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct WireRecord {
    pub id: String,
    pub kind: String,
    pub version: u64,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub ciphertext: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub updated_at: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

#[derive(Deserialize, Debug)]
pub struct ChangesWire {
    pub records: Vec<WireRecord>,
    pub next_since: u64,
    #[serde(default)]
    pub more: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Accepted {
    pub id: String,
    pub version: u64,
    pub seq: u64,
}

#[derive(Deserialize, Debug, Default)]
pub struct PushWire {
    #[serde(default)]
    pub accepted: Vec<Accepted>,
    #[serde(default)]
    pub conflicts: Vec<WireRecord>,
}

/// A record as the JS engine sees it: plaintext JSON text instead of ciphertext.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PlainRecord {
    pub id: String,
    pub kind: String,
    pub version: u64,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub plaintext: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub updated_at: serde_json::Value,
    /// Set when the record couldn't be decrypted (wrong key, tampered).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct PullResult {
    pub records: Vec<PlainRecord>,
    pub next_since: u64,
    pub more: bool,
}

#[derive(Serialize, Debug, Default)]
pub struct PushResult {
    pub accepted: Vec<Accepted>,
    pub conflicts: Vec<PlainRecord>,
}

pub fn valid_id(id: &str) -> bool {
    (1..=128).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn decrypt_wire(key: &[u8; 32], r: WireRecord) -> PlainRecord {
    let (plaintext, error) = match (&r.ciphertext, r.deleted) {
        (Some(ct), false) => match decrypt_record(key, &r.kind, &r.id, ct) {
            Ok(p) => (Some(p), None),
            Err(e) => (None, Some(e)),
        },
        _ => (None, None),
    };
    PlainRecord { id: r.id, kind: r.kind, version: r.version, deleted: r.deleted, plaintext, seq: r.seq, updated_at: r.updated_at, error }
}

pub fn encrypt_plain(key: &[u8; 32], r: &PlainRecord) -> Result<WireRecord, String> {
    if !KINDS.contains(&r.kind.as_str()) {
        return Err(format!("Unknown record kind: {}", r.kind));
    }
    if !valid_id(&r.id) {
        return Err(format!("Record id can't be synced: {}", r.id));
    }
    let ciphertext = if r.deleted {
        None
    } else {
        let pt = r.plaintext.as_deref().ok_or_else(|| format!("{}:{} has no content", r.kind, r.id))?;
        Some(encrypt_record(key, &r.kind, &r.id, pt))
    };
    Ok(WireRecord {
        id: r.id.clone(),
        kind: r.kind.clone(),
        version: r.version,
        deleted: r.deleted,
        ciphertext,
        updated_at: serde_json::Value::Null,
        seq: None,
    })
}

// ── HTTP ──────────────────────────────────────────────────────────────────────

async fn send(req: reqwest::RequestBuilder) -> Result<reqwest::Response, ApiError> {
    let resp = req.send().await.map_err(|e| ApiError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(account::read_error(resp).await);
    }
    Ok(resp)
}

async fn json<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T, ApiError> {
    resp.json().await.map_err(|e| ApiError::Server("bad_response".into(), e.to_string()))
}

/// `None` when the account has no sync key yet.
pub async fn get_key(base: &str, token: &str) -> Result<Option<ServerKey>, ApiError> {
    match send(account::http().get(format!("{base}/api/sync/key")).bearer_auth(token)).await {
        Ok(resp) => Ok(Some(json(resp).await?)),
        Err(ApiError::Server(code, _)) if code == "no_sync_key" => Ok(None),
        Err(e) => Err(e),
    }
}

pub async fn put_key(base: &str, token: &str, wrapped_key: &str, kdf: &Kdf, replace: bool) -> Result<(), ApiError> {
    let mut body = serde_json::json!({ "wrapped_key": wrapped_key, "kdf": kdf });
    if replace {
        body["replace"] = serde_json::Value::Bool(true);
    }
    send(account::http().put(format!("{base}/api/sync/key")).bearer_auth(token).json(&body))
        .await
        .map(|_| ())
}

pub async fn get_changes(base: &str, token: &str, since: u64, limit: u32) -> Result<ChangesWire, ApiError> {
    let url = format!("{base}/api/sync/changes?since={since}&limit={}", limit.clamp(1, 500));
    json(send(account::http().get(url).bearer_auth(token)).await?).await
}

pub async fn post_push(base: &str, token: &str, records: &[WireRecord]) -> Result<PushWire, ApiError> {
    let body = serde_json::json!({ "records": records });
    json(send(account::http().post(format!("{base}/api/sync/push")).bearer_auth(token).json(&body)).await?).await
}

fn sync_message(e: ApiError) -> String {
    match &e {
        ApiError::Server(code, msg) => match code.as_str() {
            "quota_exceeded" => "Sync storage is full (50 MB). Remove some connections and try again.".into(),
            "record_too_large" => "One item is too large to sync.".into(),
            "rate_limited" => "rate_limited".into(),
            "no_sync_key" => KEY_CHANGED.into(),
            "too_many_records" | "invalid_request" | "invalid_json" => format!("The server rejected the sync request ({code})."),
            "key_exists" => "Sync is already set up for this account. Unlock it with your passphrase.".into(),
            "invalid_kind" | "invalid_id" => format!("The server rejected a record: {msg}"),
            "404" | "not_found" => "Sync isn't available on the server yet.".into(),
            _ => e.user_message(),
        },
        _ => e.user_message(),
    }
}

// ── Local key ─────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct StoredKey {
    key: String,
    /// The wrapped key this came from; another value on the server = reset elsewhere.
    wrapped: String,
}

async fn load_local(app: &tauri::AppHandle) -> Option<([u8; 32], String)> {
    let raw = crate::secrets::read_all_async(app).await.get(SYNC_KEY_VAULT).cloned()?;
    let s: StoredKey = serde_json::from_str(&raw).ok()?;
    let key: [u8; 32] = unb64(&s.key).ok()?.try_into().ok()?;
    Some((key, s.wrapped))
}

async fn store_local(app: &tauri::AppHandle, key: &[u8; 32], wrapped: &str) -> Result<(), String> {
    let json = serde_json::to_string(&StoredKey { key: b64(key), wrapped: wrapped.to_string() }).map_err(|e| e.to_string())?;
    crate::secrets::update_async(app, move |m| {
        m.insert(SYNC_KEY_VAULT.to_string(), json);
    })
    .await
}

pub(crate) async fn forget_local(app: &tauri::AppHandle) {
    let _ = crate::secrets::update_async(app, |m| {
        m.remove(SYNC_KEY_VAULT);
    })
    .await;
}

async fn token(app: &tauri::AppHandle) -> Result<String, String> {
    account::load_token(app).await.ok_or_else(|| "Sign in to use sync.".to_string())
}

/// The server still holds the key this device unlocked. A reset elsewhere
/// replaces it (and leaves no tombstones), so a mismatch or a missing key means
/// stop: nothing may be pulled into, or pushed with, the old key.
pub fn key_matches(server: Option<&ServerKey>, wrapped: &str) -> bool {
    server.is_some_and(|k| k.wrapped_key == wrapped)
}

/// The local key, checked against the server's right before a pull or push.
async fn checked_key(app: &tauri::AppHandle, base: &str, tok: &str) -> Result<[u8; 32], String> {
    let (key, wrapped) = load_local(app).await.ok_or_else(|| "locked".to_string())?;
    let server = get_key(base, tok).await.map_err(sync_message)?;
    if !key_matches(server.as_ref(), &wrapped) {
        forget_local(app).await;
        return Err(KEY_CHANGED.into());
    }
    Ok(key)
}

/// A push or pull that learns the key is gone locks this device.
async fn lock_if_gone(app: &tauri::AppHandle, e: String) -> String {
    if e == KEY_CHANGED {
        forget_local(app).await;
    }
    e
}

/// Argon2 at 64 MiB takes a moment: keep it off the async workers.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

fn check_passphrase(p: &str) -> Result<(), String> {
    if p.chars().count() < MIN_PASSPHRASE {
        return Err(format!("Use at least {MIN_PASSPHRASE} characters."));
    }
    Ok(())
}

// ── Commands ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug, Default)]
pub struct SyncState {
    pub signed_in: bool,
    /// The sync key is in the keychain.
    pub unlocked: bool,
    /// `None` when the server couldn't be asked (offline, not signed in).
    pub server_has_key: Option<bool>,
    /// The server's key changed (reset on another device): this device was locked.
    pub key_changed: bool,
    pub offline: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn sync_state(app: tauri::AppHandle) -> Result<SyncState, String> {
    let Some(tok) = account::load_token(&app).await else {
        return Ok(SyncState::default());
    };
    let local = load_local(&app).await;
    let mut s = SyncState { signed_in: true, unlocked: local.is_some(), ..Default::default() };
    match get_key(&account::base_url(), &tok).await {
        Ok(server) => {
            s.server_has_key = Some(server.is_some());
            if let Some((_, wrapped)) = &local {
                if server.as_ref().map(|k| &k.wrapped_key) != Some(wrapped) {
                    forget_local(&app).await;
                    s.unlocked = false;
                    s.key_changed = true;
                }
            }
        }
        Err(ApiError::Network(_)) => s.offline = true,
        Err(e) => s.error = Some(sync_message(e)),
    }
    Ok(s)
}

/// First device: create the account's sync key under `passphrase`.
#[tauri::command]
pub async fn sync_setup(app: tauri::AppHandle, passphrase: String) -> Result<(), String> {
    check_passphrase(&passphrase)?;
    let tok = token(&app).await?;
    let base = account::base_url();
    if get_key(&base, &tok).await.map_err(sync_message)?.is_some() {
        return Err(sync_message(ApiError::Server("key_exists".into(), String::new())));
    }
    let key = random::<32>();
    let (wrapped, kdf) = blocking(move || wrap_key(&passphrase, &key)).await??;
    put_key(&base, &tok, &wrapped, &kdf, false).await.map_err(sync_message)?;
    store_local(&app, &key, &wrapped).await
}

/// Another device: fetch the wrapped key and open it with `passphrase`.
#[tauri::command]
pub async fn sync_unlock(app: tauri::AppHandle, passphrase: String) -> Result<(), String> {
    let tok = token(&app).await?;
    let server = get_key(&account::base_url(), &tok)
        .await
        .map_err(sync_message)?
        .ok_or_else(|| "Sync isn't set up for this account yet.".to_string())?;
    let (wrapped, kdf) = (server.wrapped_key.clone(), server.kdf.clone());
    let key = blocking(move || unwrap_key(&passphrase, &wrapped, &kdf)).await??;
    store_local(&app, &key, &server.wrapped_key).await
}

/// Lost passphrase: a new key; the server deletes every record. The engine then
/// pushes this device's data again.
#[tauri::command]
pub async fn sync_reset(app: tauri::AppHandle, passphrase: String) -> Result<(), String> {
    check_passphrase(&passphrase)?;
    let tok = token(&app).await?;
    let key = random::<32>();
    let (wrapped, kdf) = blocking(move || wrap_key(&passphrase, &key)).await??;
    put_key(&account::base_url(), &tok, &wrapped, &kdf, true).await.map_err(sync_message)?;
    store_local(&app, &key, &wrapped).await
}

#[tauri::command]
pub async fn sync_pull(app: tauri::AppHandle, since: u64, limit: Option<u32>) -> Result<PullResult, String> {
    let tok = token(&app).await?;
    let base = account::base_url();
    let key = checked_key(&app, &base, &tok).await?;
    let w = match get_changes(&base, &tok, since, limit.unwrap_or(500)).await {
        Ok(w) => w,
        Err(e) => return Err(lock_if_gone(&app, sync_message(e)).await),
    };
    Ok(PullResult {
        records: w.records.into_iter().map(|r| decrypt_wire(&key, r)).collect(),
        next_since: w.next_since,
        more: w.more,
    })
}

pub async fn push_with(base: &str, tok: &str, key: &[u8; 32], records: &[PlainRecord]) -> Result<PushResult, String> {
    let wire = records.iter().map(|r| encrypt_plain(key, r)).collect::<Result<Vec<_>, _>>()?;
    let mut out = PushResult::default();
    for chunk in wire.chunks(PUSH_BATCH) {
        let r = post_push(base, tok, chunk).await.map_err(sync_message)?;
        out.accepted.extend(r.accepted);
        out.conflicts.extend(r.conflicts.into_iter().map(|c| decrypt_wire(key, c)));
    }
    Ok(out)
}

#[tauri::command]
pub async fn sync_push(app: tauri::AppHandle, records: Vec<PlainRecord>) -> Result<PushResult, String> {
    let tok = token(&app).await?;
    let base = account::base_url();
    let key = checked_key(&app, &base, &tok).await?;
    match push_with(&base, &tok, &key, &records).await {
        Ok(r) => Ok(r),
        Err(e) => Err(lock_if_gone(&app, e).await),
    }
}

/// Drop this device's copy of the sync key (lock). Server data is untouched.
#[tauri::command]
pub async fn sync_forget(app: tauri::AppHandle) -> Result<(), String> {
    forget_local(&app).await;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn seq<const N: usize>(start: u8) -> [u8; N] {
        std::array::from_fn(|i| start.wrapping_add(i as u8))
    }

    /// Cheap parameters for tests that don't pin the real ones.
    fn fast_kdf() -> Kdf {
        Kdf { alg: "argon2id".into(), salt: b64(&seq::<16>(1)), m: 64, t: 1, p: 1 }
    }

    #[test]
    fn record_round_trip() {
        let key = random::<32>();
        let pt = r#"{"updated_at":1,"data":{"id":"c1","password":"hunter2"}}"#;
        let ct = encrypt_record(&key, "connection", "c1", pt);
        assert!(!ct.contains('=') && !ct.contains('+') && !ct.contains('/'));
        assert_eq!(unb64(&ct).unwrap().len(), 24 + pt.len() + 16);
        assert_eq!(decrypt_record(&key, "connection", "c1", &ct).unwrap(), pt);
        // Fresh nonce per encryption.
        assert_ne!(ct, encrypt_record(&key, "connection", "c1", pt));
        // Another key fails.
        assert!(decrypt_record(&random::<32>(), "connection", "c1", &ct).is_err());
        // Tampering fails.
        let mut raw = unb64(&ct).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 1;
        assert!(decrypt_record(&key, "connection", "c1", &b64(&raw)).is_err());
    }

    #[test]
    fn aad_binds_kind_and_id() {
        let key = random::<32>();
        let ct = encrypt_record(&key, "workspace", "w1", "{}");
        assert!(decrypt_record(&key, "workspace", "w1", &ct).is_ok());
        assert!(decrypt_record(&key, "connection", "w1", &ct).is_err(), "swapped kind");
        assert!(decrypt_record(&key, "workspace", "w2", &ct).is_err(), "swapped id");
        let r = decrypt_wire(
            &key,
            WireRecord { id: "w2".into(), kind: "workspace".into(), version: 1, deleted: false, ciphertext: Some(ct), updated_at: serde_json::Value::Null, seq: Some(1) },
        );
        assert!(r.plaintext.is_none() && r.error.is_some());
    }

    #[test]
    fn wrong_passphrase_fails() {
        let key = random::<32>();
        let kdf = fast_kdf();
        let kek = derive_kek("right passphrase", &kdf).unwrap();
        let wrapped = wrap_key_with_nonce(&kek, &random::<24>(), &key);
        assert_eq!(unwrap_key("right passphrase", &wrapped, &kdf).unwrap(), key);
        assert_eq!(unwrap_key("wrong passphrase", &wrapped, &kdf).unwrap_err(), "That passphrase doesn't unlock sync.");
        // The wrap AAD is part of the format: a record-style AAD doesn't open it.
        assert!(open(&kek, &wrapped, b"workspace:x").is_err());
        // Hostile parameters are refused before any work.
        assert!(derive_kek("x", &Kdf { m: 1 << 30, ..fast_kdf() }).is_err());
        assert!(derive_kek("x", &Kdf { alg: "scrypt".into(), ..fast_kdf() }).is_err());
    }

    /// Vectors computed independently with Node's `crypto.argon2Sync` and a
    /// hand-written HChaCha20 over Node's ChaCha20-Poly1305 (checked against the
    /// draft-irtf-cfrg-xchacha HChaCha20 test vector).
    #[test]
    fn known_answer_wrap_format() {
        let kdf = Kdf { alg: "argon2id".into(), salt: b64(&seq::<16>(1)), m: KDF_M_KIB, t: KDF_T, p: KDF_P };
        let kek = derive_kek("correct horse battery staple", &kdf).unwrap();
        assert_eq!(hex::encode(kek), "d8ebeb50632cc1993711eda85db155a107f0fb1cafab88a3a79f3d4661ac7a0c");
        let sync_key = seq::<32>(0x40);
        let wrapped = wrap_key_with_nonce(&kek, &seq::<24>(0x20), &sync_key);
        assert_eq!(
            wrapped,
            "ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3CoJKs58KAUEC2YeU1ie3oy1ja4bdXWU1tJBuX3lNFLrTDl6P2NT5rS1g84taiPQ9"
        );
        assert_eq!(unwrap_key("correct horse battery staple", &wrapped, &kdf).unwrap(), sync_key);

        let rec = seal_with_nonce(
            &sync_key,
            &seq::<24>(0x80),
            br#"{"updated_at":1700000000000,"data":{"id":"w1","name":"Home"}}"#,
            record_aad("workspace", "w1").as_bytes(),
        );
        assert_eq!(
            rec,
            "gIGCg4SFhoeIiYqLjI2Oj5CRkpOUlZaXeYEDiaRTLRFO051tpFXyYp4X4Ymj51UQjOUkxcmVX6YoPMwdT0mObXhlOXO4sNBnc_5gc3oRIWUvyLfbwVQWFHsd9dYdXzVkW98zAvY"
        );
    }

    #[test]
    fn key_check_and_server_codes() {
        let k = ServerKey { wrapped_key: "w1".into(), kdf: fast_kdf(), created_at: serde_json::json!(1700000000000u64) };
        assert!(key_matches(Some(&k), "w1"));
        assert!(!key_matches(Some(&k), "w0"), "reset elsewhere");
        assert!(!key_matches(None, "w1"), "turned off");
        assert_eq!(sync_message(ApiError::Server("no_sync_key".into(), String::new())), KEY_CHANGED);
        assert_eq!(sync_message(ApiError::Server("rate_limited".into(), String::new())), "rate_limited");
        // A conflict placeholder for an id the server doesn't have.
        let r = decrypt_wire(
            &random::<32>(),
            serde_json::from_value(serde_json::json!({ "id": "x", "kind": "workspace", "version": 0, "deleted": true, "ciphertext": null, "updated_at": 0, "seq": 0 })).unwrap(),
        );
        assert!(r.deleted && r.version == 0 && r.plaintext.is_none() && r.error.is_none());
    }

    #[test]
    fn ids_and_kinds_are_checked() {
        let key = random::<32>();
        let mk = |id: &str, kind: &str| PlainRecord {
            id: id.into(),
            kind: kind.into(),
            version: 1,
            deleted: false,
            plaintext: Some("{}".into()),
            seq: None,
            updated_at: serde_json::Value::Null,
            error: None,
        };
        assert!(encrypt_plain(&key, &mk("0b6f1c2e-1a2b-4c3d-9e8f-001122334455", "connection")).is_ok());
        assert!(encrypt_plain(&key, &mk("settings", "settings")).is_ok());
        assert!(encrypt_plain(&key, &mk("a b", "workspace")).is_err());
        assert!(encrypt_plain(&key, &mk(&"x".repeat(129), "workspace")).is_err());
        assert!(encrypt_plain(&key, &mk("x", "query")).is_err());
        let tomb = encrypt_plain(&key, &PlainRecord { deleted: true, plaintext: None, ..mk("x", "workspace") }).unwrap();
        assert!(tomb.ciphertext.is_none());
        // Wire shape: no seq/updated_at on push.
        let v = serde_json::to_value(&tomb).unwrap();
        assert_eq!(v, serde_json::json!({ "id": "x", "kind": "workspace", "version": 1, "deleted": true, "ciphertext": null }));
    }

    // ── HTTP against a small in-process server ──

    #[derive(Default)]
    struct Mock {
        key: Option<serde_json::Value>,
        /// (kind, id) → (version, deleted, ciphertext, seq)
        recs: std::collections::BTreeMap<(String, String), (u64, bool, Option<String>, u64)>,
        seq: u64,
    }

    impl Mock {
        fn rec_json(&self, k: &(String, String)) -> serde_json::Value {
            let (v, d, c, s) = &self.recs[k];
            serde_json::json!({ "id": k.1, "kind": k.0, "version": v, "deleted": d, "ciphertext": c, "updated_at": 1700000000000u64 + s, "seq": s })
        }

        fn handle(&mut self, method: &str, path: &str, body: &str) -> (u16, serde_json::Value) {
            let (route, query) = path.split_once('?').unwrap_or((path, ""));
            match (method, route) {
                ("GET", "/api/sync/key") => match &self.key {
                    Some(k) => (200, k.clone()),
                    None => (404, serde_json::json!({ "error": "no_sync_key", "message": "No key" })),
                },
                ("PUT", "/api/sync/key") => {
                    let b: serde_json::Value = serde_json::from_str(body).unwrap();
                    let replace = b["replace"].as_bool().unwrap_or(false);
                    if self.key.is_some() && !replace {
                        return (409, serde_json::json!({ "error": "key_exists", "message": "exists" }));
                    }
                    let had = self.key.is_some();
                    if replace {
                        self.recs.clear();
                    }
                    self.key = Some(serde_json::json!({ "wrapped_key": b["wrapped_key"], "kdf": b["kdf"], "created_at": 1 }));
                    (if had { 200 } else { 201 }, serde_json::json!({}))
                }
                ("GET", "/api/sync/changes") => {
                    let since: u64 = query.split('&').find_map(|p| p.strip_prefix("since=")).unwrap().parse().unwrap();
                    let mut keys: Vec<_> = self.recs.iter().filter(|(_, r)| r.3 > since).map(|(k, r)| (r.3, k.clone())).collect();
                    keys.sort();
                    let records: Vec<_> = keys.iter().map(|(_, k)| self.rec_json(k)).collect();
                    let next = keys.last().map(|(s, _)| *s).unwrap_or(since);
                    (200, serde_json::json!({ "records": records, "next_since": next, "more": false }))
                }
                ("POST", "/api/sync/push") => {
                    let b: serde_json::Value = serde_json::from_str(body).unwrap();
                    let (mut accepted, mut conflicts) = (vec![], vec![]);
                    for r in b["records"].as_array().unwrap() {
                        let k = (r["kind"].as_str().unwrap().to_string(), r["id"].as_str().unwrap().to_string());
                        let cur = self.recs.get(&k).map(|x| x.0).unwrap_or(0);
                        if r["version"].as_u64().unwrap() == cur + 1 {
                            self.seq += 1;
                            self.recs.insert(
                                k.clone(),
                                (cur + 1, r["deleted"].as_bool().unwrap(), r["ciphertext"].as_str().map(String::from), self.seq),
                            );
                            accepted.push(serde_json::json!({ "id": k.1, "version": cur + 1, "seq": self.seq }));
                        } else {
                            conflicts.push(self.rec_json(&k));
                        }
                    }
                    (200, serde_json::json!({ "accepted": accepted, "conflicts": conflicts }))
                }
                _ => (404, serde_json::json!({ "error": "not_found", "message": "" })),
            }
        }
    }

    async fn mock_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = std::sync::Arc::new(tokio::sync::Mutex::new(Mock::default()));
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else { return };
                let state = state.clone();
                tokio::spawn(async move {
                    // Read headers, then the body by Content-Length.
                    let mut buf = Vec::new();
                    let mut tmp = [0u8; 8192];
                    let (head_end, len) = loop {
                        let n = s.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&buf[..i]).to_ascii_lowercase();
                            let len = head
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length:"))
                                .map(|v| v.trim().parse::<usize>().unwrap())
                                .unwrap_or(0);
                            break (i + 4, len);
                        }
                    };
                    while buf.len() < head_end + len {
                        let n = s.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                    let body = String::from_utf8_lossy(&buf[head_end..]).to_string();
                    let mut first = head.lines().next().unwrap_or("").split_whitespace();
                    let (method, path) = (first.next().unwrap_or("").to_string(), first.next().unwrap_or("").to_string());
                    let (status, json) = if !head.to_ascii_lowercase().contains("authorization: bearer tok") {
                        (401, serde_json::json!({ "error": "unauthorized", "message": "" }))
                    } else {
                        state.lock().await.handle(&method, &path, &body)
                    };
                    let body = json.to_string();
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

    fn plain(kind: &str, id: &str, version: u64, pt: Option<&str>) -> PlainRecord {
        PlainRecord {
            id: id.into(),
            kind: kind.into(),
            version,
            deleted: pt.is_none(),
            plaintext: pt.map(String::from),
            seq: None,
            updated_at: serde_json::Value::Null,
            error: None,
        }
    }

    #[tokio::test]
    async fn http_key_changes_push_conflict() {
        let base = mock_server().await;

        // No key yet; unauthorized token.
        assert!(get_key(&base, "tok").await.unwrap().is_none());
        assert_eq!(get_key(&base, "nope").await.unwrap_err(), ApiError::Unauthorized);

        // Set up: wrap, PUT, then a second PUT without replace is refused.
        let key = random::<32>();
        let kdf = fast_kdf();
        let wrapped = wrap_key_with_nonce(&derive_kek("passphrase!", &kdf).unwrap(), &random::<24>(), &key);
        put_key(&base, "tok", &wrapped, &kdf, false).await.unwrap();
        assert_eq!(
            put_key(&base, "tok", &wrapped, &kdf, false).await.unwrap_err(),
            ApiError::Server("key_exists".into(), "exists".into())
        );
        let server = get_key(&base, "tok").await.unwrap().unwrap();
        assert_eq!(server.kdf, kdf);
        assert_eq!(unwrap_key("passphrase!", &server.wrapped_key, &server.kdf).unwrap(), key);

        // Push two records, pull them back decrypted.
        let ws = r#"{"updated_at":1,"data":{"id":"w1","name":"Home"}}"#;
        let conn = r#"{"updated_at":2,"data":{"id":"c1","password":"pw"}}"#;
        let r = push_with(&base, "tok", &key, &[plain("workspace", "w1", 1, Some(ws)), plain("connection", "c1", 1, Some(conn))])
            .await
            .unwrap();
        assert_eq!(r.accepted.len(), 2);
        assert!(r.conflicts.is_empty());

        let w = get_changes(&base, "tok", 0, 500).await.unwrap();
        assert_eq!(w.records.len(), 2);
        assert!(w.records.iter().all(|r| r.ciphertext.as_deref().is_some_and(|c| !c.contains("pw"))), "server sees ciphertext only");
        let pulled: Vec<_> = w.records.into_iter().map(|r| decrypt_wire(&key, r)).collect();
        assert_eq!(pulled[0].plaintext.as_deref(), Some(ws));
        assert_eq!(pulled[1].plaintext.as_deref(), Some(conn));
        assert_eq!(w.next_since, 2);
        assert!(get_changes(&base, "tok", 2, 500).await.unwrap().records.is_empty());

        // A stale version conflicts and comes back decrypted; a tombstone is accepted.
        let r = push_with(&base, "tok", &key, &[plain("workspace", "w1", 1, Some("{}")), plain("connection", "c1", 2, None)])
            .await
            .unwrap();
        assert_eq!(r.accepted, vec![Accepted { id: "c1".into(), version: 2, seq: 3 }]);
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(r.conflicts[0].version, 1);
        assert_eq!(r.conflicts[0].plaintext.as_deref(), Some(ws));
        let w = get_changes(&base, "tok", 2, 500).await.unwrap();
        assert!(w.records[0].deleted && w.records[0].ciphertext.is_none());

        // Reset: replace wipes the records.
        put_key(&base, "tok", &wrapped, &kdf, true).await.unwrap();
        assert!(get_changes(&base, "tok", 0, 500).await.unwrap().records.is_empty());
    }

    /// Round trip against a running stroke-web (`STROKE_API_BASE`) with a token
    /// minted into its local D1 (`STROKE_SYNC_TEST_TOKEN`). Run with
    /// `cargo test --lib sync::tests::live -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn live_round_trip() {
        let base = std::env::var("STROKE_API_BASE").unwrap_or_else(|_| "http://localhost:3000".into());
        let tok = std::env::var("STROKE_SYNC_TEST_TOKEN").expect("STROKE_SYNC_TEST_TOKEN");
        let pass = "live test passphrase";

        // Start from a known key: create, or reset when one is there.
        let key = random::<32>();
        let (wrapped, kdf) = wrap_key(pass, &key).unwrap();
        let had = get_key(&base, &tok).await.unwrap().is_some();
        put_key(&base, &tok, &wrapped, &kdf, had).await.unwrap();
        assert!(matches!(
            put_key(&base, &tok, &wrapped, &kdf, false).await,
            Err(ApiError::Server(code, _)) if code == "key_exists"
        ));
        let server = get_key(&base, &tok).await.unwrap().unwrap();
        assert_eq!(unwrap_key(pass, &server.wrapped_key, &server.kdf).unwrap(), key);

        let ws = r#"{"updated_at":1700000000000,"data":{"id":"live-w1","name":"Live"}}"#;
        let r = push_with(&base, &tok, &key, &[plain("workspace", "live-w1", 1, Some(ws))]).await.unwrap();
        println!("push: {:?}", r.accepted);
        assert_eq!(r.accepted.len(), 1);
        let w = get_changes(&base, &tok, 0, 500).await.unwrap();
        let rec = w.records.into_iter().find(|r| r.id == "live-w1").expect("pulled");
        let rec = decrypt_wire(&key, rec);
        assert_eq!(rec.plaintext.as_deref(), Some(ws));

        // Stale version → conflict, decrypted.
        let r = push_with(&base, &tok, &key, &[plain("workspace", "live-w1", 1, Some(ws))]).await.unwrap();
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(r.conflicts[0].plaintext.as_deref(), Some(ws));
        println!("live round trip ok against {base}");
    }

    /// Two devices of one account against a running stroke-web: A sets sync up
    /// and pushes, B unlocks with the passphrase and pulls, edits travel both
    /// ways, a stale edit conflicts, a delete reaches the other side, and a
    /// reset on A is noticed by B. Needs two tokens of the same user:
    /// `STROKE_SYNC_TEST_TOKEN` and `STROKE_SYNC_TEST_TOKEN_B`.
    #[tokio::test]
    #[ignore]
    async fn live_two_devices() {
        let base = std::env::var("STROKE_API_BASE").unwrap_or_else(|_| "http://localhost:3000".into());
        let a = std::env::var("STROKE_SYNC_TEST_TOKEN").expect("STROKE_SYNC_TEST_TOKEN");
        let b = std::env::var("STROKE_SYNC_TEST_TOKEN_B").expect("STROKE_SYNC_TEST_TOKEN_B");
        let pass = "two device passphrase";
        let run = format!("{:x}", u64::from_le_bytes(random::<8>()));
        let conn_id = format!("conn-{run}");

        // A: set up sync (reset whatever an earlier run left).
        let key_a = random::<32>();
        let (wrapped, kdf) = wrap_key(pass, &key_a).unwrap();
        let had = get_key(&base, &a).await.unwrap().is_some();
        put_key(&base, &a, &wrapped, &kdf, had).await.unwrap();
        let start = get_changes(&base, &a, 0, 500).await.unwrap().next_since;

        // A: push a connection carrying a password.
        let v1 = format!(r#"{{"updated_at":1,"data":{{"id":"{conn_id}","name":"Prod","password":"hunter2-{run}"}}}}"#);
        let r = push_with(&base, &a, &key_a, &[plain("connection", &conn_id, 1, Some(&v1))]).await.unwrap();
        assert_eq!(r.accepted.len(), 1, "A push");

        // The server holds ciphertext only.
        let raw = get_changes(&base, &b, start, 500).await.unwrap();
        let wire = raw.records.iter().find(|r| r.id == conn_id).expect("on server");
        assert!(!wire.ciphertext.as_deref().unwrap().contains("hunter2"), "server saw plaintext");

        // B: a wrong passphrase can't unlock; the right one gives A's key.
        let server = get_key(&base, &b).await.unwrap().unwrap();
        assert!(unwrap_key("not the passphrase", &server.wrapped_key, &server.kdf).is_err());
        let key_b = unwrap_key(pass, &server.wrapped_key, &server.kdf).unwrap();
        assert_eq!(key_b, key_a);
        assert!(key_matches(Some(&server), &wrapped));

        // B: pull and decrypt A's connection.
        let pulled = get_changes(&base, &b, start, 500).await.unwrap();
        let rec = decrypt_wire(&key_b, pulled.records.into_iter().find(|r| r.id == conn_id).unwrap());
        assert_eq!(rec.plaintext.as_deref(), Some(v1.as_str()), "B decrypts A's record");
        let cursor_a = get_changes(&base, &a, start, 500).await.unwrap().next_since;

        // B: rename it (v2); A pulls the rename.
        let v2 = v1.replace("\"Prod\"", "\"Prod (renamed on B)\"");
        let r = push_with(&base, &b, &key_b, &[plain("connection", &conn_id, 2, Some(&v2))]).await.unwrap();
        assert_eq!(r.accepted.len(), 1, "B push");
        let got = get_changes(&base, &a, cursor_a, 500).await.unwrap();
        let rec = decrypt_wire(&key_a, got.records.into_iter().find(|r| r.id == conn_id).expect("A sees B's edit"));
        assert_eq!(rec.version, 2);
        assert_eq!(rec.plaintext.as_deref(), Some(v2.as_str()));
        let cursor_a = got.next_since;

        // A: an edit made before seeing v2 is a conflict carrying B's version.
        let stale = v1.replace("\"Prod\"", "\"Prod (edited on A)\"");
        let r = push_with(&base, &a, &key_a, &[plain("connection", &conn_id, 2, Some(&stale))]).await.unwrap();
        assert!(r.accepted.is_empty());
        assert_eq!(r.conflicts.len(), 1, "stale push conflicts");
        assert_eq!(r.conflicts[0].plaintext.as_deref(), Some(v2.as_str()));

        // B: delete it; A sees the tombstone.
        let r = push_with(&base, &b, &key_b, &[plain("connection", &conn_id, 3, None)]).await.unwrap();
        assert_eq!(r.accepted.len(), 1, "B delete");
        let got = get_changes(&base, &a, cursor_a, 500).await.unwrap();
        let rec = got.records.into_iter().find(|r| r.id == conn_id).expect("A sees delete");
        assert!(rec.deleted && rec.ciphertext.is_none());

        // A: reset sync with a new passphrase; B's stored key no longer matches.
        let (wrapped2, kdf2) = wrap_key("a brand new passphrase", &random::<32>()).unwrap();
        put_key(&base, &a, &wrapped2, &kdf2, true).await.unwrap();
        let server = get_key(&base, &b).await.unwrap();
        assert!(!key_matches(server.as_ref(), &wrapped), "B notices the reset");
        assert!(get_changes(&base, &b, 0, 500).await.unwrap().records.is_empty(), "reset cleared records");
        println!("two devices ok against {base}");
    }
}
