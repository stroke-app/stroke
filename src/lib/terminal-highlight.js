/**
 * Colour the error, warning and notice lines a database client prints, as its
 * bytes stream to the terminal.
 *
 * The clients print these plain: psql's `ERROR:  relation "x" does not exist`
 * looks like any other line of output, and in a long session the one line that
 * matters is easy to scroll past. This watches the start of every line for the
 * prefixes the clients use (psql, mysql/mariadb, sqlite3, sqlcmd, redis-cli) and
 * wraps a matching line in an ANSI colour, which the terminal then draws in the
 * theme's own red, yellow or blue.
 *
 * It works on bytes and passes everything else through untouched, so UTF-8,
 * escape sequences and the client's own colours are never re-encoded. A chunk
 * can end halfway through a prefix (`ERR`), so the bytes after a line break are
 * held until they either match or cannot; `flushAfterMs` lets them go anyway if
 * nothing more arrives, so a prompt is never left waiting on its first letters.
 */

const RED = '\x1b[31m'
const YELLOW = '\x1b[33m'
const BLUE = '\x1b[34m'
const DIM = '\x1b[2m'
const RESET = '\x1b[0m'

/**
 * Line prefixes, by client: literal, case-sensitive, matched at column 0. Kept
 * per client so one client's rule cannot fire on another's output (a psql
 * column called `Msg` in `\x` mode starts its line with sqlcmd's `Msg `).
 * @type {Record<string, Array<[string, string]>>}
 */
const RULES_BY_CLIENT = {
  psql: [
    ['ERROR:', RED], ['FATAL:', RED], ['PANIC:', RED], ['psql: error:', RED],
    ['WARNING:', YELLOW],
    ['NOTICE:', BLUE], ['INFO:', BLUE], ['HINT:', BLUE],
    ['DETAIL:', DIM], ['CONTEXT:', DIM], ['QUERY:', DIM],
  ],
  // `ERROR 1146 (42S02): Table 'x.y' doesn't exist`
  mysql: [['ERROR ', RED], ['Warning (Code', YELLOW], ['Note (Code', BLUE]],
  // `Parse error: no such table: x`; builds before 3.44 say `Error: ...`
  sqlite3: [['Parse error', RED], ['Runtime error', RED], ['Error:', RED]],
  // `Msg 208, Level 16, State 1, Server x, Line 1`
  sqlcmd: [['Msg ', RED], ['Sqlcmd: Error:', RED], ['Sqlcmd: Warning:', YELLOW]],
  'redis-cli': [['(error)', RED]],
}
RULES_BY_CLIENT.mariadb = RULES_BY_CLIENT.mysql
RULES_BY_CLIENT['valkey-cli'] = RULES_BY_CLIENT['redis-cli']

const encoder = new TextEncoder()

/** @param {string} client */
function rulesFor(client) {
  const rules = RULES_BY_CLIENT[client] ?? Object.values(RULES_BY_CLIENT).flat()
  return rules.map(([prefix, sgr]) => ({ prefix: encoder.encode(prefix), sgr: encoder.encode(sgr) }))
}

const RESET_BYTES = encoder.encode(RESET)
const CR = 0x0d
const LF = 0x0a

/** Index of the next CR or LF at or after `from`, or the length. */
function nextBreak(/** @type {Uint8Array} */ bytes, /** @type {number} */ from) {
  for (let i = from; i < bytes.length; i++) {
    if (bytes[i] === CR || bytes[i] === LF) return i
  }
  return bytes.length
}

/** @param {Uint8Array[]} parts */
function concat(parts) {
  if (parts.length === 1) return parts[0]
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
  let at = 0
  for (const p of parts) { out.set(p, at); at += p.length }
  return out
}

/**
 * @param {(bytes: Uint8Array) => void} write where the (coloured) output goes
 * @param {{ client?: string, flushAfterMs?: number }} [opts] `client` picks the
 *   rules (`psql`, `mysql`, ...); unknown or absent uses every client's.
 */
export function createHighlighter(write, { client = '', flushAfterMs = 40 } = {}) {
  const RULES = rulesFor(client)
  let atLineStart = true
  /** Bytes after a line break that might still become a prefix. */
  let held = /** @type {number[]} */ ([])
  /** Inside a coloured line: reset at its end. */
  let colouring = false
  /** @type {ReturnType<typeof setTimeout> | null} */
  let timer = null

  /** @param {number[]} bytes */
  function couldMatch(bytes) {
    return RULES.some(({ prefix }) => bytes.length <= prefix.length && bytes.every((b, i) => prefix[i] === b))
  }

  /** @param {number[]} bytes */
  function matched(bytes) {
    return RULES.find(({ prefix }) => prefix.length === bytes.length && bytes.every((b, i) => prefix[i] === b))
  }

  function releaseHeld() {
    timer = null
    if (!held.length) return
    const out = Uint8Array.from(held)
    held = []
    atLineStart = false
    write(out)
  }

  /** @param {Uint8Array} chunk */
  function push(chunk) {
    if (timer) { clearTimeout(timer); timer = null }
    /** @type {Uint8Array[]} */
    const parts = []
    const n = chunk.length
    let i = 0
    while (i < n) {
      if (colouring) {
        // The rest of a coloured line goes through as is; its break resets.
        const end = nextBreak(chunk, i)
        parts.push(chunk.subarray(i, end))
        i = end
        if (end < n) {
          parts.push(RESET_BYTES)
          colouring = false
          atLineStart = true
        }
        continue
      }
      if (atLineStart || held.length) {
        const b = chunk[i++]
        if (b === CR || b === LF) {
          parts.push(Uint8Array.from([...held, b]))
          held = []
          atLineStart = true
          continue
        }
        held.push(b)
        const rule = matched(held)
        if (rule) {
          parts.push(rule.sgr, Uint8Array.from(held))
          held = []
          colouring = true
          atLineStart = false
        } else if (!couldMatch(held)) {
          parts.push(Uint8Array.from(held))
          held = []
          atLineStart = false
        }
        continue
      }
      // Plain text mid-line: copy through the next line break in one piece.
      const end = nextBreak(chunk, i)
      const stop = end < n ? end + 1 : n
      parts.push(chunk.subarray(i, stop))
      if (end < n) atLineStart = true
      i = stop
    }
    if (parts.length) write(concat(parts))
    if (held.length) timer = setTimeout(releaseHeld, flushAfterMs)
  }

  /** Write out anything held, and close an open colour (the client exited). */
  function flush() {
    if (timer) { clearTimeout(timer); timer = null }
    releaseHeld()
    if (colouring) { colouring = false; write(RESET_BYTES) }
  }

  return { push, flush }
}
