/**
 * Bridge between the native macOS menu bar (src-tauri/src/mac_chrome.rs) and the
 * same named actions the in-window AppMenuBar runs.
 *
 * The native items list their shortcuts, but the page's hotkeys stay in charge
 * of the keyboard: WKWebView hands a key equivalent to the page first and only
 * falls through to the menu when nothing called preventDefault(). A fall-through
 * means a hotkey chose not to act in this context (⌘S with nothing staged, ⌘⇧T
 * inside the AI chat), so it is dropped here - the menu must not add behaviour
 * the keyboard did not have. Clicks always run.
 */

import { keybindings, nativeMenuAccelerators } from '$lib/stores/keybindings.js'

/** A menu event this soon after a modified keydown came from that keydown. */
const KEY_FALLTHROUGH_MS = 400

/**
 * @param {() => Record<string, () => void>} getActions
 * @param {() => boolean} isConnected
 * @returns {() => void} cleanup
 */
export function bindNativeMenu(getActions, isConnected) {
  let lastShortcutKeyAt = -Infinity
  /** @param {KeyboardEvent} e */
  const onKeyDown = (e) => {
    if (e.metaKey || e.ctrlKey || e.key === 'F11') lastShortcutKeyAt = performance.now()
  }
  window.addEventListener('keydown', onKeyDown, true)

  let disposed = false
  /** @type {(() => void) | null} */
  let unlisten = null
  void (async () => {
    try {
      const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow')
      const win = getCurrentWebviewWindow()
      const off = await win.listen('native-menu', (event) => {
        const action = String(event.payload ?? '')
        // DEBUG-SELECTALL (temporary)
        void import('@tauri-apps/api/core').then(({ invoke }) => invoke('perf_log', { line: `DEBUG-SELECTALL menu action=${action} active=${document.activeElement?.tagName} has=${typeof getActions()[action]}` })).catch(() => {})
        const fromKeyboard = performance.now() - lastShortcutKeyAt < KEY_FALLTHROUGH_MS
        // ⌘A reaches the menu only when no page handler took the key, and the
        // webview has no select-all of its own without the stock item, so run it.
        if (fromKeyboard && action !== 'selectAll') {
          // ⌘W with nothing to close inside closed the window before the menu
          // owned the key (the stock Close Window item); keep that.
          if (action === 'closeTab' && !isConnected()) void win.close()
          return
        }
        getActions()[action]?.()
      })
      if (disposed) off()
      else unlisten = off
    } catch {
      /* not running under Tauri */
    }
  })()

  // The menu shows the user's keys, not the defaults it was built with. This
  // matters beyond the label: a key the page no longer handles falls through to
  // the menu, so a stale ⌘W accelerator would still close the window.
  const unsubscribeKeys = keybindings.subscribe((overrides) => {
    void syncNativeMenuAccelerators(nativeMenuAccelerators(overrides))
  })

  return () => {
    disposed = true
    window.removeEventListener('keydown', onKeyDown, true)
    unsubscribeKeys()
    unlisten?.()
  }
}

/**
 * Hand the native menu its accelerators, `{ action: 'CmdOrCtrl+W' | null }`.
 * @param {Record<string, string | null>} accelerators
 */
async function syncNativeMenuAccelerators(accelerators) {
  try {
    const { emit } = await import('@tauri-apps/api/event')
    await emit('native-menu-accelerators', accelerators)
  } catch {
    /* not running under Tauri */
  }
}

/**
 * Tell the native menu whether a database is connected, so connection-only
 * items grey out like they do in the in-window bar.
 * @param {boolean} connected
 */
export async function syncNativeMenuState(connected) {
  try {
    const { emit } = await import('@tauri-apps/api/event')
    await emit('native-menu-state', { connected })
  } catch {
    /* not running under Tauri */
  }
}

/**
 * Track whether the window is in native full screen, where macOS hides the
 * traffic lights and the title row should not reserve room for them.
 * @param {(fullscreen: boolean) => void} onChange
 * @returns {() => void} cleanup
 */
export function watchFullscreen(onChange) {
  let disposed = false
  /** @type {(() => void) | null} */
  let unlisten = null
  void (async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      const win = getCurrentWindow()
      const check = async () => {
        try { if (!disposed) onChange(await win.isFullscreen()) } catch { /* window gone */ }
      }
      await check()
      const off = await win.onResized(() => void check())
      if (disposed) off()
      else unlisten = off
    } catch {
      /* not running under Tauri */
    }
  })()
  return () => {
    disposed = true
    unlisten?.()
  }
}
