// The page boots at opacity 0 (index.html) and is faded in exactly once, by the
// first screen that is actually complete: the shell after its startup decision
// (reconnect overlay, welcome, onboarding), the lock screen, the trial screen,
// or the crash screen. Revealing earlier - on App mount - faded in an empty page
// while AppLockGate was still reading the keychain, and the shell then popped in
// unfaded on top of it.
//
// The native window is built hidden too (src-tauri/src/lib.rs) and is shown
// here, so nothing that paints before the page (host surface, webview backdrop,
// the maximize resize) ever reaches the screen.

const FAILSAFE_MS = 2500

let revealed = false

/** Show the native window. No-op outside Tauri (browser dev). */
async function showWindow() {
  try {
    // A glass theme turns the window translucent through an async call; show
    // the window only once that has landed, or it opens solid and flips.
    const { windowGlassReady } = await import('$lib/stores/settings.js')
    await windowGlassReady()
  } catch { /* settings unavailable: show anyway */ }
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('reveal_window')
  } catch { /* browser dev, or the window is already up */ }
}

/** Has the page been revealed yet? Lets a screen skip an exit animation that
 *  would otherwise play, unseen, while the window is still hidden. */
export function isRevealed() {
  return revealed
}

/** Fade the page in. Idempotent - only the first caller does anything. */
export function revealApp() {
  if (revealed || typeof document === 'undefined') return
  revealed = true
  // Mark first, and synchronously. This used to wait on the show IPC to come
  // back and then on a requestAnimationFrame - a frame a hidden window never
  // produces, so when the show did not land the page stayed hidden and what
  // finally uncovered it was a timer, seconds later. Nothing here waits now:
  // the attribute is set, the CSS swaps the splash for the app, and the window
  // is asked to show itself afterwards. Marking before the window appears is
  // the right order anyway - the first thing on screen is the finished app
  // rather than a page fading in.
  document.documentElement.dataset.revealed = ''
  void showWindow()
}

/** Never leave a blank window: reveal regardless if no screen claimed it. */
export function armRevealFailsafe() {
  setTimeout(revealApp, FAILSAFE_MS)
}
