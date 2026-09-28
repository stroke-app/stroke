//! Durable copy of the saved-connections list.
//!
//! The frontend keeps connections in `localStorage`, but the webview flushes
//! that to disk on its own schedule. WebView2 on Windows in particular can drop
//! the last writes when the app closes soon after them, so a deleted connection
//! came back on the next launch (and a freshly added one could vanish). This
//! file is written synchronously and fsynced on every change, and the frontend
//! loads it before anything reads the list.

use std::path::{Path, PathBuf};

const FILE_NAME: &str = "connections.json";

fn store_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

/// The stored payload, or `None` before the first write.
pub fn read(data_dir: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(store_path(data_dir)) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Could not read saved connections: {e}")),
    }
}

/// Replace the stored payload. Written to a sibling temp file, fsynced, then
/// renamed over the old one, so a crash mid-write leaves the previous list
/// intact instead of a truncated file.
pub fn write(data_dir: &Path, json: &str) -> Result<(), String> {
    use std::io::Write;

    serde_json::from_str::<serde_json::Value>(json)
        .map_err(|e| format!("Refusing to save connections: payload is not JSON ({e})"))?;
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;

    let path = store_path(data_dir);
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
