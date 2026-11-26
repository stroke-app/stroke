// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
  // First thing in the process, before Tauri, GTK or WebKit read their
  // environment and before any thread exists (set_var is only sound then).
  //
  // WebKitGTK's DMA-buf renderer kills the app on some Wayland + NVIDIA
  // setups: "Gdk-Message: Error 71 (Protocol error) dispatching to Wayland
  // display" and exit 1 before a window shows. 3.7.5 set this unconditionally;
  // 3.8.0 (#415) only set it after a failed launch, and the failure-detection
  // marker is cleared before the window is shown, so it never kicked in.
  //
  // A value the user set wins. "0" or empty opts into the GPU renderer, and is
  // removed rather than passed through because WebKit checks some of these
  // variables for presence, not value.
  #[cfg(target_os = "linux")]
  match std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER") {
    Some(v) if v.is_empty() || v == "0" => std::env::remove_var("WEBKIT_DISABLE_DMABUF_RENDERER"),
    Some(_) => {}
    None => std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
  }

  app_lib::run();
}
