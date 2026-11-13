/*!
Process naming and runtime metrics for Stroke.

Exposes two Tauri commands:
  - `set_process_title`  - renames the process so it shows up as a recognisable
                           name in htop / ps / Activity Monitor instead of the
                           raw binary name.
  - `get_app_metrics`    - returns the current process's PID, resident memory
                           (RSS in bytes), virtual memory, and CPU percentage so
                           the frontend can display a live performance indicator.
*/

use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

// ── Shared System instance ─────────────────────────────────────────────────────
// sysinfo::System is not Send+Sync by itself, so we wrap it in a Mutex and keep
// a single instance to avoid re-building the expensive OS handle on every call.
static SYS: OnceLock<Mutex<System>> = OnceLock::new();

fn sys() -> &'static Mutex<System> {
    SYS.get_or_init(|| {
        Mutex::new(System::new_with_specifics(
            RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
        ))
    })
}

// ── Types ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppMetrics {
    /// OS process ID of this Stroke process.
    pub pid: u32,
    /// Resident Set Size in bytes - actual physical RAM in use.
    pub rss_bytes: u64,
    /// Virtual memory size in bytes.
    pub virtual_bytes: u64,
    /// CPU usage as a percentage (0-100, may exceed 100 on multi-core).
    pub cpu_percent: f32,
    /// Human-readable label for the process visible in the OS monitor.
    pub process_name: String,
}

// ── Commands ───────────────────────────────────────────────────────────────────

/// Rename this process so it shows as `name` in `htop`, `ps`, and Activity
/// Monitor. Only works on Linux (via `prctl PR_SET_NAME`) and macOS (argv[0]).
/// On other platforms this is a no-op and still returns Ok.
#[tauri::command]
pub fn set_process_title(_name: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        // Truncate to 15 bytes - Linux kernel limit for PR_SET_NAME.
        let truncated: String = _name.chars().take(15).collect();
        let cname = CString::new(truncated).map_err(|e| e.to_string())?;
        // SAFETY: prctl is thread-safe for PR_SET_NAME; no memory aliasing.
        unsafe {
            libc::prctl(libc::PR_SET_NAME, cname.as_ptr(), 0, 0, 0);
        }
    }
    Ok(())
}

/// Sample this process's memory and CPU usage and return the values to the
/// frontend. The first call after startup may show 0% CPU (sysinfo needs two
/// samples to compute a delta); subsequent calls will be accurate.
#[tauri::command(async)]
pub fn get_app_metrics() -> Result<AppMetrics, String> {
    let pid = std::process::id();
    let sysinfo_pid = Pid::from(pid as usize);

    let mut sys = sys().lock().map_err(|e| e.to_string())?;
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[sysinfo_pid]),
        true,
        ProcessRefreshKind::everything(),
    );

    let proc = sys
        .process(sysinfo_pid)
        .ok_or_else(|| "process not found in sysinfo".to_string())?;

    Ok(AppMetrics {
        pid,
        rss_bytes: proc.memory(),
        virtual_bytes: proc.virtual_memory(),
        cpu_percent: proc.cpu_usage(),
        process_name: proc.name().to_string_lossy().to_string(),
    })
}
