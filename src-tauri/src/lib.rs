// Global allocator.
//
// The default (glibc malloc) keeps a per-thread arena and does not return it to
// the OS. Serialising one large result set therefore left roughly 200MB of
// resident arenas behind for the rest of the session, and the process never
// shrank back after you moved on to a smaller table. mimalloc decommits pages it
// no longer needs and is measurably faster on the allocation-heavy row-decode
// and JSON paths, which is most of what a browse does.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod app_lock;
mod cloudflare;
mod commands;
mod connection_store;
mod copilot;
mod db;
mod docker;
mod license;
mod mcp;
mod oauth_page;
mod omniroute;
mod plugins;
mod metrics;
mod proc;
mod providers;
mod secrets;
mod updates;
mod web_search;

use db::{ActiveConnection, DbState, TunnelState};
use mcp::McpState;
use std::sync::{Arc, Mutex};
use tauri::Manager;

// The surface behind the page, shown whenever the webview has yet to composite a
// frame - a cold start, a reload, or any moment the UI is mid-repaint. The
// default is white, which flashes hard against the (mostly dark) app themes, so
// match the `--background` token of the base light/dark theme in
// src/lib/themes/app-themes.css: oklch(0.132 0 0) and oklch(0.975 0 0).
// Only the light/dark end has to be right - the frontend paints the exact
// per-theme background as soon as it has a frame.
const DARK_SURFACE: tauri::window::Color = tauri::window::Color(8, 8, 8, 255);
const LIGHT_SURFACE: tauri::window::Color = tauri::window::Color(247, 247, 247, 255);

pub(crate) fn surface_for_theme(theme: tauri::Theme) -> tauri::window::Color {
    match theme {
        tauri::Theme::Light => LIGHT_SURFACE,
        _ => DARK_SURFACE,
    }
}

/// How long a window may stay hidden before it is shown regardless.
///
/// Windows are built hidden and the frontend shows them on its first finished
/// screen (src/lib/app-reveal.js). The frontend has its own 2.5s failsafe; this
/// one covers the case where no JS runs at all (a broken bundle, a webview that
/// never loads), which would otherwise leave a live process with no window.
const REVEAL_FAILSAFE: std::time::Duration = std::time::Duration::from_secs(4);

/// Whether a window may navigate to `url`: only the app's own pages. A link
/// to anywhere else opens in the browser instead of replacing the app.
pub(crate) fn navigation_allowed(url: &tauri::Url) -> bool {
    if matches!(url.scheme(), "tauri" | "ipc") {
        return true;
    }
    let host = url.host_str().unwrap_or("");
    host == "localhost" || host == "tauri.localhost" || host == "127.0.0.1"
}

pub(crate) fn arm_reveal_failsafe(window: &tauri::WebviewWindow) {
    let window = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(REVEAL_FAILSAFE);
        if !window.is_visible().unwrap_or(true) {
            let _ = window.show();
        }
    });
}

/// Paint the webview's own backdrop, whichever webview this platform uses.
///
/// The window background set by `set_background_color` is behind the webview
/// widget, and the widget fills the window - so what the user actually sees
/// before the page paints is the *webview's* backdrop, which defaults to white
/// on all three engines. The page then boots at `opacity: 0` (index.html) and
/// stays there until the first finished screen, so that white sits on screen
/// for the whole startup, not just a frame.
///
/// Each engine names the knob differently and none of them is reachable through
/// Tauri's own API:
///   - WKWebView (macOS):    `underPageBackgroundColor`
///   - WebView2 (Windows):   `ICoreWebView2Controller2::DefaultBackgroundColor`
///   - WebKitGTK (Linux):    `webkit_web_view_set_background_color`
pub(crate) fn set_webview_backdrop(window: &tauri::WebviewWindow, color: tauri::window::Color) {
    #[cfg(target_os = "macos")]
    set_macos_webview_backdrop(window, color);
    #[cfg(target_os = "windows")]
    set_windows_webview_backdrop(window, color);
    #[cfg(target_os = "linux")]
    set_linux_webview_backdrop(window, color);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    let _ = (window, color);
}

/// WebView2's pre-paint colour.
///
/// `DefaultBackgroundColor` lives on `ICoreWebView2Controller2`, a later
/// revision of the controller Tauri hands back, so the interface is queried for
/// rather than assumed - on a runtime too old to carry it the cast fails and the
/// backdrop stays at the default instead of the call being a hard error.
#[cfg(target_os = "windows")]
fn set_windows_webview_backdrop(window: &tauri::WebviewWindow, color: tauri::window::Color) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Controller2, COREWEBVIEW2_COLOR,
    };
    use windows_core::Interface;

    let tauri::window::Color(r, g, b, a) = color;
    let _ = window.with_webview(move |webview| {
        if let Ok(controller) = webview.controller().cast::<ICoreWebView2Controller2>() {
            // A is the alpha channel of the backdrop itself: a translucent value
            // would let the (white) host window show through again, so it stays
            // fully opaque whatever the caller passed for the window colour.
            let _ = unsafe {
                controller.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                    A: a,
                    R: r,
                    G: g,
                    B: b,
                })
            };
        }
    });
}

/// Keep the webview alive through F6, and the window closable if it dies anyway.
///
/// Only WebView2 needs this. WebKitGTK and WKWebView bind nothing to F6, so
/// there the key goes to the page and nowhere else.
pub(crate) fn guard_webview(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "windows")]
    guard_windows_webview(window);
    #[cfg(not(target_os = "windows"))]
    let _ = window;
}

/// F6 is Chromium's "focus next pane". WebView2 154.0.4258.62 runs it into a
/// null dereference in the browser process (`MultiContentsView::
/// GetActiveContentsContainerView`, under `chrome::FocusNextPane`), and the
/// whole WebView2 process tree exits. The page goes with it, leaving the bare
/// window surface on screen, and with it TitleBar.svelte's close button: Windows
/// gets no native decorations, so the window looked frozen and could not be
/// closed. Same crash, same fix: NeuralNomadsAI/CodeNomad#875 and #881.
///
/// Only the browser's own action is switched off, per key press, through
/// `ICoreWebView2AcceleratorKeyPressedEventArgs2`. F6 still reaches the page
/// (the Terminal tab sends it on to psql and friends) and every other browser
/// accelerator is untouched. A runtime too old to carry that interface
/// swallows F6 instead.
///
/// If the browser process exits for any other reason, the native frame comes
/// back so the window can still be moved and closed.
#[cfg(target_os = "windows")]
fn guard_windows_webview(window: &tauri::WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2AcceleratorKeyPressedEventArgs2, COREWEBVIEW2_KEY_EVENT_KIND,
        COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    };
    use webview2_com::{AcceleratorKeyPressedEventHandler, ProcessFailedEventHandler};
    use windows_core::Interface;

    const VK_F6: u32 = 0x75;

    let host = window.clone();
    let _ = window.with_webview(move |webview| unsafe {
        let controller = webview.controller();
        let mut token = 0;

        let on_key = AcceleratorKeyPressedEventHandler::create(Box::new(|_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
            args.KeyEventKind(&mut kind)?;
            // Alt+F6 arrives as a system key.
            if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
            {
                return Ok(());
            }
            let mut key = 0;
            args.VirtualKey(&mut key)?;
            if key != VK_F6 {
                return Ok(());
            }
            match args.cast::<ICoreWebView2AcceleratorKeyPressedEventArgs2>() {
                Ok(args) => args.SetIsBrowserAcceleratorKeyEnabled(false),
                Err(_) => args.SetHandled(true),
            }
        }));
        if let Err(e) = controller.add_AcceleratorKeyPressed(&on_key, &mut token) {
            log::warn!("webview2: F6 guard not installed: {e}");
        }

        let core = match controller.CoreWebView2() {
            Ok(core) => core,
            Err(e) => {
                log::warn!("webview2: crash guard not installed: {e}");
                return;
            }
        };
        let on_failed = ProcessFailedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
            args.ProcessFailedKind(&mut kind)?;
            if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                log::error!("webview2: browser process exited, restoring the native frame");
                // Through the event loop, not from inside this COM callback.
                let host = host.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = host.set_decorations(true);
                });
            }
            Ok(())
        }));
        if let Err(e) = core.add_ProcessFailed(&on_failed, &mut token) {
            log::warn!("webview2: crash guard not installed: {e}");
        }
    });
}

/// WebKitGTK's pre-paint colour.
#[cfg(target_os = "linux")]
fn set_linux_webview_backdrop(window: &tauri::WebviewWindow, color: tauri::window::Color) {
    use webkit2gtk::WebViewExt;

    let tauri::window::Color(r, g, b, a) = color;
    let _ = window.with_webview(move |webview| {
        webview.inner().set_background_color(&gdk::RGBA::new(
            r as f64 / 255.0,
            g as f64 / 255.0,
            b as f64 / 255.0,
            a as f64 / 255.0,
        ));
    });
}

/// Paint the webview's own backdrop on macOS.
///
/// `WebviewWindow::set_background_color` is documented as "not implemented for
/// the webview layer" on macOS, and it is the WKWebView - not the NSWindow -
/// that owns the white rectangle the user sees before the page paints. The
/// equivalent knob there is `underPageBackgroundColor`.
#[cfg(target_os = "macos")]
fn set_macos_webview_backdrop(window: &tauri::WebviewWindow, color: tauri::window::Color) {
    let _ = window.with_webview(move |webview| unsafe {
        use objc2::{msg_send, runtime::AnyObject, sel};
        use objc2_app_kit::NSColor;
        use objc2_web_kit::WKWebView;

        let view: &WKWebView = &*webview.inner().cast();
        // underPageBackgroundColor is macOS 12+. Tauri still supports older
        // versions, where sending the selector would be a hard crash
        // ("unrecognized selector"), so ask before sending.
        let obj: &AnyObject = &*(view as *const WKWebView).cast();
        let responds: bool = msg_send![obj, respondsToSelector: sel!(setUnderPageBackgroundColor:)];
        if !responds {
            return;
        }
        let tauri::window::Color(r, g, b, a) = color;
        let ns_color = NSColor::colorWithSRGBRed_green_blue_alpha(
            r as f64 / 255.0,
            g as f64 / 255.0,
            b as f64 / 255.0,
            a as f64 / 255.0,
        );
        view.setUnderPageBackgroundColor(Some(&ns_color));
    });
}

/// Let the page render at the display's real refresh rate instead of 60fps.
///
/// WebKit ships a feature flag called `PreferPageRenderingUpdatesNear60FPSEnabled`,
/// default ON, which clamps the whole rendering update - `requestAnimationFrame`,
/// CSS animations, and the compositor commit that follows them - to ~60Hz no
/// matter what the panel can do. On a ProMotion display that is half the frames
/// the hardware is already refreshing at, and it shows up as judder anywhere the
/// main thread is drawing while the scrolling thread moves at the full 120Hz:
/// the canvas grid lags the container it is pinned inside by up to a frame.
///
/// There is no public API for this. It is reachable through WebKit's own feature
/// registry - the same list Safari's Develop > Feature Flags menu drives - via
/// `+[WKPreferences _features]` and `-[WKPreferences _setEnabled:forFeature:]`.
/// Both are underscore SPI, so every selector is checked before it is sent and
/// the whole thing degrades to "stay at 60" rather than trapping on a WebKit
/// version that has renamed or removed the flag.
///
/// Every exit logs what it did. Without that the failure mode is a silent 60fps
/// that looks exactly like a machine whose display is 60Hz, and there is nothing
/// in the app to tell the two apart.
#[cfg(target_os = "macos")]
fn unlock_macos_webview_frame_rate(window: &tauri::WebviewWindow) {
    /// WebKit's key for the 60fps clamp, as it appears in `_features`.
    const FLAG: &str = "PreferPageRenderingUpdatesNear60FPSEnabled";

    let _ = window.with_webview(|webview| unsafe {
        use objc2::runtime::{AnyClass, AnyObject, Bool};
        use objc2::{msg_send, sel};

        let view: *mut AnyObject = webview.inner().cast();
        if view.is_null() {
            log::warn!("fps unclamp: no WKWebView, staying at 60fps");
            return;
        }
        let Some(prefs_cls) = AnyClass::get(c"WKPreferences") else {
            log::warn!("fps unclamp: WKPreferences class missing, staying at 60fps");
            return;
        };
        let has_features: bool = msg_send![prefs_cls, respondsToSelector: sel!(_features)];
        if !has_features {
            log::warn!("fps unclamp: +[WKPreferences _features] gone, staying at 60fps");
            return;
        }

        let config: *mut AnyObject = msg_send![view, configuration];
        if config.is_null() {
            log::warn!("fps unclamp: no WKWebViewConfiguration, staying at 60fps");
            return;
        }
        let prefs: *mut AnyObject = msg_send![config, preferences];
        if prefs.is_null() {
            log::warn!("fps unclamp: no WKPreferences, staying at 60fps");
            return;
        }
        let can_set: bool = msg_send![prefs, respondsToSelector: sel!(_setEnabled:forFeature:)];
        if !can_set {
            log::warn!("fps unclamp: -[WKPreferences _setEnabled:forFeature:] gone, staying at 60fps");
            return;
        }

        // `_features` is every flag WebKit knows, ~600 of them, and the only way
        // to reach one is to find the object whose `key` matches: the setter takes
        // a WKFeature, not a name.
        let features: *mut AnyObject = msg_send![prefs_cls, _features];
        if features.is_null() {
            log::warn!("fps unclamp: _features returned nil, staying at 60fps");
            return;
        }
        let count: usize = msg_send![features, count];
        for i in 0..count {
            let feature: *mut AnyObject = msg_send![features, objectAtIndex: i];
            if feature.is_null() {
                continue;
            }
            let key: *mut AnyObject = msg_send![feature, key];
            if key.is_null() {
                continue;
            }
            // Read the NSString as UTF-8 rather than building one to compare
            // against - this is the only string work in the loop and it runs once.
            let utf8: *const std::ffi::c_char = msg_send![key, UTF8String];
            if utf8.is_null() {
                continue;
            }
            if std::ffi::CStr::from_ptr(utf8).to_bytes() == FLAG.as_bytes() {
                let _: () = msg_send![prefs, _setEnabled: Bool::NO, forFeature: feature];
                log::info!("fps unclamp: {FLAG} off, rendering at the display's rate");
                return;
            }
        }
        // macOS 26 dropped the clamp and the flag with it, so this is the healthy
        // path there - the webview is already at native rate. On 13-15 it means
        // WebKit renamed the key and the clamp is still on.
        log::info!("fps unclamp: {FLAG} not in _features ({count} flags); already unclamped, or renamed");
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Linux WebKitGTK rendering fix - set before any threads spawn.
    #[cfg(target_os = "linux")]
    // SAFETY: called before any threads are spawned.
    unsafe {
        // WEBKIT_DISABLE_DMABUF_RENDERER: WebKitGTK's DMA-buf renderer composites
        // text as GPU textures that get bilinearly sampled at fractional pixel
        // offsets during scroll/zoom, producing the characteristic blur on Linux.
        // Disabling it falls back to a Cairo/FreeType software path that stays crisp.
        // This is the only verified safe WebKitGTK rendering env var - others like
        // WEBKIT_USE_LEGACY_TEXT_RENDERER are not real and can trigger SIGTRAP crashes.
        //
        // The cost is per-frame CPU. rAF still rides the GdkFrameClock, so the
        // cadence the compositor offers is the cadence WebKit asks for - but
        // rasterising in software is expensive enough that a busy frame can miss
        // it, and the miss gets more likely the higher the panel's rate. Default
        // to crisp, and let a high-refresh setup buy frames back with
        // `WEBKIT_DISABLE_DMABUF_RENDERER=0`, which puts the webview on the GPU
        // compositor. The "0" is unset rather than passed through, because WebKit
        // tests some of these vars for presence and not for value.
        match std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").as_deref() {
            Ok("0") | Ok("") => std::env::remove_var("WEBKIT_DISABLE_DMABUF_RENDERER"),
            Ok(_) => {}
            Err(_) => std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
        }
        // GDK_SCALE is intentionally NOT forced here - overriding it breaks HiDPI
        // setups (2× displays) and can cause rendering panics on Wayland compositors.
    }

    // Set a human-readable process title so the app shows as "stroke" in
    // htop / ps / /proc - makes it easy to identify among WebKit helper processes.
    let _ = metrics::set_process_title("stroke".into());
    // Create the shared connection Arc - both DbState and McpState point to the same lock.
    let db_conn: Arc<Mutex<Option<ActiveConnection>>> = Arc::new(Mutex::new(None));
    let db_state = DbState {
        conn: Arc::clone(&db_conn),
        cancels: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    };
    let db_conn_for_setup = Arc::clone(&db_conn);
    let mcp_state = McpState::new(db_conn);

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::AppleScript, None))
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(db_state)
        .manage(mcp_state)
        .manage(TunnelState::new())
        .manage(omniroute::OmniRouteState::new())
        .manage(db::live::LiveState::default())
        .manage(db::tx::TxState::default())
        .manage(db::terminal::TerminalState::default())
        .manage(db::result_store::ResultStore::default())
        .manage(updates::UpdateState::default())
        .setup(move |app| {
            // Load or generate a stable MCP token from the app data directory.
            app.state::<McpState>().init_token(app.handle());
            // Park a handle and the shared connection Arc for the two code
            // paths that need them without a State/AppHandle argument: the D1
            // driver refreshing an expired Cloudflare token mid-session.
            cloudflare::set_app_handle(app.handle().clone());
            if let Ok(dir) = app.path().app_data_dir() {
                db::connection::set_data_dir(dir);
            }
            db::connection::register_active_conn(std::sync::Arc::clone(&db_conn_for_setup));
            updates::clear_stale(app.handle());

            let mut window_builder = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("/".into()),
            )
            .title("Stroke")
            .inner_size(1280.0, 800.0)
            .min_inner_size(960.0, 600.0)
            .resizable(true)
            .maximized(true)
            // Hidden until the page has its first finished screen: revealApp()
            // in src/lib/app-reveal.js shows it. A window visible from build()
            // put every pre-paint layer on screen in turn - the host surface,
            // the webview backdrop, the maximize resize - and on Windows that
            // read as the window flickering between two blacks for half a second
            // before the app appeared. tao keeps MAXIMIZED across the hide, and
            // show() issues SW_SHOW then SW_MAXIMIZE, so the OS still records a
            // real maximized state (minimize -> restore keeps working).
            .visible(false)
            // Never let the default white surface show. The real theme is only
            // known to the frontend (localStorage), so start on the dark base -
            // 11 of the 16 themes are dark - and correct to light right after
            // build(), which still runs before the event loop composites.
            .background_color(DARK_SURFACE);

            // Custom titlebar (TitleBar.svelte) everywhere. macOS keeps the real,
            // OS-drawn traffic lights via the Overlay style - they just float over
            // our content instead of sitting in a native title bar strip, so
            // clicking/hovering/dragging them stays pixel-native with no code on
            // our side. Windows/Linux get no decorations at all; TitleBar.svelte
            // draws its own drag region and minimize/maximize/close buttons.
            #[cfg(target_os = "macos")]
            {
                window_builder = window_builder
                    .title_bar_style(tauri::TitleBarStyle::Overlay)
                    .hidden_title(true);
            }
            #[cfg(not(target_os = "macos"))]
            {
                window_builder = window_builder.decorations(false);
            }

            let window = window_builder
            // devtools(true) enables WebKit's inspector protocol. On Linux with
            // WebKitGTK 2.48+, having the protocol active without a connected
            // DevTools client causes JavaScriptCore to emit SIGTRAP
            // ("NeedDebuggerBreak trap") on any JS exception or font-load race,
            // crashing the process. Auto-enable only in debug builds (dev mode);
            // keep disabled in release. The toggle_devtools command also exposes
            // them on demand via F12.
            .devtools(cfg!(debug_assertions))
            // The app implements its own CSS-based zoom; disable Tauri's injected
            // zoom polyfill. On macOS/Linux that polyfill attaches a `mousewheel`
            // (legacy event) listener that calls set_webview_zoom on ctrl+scroll -
            // a stray trackpad pinch near a column resize handle would then page-zoom
            // the whole webview (devicePixelRatio jumps, canvas renders blurry).
            .zoom_hotkeys_enabled(false)
            .on_navigation(navigation_allowed)
            .build()?;

            // Match the surface to the OS appearance before the first frame. Both
            // calls land inside `setup`, i.e. before the event loop starts, so
            // nothing has been composited yet and there is no flash of the wrong
            // colour. `window.theme()` is the same signal the tray icon uses below.
            let window_theme = window.theme().unwrap_or(tauri::Theme::Dark);
            let surface = surface_for_theme(window_theme);
            let _ = window.set_background_color(Some(surface));
            set_webview_backdrop(&window, surface);
            guard_webview(&window);
            arm_reveal_failsafe(&window);

            #[cfg(target_os = "macos")]
            {
                // Defensive: disable every native WKWebView zoom path. App zoom is
                // CSS-based (--app-zoom); stray pinch near column resize handles
                // must never page-zoom the webview (devicePixelRatio drift → blur).
                let _ = window.with_webview(|webview| unsafe {
                    use objc2_web_kit::WKWebView;
                    let view: &WKWebView = &*webview.inner().cast();
                    view.setAllowsMagnification(false);
                    view.setMagnification(1.0);
                    view.setPageZoom(1.0);
                });

                // Unclamp the render loop from 60fps. Done here, before the page
                // has finished loading, so the first frame the user sees is
                // already running at the display's rate.
                unlock_macos_webview_frame_rate(&window);

                // Install the standard macOS application menu. WKWebView text
                // fields rely on the app menu's Edit items for the standard editing
                // shortcuts - ⌘Z undo, ⌘⇧Z redo, ⌘X/⌘C/⌘V, ⌘A select-all, and
                // ⌥⌫ / ⌘⌫ word/line delete. Install the standard menu so native
                // text editing works everywhere (inputs, textareas, cell editors).
                let menu = tauri::menu::Menu::default(app.handle())?;
                app.set_menu(menu)?;
            }

            // Logging is installed in release too, not just dev. Connection
            // problems get reported from machines we can't attach a debugger to (a
            // user's Windows PC behind a VPN), and the connect path logs which
            // phase was slow - name lookup vs TCP vs TLS+auth. Without a release
            // log there is nothing to go on but "it's slow".
            //
            // The plugin's default targets are already [Stdout, LogDir], so dev
            // gets the terminal and release gets the rotating file in the platform
            // log dir (%LOCALAPPDATA%\<app>\logs on Windows, ~/.local/share/<app>/logs
            // on Linux, ~/Library/Logs/<app> on macOS). Don't add a target here -
            // `target()` appends to that list and every line would be logged twice.
            // Info level keeps this to phase timings and failures, not query traffic.
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .build(),
            )?;
            // No tray and no hide-on-close: closing the last window quits the
            // process. Hiding to the tray left a live instance behind on every
            // close, and each relaunch added another icon next to the old ones.
            let app_handle = app.handle().clone();
            window.on_window_event(move |event| match event {
                // Keep the pre-paint surface on the same end of the scale as the OS.
                tauri::WindowEvent::ThemeChanged(theme) => {
                    if let Some(w) = app_handle.get_webview_window("main") {
                        let surface = surface_for_theme(*theme);
                        let _ = w.set_background_color(Some(surface));
                        set_webview_backdrop(&w, surface);
                    }
                }
                _ => {}
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            plugins::plugins_list,
            plugins::plugins_read_source,
            plugins::plugins_install,
            plugins::plugins_uninstall,
            plugins::plugins_dir,
            commands::ai_fetch,
            commands::ai_fetch_cancel,
            commands::ai_list_models,
            commands::ollama_registry,
            commands::ai_device_id,
            commands::save_file,
            commands::save_file_bytes,
            commands::ai_web_search,
            commands::ai_fetch_page,
            commands::read_file,
            commands::open_new_window,
            commands::reveal_window,
            commands::restart_app,
            updates::update_check,
            updates::update_download,
            updates::update_status,
            updates::update_restart,
            commands::toggle_devtools,
            commands::test_postgres_connection,
            commands::connect_postgres,
            commands::disconnect_postgres,
            commands::live_start,
            commands::live_stop,
            commands::test_sqlite,
            commands::connect_sqlite_db,
            commands::test_mysql,
            commands::connect_mysql_db,
            commands::test_d1,
            commands::connect_d1_db,
            commands::test_libsql,
            commands::connect_libsql_db,
            commands::test_clickhouse,
            commands::connect_clickhouse_db,
            commands::test_posthog,
            commands::connect_posthog_db,
            commands::test_redis,
            commands::connect_redis_db,
            commands::redis_scan,
            commands::test_duckdb,
            commands::connect_duckdb_db,
            commands::test_mssql,
            commands::connect_mssql_db,
            commands::pg_list_schemas,
            commands::pg_list_tables,
            commands::pg_table_row_counts,
            commands::pg_list_indexes,
            commands::pg_get_table_column_structure,
            commands::pg_get_schema_column_structure,
            commands::pg_get_incoming_foreign_keys,
            commands::pg_list_enums,
            commands::pg_list_functions,
            commands::pg_list_triggers,
            commands::pg_list_sequences,
            commands::list_db_objects,
            commands::get_object_definition,
            commands::drop_db_object,
            commands::list_object_comments,
            commands::ping_db_connection,
            commands::pg_truncate_table,
            commands::pg_drop_table,
            commands::get_table_ddl,
            commands::get_table_ddl_on_connection,
            commands::pg_get_table_rows,
            commands::pg_count_table_rows,
            commands::pg_get_column_stats,
            commands::geo_overview,
            commands::geo_features,
            commands::instance_version,
            commands::instance_activity,
            commands::instance_state,
            commands::instance_config,
            commands::instance_set_config,
            commands::instance_replication,
            commands::pg_execute_sql,
            commands::pg_execute_sql_multi,
            commands::pg_execute_sql_stream,
            commands::result_window,
            commands::result_sort,
            commands::result_drop,
            commands::perf_log,
            commands::pg_explain_sql,
            commands::execute_sql_on_connection,
            commands::list_schemas_on_connection,
            commands::list_tables_on_connection,
            commands::pg_execute_ddl,
            commands::pg_clone_database,
            commands::pg_update_table_cell,
            commands::pg_fetch_cell_value,
            commands::pg_delete_table_row,
            commands::pg_delete_table_rows,
            commands::pg_insert_table_row,
            mcp::mcp_start,
            mcp::mcp_stop,
            mcp::mcp_status,
            mcp::mcp_update_connections,
            mcp::mcp_set_readonly,
            db::connection::prewarm_dns,
            omniroute::omniroute_env,
            omniroute::omniroute_install,
            omniroute::omniroute_start,
            omniroute::omniroute_stop,
            omniroute::omniroute_running,
            docker::docker_check,
            docker::docker_run_db,
            docker::scan_docker_databases,
            docker::docker_container_action,
            db::local_scan::scan_local_studios,
            db::local_scan::scan_machine_databases,
            app_lock::app_lock_status,
            app_lock::app_lock_set_pin,
            app_lock::app_lock_verify,
            app_lock::app_lock_disable,
            app_lock::app_lock_set_prefs,
            secrets::ai_store_key,
            secrets::ai_load_key,
            secrets::ai_delete_key,
            copilot::copilot_start_device_flow,
            copilot::copilot_poll_oauth_token,
            copilot::copilot_get_copilot_token,
            copilot::copilot_fetch_models,
            cloudflare::cloudflare_start_oauth,
            cloudflare::cloudflare_oauth_status,
            cloudflare::cloudflare_get_valid_token,
            cloudflare::cloudflare_logout,
            cloudflare::cloudflare_list_accounts,
            cloudflare::cloudflare_list_d1_databases,
            providers::provider_start_oauth,
            providers::provider_cancel_oauth,
            providers::provider_store_token,
            providers::provider_oauth_status,
            providers::provider_logout,
            providers::provider_warm,
            providers::provider_list_databases,
            providers::provider_build_connection,
            db::backup::backup_export,
            db::backup::backup_import,
            db::backup::backup_cancel,
            db::import::import_rows,
            db::import::import_cancel,
            db::tx::tx_begin,
            db::tx::tx_execute,
            db::tx::tx_commit,
            db::tx::tx_rollback,
            db::tx::tx_status,
            db::terminal::terminal_client,
            db::terminal::terminal_open,
            db::terminal::terminal_close,
            commands::check_license_status,
            commands::activate_license,
            commands::deactivate_license,
            commands::run_license_check,
            commands::init_sample_db,
            commands::connections_store_read,
            commands::connections_store_write,
            metrics::get_app_metrics,
            metrics::set_process_title,
            commands::enable_autostart,
            commands::disable_autostart,
            commands::get_autostart_status,
            commands::advisor_scan,
            commands::cancel_query,
            #[cfg(debug_assertions)]
            commands::debug_set_trial_days_ago,
            #[cfg(debug_assertions)]
            commands::debug_reset_trial,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested {
                code: Some(tauri::RESTART_EXIT_CODE),
                ..
            } => updates::note_restart(app),
            // The last window closed, or the OS is shutting down. Reap the
            // OmniRoute proxy we spawned, or it survives every app quit.
            tauri::RunEvent::Exit => {
                app.state::<omniroute::OmniRouteState>().kill_now();
                // And every terminal tab's client, which would otherwise keep its
                // server connection (and any open transaction) after the app.
                app.state::<db::terminal::TerminalState>().kill_all();
                // Last: on Windows a successful install ends the process here.
                updates::apply_on_quit(app);
            }
            _ => {}
        });
}
