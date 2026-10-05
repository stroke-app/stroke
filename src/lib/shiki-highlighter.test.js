import { describe, it, expect } from 'vitest'
import { highlightCode, resolveShikiLang } from './shiki-highlighter.js'

describe('shiki highlighter', () => {
  it('maps fence names onto the grammars it ships, the rest to plain text', () => {
    expect(resolveShikiLang('py')).toBe('python')
    expect(resolveShikiLang('bash')).toBe('shellscript')
    expect(resolveShikiLang('PostgreSQL')).toBe('sql')
    expect(resolveShikiLang('cobol')).toBe('plaintext')
    expect(resolveShikiLang(undefined)).toBe('plaintext')
  })

  it('highlights SQL, loads another grammar on demand, and falls back to plain text', async () => {
    const sqlHtml = await highlightCode('SELECT 1 FROM users', 'sql', 'dark')
    expect(sqlHtml).toContain('<pre class="shiki')
    expect(sqlHtml.match(/style="color/g)?.length).toBeGreaterThan(1)
    const py = await highlightCode('def f():\n  return 1', 'py', 'light')
    expect(py.match(/style="color/g)?.length).toBeGreaterThan(1)
    const plain = await highlightCode('anything', 'cobol')
    expect(plain).toContain('anything')
  })
})
