/// Licensing primitives shared by the account code (`account.rs`).
///
/// Stroke 3 has no license keys and no device trial: Pro comes from the
/// signed-in account's signed entitlement. What is left here is the status the
/// UI reads, the key that verifies entitlements, the clock guard and the device
/// fingerprint.
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Ed25519 public key that signs account entitlements on stroke.click.
pub const PUBLIC_KEY_HEX: &str =
    "b5f14fed4694302a767e32c4e6a50ba0074c4e50f5198ad781de00d0e8ba8771";

#[derive(serde::Serialize, Debug, Clone)]
#[serde(tag = "status")]
pub enum LicenseStatus {
    /// Paid Pro on the account
    Valid {
        email: String,
        plan: String,
        issued_at: u64,
        expires_at: Option<u64>,
    },
    /// The account's free trial is running
    Trial { days_remaining: i64 },
    /// No Pro: trial over, free plan, or signed out
    TrialExpired,
    /// App data dir inaccessible (should not normally happen)
    Error { message: String },
}

// ── Paths ─────────────────────────────────────────────────────────────────────

/// The user's home directory, cross-platform (HOME on unix/macOS, USERPROFILE on Windows).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

// ── Clock guard ───────────────────────────────────────────────────────────────
//
// Offline, every expiry check compares against the system clock - and setting
// the clock back would stretch a trial or a cached entitlement for as long as
// the machine stays off the network. The latest time the app has ever seen is
// kept (in two places, so deleting one copy changes nothing),
// and offline checks use whichever is later: the clock, or that mark. Winding
// the clock back then gains nothing. A successful server check resets the mark
// to the server's own time, so a clock that was wrong in the future direction
// heals the next time the app is online.

fn clock_paths(data_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![data_dir.join(".clock")];
    if let Some(home) = home_dir() {
        paths.push(home.join(".stroke").join("clock"));
    }
    paths
}

fn read_clock_mark(data_dir: &Path) -> u64 {
    clock_paths(data_dir)
        .iter()
        .filter_map(|p| std::fs::read(p).ok())
        .filter_map(|b| <[u8; 8]>::try_from(b.as_slice()).ok())
        .map(u64::from_le_bytes)
        .max()
        .unwrap_or(0)
}

fn write_clock_mark(data_dir: &Path, t: u64) {
    for p in clock_paths(data_dir) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, t.to_le_bytes());
    }
}

/// "Now" for offline license checks: the later of the system clock and the
/// latest time this app has seen. Advances the mark (at most every few minutes,
/// so a status check is not a disk write).
pub fn trusted_now(data_dir: &Path) -> u64 {
    let now = now_secs();
    let mark = read_clock_mark(data_dir);
    if now > mark.saturating_add(300) {
        write_clock_mark(data_dir, now);
    }
    now.max(mark)
}

/// The server has vouched for the time: reset the mark to it.
pub fn reset_clock_mark(data_dir: &Path, server_now: u64) {
    if server_now > 0 {
        write_clock_mark(data_dir, server_now);
    }
}

// ── Device fingerprint ────────────────────────────────────────────────────────

/// Returns a stable SHA-256 hex fingerprint for this machine.
///
/// Cached for the process lifetime: computing it shells out to `ioreg`/`reg`
/// on macOS/Windows, and callers hit this on every license and quota check.
pub fn device_id() -> String {
    static DEVICE_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DEVICE_ID
        .get_or_init(|| {
            let raw = machine_id_raw();
            let mut h = Sha256::new();
            h.update(raw.as_bytes());
            hex::encode(h.finalize())
        })
        .clone()
}

fn machine_id_raw() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/etc/machine-id") {
            let t = s.trim().to_string();
            if !t.is_empty() {
                return t;
            }
        }
        if let Ok(s) = std::fs::read_to_string("/var/lib/dbus/machine-id") {
            let t = s.trim().to_string();
            if !t.is_empty() {
                return t;
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
        {
            if let Ok(s) = String::from_utf8(out.stdout) {
                for line in s.lines() {
                    if line.contains("IOPlatformUUID") {
                        if let Some(uuid) = line.split('"').nth(3) {
                            return uuid.to_string();
                        }
                    }
                }
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        // quiet_std: the machine-id lookup runs at startup for the licence check.
        if let Ok(out) = crate::proc::quiet_std(std::process::Command::new("reg").args([
            "query",
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Cryptography",
            "/v",
            "MachineGuid",
        ]))
        .output()
        {
            if let Ok(s) = String::from_utf8(out.stdout) {
                for line in s.lines() {
                    if line.contains("MachineGuid") {
                        if let Some(guid) = line.split_whitespace().last() {
                            return guid.to_string();
                        }
                    }
                }
            }
        }
    }
    // Fallback: try platform-appropriate env vars then /etc/hostname.
    // COMPUTERNAME is the correct Windows variable; HOSTNAME is POSIX-standard.
    std::env::var("COMPUTERNAME")                          // Windows
        .or_else(|_| std::env::var("HOSTNAME"))           // Linux/macOS shell env
        .or_else(|_| std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()))
        .unwrap_or_else(|_| "unknown-host".to_string())
}

// ── Time ──────────────────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
