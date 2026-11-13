//! macOS window chrome: the native application menu and the traffic-light inset.
//!
//! On macOS the window keeps the OS-drawn traffic lights (Overlay title bar,
//! hidden title) and the in-window File/Edit/View/Tools/Help bar is replaced by
//! the real menu bar at the top of the screen. Every app item here mirrors an
//! entry of `AppMenuBar.svelte`: it carries the same action name, and a click
//! is emitted to the focused window as a `native-menu` event, where
//! `src/lib/native-menu.js` runs the same `menuActions[name]` the in-window bar
//! runs. The menu owns no behaviour of its own.
//!
//! Accelerators are listed so the menu shows the shortcuts, but the keyboard
//! stays owned by the page's hotkeys: WKWebView offers a key equivalent to the
//! page first and only falls back to the menu when the page did not
//! `preventDefault()` it. `native-menu.js` drops those fall-throughs, so a
//! context-guarded hotkey (⌘S with nothing staged, ⌘⇧T in the AI chat) keeps
//! doing exactly what it did before the menu existed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Listener, LogicalPosition, Manager, Runtime};

/// Prefix of every app-owned menu id; the rest is the frontend action name.
const ACTION_PREFIX: &str = "stroke:";

/// Event the frontend listens for (payload: the action name).
const MENU_EVENT: &str = "native-menu";

/// Event the frontend emits with `{ connected: bool }` so items that need a
/// database grey out the way they do in the in-window bar.
const STATE_EVENT: &str = "native-menu-state";

/// Event the frontend emits with `{ action: accelerator | null }` whenever the
/// user remaps a shortcut, so the menu shows (and owns) the keys the page binds.
const ACCEL_EVENT: &str = "native-menu-accelerators";

/// Where the traffic lights sit. tao grows the title-bar container to
/// `button height + y` and leaves the buttons at its foot, so their centre
/// lands `y` points from the top. The title row (TitleBar.svelte) is 38px tall,
/// so 21 (measured: the buttons sit ~2pt above `y`) puts them on the icons' centre line beside
/// them (19 and 17 left them high). Shared by the main window and
/// `open_new_window`.
pub(crate) fn traffic_light_position() -> LogicalPosition<f64> {
    LogicalPosition::new(16.0, 28.0)
}

/// One app item: (action, label, accelerator, needs a connection).
type Item = (&'static str, &'static str, Option<&'static str>, bool);

const FILE_ITEMS: &[&[Item]] = &[
    &[
        ("newTab", "New Tab", Some("CmdOrCtrl+Shift+T"), false),
        ("newSql", "New Query Editor", None, true),
        ("newWindow", "New Window", Some("CmdOrCtrl+Shift+N"), false),
    ],
    &[
        ("openConnection", "Connect…", None, false),
        ("disconnect", "Disconnect", Some("CmdOrCtrl+Shift+D"), true),
    ],
    &[
        ("closeTab", "Close Tab", Some("CmdOrCtrl+W"), false),
        ("closeAllTabs", "Close All Tabs", Some("CmdOrCtrl+Shift+W"), false),
    ],
];

const EDIT_APP_ITEMS: &[&[Item]] = &[
    // Select All is the app's, not AppKit's: the predefined item swallowed ⌘A
    // before the page saw it, and its `selectAll:` never reached the SQL editor
    // or the grid's own select-all (native-menu.js → menuActions.selectAll).
    &[("selectAll", "Select All", Some("CmdOrCtrl+A"), false)],
    &[
        ("search", "Search Rows", Some("CmdOrCtrl+F"), true),
        ("findReplace", "Find & Replace…", Some("CmdOrCtrl+Alt+F"), true),
        ("findInDatabase", "Find in Database…", None, true),
    ],
    &[
        ("applyEdits", "Apply Staged Changes", Some("CmdOrCtrl+S"), true),
        ("copyEditsSql", "Copy Staged Changes as SQL", Some("CmdOrCtrl+Alt+S"), true),
        ("resetEdits", "Discard Staged Changes", None, true),
    ],
];

const VIEW_ITEMS: &[&[Item]] = &[
    &[
        ("toggleSidebar", "Toggle Sidebar", Some("CmdOrCtrl+B"), false),
        ("toggleChat", "Toggle Chat", Some("CmdOrCtrl+I"), false),
    ],
    &[
        ("zoomIn", "Zoom In", Some("CmdOrCtrl+="), false),
        ("zoomOut", "Zoom Out", Some("CmdOrCtrl+-"), false),
        ("zoomReset", "Reset Zoom", Some("CmdOrCtrl+0"), false),
    ],
    &[
        ("fullscreen", "Toggle Full Screen", Some("F11"), false),
        ("commandPalette", "Command Palette…", Some("CmdOrCtrl+K"), false),
    ],
];

const TOOLS_ITEMS: &[&[Item]] = &[
    &[
        ("objects", "Database Objects", None, true),
        ("insights", "Instance Insights", None, true),
        ("schema", "Schema Explorer", None, true),
        ("erd", "Schema Diagram", None, true),
    ],
    &[
        ("dataDiff", "Data Diff", None, true),
        ("dashboard", "Dashboard", None, true),
        ("logs", "Activity Log", Some("CmdOrCtrl+Shift+L"), true),
    ],
    &[("extensions", "Extensions", Some("CmdOrCtrl+Shift+X"), false)],
];

const HELP_ITEMS: &[&[Item]] = &[
    &[
        ("shortcuts", "Keyboard Shortcuts", Some("CmdOrCtrl+/"), false),
        ("changelog", "What's New", None, false),
    ],
    &[("reportIssue", "Report an Issue…", None, false)],
];

/// Builds the app items of `groups` into `submenu`, separators between groups,
/// remembering the ones that need a connection.
fn append_groups<R: Runtime>(
    app: &AppHandle<R>,
    submenu: &Submenu<R>,
    groups: &[&[Item]],
    gated: &mut Vec<MenuItem<R>>,
    by_action: &mut HashMap<String, MenuItem<R>>,
) -> tauri::Result<()> {
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            submenu.append(&PredefinedMenuItem::separator(app)?)?;
        }
        for &(action, label, accel, needs_conn) in group.iter() {
            let item = app_item(app, action, label, accel)?;
            by_action.insert(action.to_string(), item.clone());
            if needs_conn {
                // Same starting state as the in-window bar before a connection.
                item.set_enabled(false)?;
                gated.push(item.clone());
            }
            submenu.append(&item)?;
        }
    }
    Ok(())
}

fn app_item<R: Runtime>(
    app: &AppHandle<R>,
    action: &str,
    label: &str,
    accel: Option<&str>,
) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(app, format!("{ACTION_PREFIX}{action}"), label, true, accel)
}

type Built<R> = (Menu<R>, Vec<MenuItem<R>>, HashMap<String, MenuItem<R>>);

fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Built<R>> {
    let mut gated = Vec::new();
    let mut by_action = HashMap::new();
    let name = app.package_info().name.clone();

    // Application menu. About, Check for Updates and Settings live here on
    // macOS (they are under Help / File in the in-window bar).
    let app_menu = Submenu::new(app, &name, true)?;
    app_menu.append(&app_item(app, "about", &format!("About {name}"), None)?)?;
    app_menu.append(&app_item(app, "checkUpdates", "Check for Updates…", None)?)?;
    app_menu.append(&PredefinedMenuItem::separator(app)?)?;
    let settings = app_item(app, "openSettings", "Settings…", Some("CmdOrCtrl+,"))?;
    by_action.insert("openSettings".to_string(), settings.clone());
    app_menu.append(&settings)?;
    app_menu.append(&PredefinedMenuItem::separator(app)?)?;
    app_menu.append(&PredefinedMenuItem::services(app, None)?)?;
    app_menu.append(&PredefinedMenuItem::separator(app)?)?;
    app_menu.append(&PredefinedMenuItem::hide(app, None)?)?;
    app_menu.append(&PredefinedMenuItem::hide_others(app, None)?)?;
    app_menu.append(&PredefinedMenuItem::show_all(app, None)?)?;
    app_menu.append(&PredefinedMenuItem::separator(app)?)?;
    app_menu.append(&PredefinedMenuItem::quit(app, None)?)?;

    let file = Submenu::new(app, "File", true)?;
    append_groups(app, &file, FILE_ITEMS, &mut gated, &mut by_action)?;

    // Edit keeps the standard text-editing items first: WKWebView inputs rely
    // on them for ⌘Z / ⌘⇧Z / ⌘X / ⌘C / ⌘V / ⌘A.
    let edit = Submenu::new(app, "Edit", true)?;
    edit.append(&PredefinedMenuItem::undo(app, None)?)?;
    edit.append(&PredefinedMenuItem::redo(app, None)?)?;
    edit.append(&PredefinedMenuItem::separator(app)?)?;
    edit.append(&PredefinedMenuItem::cut(app, None)?)?;
    edit.append(&PredefinedMenuItem::copy(app, None)?)?;
    edit.append(&PredefinedMenuItem::paste(app, None)?)?;
    edit.append(&PredefinedMenuItem::separator(app)?)?;
    append_groups(app, &edit, EDIT_APP_ITEMS, &mut gated, &mut by_action)?;

    let view = Submenu::new(app, "View", true)?;
    append_groups(app, &view, VIEW_ITEMS, &mut gated, &mut by_action)?;

    let tools = Submenu::new(app, "Tools", true)?;
    append_groups(app, &tools, TOOLS_ITEMS, &mut gated, &mut by_action)?;

    // Window: the system items. Close Window keeps its ⌘W, but File ▸ Close Tab
    // comes first in the bar and so owns the key; native-menu.js closes the
    // window from it when there is nothing to close inside.
    let window = Submenu::new(app, "Window", true)?;
    window.append(&PredefinedMenuItem::minimize(app, None)?)?;
    window.append(&PredefinedMenuItem::maximize(app, None)?)?;
    window.append(&PredefinedMenuItem::separator(app)?)?;
    window.append(&PredefinedMenuItem::close_window(app, None)?)?;

    let help = Submenu::new(app, "Help", true)?;
    append_groups(app, &help, HELP_ITEMS, &mut gated, &mut by_action)?;

    let menu = Menu::with_items(app, &[&app_menu, &file, &edit, &view, &tools, &window, &help])?;
    Ok((menu, gated, by_action))
}

/// The window a menu click is for: the focused one, else `main`.
fn target_window<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    let windows = app.webview_windows();
    windows
        .iter()
        .find(|(_, w)| w.is_focused().unwrap_or(false))
        .map(|(label, _)| label.clone())
        .or_else(|| windows.contains_key("main").then(|| "main".to_string()))
}

/// Installs the native menu and wires it to the frontend.
pub(crate) fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let (menu, gated, by_action) = build(app)?;
    app.set_menu(menu)?;

    app.on_menu_event(|app, event| {
        // DEBUG-SELECTALL (temporary)
        log::info!("DEBUG-SELECTALL native menu event id={} target={:?}", event.id().as_ref(), target_window(app));
        let Some(action) = event.id().as_ref().strip_prefix(ACTION_PREFIX) else {
            return;
        };
        if let Some(label) = target_window(app) {
            let _ = app.emit_to(
                tauri::EventTarget::webview_window(label),
                MENU_EVENT,
                action.to_string(),
            );
        }
    });

    let gated = Arc::new(Mutex::new(gated));
    app.listen_any(STATE_EVENT, move |event| {
        let connected = serde_json::from_str::<serde_json::Value>(event.payload())
            .ok()
            .and_then(|v| v.get("connected").and_then(|c| c.as_bool()))
            .unwrap_or(false);
        if let Ok(items) = gated.lock() {
            for item in items.iter() {
                let _ = item.set_enabled(connected);
            }
        }
    });

    let by_action = Arc::new(Mutex::new(by_action));
    app.listen_any(ACCEL_EVENT, move |event| {
        let Ok(map) =
            serde_json::from_str::<HashMap<String, Option<String>>>(event.payload())
        else {
            return;
        };
        let Ok(items) = by_action.lock() else { return };
        for (action, accel) in map {
            let Some(item) = items.get(&action) else { continue };
            // A combo the menu cannot express shows no key rather than a stale one.
            if item.set_accelerator(accel.as_deref()).is_err() {
                let _ = item.set_accelerator(None::<&str>);
            }
        }
    });
    Ok(())
}
