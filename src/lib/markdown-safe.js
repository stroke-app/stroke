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

/**
 * A fence opener whose info string is a sentence is a model talking about a
 * code block, not starting one: "schema changes still go in a ```sql block for
 * you to run" wrapped before the backticks, and the rest of the reply rendered
 * as a code block captioned with that sentence. A real info string is a
 * language and maybe an option or two. Such a line goes back to prose (the
 * language as inline code) on the line it was split from. Lines inside a real
 * fence are left alone.
 * @param {string} md
 */
export function unfenceProse(md) {
  const text = String(md ?? '')
  if (!text.includes('```') && !text.includes('~~~')) return text
  const lines = text.split('\n')
  /** @type {string[]} */
  const out = []
  /** The open fence's marker (``` or ~~~, at its length), or '' outside one. */
  let open = ''
  for (const line of lines) {
    const m = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line)
    if (open) {
      // Closed by the same character, at least as long, with nothing after it.
      if (m && m[1][0] === open[0] && m[1].length >= open.length && !m[2].trim()) open = ''
      out.push(line)
      continue
    }
    if (!m || (m[1][0] === '`' && m[2].includes('`'))) { out.push(line); continue }
    const words = m[2].trim().split(/\s+/).filter(Boolean)
    if (words.length < 4) { open = m[1]; out.push(line); continue }
    const prose = `\`${words[0]}\` ${words.slice(1).join(' ')}`
    const prev = out.length ? out[out.length - 1] : ''
    if (prev.trim() && !/^\s*(#|[-*+]\s|\d+[.)]\s|>|\|)/.test(prev)) out[out.length - 1] = `${prev.trimEnd()} ${prose}`
    else out.push(prose)
  }
  return out.join('\n')
}
