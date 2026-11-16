//! Crash reports. Release builds abort on a panic (Cargo.toml), so the window
//! is simply gone and nothing on screen says why. The panic hook writes what it
//! knows to a file before the abort; the next launch finds it and the page
//! offers to report it (`take_crash_report`, ReportIssueDialog).

use std::path::PathBuf;
use std::sync::OnceLock;

const FILE: &str = "last-crash.json";

/// The app's data directory, once `setup()` knows it. A panic before that
/// writes to the temp directory instead; both are checked on the next launch.
static DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_dir(dir: PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    let _ = DIR.set(dir);
}

fn fallback_path() -> PathBuf {
    std::env::temp_dir().join(format!("stroke-{FILE}"))
}

fn report_path() -> PathBuf {
    DIR.get().map(|d| d.join(FILE)).unwrap_or_else(fallback_path)
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct CrashReport {
    pub message: String,
    pub location: String,
    pub thread: String,
    pub version: String,
    pub os: String,
    /// Seconds since the Unix epoch.
    pub at: u64,
    pub backtrace: String,
}

fn payload_text(info: &std::panic::PanicHookInfo<'_>) -> String {
    let p = info.payload();
    p.downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

/// Install the hook. Call first thing in `run()`. The previous hook still runs
/// (stderr), and after it the process aborts as before, so the operating
/// system's own crash reporter sees the crash too.
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = CrashReport {
            message: payload_text(info),
            location: info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default(),
            thread: std::thread::current().name().unwrap_or("unnamed").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            backtrace: std::backtrace::Backtrace::force_capture().to_string(),
        };
        log::error!("panic at {}: {}", report.location, report.message);
        if let Ok(json) = serde_json::to_vec_pretty(&report) {
            let _ = std::fs::write(report_path(), json);
        }
        previous(info);
    }));
}

/// The report the last run left behind, if it crashed. Reading it deletes it,
/// so it is offered once.
#[tauri::command]
pub fn take_crash_report() -> Option<CrashReport> {
    let mut found = None;
    for path in [report_path(), fallback_path()] {
        if let Ok(bytes) = std::fs::read(&path) {
            let _ = std::fs::remove_file(&path);
            if found.is_none() {
                found = serde_json::from_slice::<CrashReport>(&bytes).ok();
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_round_trips_and_is_taken_once() {
        let dir = std::env::temp_dir().join(format!("stroke-crash-test-{}", std::process::id()));
        set_dir(dir.clone());
        let r = CrashReport {
            message: "boom".into(),
            location: "src/x.rs:1".into(),
            thread: "main".into(),
            version: "1.0.0".into(),
            os: "linux x86_64".into(),
            at: 1,
            backtrace: String::new(),
        };
        std::fs::write(report_path(), serde_json::to_vec(&r).unwrap()).unwrap();
        assert_eq!(take_crash_report(), Some(r));
        assert_eq!(take_crash_report(), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
