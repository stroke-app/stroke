//! Linux: native Wayland by default, X11 where it can't draw.
//!
//! Stroke used to force X11 (XWayland) everywhere, because on some Wayland
//! setups WebKit's web process aborts while creating its EGL display and the
//! window stays blank (issue #112 and the EGL_BAD_ALLOC reports). Most setups
//! draw fine on Wayland and look sharper there, so Wayland is now the default,
//! with a launch check:
//!
//! - Before GTK starts, a Wayland launch leaves a `wayland-pending` marker.
//! - The page's first finished screen (`reveal_window`) removes it.
//! - If the next launch still finds it, the last Wayland launch never drew a
//!   page: this one, and every one after, uses X11 (`use-x11`).
//!
//! Overrides: `GDK_BACKEND` set by the user wins; `STROKE_X11=1` forces X11;
//! `STROKE_WAYLAND=1` forces Wayland and forgets an earlier fallback.

#[cfg(target_os = "linux")]
mod imp {
    use std::path::PathBuf;

    use std::sync::atomic::{AtomicBool, Ordering};

    const PENDING: &str = "wayland-pending";
    /// This launch is a Wayland trial (it left the marker).
    static TRIAL: AtomicBool = AtomicBool::new(false);
    const FALLBACK: &str = "use-x11";

    /// `~/.local/state/<binary>`: Stroke and Stroke Nightly keep their own.
    fn state_dir() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
        let exe = std::env::current_exe().ok()?;
        let name = exe.file_stem()?.to_string_lossy().to_lowercase();
        Some(base.join(format!("{name}-display")))
    }

    fn set(backend: &str) {
        // SAFETY: called from run() before any thread is spawned.
        unsafe { std::env::set_var("GDK_BACKEND", backend) };
    }

    /// Pick the backend. Must run before GTK initialises and before any thread.
    pub fn choose() {
        let Some(dir) = state_dir() else { return };
        let pending = dir.join(PENDING);
        let fallback = dir.join(FALLBACK);
        if std::env::var("STROKE_WAYLAND").as_deref() == Ok("1") {
            let _ = std::fs::remove_file(&fallback);
            set("wayland");
            return;
        }
        if std::env::var("STROKE_X11").as_deref() == Ok("1") {
            set("x11");
            return;
        }
        // The user (or their desktop) already chose.
        if std::env::var_os("GDK_BACKEND").is_some() {
            return;
        }
        // An X11 session: nothing to decide.
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return;
        }
        if fallback.exists() {
            set("x11");
            return;
        }
        if pending.exists() {
            log::warn!("display: the last Wayland launch never drew a page; using X11 from now on (STROKE_WAYLAND=1 to try Wayland again)");
            let _ = std::fs::write(&fallback, "The last Wayland launch didn't draw. Delete this file, or start Stroke with STROKE_WAYLAND=1, to try Wayland again.\n");
            let _ = std::fs::remove_file(&pending);
            set("x11");
            return;
        }
        let _ = std::fs::create_dir_all(&dir);
        if std::fs::write(&pending, b"").is_ok() {
            TRIAL.store(true, Ordering::Relaxed);
        }
    }

    /// A Wayland launch that hasn't drawn in 25s won't: the web process died
    /// creating its EGL display and the window stays hidden. Restart on X11
    /// now instead of leaving a blank launch. Release builds only (a dev
    /// build's first page waits on the Vite dev server).
    pub fn watch(app: tauri::AppHandle) {
        if cfg!(debug_assertions) || !TRIAL.load(Ordering::Relaxed) {
            return;
        }
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(25));
            let Some(dir) = state_dir() else { return };
            if !dir.join(PENDING).exists() {
                return;
            }
            log::warn!("display: no page drawn on Wayland after 25s; restarting on X11");
            let _ = std::fs::write(dir.join(FALLBACK), "A Wayland launch didn't draw. Delete this file, or start Stroke with STROKE_WAYLAND=1, to try Wayland again.\n");
            let _ = std::fs::remove_file(dir.join(PENDING));
            app.restart();
        });
    }

    /// The page drew: this backend works here.
    pub fn drawn() {
        if let Some(dir) = state_dir() {
            let _ = std::fs::remove_file(dir.join(PENDING));
        }
    }
}

#[cfg(target_os = "linux")]
pub use imp::{choose, drawn, watch};

#[cfg(not(target_os = "linux"))]
pub fn choose() {}

#[cfg(not(target_os = "linux"))]
pub fn drawn() {}

#[cfg(not(target_os = "linux"))]
pub fn watch(_app: tauri::AppHandle) {}
