//! Quitting with the window open (⌘Q, Quit in the menu): the page gets a
//! moment to finish (unsaved edits, the last sync push) before the process
//! ends. See the `ExitRequested` arm in lib.rs.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

static APPROVED: AtomicBool = AtomicBool::new(false);
/// Bumped by every request and every cancel, so a stale fallback timer does nothing.
static GEN: AtomicU64 = AtomicU64::new(0);

const FALLBACK: Duration = Duration::from_secs(6);

pub fn approved() -> bool {
    APPROVED.load(Ordering::SeqCst)
}

/// Ask the page, and quit anyway if it never answers.
pub fn requested<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
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

// ── ⌘Q guard ────────────────────────────────────────────────────────────────
// Only the macOS menu (mac_chrome.rs) calls into this; elsewhere it is unused.
// The menu's Quit asks for a second ⌘Q (or a held one: key repeat sends the
// shortcut again) within the window below. The first press only shows a hint.

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub const QUIT_MENU_ID: &str = "stroke-quit";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const SECOND_PRESS: Duration = Duration::from_millis(1600);
static GUARD_OFF: AtomicBool = AtomicBool::new(false);
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
static FIRST_PRESS: Mutex<Option<Instant>> = Mutex::new(None);

/// Settings: "Press ⌘Q twice to quit". Off quits on the first press.
#[tauri::command]
pub fn set_quit_guard(on: bool) {
    GUARD_OFF.store(!on, Ordering::SeqCst);
}

/// The menu's Quit item (and its ⌘Q).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn menu_quit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let now = Instant::now();
    let second = {
        let mut first = FIRST_PRESS.lock().unwrap_or_else(|e| e.into_inner());
        let second = GUARD_OFF.load(Ordering::SeqCst)
            || first.is_some_and(|t| now.duration_since(t) <= SECOND_PRESS);
        *first = if second { None } else { Some(now) };
        second
    };
    if second {
        // Through the same path as any other quit: the page finishes unsaved
        // edits and the last sync push first.
        if app.webview_windows().is_empty() {
            app.exit(0);
        } else {
            requested(app);
        }
    } else {
        let _ = app.emit("app-quit-hint", SECOND_PRESS.as_millis() as u64);
    }
}
