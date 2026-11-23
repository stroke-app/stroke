import { describe, it, expect } from 'vitest'
import { marked } from 'marked'
import { safeRawHtml, isSafeHref, unfenceProse } from './markdown-safe.js'

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

import { renderSafeMarkdown } from './markdown-safe.js'
describe('renderSafeMarkdown', () => {
  it('renders markdown but not raw HTML, script links or images', () => {
    const html = renderSafeMarkdown('# T\n\n**b** <img src=x onerror=alert(1)> [x](javascript:alert(1)) [ok](https://a.b) ![i](https://a.b/i.png)')
    expect(html).toContain('<h1')
    expect(html).toContain('<strong>b</strong>')
    expect(html).not.toContain('<img')
    expect(html).not.toContain('href="javascript')
    expect(html).toContain('href="https://a.b"')
  })
})

describe('unfenceProse', () => {
  it('turns a sentence that opens with a fence back into prose', () => {
    const md = 'Since this chat is read & write, schema changes still go in a\n```sql block for you to run. I can open the statement in the editor.\n\nTwo things I need from you:\n1. What should it contain?'
    const out = unfenceProse(md)
    expect(out).toContain('still go in a `sql` block for you to run. I can open the statement in the editor.')
    expect(marked.parse(out)).not.toContain('<pre')
  })

  it('keeps real fences, with or without options', () => {
    for (const md of ['```sql\nselect 1\n```', '```js title=a.js\nx()\n```', '~~~\nplain\n~~~']) expect(unfenceProse(md)).toBe(md)
  })

  it('leaves lines inside a real fence alone', () => {
    const md = '```md\n```sql block for you to run. I can open it.\n```'
    expect(unfenceProse(md)).toBe(md)
  })

  it('starts a new line when the previous one is a list item or blank', () => {
    expect(unfenceProse('- one\n```sql block for you to run now')).toBe('- one\n`sql` block for you to run now')
    expect(unfenceProse('```sql block for you to run now')).toBe('`sql` block for you to run now')
  })
})
