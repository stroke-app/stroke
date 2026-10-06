/*!
Background updates.

The updater plugin's JS flow downloads and installs in one step and then
restarts, so an update always interrupts whatever is open. This module splits
that apart so an update can download while Stroke runs and install as it quits,
the way a browser does it:

  - `update_check` asks the release feed for a newer build.
  - `update_download` fetches it (the plugin verifies the signature) and
    stages the bytes on disk. The AppImage alone is ~100MB, too much to hold
    in memory for a whole session.
  - `update_status` returns the staged build, so a reloaded webview can pick
    it up again.
  - `update_restart` installs the staged build now and relaunches.
  - `apply_on_quit` installs the staged build from `RunEvent::Exit`, so the
    next launch is the new version.

Installing on quit only happens where it can finish without asking anyone
anything (see `can_apply_on_quit`). Everywhere else the staged build waits for
`update_restart`, which is the old prompt-and-restart flow.
*/

use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// A check that cannot reach GitHub should give up in seconds. Only the check:
/// the download carries no timeout, a 100MB AppImage on a slow line needs one.
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);

/// Progress events per download are throttled to this. The plugin reports one
/// per HTTP chunk, thousands of them, and each one is an IPC message.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Read by the NSIS template (src-tauri/nsis/installer.nsi): install silently
/// and do not reopen the app, because the person just closed it.
#[cfg(target_os = "windows")]
const QUIT_UPDATE_ENV: &str = "STROKE_UPDATE_ON_QUIT";

// ── Types ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// Release notes from the feed (the CHANGELOG section for that version).
    pub notes: String,
    /// Publish date, unix seconds.
    pub date: Option<i64>,
    /// Downloaded and waiting to install.
    pub staged: bool,
    /// This install can apply a staged build by itself when Stroke quits.
    pub apply_on_quit: bool,
}

#[derive(Clone, Serialize)]
#[serde(
    tag = "event",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DownloadEvent {
    Started { content_length: Option<u64> },
    Progress { downloaded: u64 },
    Finished,
}

struct Staged {
    update: Update,
    path: PathBuf,
}

#[derive(Default)]
pub struct UpdateState {
    /// The last check's result, waiting for `update_download`.
    pending: Mutex<Option<Update>>,
    staged: Mutex<Option<Staged>>,
    /// Set when the exit under way is a restart. A Windows install then keeps
    /// the installer's own relaunch instead of suppressing it.
    restarting: AtomicBool,
}

/// A poisoned lock only means another command panicked mid-update; the Option
/// inside is still whole.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn info(update: &Update, staged: bool) -> UpdateInfo {
    UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        notes: update.body.clone().unwrap_or_default(),
        date: update.date.map(|d| d.unix_timestamp()),
        staged,
        apply_on_quit: can_apply_on_quit(),
    }
}

fn staged_info(state: &UpdateState) -> Option<UpdateInfo> {
    lock(&state.staged).as_ref().map(|s| info(&s.update, true))
}

fn updates_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_cache_dir()
        .map(|dir| dir.join("updates"))
        .map_err(|e| e.to_string())
}

// ── Commands ───────────────────────────────────────────────────────────────────

/// Ask the release feed for a newer build. Returns the staged build instead
/// when one is already waiting, so a second check never downloads twice.
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    state: State<'_, UpdateState>,
) -> Result<Option<UpdateInfo>, String> {
    if let Some(staged) = staged_info(&state) {
        return Ok(Some(staged));
    }
    let update = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let found = update.as_ref().map(|u| info(u, false));
    *lock(&state.pending) = update;
    Ok(found)
}

/// Download the update from the last check and stage it on disk.
#[tauri::command]
pub async fn update_download(
    app: AppHandle,
    state: State<'_, UpdateState>,
    on_event: Channel<DownloadEvent>,
) -> Result<UpdateInfo, String> {
    if let Some(staged) = staged_info(&state) {
        return Ok(staged);
    }
    let update = lock(&state.pending)
        .clone()
        .ok_or("No update to download. Check for updates first.")?;

    let mut downloaded: u64 = 0;
    let mut started = false;
    let mut last_sent: Option<Instant> = None;
    let bytes = update
        .download(
            |chunk, content_length| {
                if !started {
                    started = true;
                    let _ = on_event.send(DownloadEvent::Started { content_length });
                }
                downloaded += chunk as u64;
                if last_sent.map_or(true, |t| t.elapsed() >= PROGRESS_INTERVAL) {
                    last_sent = Some(Instant::now());
                    let _ = on_event.send(DownloadEvent::Progress { downloaded });
                }
            },
            || {
                let _ = on_event.send(DownloadEvent::Finished);
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    let dir = updates_dir(&app)?;
    // The version comes from the feed but was parsed as semver by the plugin,
    // so it is only digits, letters, dots, dashes and plus signs.
    let path = dir.join(format!("{}.update", update.version));
    let write_path = path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(&write_path, bytes)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("Could not save the update: {e}"))?;

    let staged = info(&update, true);
    *lock(&state.staged) = Some(Staged { update, path });
    *lock(&state.pending) = None;
    Ok(staged)
}

/// The staged build, if there is one. A webview reload loses the frontend's
/// state but not this, so the status bar can still say an update is ready.
#[tauri::command]
pub fn update_status(state: State<'_, UpdateState>) -> Option<UpdateInfo> {
    staged_info(&state)
}

/// Install the staged build now and relaunch. Async so it runs off the main
/// thread: a macOS install that needs admin rights asks through the main
/// thread, and `restart()` from another thread goes through the normal exit.
#[tauri::command]
pub async fn update_restart(app: AppHandle, state: State<'_, UpdateState>) -> Result<(), String> {
    let staged = lock(&state.staged)
        .take()
        .ok_or("No update is ready to install.")?;
    let bytes = match std::fs::read(&staged.path) {
        Ok(bytes) => bytes,
        Err(e) => return Err(format!("The downloaded update is missing: {e}")),
    };
    // On Windows this hands over to the installer and exits; the installer
    // relaunches the app when it is done (`/R`).
    if let Err(e) = staged.update.install(&bytes) {
        let msg = e.to_string();
        *lock(&state.staged) = Some(staged);
        return Err(msg);
    }
    let _ = std::fs::remove_file(&staged.path);
    app.restart();
}

// ── Lifecycle ──────────────────────────────────────────────────────────────────

/// Delete builds staged by an earlier session. One that was never installed is
/// fetched again by the next check, which also catches a newer release.
pub fn clear_stale(app: &AppHandle) {
    let Ok(dir) = updates_dir(app) else { return };
    std::thread::spawn(move || {
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        for entry in entries.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    });
}

/// `RunEvent::ExitRequested` with the restart code: the exit that follows is a
/// restart, not a quit.
pub fn note_restart(app: &AppHandle) {
    app.state::<UpdateState>()
        .restarting
        .store(true, Ordering::Relaxed);
}

/// Install a staged build as Stroke quits. Called from `RunEvent::Exit`, on the
/// main thread, after everything else has shut down: on Windows a successful
/// install ends the process.
pub fn apply_on_quit(app: &AppHandle) {
    let state = app.state::<UpdateState>();
    let Some(staged) = lock(&state.staged).take() else { return };
    if !can_apply_on_quit() {
        return;
    }
    let bytes = match std::fs::read(&staged.path) {
        Ok(bytes) => bytes,
        Err(e) => {
            log::warn!("[updater] staged update unreadable, skipping install: {e}");
            return;
        }
    };

    #[cfg(target_os = "windows")]
    {
        // Windows is ending the session: an installer started now would be
        // killed halfway through and leave a broken install behind.
        if session_ending() {
            log::info!("[updater] session ending, leaving the update for next time");
            return;
        }
        if !state.restarting.load(Ordering::Relaxed) {
            std::env::set_var(QUIT_UPDATE_ENV, "1");
        }
    }

    log::info!("[updater] installing {} on quit", staged.update.version);
    match staged.update.install(&bytes) {
        Ok(()) => {
            let _ = std::fs::remove_file(&staged.path);
        }
        Err(e) => log::error!("[updater] install on quit failed: {e}"),
    }
}

// ── Can this install update itself on quit? ────────────────────────────────────

/// True when a staged build can be installed at quit with nobody watching: no
/// password prompt, no second copy beside a package manager's, nothing that
/// would need the main thread while it is shutting down. Never in dev builds.
pub fn can_apply_on_quit() -> bool {
    !cfg!(debug_assertions) && platform_can_apply()
}

/// The NSIS install, in the folder it installed to. Scoop unpacks the same
/// setup into its own folder (bundle type still reads NSIS); installing there
/// would add a second copy under %LOCALAPPDATA% and repeat on every quit. The
/// uninstaller is written by the installer itself, so it marks a real install.
#[cfg(target_os = "windows")]
fn platform_can_apply() -> bool {
    use tauri::utils::config::BundleType;
    if !matches!(tauri::utils::platform::bundle_type(), Some(BundleType::Nsis)) {
        return false;
    }
    let Ok(exe) = tauri::utils::platform::current_exe() else { return false };
    if exe.to_string_lossy().to_lowercase().contains("\\scoop\\") {
        return false;
    }
    exe.parent()
        .is_some_and(|dir| dir.join("uninstall.exe").is_file())
}

/// The .app bundle, where its folder and the bundle itself are writable. If not
/// (another user's /Applications, a translocated or DMG-mounted copy) the
/// plugin asks for an admin password through the main thread, which deadlocks
/// inside the exit handler. The plugin also swaps the bundle by renaming it
/// through the temp dir, which cannot work across volumes.
#[cfg(target_os = "macos")]
fn platform_can_apply() -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(exe) = tauri::utils::platform::current_exe() else { return false };
    // Stroke.app/Contents/MacOS/stroke
    let Some(bundle) = exe.ancestors().nth(3) else { return false };
    if bundle.extension().and_then(|e| e.to_str()) != Some("app") {
        return false;
    }
    let same_volume = match (bundle.metadata(), std::env::temp_dir().metadata()) {
        (Ok(app), Ok(tmp)) => app.dev() == tmp.dev(),
        _ => false,
    };
    same_volume && bundle.parent().is_some_and(writable) && writable(bundle)
}

/// The AppImage only. A .deb or .rpm installs through pkexec, which asks for a
/// password, and latest.json ships only the AppImage for Linux anyway.
#[cfg(target_os = "linux")]
fn platform_can_apply() -> bool {
    use tauri::utils::config::BundleType;
    if !matches!(
        tauri::utils::platform::bundle_type(),
        Some(BundleType::AppImage)
    ) {
        return false;
    }
    let Some(appimage) = std::env::var_os("APPIMAGE") else { return false };
    std::path::Path::new(&appimage).parent().is_some_and(writable)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn platform_can_apply() -> bool {
    false
}

/// Write access as the OS would grant it, checked without writing anything:
/// the macOS probe runs against a signed bundle.
#[cfg(unix)]
fn writable(path: &std::path::Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `c` is a valid NUL-terminated string that outlives the call.
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

#[cfg(target_os = "windows")]
fn session_ending() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(index: i32) -> i32;
    }
    const SM_SHUTTINGDOWN: i32 = 0x2000;
    // SAFETY: GetSystemMetrics takes a plain index and has no preconditions.
    unsafe { GetSystemMetrics(SM_SHUTTINGDOWN) != 0 }
}
