//! Main window geometry & state persistence (`window-state.json`).
//!
//! Native desktop apps remember the window's size, position and maximized state
//! across launches instead of forcing maximized on every boot. Bounds are
//! validated against currently connected monitors so a disconnected external
//! display can never leave the window off-screen.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow, WindowEvent};

const STATE_FILE: &str = "window-state.json";
const MIN_WIDTH: u32 = 960;
const MIN_HEIGHT: u32 = 600;
const SAVE_DEBOUNCE: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

fn state_path(data_dir: &Path) -> PathBuf {
    data_dir.join(STATE_FILE)
}

pub(crate) fn load(data_dir: &Path) -> Option<WindowState> {
    let raw = std::fs::read_to_string(state_path(data_dir)).ok()?;
    let st: WindowState = serde_json::from_str(&raw).ok()?;
    if st.width < MIN_WIDTH || st.height < MIN_HEIGHT {
        return None;
    }
    Some(st)
}

fn save(data_dir: &Path, state: &WindowState) {
    let _ = std::fs::create_dir_all(data_dir);
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(state_path(data_dir), json);
    }
}

/// Checks whether at least a usable portion of the saved window sits on a
/// currently available monitor so its titlebar stays reachable.
fn fits_any_monitor(window: &WebviewWindow, st: &WindowState) -> bool {
    let Ok(monitors) = window.available_monitors() else {
        return false;
    };
    if monitors.is_empty() {
        return false;
    }
    let w_left = st.x;
    let w_top = st.y;
    let w_right = st.x.saturating_add(st.width as i32);
    let w_bottom = st.y.saturating_add(st.height as i32);

    monitors.iter().any(|m| {
        let pos = m.position();
        let size = m.size();
        let m_left = pos.x;
        let m_top = pos.y;
        let m_right = pos.x.saturating_add(size.width as i32);
        let m_bottom = pos.y.saturating_add(size.height as i32);
        // At least 80px horizontally and 40px vertically on-screen, and titlebar not above monitor top.
        let overlap_x = w_right.min(m_right) - w_left.max(m_left);
        let overlap_y = w_bottom.min(m_bottom) - w_top.max(m_top);
        overlap_x >= 80 && overlap_y >= 40 && w_top >= m_top - 16
    })
}

/// Restores saved window bounds when available and visible, otherwise centers
/// the default window rather than forcing maximized on every launch.
pub(crate) fn restore(window: &WebviewWindow, data_dir: Option<&Path>) {
    let saved = data_dir.and_then(load);
    if let Some(st) = saved {
        let _ = window.set_size(PhysicalSize::new(st.width, st.height));
        if fits_any_monitor(window, &st) {
            let _ = window.set_position(PhysicalPosition::new(st.x, st.y));
        } else {
            let _ = window.center();
        }
        if st.maximized {
            let _ = window.maximize();
        }
    } else {
        let _ = window.center();
    }
}

fn capture(window: &WebviewWindow, prev: Option<&WindowState>) -> Option<WindowState> {
    if window.is_minimized().unwrap_or(false) {
        return None;
    }
    let maximized = window.is_maximized().unwrap_or(false);
    if maximized {
        // Keep the last non-maximized bounds so un-maximizing later restores cleanly.
        if let Some(p) = prev {
            return Some(WindowState {
                maximized: true,
                ..p.clone()
            });
        }
    }
    let size = window.inner_size().ok()?;
    let pos = window.outer_position().ok()?;
    if size.width < MIN_WIDTH || size.height < MIN_HEIGHT {
        return None;
    }
    Some(WindowState {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
        maximized,
    })
}

/// Persists window bounds on move/resize (debounced) and immediately on close.
pub(crate) fn attach(window: &WebviewWindow, data_dir: PathBuf) {
    let initial = load(&data_dir);
    let last_state: Arc<Mutex<Option<WindowState>>> = Arc::new(Mutex::new(initial));
    let last_saved: Arc<Mutex<Instant>> = Arc::new(Mutex::new(
        Instant::now().checked_sub(SAVE_DEBOUNCE).unwrap_or_else(Instant::now),
    ));
    let win = window.clone();

    window.on_window_event(move |event| match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            let now = Instant::now();
            let should_write = last_saved
                .lock()
                .map(|t| now.duration_since(*t) >= SAVE_DEBOUNCE)
                .unwrap_or(false);
            let prev = last_state.lock().ok().and_then(|g| g.clone());
            if let Some(st) = capture(&win, prev.as_ref()) {
                if let Ok(mut g) = last_state.lock() {
                    *g = Some(st.clone());
                }
                if should_write {
                    if let Ok(mut t) = last_saved.lock() {
                        *t = now;
                    }
                    save(&data_dir, &st);
                }
            }
        }
        WindowEvent::CloseRequested { .. } => {
            let prev = last_state.lock().ok().and_then(|g| g.clone());
            if let Some(st) = capture(&win, prev.as_ref()).or(prev) {
                save(&data_dir, &st);
            }
        }
        _ => {}
    });
}
