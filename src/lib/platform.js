/** @returns {'windows' | 'macos' | 'linux'} */
export function detectOs() {
  if (typeof navigator === 'undefined') return 'linux'

  const ua = navigator.userAgent ?? ''
  const platform = navigator.userAgentData?.platform ?? navigator.platform ?? ''

  if (/Win/i.test(platform) || /Windows/i.test(ua)) return 'windows'
  if (/Mac/i.test(platform) || /Macintosh|Mac OS/i.test(ua)) return 'macos'
  return 'linux'
}

/** Sets `data-os` on `<html>` for OS-specific CSS (e.g. Windows scrollbars). */
export function installPlatformClass() {
  if (typeof document === 'undefined') return
  document.documentElement.dataset.os = detectOs()
}

/**
 * Syncs native window focus, maximized, and fullscreen state onto `<html>`
 * (`data-window-focused`, `data-maximized`, `data-fullscreen`) so chrome dims
 * subtly when unfocused and adapts borders when maximized/windowed.
 * @returns {() => void} cleanup
 */
export function installWindowStateListeners() {
  if (typeof window === 'undefined' || typeof document === 'undefined') return () => {}
  const root = document.documentElement

  const setFocused = (/** @type {boolean} */ focused) => {
    root.dataset.windowFocused = focused ? 'true' : 'false'
  }
  setFocused(document.hasFocus())

  const onFocus = () => setFocused(true)
  const onBlur = () => setFocused(false)
  window.addEventListener('focus', onFocus)
  window.addEventListener('blur', onBlur)

  let disposed = false
  /** @type {(() => void)[]} */
  const unsubs = []

  if ('__TAURI_INTERNALS__' in window) {
    void (async () => {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window')
        const win = getCurrentWindow()
        const syncLayout = async () => {
          try {
            if (disposed) return
            const [max, fs] = await Promise.all([win.isMaximized(), win.isFullscreen()])
            if (disposed) return
            root.dataset.maximized = max ? 'true' : 'false'
            root.dataset.fullscreen = fs ? 'true' : 'false'
          } catch { /* window gone */ }
        }
        try {
          const focused = await win.isFocused()
          if (!disposed) setFocused(focused)
        } catch { /* fallback to document.hasFocus */ }
        await syncLayout()
        const offFocus = await win.onFocusChanged(({ payload }) => {
          if (!disposed) setFocused(!!payload)
        })
        const offResize = await win.onResized(() => void syncLayout())
        if (disposed) {
          offFocus()
          offResize()
        } else {
          unsubs.push(offFocus, offResize)
        }
      } catch { /* not running under Tauri */ }
    })()
  }

  return () => {
    disposed = true
    window.removeEventListener('focus', onFocus)
    window.removeEventListener('blur', onBlur)
    for (const off of unsubs) off()
  }
}
