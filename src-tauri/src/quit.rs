//! Quitting with the window open (⌘Q, Quit in the menu): the page gets a
//! moment to finish (unsaved edits, the last sync push) before the process
//! ends. See the `ExitRequested` arm in lib.rs.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tauri::{Emitter, Manager};

static APPROVED: AtomicBool = AtomicBool::new(false);
/// Bumped by every request and every cancel, so a stale fallback timer does nothing.
static GEN: AtomicU64 = AtomicU64::new(0);

const FALLBACK: Duration = Duration::from_secs(6);

pub fn approved() -> bool {
    APPROVED.load(Ordering::SeqCst)
}

/// Ask the page, and quit anyway if it never answers.
pub fn requested(app: &tauri::AppHandle) {
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = app.emit("app-quit-requested", ());
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FALLBACK);
        if GEN.load(Ordering::SeqCst) == gen && !approved() {
            APPROVED.store(true, Ordering::SeqCst);
            handle.exit(0);
        }
    });
}

/// The page is done: quit now.
#[tauri::command]
pub fn quit_now(app: tauri::AppHandle) {
    APPROVED.store(true, Ordering::SeqCst);
    // Every window first, so their close paths don't ask again.
    for (_, w) in app.webview_windows() {
        let _ = w.destroy();
    }
    app.exit(0);
}

/// The user chose to stay (kept their unsaved edits).
#[tauri::command]
pub fn cancel_quit() {
    GEN.fetch_add(1, Ordering::SeqCst);
}
