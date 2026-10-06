import { describe, it, expect, vi } from 'vitest'
import { createHighlighter } from './terminal-highlight.js'

const enc = new TextEncoder()
const dec = new TextDecoder()

/** Push `chunks` through a highlighter and return everything it wrote, as text. */
function run(chunks, client = 'psql') {
  let out = ''
  const h = createHighlighter((b) => { out += dec.decode(b, { stream: true }) }, { client })
  for (const c of chunks) h.push(enc.encode(c))
  h.flush()
  return out
}

describe('createHighlighter', () => {
  it('colours a psql error line and resets at its end', () => {
    expect(run(['ERROR:  relation "x" does not exist\r\nLINE 1: select\r\n']))
      .toBe('\x1b[31mERROR:  relation "x" does not exist\x1b[0m\r\nLINE 1: select\r\n')
  })

  it('matches a prefix split across chunks', () => {
    expect(run(['db=# select 1/0;\r\nER', 'ROR:  division by zero\r\n']))
      .toBe('db=# select 1/0;\r\n\x1b[31mERROR:  division by zero\x1b[0m\r\n')
  })

  it('leaves ordinary output and prompts alone', () => {
    const text = ' id | name\r\n----+------\r\n  1 | ERROR: not at column 0\r\n(1 row)\r\n\r\npsql_test=# '
    expect(run([text])).toBe(text)
  })

  it('passes UTF-8 and escape sequences through byte for byte', () => {
    const text = '\x1b[1;34mcafé\x1b[0m=# naïve ✓\r\n'
    expect(run([text])).toBe(text)
  })

  it('uses only the running client\'s rules', () => {
    expect(run(['Msg 208, Level 16\r\n'], 'psql')).toBe('Msg 208, Level 16\r\n')
    expect(run(['Msg 208, Level 16\r\n'], 'sqlcmd')).toBe('\x1b[31mMsg 208, Level 16\x1b[0m\r\n')
    expect(run(["ERROR 1146 (42S02): Table 'a.b' doesn't exist\n"], 'mariadb'))
      .toBe("\x1b[31mERROR 1146 (42S02): Table 'a.b' doesn't exist\x1b[0m\n")
  })

  it('colours warnings and notices in their own colours', () => {
    expect(run(['WARNING:  there is no transaction in progress\r\n']))
      .toBe('\x1b[33mWARNING:  there is no transaction in progress\x1b[0m\r\n')
    expect(run(['NOTICE:  table "t" does not exist, skipping\r\n']))
      .toBe('\x1b[34mNOTICE:  table "t" does not exist, skipping\x1b[0m\r\n')
  })

  it('releases a held partial prefix when no more output comes', () => {
    vi.useFakeTimers()
    let out = ''
    const h = createHighlighter((b) => { out += dec.decode(b) }, { client: 'psql' })
    h.push(enc.encode('done\r\nE'))
    expect(out).toBe('done\r\n')
    vi.advanceTimersByTime(50)
    expect(out).toBe('done\r\nE')
    vi.useRealTimers()
  })

  it('closes an open colour on flush', () => {
    expect(run(['FATAL:  password authentication failed'])).toBe('\x1b[31mFATAL:  password authentication failed\x1b[0m')
  })
})
