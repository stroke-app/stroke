use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tauri::Manager;

// Session cache: once loaded (or written) it is authoritative for the process,
// so a value is always readable immediately after it is stored - even if the OS
// keychain read-back is flaky, unavailable, or (mis)configured as the mock store.
// Durable persistence still goes to the keychain/file below; this only guarantees
// read-after-write within a run.
static CACHE: OnceLock<Mutex<Option<HashMap<String, String>>>> = OnceLock::new();
fn cache() -> &'static Mutex<Option<HashMap<String, String>>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

// Serialises read-modify-write of the vault. Every caller rewrites the whole
// map, so two unguarded updates (a provider token refresh and an AI key save,
// say) each wrote back the map they read and one silently undid the other.
static VAULT_LOCK: Mutex<()> = Mutex::new(());

// All secrets (AI keys, provider OAuth tokens, Cloudflare tokens) live in one
// JSON map stored in the OS keychain - macOS Keychain, Windows Credential
// Manager, or Linux Secret Service. A plaintext file (`ai-keys.json`) is the
// fallback when the keychain can't hold it, and the pre-keychain format.
const KEYCHAIN_SERVICE: &str = "app.stroke.desktop";
const KEYCHAIN_ACCOUNT: &str = "secrets-vault";

// Windows caps one credential at 2560 bytes of UTF-16 - 1280 characters. A
// single OAuth access + refresh token pair is past that, so the map is split
// across several credentials there. Before this, every write after the first
// sign-in failed, the tokens went to the fallback file, and the next launch
// read the older, smaller credential first: signed out on every restart.
#[cfg(windows)]
const KEYCHAIN_CHUNK_UTF16: usize = 1200;
// macOS and Linux have no practical limit; one chunk keeps a single item.
#[cfg(not(windows))]
const KEYCHAIN_CHUNK_UTF16: usize = 1 << 20;

type Map = HashMap<String, String>;

fn legacy_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("app data dir not found")
        .join("ai-keys.json")
}

// ── Credential store ─────────────────────────────────────────────────────────

/// Named secret slots. The OS keychain in the app; an in-memory map in tests.
trait CredStore {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
    /// Largest value one slot takes, in UTF-16 code units.
    fn max_utf16(&self) -> usize;
}

struct Keychain;

impl Keychain {
    fn entry(account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|e| e.to_string())
    }
}

impl CredStore for Keychain {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match Self::entry(account)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        Self::entry(account)?.set_password(value).map_err(|e| e.to_string())
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
    fn max_utf16(&self) -> usize {
        KEYCHAIN_CHUNK_UTF16
    }
}

// ── Chunked vault format ─────────────────────────────────────────────────────
//
// `secrets-vault` holds a small header, `{"v":2,"gen":"…","chunks":N}`, and the
// map's JSON is split over `secrets-vault.<gen>.<i>`. Each write uses a fresh
// `gen`, so the header flips from the old chunk set to the new one in a single
// write and a failure part-way never leaves a mix of both. A header that is not
// a v2 object is the old format: the whole map in one credential.

#[derive(serde::Serialize, serde::Deserialize)]
struct Header {
    v: u8,
    gen: String,
    chunks: usize,
}

fn chunk_account(gen: &str, i: usize) -> String {
    format!("{KEYCHAIN_ACCOUNT}.{gen}.{i}")
}

/// Split on char boundaries into pieces of at most `max` UTF-16 units.
fn split_utf16(s: &str, max: usize) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut units = 0;
    for c in s.chars() {
        let n = c.len_utf16();
        if units + n > max {
            out.push(String::new());
            units = 0;
        }
        out.last_mut().expect("non-empty").push(c);
        units += n;
    }
    out
}

fn parse_map(s: &str) -> Option<Map> {
    serde_json::from_str::<Map>(s).ok()
}

fn read_header(store: &dyn CredStore) -> Result<Option<(String, Option<Header>)>, String> {
    let Some(raw) = store.get(KEYCHAIN_ACCOUNT)? else {
        return Ok(None);
    };
    let header = serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .filter(|v| v.get("v").and_then(|v| v.as_u64()) == Some(2))
        .and_then(|v| serde_json::from_value::<Header>(v).ok());
    Ok(Some((raw, header)))
}

/// The stored map, `None` when the keychain holds nothing.
fn read_vault(store: &dyn CredStore) -> Result<Option<Map>, String> {
    let Some((raw, header)) = read_header(store)? else {
        return Ok(None);
    };
    let Some(h) = header else {
        // Old single-credential format.
        return parse_map(&raw).map(Some).ok_or_else(|| "keychain vault is not valid JSON".into());
    };
    let mut json = String::new();
    for i in 0..h.chunks {
        json.push_str(
            &store
                .get(&chunk_account(&h.gen, i))?
                .ok_or_else(|| format!("keychain vault is missing part {i}"))?,
        );
    }
    parse_map(&json).map(Some).ok_or_else(|| "keychain vault is not valid JSON".into())
}

/// Write `map` and prove it round-trips. On failure nothing the old header
/// points at has been touched.
fn write_vault(store: &dyn CredStore, map: &Map) -> Result<(), String> {
    let json = serde_json::to_string(map).map_err(|e| e.to_string())?;
    let old = read_header(store).ok().flatten().and_then(|(_, h)| h);
    let gen = {
        let mut b = [0u8; 6];
        getrandom::getrandom(&mut b).map_err(|e| e.to_string())?;
        b.iter().map(|x| format!("{x:02x}")).collect::<String>()
    };
    let chunks = split_utf16(&json, store.max_utf16());
    let cleanup_new = |upto: usize| {
        for i in 0..upto {
            let _ = store.delete(&chunk_account(&gen, i));
        }
    };
    for (i, c) in chunks.iter().enumerate() {
        if let Err(e) = store.set(&chunk_account(&gen, i), c) {
            cleanup_new(i);
            return Err(e);
        }
    }
    let header = serde_json::to_string(&Header { v: 2, gen: gen.clone(), chunks: chunks.len() })
        .map_err(|e| e.to_string())?;
    if let Err(e) = store.set(KEYCHAIN_ACCOUNT, &header) {
        cleanup_new(chunks.len());
        return Err(e);
    }
    // A write can "succeed" yet not persist (keyring's mock store, a locked
    // keyring): only a read-back that matches counts.
    if read_vault(store).ok().flatten().as_ref() != Some(map) {
        return Err("keychain did not keep the vault".into());
    }
    if let Some(h) = old {
        for i in 0..h.chunks {
            let _ = store.delete(&chunk_account(&h.gen, i));
        }
    }
    Ok(())
}

/// Remove the vault from the keychain entirely (header first, so a half-done
/// delete reads as empty rather than as a broken vault).
fn clear_vault(store: &dyn CredStore) -> Result<(), String> {
    let old = read_header(store)?.and_then(|(_, h)| h);
    store.delete(KEYCHAIN_ACCOUNT)?;
    if let Some(h) = old {
        for i in 0..h.chunks {
            let _ = store.delete(&chunk_account(&h.gen, i));
        }
    }
    Ok(())
}

// ── Fallback file ────────────────────────────────────────────────────────────

fn write_file(path: &Path, map: &Map) -> Result<(), String> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(map).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    // Plaintext secrets: private to this user.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp).map_err(|e| e.to_string())?;
    f.write_all(json.as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

// ── Durable load / persist ───────────────────────────────────────────────────
//
// Invariant: the fallback file exists only while it is the newest copy. A
// keychain write that round-trips deletes it; a keychain write that fails
// writes it and then removes the keychain copy. So on load the file, when
// present, always wins - which also rescues installs where the old code left a
// stale credential in front of a newer file.

fn load_from(path: &Path, store: &dyn CredStore) -> Map {
    if let Some(map) = std::fs::read_to_string(path).ok().and_then(|s| parse_map(&s)) {
        // Move it into the keychain when that verifiably works.
        let _ = persist_to(path, store, &map);
        return map;
    }
    read_vault(store).ok().flatten().unwrap_or_default()
}

fn persist_to(path: &Path, store: &dyn CredStore, map: &Map) -> Result<(), String> {
    if write_vault(store, map).is_ok() {
        if path.exists() && std::fs::remove_file(path).is_err() {
            // Could not delete it: keep it current instead, or it would win the
            // next load with old contents.
            write_file(path, map)?;
        }
        return Ok(());
    }
    // Keychain missing, too small, or not round-tripping: the file becomes the
    // store, and the keychain copy goes so it can never be read instead of it.
    write_file(path, map)?;
    let _ = clear_vault(store);
    Ok(())
}

fn load_durable(app: &tauri::AppHandle) -> Map {
    load_from(&legacy_path(app), &Keychain)
}

pub(crate) fn read_all(app: &tauri::AppHandle) -> Map {
    let mut guard = cache().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(map) = guard.as_ref() {
        return map.clone();
    }
    let map = load_durable(app);
    *guard = Some(map.clone());
    map
}

fn write_unlocked(app: &tauri::AppHandle, map: &Map) -> Result<(), String> {
    // Session cache is authoritative first, so reads right after this always see
    // the new value regardless of what the durable backend does.
    *cache().lock().unwrap_or_else(|e| e.into_inner()) = Some(map.clone());
    persist_to(&legacy_path(app), &Keychain, map)
}

/// Read, change and write the vault as one step. Use this for every change:
/// a separate read and write can lose a concurrent update.
pub(crate) fn update<R>(app: &tauri::AppHandle, f: impl FnOnce(&mut Map) -> R) -> Result<R, String> {
    let _guard = VAULT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut map = read_all(app);
    let r = f(&mut map);
    write_unlocked(app, &map)?;
    Ok(r)
}

// ── Keychain access must never run on the caller's thread ────────────────────
//
// A keychain call blocks for an unbounded time: the first one of a run can put
// the OS "<app> wants to use your confidential information" prompt on screen and
// only returns once the user answers it. Tauri runs a command *without* `async`
// on the main thread (see "Async Commands" in the Tauri docs), and blocking the
// main thread stalls the event loop - the window stops compositing and shows an
// unpainted surface (pure white) for as long as the prompt is up. Inside an
// async command the same call instead parks a runtime worker, stalling unrelated
// queries.
//
// So every path that can reach the keychain goes through `read_all_async` /
// `update_async`, which move the blocking work to the dedicated blocking pool.
// The sync `read_all`/`update` stay for use *inside* those closures.
async fn off_thread<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn read_all_async(app: &tauri::AppHandle) -> Map {
    let app = app.clone();
    // A join error here means the closure panicked; an empty vault is the same
    // answer callers already handle for "nothing stored yet".
    off_thread(move || read_all(&app)).await.unwrap_or_default()
}

pub(crate) async fn update_async<R, F>(app: &tauri::AppHandle, f: F) -> Result<R, String>
where
    F: FnOnce(&mut Map) -> R + Send + 'static,
    R: Send + 'static,
{
    let app = app.clone();
    off_thread(move || update(&app, f)).await?
}

#[tauri::command]
pub async fn ai_store_key(
    app: tauri::AppHandle,
    profile_id: String,
    api_key: String,
) -> Result<(), String> {
    update_async(&app, move |map| {
        if api_key.is_empty() {
            map.remove(&profile_id);
        } else {
            map.insert(profile_id, api_key);
        }
    })
    .await
}

#[tauri::command]
pub async fn ai_load_key(app: tauri::AppHandle, profile_id: String) -> Result<String, String> {
    Ok(read_all_async(&app)
        .await
        .get(&profile_id)
        .cloned()
        .unwrap_or_default())
}

#[tauri::command]
pub async fn ai_delete_key(app: tauri::AppHandle, profile_id: String) -> Result<(), String> {
    update_async(&app, move |map| {
        map.remove(&profile_id);
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// In-memory keychain with Windows' per-credential limit, and switches to
    /// make it refuse writes.
    struct MemStore {
        slots: RefCell<HashMap<String, String>>,
        max: usize,
        fail_writes: RefCell<bool>,
    }

    impl MemStore {
        fn new(max: usize) -> Self {
            Self { slots: RefCell::new(HashMap::new()), max, fail_writes: RefCell::new(false) }
        }
    }

    impl CredStore for MemStore {
        fn get(&self, account: &str) -> Result<Option<String>, String> {
            Ok(self.slots.borrow().get(account).cloned())
        }
        fn set(&self, account: &str, value: &str) -> Result<(), String> {
            if *self.fail_writes.borrow() {
                return Err("keychain unavailable".into());
            }
            // What Windows Credential Manager enforces (keyring's TooLong).
            if value.encode_utf16().count() * 2 > 2560 {
                return Err("Attribute 'password' is longer than platform limit of 2560 chars".into());
            }
            self.slots.borrow_mut().insert(account.into(), value.into());
            Ok(())
        }
        fn delete(&self, account: &str) -> Result<(), String> {
            self.slots.borrow_mut().remove(account);
            Ok(())
        }
        fn max_utf16(&self) -> usize {
            self.max
        }
    }

    fn big_map() -> Map {
        // Roughly an access + refresh token pair per provider, plus an AI key.
        let token = "x".repeat(1800);
        HashMap::from([
            ("__prisma_access__".into(), token.clone()),
            ("__prisma_refresh__".into(), token.clone()),
            ("__neon_access__".into(), token),
            ("openai".into(), "sk-test".into()),
        ])
    }

    fn tmp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("stroke-secrets-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("ai-keys.json")
    }

    #[test]
    fn a_vault_past_the_windows_limit_round_trips_in_chunks() {
        let store = MemStore::new(1200);
        let map = big_map();
        write_vault(&store, &map).unwrap();
        assert_eq!(read_vault(&store).unwrap(), Some(map));
        assert!(store.slots.borrow().len() > 2, "expected several chunks");
    }

    #[test]
    fn rewriting_drops_the_previous_chunks() {
        let store = MemStore::new(1200);
        write_vault(&store, &big_map()).unwrap();
        let small = HashMap::from([("k".to_string(), "v".to_string())]);
        write_vault(&store, &small).unwrap();
        assert_eq!(read_vault(&store).unwrap(), Some(small));
        // Header + one chunk.
        assert_eq!(store.slots.borrow().len(), 2);
    }

    #[test]
    fn the_old_single_credential_format_still_reads() {
        let store = MemStore::new(1200);
        store.slots.borrow_mut().insert(KEYCHAIN_ACCOUNT.into(), r#"{"openai":"sk-old"}"#.into());
        assert_eq!(read_vault(&store).unwrap().unwrap()["openai"], "sk-old");
    }

    #[test]
    fn a_newer_fallback_file_beats_a_stale_credential() {
        // What the old code left behind on Windows: a small credential from
        // before sign-in, and the tokens only in the file.
        let store = MemStore::new(1200);
        store.slots.borrow_mut().insert(KEYCHAIN_ACCOUNT.into(), r#"{"openai":"sk-old"}"#.into());
        let path = tmp_path("stale");
        write_file(&path, &big_map()).unwrap();

        let loaded = load_from(&path, &store);
        assert_eq!(loaded, big_map());
        // ...and it was moved into the (chunked) keychain, file gone.
        assert_eq!(read_vault(&store).unwrap(), Some(big_map()));
        assert!(!path.exists());
    }

    #[test]
    fn a_failing_keychain_falls_back_to_the_file_and_stays_signed_in() {
        let store = MemStore::new(1200);
        store.slots.borrow_mut().insert(KEYCHAIN_ACCOUNT.into(), r#"{"openai":"sk-old"}"#.into());
        *store.fail_writes.borrow_mut() = true;
        let path = tmp_path("fallback");

        persist_to(&path, &store, &big_map()).unwrap();
        // The stale credential can't be read in place of the file any more.
        assert_eq!(read_vault(&store).unwrap(), None);
        // Next launch.
        assert_eq!(load_from(&path, &store), big_map());
    }

    #[test]
    fn split_respects_the_limit_and_char_boundaries() {
        let s = "aé😀".repeat(500);
        let parts = split_utf16(&s, 7);
        assert!(parts.iter().all(|p| p.encode_utf16().count() <= 7));
        assert_eq!(parts.concat(), s);
    }
}
