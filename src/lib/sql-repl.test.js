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

describe('engine commands', () => {
  it('runs /name as .name and asks before a confirmed command', async () => {
    const { createSqlRepl } = await import('./sql-repl.js')
    let out = ''
    let restored = ''
    const repl = createSqlRepl({
      write: (s) => { out += s },
      run: async () => [],
      name: 'd1',
      cols: () => 120,
      commands: {
        '.bookmark': { usage: '.bookmark', help: 'b', run: async () => 'BM-1' },
        '.restore': {
          usage: '.restore <id>', help: 'r',
          run: async (arg, ask) => ((await ask('sure? ')) === 'yes' ? ((restored = arg), 'Restored.') : 'Not restored.'),
        },
      },
    })
    repl.start()
    const type = async (text) => { repl.input(text); repl.input('\r'); await new Promise((r) => setTimeout(r, 0)) }
    await type('/bookmark')
    expect(out).toContain('BM-1')
    await type('/restore BM-1')
    expect(out).toContain('sure?')
    await type('yes')
    await new Promise((r) => setTimeout(r, 0))
    expect(restored).toBe('BM-1')
    expect(out).toContain('Restored.')
  })
})
