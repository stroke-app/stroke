/**
 * Laid-out schema diagrams kept across restarts.
 *
 * Laying out a schema of 150 tables takes ELK about four seconds, and the
 * positions only lived for the session: every start of the app paid it again
 * for a schema that had not changed. A layout is saved under a key made from
 * the connection, the schema, the diagram's shape settings and a signature of
 * the drawn graph (each card's name and row count, each line's ends), so any
 * change to the tables or the settings misses and lays out afresh.
 *
 * localStorage, at most MAX_ENTRIES diagrams, oldest dropped first. A failed
 * read or write (quota, private storage) only costs the saved time.
 */

const PREFIX = 'stroke:erd-layout:v1:'
const INDEX_KEY = 'stroke:erd-layouts:v1'
const MAX_ENTRIES = 8

/** 32-bit FNV-1a, as hex. @param {string} s */
function fnv1a(s) {
  let h = 0x811c9dc5
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i)
    h = Math.imul(h, 0x01000193) >>> 0
  }
  return h.toString(16).padStart(8, '0')
}

/**
 * The key for one drawn diagram.
 * @param {{ connectionId: string, schema: string, shape: string,
 *   nodes: { id: string, rows: number }[], edges: { id: string, source: string, target: string }[] }} g
 */
export function layoutKey({ connectionId, schema, shape, nodes, edges }) {
  const n = nodes.map((x) => `${x.id}:${x.rows}`).sort().join('|')
  const e = edges.map((x) => `${x.id}=${x.source}>${x.target}`).sort().join('|')
  return `${PREFIX}${fnv1a(`${connectionId}\u0000${schema}`)}:${fnv1a(`${shape}\u0000${n}\u0000${e}`)}:${nodes.length}.${edges.length}`
}

/**
 * The saved layout for `key`, or null. Positions must cover every id in `ids`,
 * or it is not used: a partial layout would leave cards at the origin.
 * @param {string} key @param {string[]} ids
 * @returns {{ positions: Map<string, {x:number,y:number}>, hints: Map<string, {x:number,y:number}[]> } | null}
 */
export function loadLayout(key, ids) {
  try {
    const raw = localStorage.getItem(key)
    if (!raw) return null
    const data = JSON.parse(raw)
    const positions = new Map(Object.entries(data.p ?? {}).map(([id, [x, y]]) => [id, { x, y }]))
    if (!ids.every((id) => positions.has(id))) return null
    const hints = new Map(Object.entries(data.h ?? {}).map(([id, flat]) => [id, unflatten(flat)]))
    touch(key)
    return { positions, hints }
  } catch {
    return null
  }
}

/**
 * Save a layout. Points are stored as flat number arrays to keep it small.
 * @param {string} key @param {Map<string, {x:number,y:number}>} positions
 * @param {Map<string, {x:number,y:number}[]>} hints
 */
export function saveLayout(key, positions, hints) {
  try {
    /** @type {Record<string, [number, number]>} */
    const p = {}
    for (const [id, q] of positions) p[id] = [Math.round(q.x), Math.round(q.y)]
    /** @type {Record<string, number[]>} */
    const h = {}
    for (const [id, pts] of hints) h[id] = pts.flatMap((q) => [Math.round(q.x), Math.round(q.y)])
    localStorage.setItem(key, JSON.stringify({ p, h }))
    touch(key)
  } catch {
    /* storage full or unavailable: the next open lays out again */
  }
}

/** @param {number[]} flat */
function unflatten(flat) {
  /** @type {{x:number,y:number}[]} */
  const pts = []
  for (let i = 0; i + 1 < flat.length; i += 2) pts.push({ x: flat[i], y: flat[i + 1] })
  return pts
}

/** Mark `key` most recently used, dropping the oldest past MAX_ENTRIES. @param {string} key */
function touch(key) {
  /** @type {string[]} */
  let keys = []
  try { keys = JSON.parse(localStorage.getItem(INDEX_KEY) ?? '[]') } catch { keys = [] }
  keys = [key, ...keys.filter((k) => k !== key)]
  for (const old of keys.slice(MAX_ENTRIES)) {
    try { localStorage.removeItem(old) } catch { /* ignore */ }
  }
  try { localStorage.setItem(INDEX_KEY, JSON.stringify(keys.slice(0, MAX_ENTRIES))) } catch { /* ignore */ }
}
