import { describe, it, expect, vi } from 'vitest'
import { createSqlRepl } from './sql-repl.js'

/** A prompt whose last redrawn line is easy to read back. */
function prompt() {
  let out = ''
  const repl = createSqlRepl({
    write: (/** @type {string} */ s) => { out += s },
    run: async () => ({ columns: [], rows: [] }),
    name: 'db',
    historyKey: 'test-repl',
    listTables: async () => [],
    describe: async () => '',
    cols: () => 80,
  })
  repl.start()
  return {
    type: (/** @type {string} */ s) => repl.input(s),
    /** The text after the prompt on the last redraw. */
    line: () => {
      const last = out.split('\r\x1b[2K').pop() ?? ''
      // eslint-disable-next-line no-control-regex
      return last.replace(/\x1b\[[0-9;]*[A-Za-z]/g, '').replace(/^db> /, '')
    },
    rang: () => out.includes('\x07'),
  }
}

describe('built-in prompt keys', () => {
  it('Ctrl+D deletes the character under the cursor', () => {
    const p = prompt()
    p.type('select 1')
    p.type('\x01') // to the start
    p.type('\x04')
    expect(p.line()).toBe('elect 1')
  })

  it('Ctrl+D on an empty line only rings', () => {
    const p = prompt()
    p.type('\x04')
    expect(p.rang()).toBe(true)
  })

  it('Ctrl+X cuts the line to the clipboard', () => {
    const writeText = vi.fn(() => Promise.resolve())
    vi.stubGlobal('navigator', { clipboard: { writeText } })
    const p = prompt()
    p.type('select 2')
    p.type('\x18')
    expect(writeText).toHaveBeenCalledWith('select 2')
    expect(p.line()).toBe('')
    vi.unstubAllGlobals()
  })
})
