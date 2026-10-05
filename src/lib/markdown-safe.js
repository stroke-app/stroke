/**
 * What the assistant's markdown may put into the app's DOM.
 *
 * The reply is rendered with {@html}, inside the app window, so raw HTML in it
 * used to land as real elements: a `<style>` block restyled the whole app, an
 * inline `style="position:fixed;inset:0"` or a utility class covered it, and
 * Stop, the close button and Escape all stopped reaching anything. A model
 * writes such HTML when asked for a mock-up, and data it read can carry it.
 *
 * So raw HTML renders as the text it is, except a few attribute-free inline
 * formatting tags (`<br>`, `<b>`, `<kbd>`...) that models use inside tables.
 * Links keep only web and mail targets: a `javascript:` href runs script in
 * the app when clicked.
 */

import { escapeHtml } from './json-inspector.js'

// Each tag alone, as marked hands inline HTML over one tag at a time.
const SAFE_TAG = /^<\/?(br|b|i|em|strong|u|s|del|ins|mark|small|sub|sup|kbd|code)\s*\/?>$/i

/** @param {string} html one raw HTML token from the markdown */
export function safeRawHtml(html) {
  const t = String(html ?? '')
  return SAFE_TAG.test(t.trim()) ? t.trim() : escapeHtml(t)
}

/** @param {string | null | undefined} href @returns {boolean} */
export function isSafeHref(href) {
  const h = String(href ?? '').trim()
  if (!h) return false
  // Relative links and anchors have no scheme; anything with one must be web or mail.
  const scheme = /^([a-z][a-z0-9+.-]*):/i.exec(h.replace(/[\u0000- ]/g, ''))
  return !scheme || /^(https?|mailto)$/i.test(scheme[1])
}
