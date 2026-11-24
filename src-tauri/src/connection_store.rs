//! Durable copy of the saved-connections list.
//!
//! The frontend keeps connections in `localStorage`, but the webview flushes
//! that to disk on its own schedule. WebView2 on Windows in particular can drop
//! the last writes when the app closes soon after them, so a deleted connection
//! came back on the next launch (and a freshly added one could vanish). This
//! file is written synchronously and fsynced on every change, and the frontend
//! loads it before anything reads the list.
//!
//! Stroke and Stroke Nightly share one file, in Stable's data folder
//! (`shared_dir`). They already share the keychain that holds the passwords, and
//! with a file each, a connection added in one track was missing in the other,
//! and the other track then dropped its password from the shared keychain. Both
//! can run at once, so a write merges with the file on disk instead of replacing
//! it: connections the other app added are kept, and a delete is a tombstone in
//! `deleted` that wins over a stale copy still held by the other app.

use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const FILE_NAME: &str = "connections.json";
/// The suffix Nightly's identifier adds to Stable's (tauri.nightly.conf.json).
const NIGHTLY_SUFFIX: &str = ".nightly";
/// Matches MAX_DELETED_IDS in src/lib/stores/connections.js.
const MAX_DELETED: usize = 500;

/// Serialises read-merge-write within this process.
static LOCK: Mutex<()> = Mutex::new(());

fn store_path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// Stable's data folder, which both tracks use for this file. Nightly's own
/// folder is Stable's with `.nightly` on the end, beside it.
pub fn shared_dir(app_data_dir: &Path) -> PathBuf {
    match app_data_dir.file_name().and_then(|n| n.to_str()) {
        Some(name) if name.ends_with(NIGHTLY_SUFFIX) => {
            app_data_dir.with_file_name(&name[..name.len() - NIGHTLY_SUFFIX.len()])
        }
        _ => app_data_dir.to_path_buf(),
    }
}

fn read_file(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Could not read saved connections: {e}")),
    }
}

/// The stored payload, or `None` before the first write. On Nightly, a list it
/// kept in its own folder before the file was shared is merged in once, then
/// renamed to `connections.json.merged` (kept, not deleted).
pub fn read(app_data_dir: &Path) -> Result<Option<String>, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let shared = shared_dir(app_data_dir);
    if shared != app_data_dir {
        let own = store_path(app_data_dir);
        if let Some(own_json) = read_file(&own)? {
            let base = read_file(&store_path(&shared))?;
            let merged = merge_newer(base.as_deref(), &own_json)?;
            write_file(&shared, &merged)?;
            let _ = std::fs::rename(&own, own.with_extension("json.merged"));
        }
    }
    read_file(&store_path(&shared))
}

/// Save the payload, merged with what is on disk (see the module comment).
pub fn write(app_data_dir: &Path, json: &str) -> Result<(), String> {
    let incoming: Value = serde_json::from_str(json)
        .map_err(|e| format!("Refusing to save connections: payload is not JSON ({e})"))?;
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = shared_dir(app_data_dir);
    let disk = read_file(&store_path(&dir))?.and_then(|s| serde_json::from_str::<Value>(&s).ok());
    write_file(&dir, &merge(disk.as_ref(), &incoming).to_string())
}

fn ids(list: &Value) -> Vec<String> {
    list.as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn conn_id(c: &Value) -> Option<&str> {
    c.get("id").and_then(Value::as_str)
}

/// `incoming` (this app's list) merged with `disk` (the file as it is now):
/// - tombstones are the union of both, less the ids `incoming` revived;
/// - every connection in `incoming` that has no tombstone, in its order;
/// - then every connection only on disk (the other app added it) without one.
pub fn merge(disk: Option<&Value>, incoming: &Value) -> Value {
    let revived: HashSet<String> = ids(&incoming["revived"]).into_iter().collect();
    let mut deleted: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for id in disk.map(|d| ids(&d["deleted"])).unwrap_or_default().into_iter().chain(ids(&incoming["deleted"])) {
        if !revived.contains(&id) && seen.insert(id.clone()) {
            deleted.push(id);
        }
    }
    if deleted.len() > MAX_DELETED {
        deleted.drain(..deleted.len() - MAX_DELETED);
    }
    let gone: HashSet<&str> = deleted.iter().map(String::as_str).collect();

    let mut out: Vec<Value> = Vec::new();
    let mut have = HashSet::new();
    let lists = [incoming.get("connections"), disk.and_then(|d| d.get("connections"))];
    for c in lists.into_iter().flatten().filter_map(Value::as_array).flatten() {
        match conn_id(c) {
            Some(id) if gone.contains(id) => {}
            Some(id) => {
                if have.insert(id.to_string()) {
                    out.push(c.clone());
                }
            }
            // No id: keep it as it came (the frontend gives it one on load).
            None => out.push(c.clone()),
        }
    }
    json!({ "version": 1, "connections": out, "deleted": deleted })
}

/// One-time merge of Nightly's own list into the shared one. Neither side is
/// "this app's latest" here, so for an id in both, the copy connected to more
/// recently wins.
fn merge_newer(base: Option<&str>, own: &str) -> Result<String, String> {
    let own: Value = serde_json::from_str(own).map_err(|e| format!("Unreadable connections.json: {e}"))?;
    let Some(base) = base.and_then(|s| serde_json::from_str::<Value>(s).ok()) else {
        return Ok(merge(None, &own).to_string());
    };
    let stamp = |c: &Value| c.get("lastConnectedAt").and_then(Value::as_f64).unwrap_or(0.0);
    let mut own = own;
    if let (Some(mine), Some(theirs)) = (own["connections"].as_array_mut(), base["connections"].as_array()) {
        for c in mine.iter_mut() {
            if let Some(newer) = theirs.iter().find(|t| conn_id(t).is_some() && conn_id(t) == conn_id(c) && stamp(t) > stamp(c)) {
                *c = newer.clone();
            }
        }
    }
    Ok(merge(Some(&base), &own).to_string())
}

/// Written to a sibling temp file, fsynced, then renamed over the old one, so a
/// crash mid-write leaves the previous list intact instead of a truncated file.
fn write_file(dir: &Path, json: &str) -> Result<(), String> {
    use std::io::Write;

    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = store_path(dir);
    let tmp = path.with_extension("json.tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    // Saved connections include passwords: keep the file private to this user.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp).map_err(|e| format!("Could not save connections: {e}"))?;
    f.write_all(json.as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|e| format!("Could not save connections: {e}"))?;
    drop(f);
    std::fs::rename(&tmp, &path).map_err(|e| format!("Could not save connections: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("stroke-conn-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn names(v: &Value) -> Vec<&str> {
        v["connections"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect()
    }

    #[test]
    fn nightly_shares_stables_folder() {
        let base = Path::new("/data/com.broisnischal.stroke");
        assert_eq!(shared_dir(&base.with_file_name("com.broisnischal.stroke.nightly")), base);
        assert_eq!(shared_dir(base), base);
    }

    #[test]
    fn a_write_keeps_what_the_other_app_added() {
        let disk = json!({ "connections": [{ "id": "a" }, { "id": "n" }], "deleted": [] });
        // Stable never saw "n" (Nightly added it); its write must not drop it.
        let mine = json!({ "connections": [{ "id": "a", "name": "renamed" }, { "id": "s" }], "deleted": [] });
        let out = merge(Some(&disk), &mine);
        assert_eq!(names(&out), ["a", "s", "n"]);
        assert_eq!(out["connections"][0]["name"], "renamed");
    }

    #[test]
    fn a_delete_wins_over_a_stale_copy() {
        // Nightly deleted "x"; Stable still holds it and writes it back.
        let disk = json!({ "connections": [{ "id": "a" }], "deleted": ["x"] });
        let stale = json!({ "connections": [{ "id": "a" }, { "id": "x" }], "deleted": [] });
        let out = merge(Some(&disk), &stale);
        assert_eq!(names(&out), ["a"]);
        assert_eq!(out["deleted"], json!(["x"]));
        // An explicit re-add (the sample database) lifts the tombstone.
        let revive = json!({ "connections": [{ "id": "x" }], "deleted": [], "revived": ["x"] });
        let out = merge(Some(&disk), &revive);
        assert_eq!(names(&out), ["x", "a"]);
        assert_eq!(out["deleted"], json!([]));
    }

    #[test]
    fn tombstones_are_capped() {
        let many: Vec<String> = (0..MAX_DELETED + 10).map(|i| i.to_string()).collect();
        let out = merge(None, &json!({ "connections": [], "deleted": many }));
        assert_eq!(out["deleted"].as_array().unwrap().len(), MAX_DELETED);
        assert_eq!(out["deleted"][0], "10");
    }

    #[test]
    fn nightlys_own_file_is_merged_into_the_shared_one_once() {
        let root = tmp("migrate");
        let stable = root.join("com.broisnischal.stroke");
        let nightly = root.join("com.broisnischal.stroke.nightly");
        write_file(&stable, r#"{"connections":[{"id":"a","lastConnectedAt":5},{"id":"s"}],"deleted":["gone"]}"#).unwrap();
        write_file(&nightly, r#"{"connections":[{"id":"a","lastConnectedAt":9,"name":"newer"},{"id":"n"},{"id":"gone"}],"deleted":[]}"#).unwrap();

        let out: Value = serde_json::from_str(&read(&nightly).unwrap().unwrap()).unwrap();
        assert_eq!(names(&out), ["a", "n", "s"]);
        assert_eq!(out["connections"][0]["name"], "newer");
        assert!(!store_path(&nightly).exists());
        assert!(nightly.join("connections.json.merged").exists());
        // Stable reads the same list from its own folder.
        assert_eq!(read(&stable).unwrap(), read(&nightly).unwrap());

        // Writes from either app land in the one file and keep each other's rows.
        write(&nightly, r#"{"connections":[{"id":"a"},{"id":"n"},{"id":"n2"}],"deleted":[]}"#).unwrap();
        write(&stable, r#"{"connections":[{"id":"a"},{"id":"s"}],"deleted":["n"]}"#).unwrap();
        let out: Value = serde_json::from_str(&read(&stable).unwrap().unwrap()).unwrap();
        assert_eq!(names(&out), ["a", "s", "n2"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_a_payload_that_is_not_json() {
        let root = tmp("bad");
        assert!(write(&root, "{not json").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
