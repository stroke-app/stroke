// Pure helpers for the Redis keys table: staged-edit bookkeeping, the order
// staged commands are sent in, and display formatting. No Svelte, no IPC, so
// it is unit-tested directly.

/**
 * A staged write. `cell` names what it changes so a second edit to the same
 * thing replaces the first: `value`, `ttl`, `key` (rename), `row` (delete),
 * `create`, `e:<id>` for one entry inside a collection, or `post:<id>` for a
 * cleanup that must run after the entry edits (list deletes).
 * @typedef {{ id: number, key: string, binary: boolean, cell: string, label: string,
 *   display?: string, deleted?: boolean, ops: string[][], error?: string,
 *   create?: { type: string, preview: string } }} Staged
 */

/** Placeholder a deleted list element is set to before one LREM removes them all. */
export const LIST_TOMBSTONE = '__stroke_deleted_element__'

/** Redis types that can be created from the table. */
export const CREATABLE_TYPES = ['string', 'hash', 'list', 'set', 'zset']

let nextId = 1

/**
 * Add or replace a staged write. Deleting a key drops every other staged edit
 * to it, and a key staged for deletion takes no further edits except an undo.
 * @param {Staged[]} list
 * @param {Omit<Staged, 'id'>} item
 * @returns {Staged[]}
 */
export function stage(list, item) {
  const same = (/** @type {Staged} */ s) => s.key === item.key
  if (item.cell === 'row') {
    // A staged create that is then deleted never needs to reach Redis.
    if (list.some((s) => same(s) && s.cell === 'create')) return list.filter((s) => !same(s))
    return [...list.filter((s) => !same(s)), { ...item, id: nextId++ }]
  }
  if (list.some((s) => same(s) && s.cell === 'row')) return list
  const i = list.findIndex((s) => same(s) && s.cell === item.cell)
  if (i < 0) return [...list, { ...item, id: nextId++ }]
  const next = list.slice()
  next[i] = { ...item, id: list[i].id }
  return next
}

/**
 * Drop staged writes for a key (all of them, or one cell).
 * @param {Staged[]} list @param {string} key @param {string} [cell]
 */
export function unstage(list, key, cell) {
  return list.filter((s) => !(s.key === key && (cell == null || s.cell === cell)))
}

/** @param {Staged[]} list @param {string} key @param {string} cell */
export function stagedFor(list, key, cell) {
  return list.find((s) => s.key === key && s.cell === cell)
}

/**
 * The order to send staged writes in: creates, edits, cleanups (list
 * tombstone removal, so every LSET still sees the original indexes), renames
 * (so edits still address the old name), then deletes. Stable within a group.
 * @param {Staged[]} list
 */
export function ordered(list) {
  const rank = (/** @type {Staged} */ s) =>
    s.cell === 'create' ? 0 : s.cell.startsWith('post:') ? 2 : s.cell === 'key' ? 3 : s.cell === 'row' ? 4 : 1
  return list.map((s, i) => ({ s, i })).sort((a, b) => rank(a.s) - rank(b.s) || a.i - b.i).map((x) => x.s)
}

/**
 * Flatten staged writes into the op list `redisApply` takes, remembering which
 * staged item each op came from.
 * @param {Staged[]} list
 * @returns {{ ops: { args: string[], binary_key: boolean }[], owner: number[] }}
 */
export function toOps(list) {
  /** @type {{ args: string[], binary_key: boolean }[]} */
  const ops = []
  /** @type {number[]} */
  const owner = []
  for (const s of ordered(list)) {
    for (const args of s.ops) {
      ops.push({ args, binary_key: s.binary })
      owner.push(s.id)
    }
  }
  return { ops, owner }
}

/**
 * The commands that create a key holding one first value.
 * @param {string} key @param {string} type
 * @param {{ value: string, field?: string, score?: string, ttl?: number | null }} v
 * @returns {string[][]}
 */
export function createOps(key, type, v) {
  /** @type {string[][]} */
  const ops = []
  if (type === 'string') ops.push(['SET', key, v.value])
  else if (type === 'hash') ops.push(['HSET', key, v.field || 'field', v.value])
  else if (type === 'list') ops.push(['RPUSH', key, v.value])
  else if (type === 'set') ops.push(['SADD', key, v.value])
  else if (type === 'zset') ops.push(['ZADD', key, v.score || '0', v.value])
  else throw new Error(`Can't create a ${type} key`)
  if (v.ttl != null && v.ttl > 0) ops.push(['EXPIRE', key, String(Math.floor(v.ttl))])
  return ops
}

/**
 * A TTL input to commands: blank / 0 / "none" → PERSIST, a duration such as
 * `90`, `90s`, `15m`, `2h`, `1d` → EXPIRE seconds. Returns null when unreadable.
 * @param {string} key @param {string} input
 * @returns {{ ops: string[][], seconds: number | null } | null}
 */
export function ttlOps(key, input) {
  const t = input.trim().toLowerCase()
  if (t === '' || t === '0' || t === 'none' || t === '-1' || t === '∞') return { ops: [['PERSIST', key]], seconds: null }
  const m = /^(\d+(?:\.\d+)?)\s*(s|m|h|d)?$/.exec(t)
  if (!m) return null
  const mult = { s: 1, m: 60, h: 3600, d: 86400 }[/** @type {'s'|'m'|'h'|'d'} */ (m[2] ?? 's')]
  const seconds = Math.round(Number(m[1]) * mult)
  if (!Number.isFinite(seconds) || seconds <= 0) return null
  return { ops: [['EXPIRE', key, String(seconds)]], seconds }
}

/**
 * Turn what the user typed in the filter box into a SCAN MATCH glob: text with
 * no glob characters becomes a substring match.
 * @param {string} input
 */
export function toPattern(input) {
  const t = input.trim()
  if (!t) return ''
  if (/[*?[\]]/.test(t)) return t
  return `*${t}*`
}

/** Humanize seconds into "1d 2h" / "5m" / "42s". @param {number} s */
export function humanDuration(s) {
  s = Math.max(0, Math.floor(s))
  if (s < 60) return `${s}s`
  const d = Math.floor(s / 86400)
  let r = s % 86400
  const h = Math.floor(r / 3600)
  r %= 3600
  const m = Math.floor(r / 60)
  const sec = r % 60
  const parts = []
  if (d) parts.push(`${d}d`)
  if (h) parts.push(`${h}h`)
  if (m) parts.push(`${m}m`)
  if (sec && !d && !h) parts.push(`${sec}s`)
  return parts.slice(0, 2).join(' ')
}

/** Humanize bytes into "1.2 KB". @param {number | null | undefined} n */
export function humanBytes(n) {
  if (n == null || !Number.isFinite(n)) return ''
  if (n < 1024) return `${n} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let v = n / 1024
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`
}

/**
 * Remaining TTL label for a row, counting down from when it was fetched.
 * @param {number} ttlMs PTTL at fetch time (-1 none, -2 gone)
 * @param {number} fetchedAt ms timestamp @param {number} now ms timestamp
 */
export function ttlLabel(ttlMs, fetchedAt, now) {
  if (ttlMs === -1) return ''
  if (ttlMs === -2) return 'expired'
  const left = ttlMs - (now - fetchedAt)
  return left <= 0 ? 'expired' : humanDuration(left / 1000)
}

/** Quote one argument the way redis-cli would accept it back. @param {string} s */
export function quoteArg(s) {
  if (/^[A-Za-z0-9:_\-.\/@]+$/.test(s)) return s
  return '"' + s.replace(/(["\\])/g, '\\$1').replace(/\n/g, '\\n') + '"'
}

/** A staged op as a readable command line, for the review list. @param {string[]} args */
export function commandLine(args) {
  return args.map((a, i) => (i === 0 ? a : quoteArg(a))).join(' ')
}
