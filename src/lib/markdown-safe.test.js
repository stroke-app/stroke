import { describe, it, expect } from 'vitest'
import { marked } from 'marked'
import { safeRawHtml, isSafeHref } from './markdown-safe.js'

describe('safeRawHtml', () => {
  it('passes attribute-free inline formatting tags', () => {
    for (const t of ['<br>', '<br/>', '<b>', '</b>', '<kbd>', '</sub>']) expect(safeRawHtml(t)).toBe(t)
  })

  it('renders anything else as text', () => {
    expect(safeRawHtml('<style>*{pointer-events:none}</style>')).toBe('&lt;style&gt;*{pointer-events:none}&lt;/style&gt;')
    expect(safeRawHtml('<div style="position:fixed;inset:0">')).toBe('&lt;div style="position:fixed;inset:0"&gt;')
    expect(safeRawHtml('<b class="fixed inset-0">')).toBe('&lt;b class="fixed inset-0"&gt;')
    expect(safeRawHtml('<img src=x onerror=alert(1)>')).toBe('&lt;img src=x onerror=alert(1)&gt;')
    expect(safeRawHtml('<button>+ Tree</button>')).toBe('&lt;button&gt;+ Tree&lt;/button&gt;')
  })

  it('keeps a whole HTML block out of the DOM when used as the renderer', () => {
    const renderer = new marked.Renderer()
    renderer.html = ({ text }) => safeRawHtml(text)
    const out = /** @type {string} */ (marked.parse('<div class="fixed inset-0"><style>body{display:none}</style></div>\n\nok <b>bold</b>', { renderer }))
    expect(out).not.toMatch(/<div|<style/)
    expect(out).toContain('<b>bold</b>')
  })

  it('drops a script link to its text and keeps a web one', () => {
    const renderer = new marked.Renderer()
    const link = renderer.link.bind(renderer)
    renderer.link = (token) => (isSafeHref(token.href) ? link(token) : renderer.parser.parseInline(token.tokens))
    const out = /** @type {string} */ (marked.parse('[x](javascript:alert(1)) and [ok](https://a.dev)', { renderer }))
    expect(out).not.toContain('javascript:')
    expect(out).toContain('<p>x and ')
    expect(out).toContain('href="https://a.dev"')
  })
})

describe('isSafeHref', () => {
  it('allows web, mail and relative links', () => {
    for (const h of ['https://x.dev', 'http://a', 'mailto:a@b.c', '#top', '/docs', 'page.html']) expect(isSafeHref(h), h).toBe(true)
  })

  it('refuses script and other schemes', () => {
    for (const h of ['javascript:alert(1)', ' JavaScript:alert(1)', 'java\nscript:alert(1)', 'data:text/html,x', 'vbscript:x', 'tauri://x', '']) {
      expect(isSafeHref(h), h).toBe(false)
    }
  })
})
