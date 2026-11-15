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
import { Marked } from 'marked'

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

/** @type {import('marked').Marked | null} */
let _safe = null

/**
 * Markdown to HTML for anything that came from a file or a person: raw HTML
 * renders as text (a few inline tags aside), links go only to web or mail
 * targets, and images become plain links. Notebook markdown cells use it: a
 * notebook can be a downloaded .ipynb or .md, and {@html} puts it in the app.
 * @param {string} md
 */
export function renderSafeMarkdown(md) {
  if (!_safe) {
    _safe = new Marked({ gfm: true, breaks: true })
    _safe.use({
      renderer: {
        html: ({ text }) => safeRawHtml(text),
        link(token) {
          if (!isSafeHref(token.href)) return this.parser.parseInline(token.tokens)
          const href = escapeHtml(token.href).replace(/"/g, '&quot;')
          return `<a href="${href}" target="_blank" rel="noopener noreferrer">${this.parser.parseInline(token.tokens)}</a>`
        },
        image({ href, text }) {
          const label = escapeHtml(text || href || 'image')
          if (!isSafeHref(href)) return label
          return `<a href="${escapeHtml(href).replace(/"/g, '&quot;')}" target="_blank" rel="noopener noreferrer">${label}</a>`
        },
      },
    })
  }
  try {
    return /** @type {string} */ (_safe.parse(String(md ?? '')))
  } catch {
    return escapeHtml(String(md ?? ''))
  }
}
