/**
 * Title-bar double-click, done the native way on each OS.
 *
 * Tauri's drag-region script toggles maximize on every platform. On macOS that
 * ignored System Settings > Desktop & Dock > "Double-click a window's title bar
 * to" (Zoom, Fill, Minimize, Do Nothing): a Mac set to minimize still got the
 * zoom animation. `title_bar_double_click` (commands.rs) reads that choice and
 * runs AppKit's own zoom or minimize; on Windows and Linux it toggles maximize.
 */
import { invoke } from '@tauri-apps/api/core'
import { detectOs } from '$lib/platform.js'

const CLICKABLE = 'a, button, input, select, textarea, label, summary, [contenteditable]:not([contenteditable="false"]), [role="button"], [role="link"], [role="menuitem"], [role="tab"], [role="checkbox"], [role="radio"], [role="switch"], [role="option"]'

/** The same rule as Tauri's drag.js: a drag region with no control in between. @param {EventTarget[]} path */
export function inDragRegion(path) {
  for (const node of path) {
    if (!(node instanceof HTMLElement)) continue
    const attr = node.getAttribute('data-tauri-drag-region')
    if (attr === null && node.matches(CLICKABLE)) return false
    if (attr === null) continue
    if (attr === 'false') return false
    if (attr === 'deep') return true
    return node === path[0]
  }
  return false
}

/** Run the OS's title-bar double-click action on this window. */
export function titleBarDoubleClick() {
  return invoke('title_bar_double_click').catch(() => {})
}

let installed = false

/**
 * macOS only: take the drag region's double-click before Tauri's script sees
 * it. Like AppKit, it fires on the second mouseup, and not when the pointer
 * moved after the second press (that is the start of a drag).
 */
export function installTitleBarDoubleClick() {
  if (installed || typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) return
  if (detectOs() !== 'macos') return
  installed = true
  let downX = 0
  let downY = 0
  window.addEventListener('mousedown', (e) => {
    if (e.button === 0 && e.detail === 2) {
      downX = e.clientX
      downY = e.clientY
    }
  }, true)
  window.addEventListener('mouseup', (e) => {
    if (e.button !== 0 || e.detail !== 2) return
    if (!inDragRegion(e.composedPath())) return
    // Capture on window runs before Tauri's document listener: stop it zooming too.
    e.stopImmediatePropagation()
    if (e.clientX !== downX || e.clientY !== downY) return
    void titleBarDoubleClick()
  }, true)
}
