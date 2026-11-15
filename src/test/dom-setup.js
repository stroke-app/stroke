// what the webview has and jsdom lacks, so components mount as in the app.
// Tauri's bridge answers every command with nothing; components treat that as
// an empty account / list, which is all a smoke test needs.
globalThis.__TAURI_INTERNALS__ = {
  invoke: async () => null,
  transformCallback: () => 0,
  metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
}
if (!globalThis.matchMedia) {
  globalThis.matchMedia = (q) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {}, onchange: null, dispatchEvent: () => false })
}
if (!globalThis.ResizeObserver) {
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
}
if (!globalThis.IntersectionObserver) {
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} takeRecords() { return [] } }
}
if (!Element.prototype.scrollIntoView) Element.prototype.scrollIntoView = () => {}
if (!globalThis.requestIdleCallback) globalThis.requestIdleCallback = (cb) => setTimeout(cb, 1)
if (!Element.prototype.animate) {
  Element.prototype.animate = function () {
    const a = { finished: Promise.resolve(), cancel() {}, finish() {}, play() {}, pause() {}, reverse() {}, onfinish: null, oncancel: null, currentTime: 0, playState: 'finished', addEventListener() {}, removeEventListener() {} }
    queueMicrotask(() => { if (typeof a.onfinish === 'function') a.onfinish() })
    return a
  }
}
if (!Element.prototype.getAnimations) Element.prototype.getAnimations = () => []
