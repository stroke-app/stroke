import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'
import { applySettings, installZoomShortcuts, loadSettings, resetWebviewZoom } from '$lib/stores/settings.js'
import { armRevealFailsafe } from '$lib/app-reveal.js'
import { hydrateConnectionsFromDisk } from '$lib/stores/connections.js'

// Clear all local storage when VITE_FRESH_START=1 (used by `npm run tauri:fresh`)
if (import.meta.env.VITE_FRESH_START === '1') {
  try {
    localStorage.clear()
    sessionStorage.clear()
    if (window.indexedDB?.databases) {
      window.indexedDB.databases().then((dbs) => {
        for (const db of dbs) if (db.name) window.indexedDB.deleteDatabase(db.name)
      })
    }
  } catch (_) {}
}

// First, before anything that can throw. Everything below it is startup work
// that the page does not need in order to be visible, and a throw in any of it
// used to leave the failsafe unarmed and the window black.
armRevealFailsafe()
applySettings(loadSettings())
installZoomShortcuts()
resetWebviewZoom()

// ── Zoom watchdog ────────────────────────────────────────────────────────────
// Final safety net against any stray webview zoom that slips past the event
// blockers in settings.js. Two distinct symptoms, both reset to 1:
//   • visualViewport.scale > 1  → WKWebView pinch magnification
//   • devicePixelRatio drift    → page zoom (Tauri's set_webview_zoom polyfill);
//                                 this is the one that made the canvas look
//                                 blurrily magnified near resize handles.
// The app's own zoom is CSS-based (--app-zoom) and never touches devicePixelRatio,
// so any drift here is unwanted. After a reset we re-baseline so a genuine
// monitor/DPI change (e.g. moving to another display) isn't fought.
if (window.visualViewport) {
  const vv = window.visualViewport
  let baselineDpr = window.devicePixelRatio
  let resetting = false

  const enforceNoZoom = () => {
    if (resetting) return
    const scaled = vv.scale !== 1
    const dprDrift = Math.abs(window.devicePixelRatio - baselineDpr) > 0.001
    if (!scaled && !dprDrift) return
    resetting = true
    resetWebviewZoom()
    requestAnimationFrame(() => {
      baselineDpr = window.devicePixelRatio
      resetting = false
    })
  }

  vv.addEventListener('resize', enforceNoZoom)
  window.addEventListener('resize', enforceNoZoom)
}

// Quick console probe: run `__zoomDiag()` in DevTools *while the canvas looks
// magnified*. scale > 1 ⇒ webview pinch zoom. scale == 1 with a raised
// devicePixelRatio ⇒ page zoom. Both are now blocked + auto-reset.
window.__zoomDiag = () =>
  console.log('[zoomDiag]', {
    visualViewportScale: window.visualViewport?.scale,
    devicePixelRatio: window.devicePixelRatio,
  })

// ── Native-app feel ────────────────────────────────────────────────────────
// Disable browser right-click context menu everywhere.
document.addEventListener('contextmenu', (e) => e.preventDefault())

// Open external links (http/https/mailto) in the system browser via Tauri.
// capture:true ensures this fires before any element-level handlers.
document.addEventListener('click', async (e) => {
  const anchor = /** @type {Element | null} */ (e.target)?.closest('a')
  if (!anchor) return
  const href = /** @type {HTMLAnchorElement} */ (anchor).href
  if (!href) return
  const isExternal =
    href.startsWith('http://') || href.startsWith('https://') || href.startsWith('mailto:')
  if (!isExternal) return
  e.preventDefault()
  e.stopImmediatePropagation()
  try {
    const { openUrl: open } = await import('@tauri-apps/plugin-opener')
    await open(href)
  } catch {
    // ignore in dev/browser environments without Tauri
  }
}, { capture: true })

// Saved connections are loaded from their on-disk copy before the app reads
// them (see hydrateConnectionsFromDisk). A fresh-start run skips it, or the
// file would put back everything it just cleared.
if (import.meta.env.VITE_FRESH_START !== '1') await hydrateConnectionsFromDisk()

const app = mount(App, {
  target: document.getElementById('app'),
})

export default app
