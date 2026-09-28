//! App PIN lock: an optional local PIN that gates opening the app and
//! reconnecting to a database.
//!
//! The PIN itself is never stored - only a PBKDF2-HMAC-SHA256 digest over a
//! random 16-byte salt, kept in the same OS-keychain vault as every other
//! secret (see `secrets.rs`). Keeping the *enabled* flag there too is what
//! makes the lock worth having: a flag in localStorage can be flipped from
//! devtools, a keychain entry cannot.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

type HmacSha256 = Hmac<Sha256>;

/// Key the lock config lives under inside the shared secrets vault.
const VAULT_KEY: &str = "__app_lock__";
/// PBKDF2 rounds. Roughly 100ms on a current desktop; sha2/hmac are compiled at
/// opt-level 3 even in the dev profile (see Cargo.toml), so this is not a
/// debug-build trap.
const ITERATIONS: u32 = 200_000;
/// Fixed PIN length. The UI renders exactly this many boxes.
pub const PIN_LEN: usize = 4;

/// Failed unlock attempts this run. Drives a growing delay so a stolen laptop
/// can't be walked through all 10^6 PINs at IPC speed.
static FAILURES: AtomicU32 = AtomicU32::new(0);

#[derive(Serialize, Deserialize, Clone, Default)]
struct LockConfig {
    #[serde(default)]
    enabled: bool,
    /// Hex, 16 random bytes.
    #[serde(default)]
    salt: String,
    /// Hex, PBKDF2-HMAC-SHA256 of the PIN.
    #[serde(default)]
    hash: String,
    #[serde(default)]
    iterations: u32,
    /// Ask for the PIN again before opening or reconnecting to a database.
    #[serde(default)]
    require_on_connect: bool,
    /// Re-lock after this many idle minutes. 0 = never.
    #[serde(default)]
    auto_lock_minutes: u32,
}

/// What the frontend is allowed to see. Never carries the salt or the digest.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockStatus {
    enabled: bool,
    require_on_connect: bool,
    auto_lock_minutes: u32,
    pin_length: usize,
}

impl From<&LockConfig> for LockStatus {
    fn from(c: &LockConfig) -> Self {
        LockStatus {
            enabled: c.enabled && !c.hash.is_empty(),
            require_on_connect: c.require_on_connect,
            auto_lock_minutes: c.auto_lock_minutes,
            pin_length: PIN_LEN,
        }
    }
}

/// PBKDF2-HMAC-SHA256 with dkLen == hLen, so exactly one output block.
fn derive(pin: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let key = pin.as_bytes();
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(salt);
    mac.update(&1u32.to_be_bytes());
    let mut block = mac.finalize().into_bytes();
    let mut out = block;
    for _ in 1..iterations.max(1) {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(&block);
        block = mac.finalize().into_bytes();
        for (o, b) in out.iter_mut().zip(block.iter()) {
            *o ^= b;
        }
    }
    out.into()
}

/// Compare in constant time: an early return would leak how much of the digest
/// matched.
fn digests_match(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn check(cfg: &LockConfig, pin: &str) -> bool {
    if cfg.hash.is_empty() {
        return false;
    }
    let (Ok(salt), Ok(expected)) = (hex::decode(&cfg.salt), hex::decode(&cfg.hash)) else {
        return false;
    };
    digests_match(&derive(pin, &salt, cfg.iterations), &expected)
}

fn validate(pin: &str) -> Result<(), String> {
    if pin.chars().count() != PIN_LEN || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("PIN must be {PIN_LEN} digits."));
    }
    Ok(())
}

// ── Vault access ─────────────────────────────────────────────────────────────
// Every keychain call blocks for an unbounded time (it can raise the OS consent
// prompt), so it goes to the blocking pool - never the caller's thread. See the
// long note in secrets.rs.

async fn off_thread<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())
}

fn config_from(map: &std::collections::HashMap<String, String>) -> LockConfig {
    map.get(VAULT_KEY)
        .and_then(|json| serde_json::from_str::<LockConfig>(json).ok())
        .unwrap_or_default()
}

fn read_config(app: &tauri::AppHandle) -> LockConfig {
    config_from(&crate::secrets::read_all(app))
}

/// Read, mutate, write as one locked vault update, so the pair shares one
/// keychain unlock and a concurrent secret write can't be lost. A rejected
/// change (`f` errs) leaves the stored config as it was.
async fn edit<F>(app: tauri::AppHandle, f: F) -> Result<LockStatus, String>
where
    F: FnOnce(&mut LockConfig) -> Result<(), String> + Send + 'static,
{
    off_thread(move || {
        crate::secrets::update(&app, move |map| {
            let mut cfg = config_from(map);
            f(&mut cfg)?;
            let json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;
            map.insert(VAULT_KEY.to_string(), json);
            Ok(LockStatus::from(&cfg))
        })?
    })
    .await?
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn app_lock_status(app: tauri::AppHandle) -> Result<LockStatus, String> {
    off_thread(move || LockStatus::from(&read_config(&app))).await
}

/// Set the PIN, or change it when one already exists (which requires the
/// current PIN - otherwise anyone at an unlocked screen could silently rekey it).
#[tauri::command]
pub async fn app_lock_set_pin(
    app: tauri::AppHandle,
    pin: String,
    current_pin: Option<String>,
) -> Result<LockStatus, String> {
    validate(&pin)?;
    let mut salt = [0u8; 16];
    getrandom::getrandom(&mut salt).map_err(|e| e.to_string())?;
    edit(app, move |cfg| {
        if cfg.enabled && !cfg.hash.is_empty() {
            let current = current_pin.unwrap_or_default();
            if !check(cfg, &current) {
                return Err("Current PIN is incorrect.".into());
            }
        }
        cfg.hash = hex::encode(derive(&pin, &salt, ITERATIONS));
        cfg.salt = hex::encode(salt);
        cfg.iterations = ITERATIONS;
        // Turning the lock on for the first time also turns on the connect
        // prompt - guarding the app but not the databases is the surprising half.
        if !cfg.enabled {
            cfg.require_on_connect = true;
        }
        cfg.enabled = true;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn app_lock_verify(app: tauri::AppHandle, pin: String) -> Result<bool, String> {
    let ok = off_thread(move || {
        let cfg = read_config(&app);
        if !cfg.enabled || cfg.hash.is_empty() {
            return true; // nothing to unlock
        }
        check(&cfg, &pin)
    })
    .await?;

    if ok {
        FAILURES.store(0, Ordering::Relaxed);
    } else {
        // Grows to a 3s ceiling: still invisible after a typo, brutal for a script.
        let n = FAILURES.fetch_add(1, Ordering::Relaxed) + 1;
        tokio::time::sleep(Duration::from_millis((n as u64 * 300).min(3_000))).await;
    }
    Ok(ok)
}

/// Remove the PIN entirely. Requires the current PIN.
#[tauri::command]
pub async fn app_lock_disable(app: tauri::AppHandle, pin: String) -> Result<LockStatus, String> {
    edit(app, move |cfg| {
        if cfg.enabled && !check(cfg, &pin) {
            return Err("PIN is incorrect.".into());
        }
        *cfg = LockConfig::default();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn app_lock_set_prefs(
    app: tauri::AppHandle,
    require_on_connect: Option<bool>,
    auto_lock_minutes: Option<u32>,
) -> Result<LockStatus, String> {
    edit(app, move |cfg| {
        if let Some(v) = require_on_connect {
            cfg.require_on_connect = v;
        }
        if let Some(v) = auto_lock_minutes {
            cfg.auto_lock_minutes = v.min(24 * 60);
        }
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6070 vector, adapted to HMAC-SHA256 via the widely published values
    /// for P="password", S="salt", c=1 and c=2, dkLen=32.
    #[test]
    fn pbkdf2_matches_known_vectors() {
        assert_eq!(
            hex::encode(derive("password", b"salt", 1)),
            "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
        );
        assert_eq!(
            hex::encode(derive("password", b"salt", 2)),
            "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43"
        );
    }

    #[test]
    fn wrong_pin_is_rejected() {
        let salt = [7u8; 16];
        let cfg = LockConfig {
            enabled: true,
            salt: hex::encode(salt),
            hash: hex::encode(derive("1234", &salt, 1_000)),
            iterations: 1_000,
            require_on_connect: true,
            auto_lock_minutes: 0,
        };
        assert!(check(&cfg, "1234"));
        assert!(!check(&cfg, "1235"));
        assert!(!check(&cfg, ""));
    }

    #[test]
    fn only_four_digit_pins_validate() {
        assert!(validate("1234").is_ok());
        assert!(validate("123").is_err());
        assert!(validate("12345").is_err());
        assert!(validate("123a").is_err());
    }
}
