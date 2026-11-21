/**
 * Share a connection as a web studio (Labs). The app answers the studio's
 * calls itself, on the connection it already has open, through the relay at
 * stroke.click: no strokedb, no Node, nothing to install on either side.
 *
 * Everything is end-to-end encrypted: AES-256-GCM with a key that exists only
 * in the share link's #fragment, which no browser sends to a server. The relay
 * sees the room id and ciphertext. Same scheme as strokedb (cli/src/relay.js)
 * and the studio page (stroke-web src/lib/relay/relay-crypto.ts).
 *
 * A read-only share runs every statement through `share_execute_sql`, which
 * is the MCP server's path: the access classifier, then the engine's read-only
 * transaction. Its links have no edit routes at all.
 */
import { invoke } from '@tauri-apps/api/core'
import { listSchemas, listTables, getTableColumnStructure, listIndexes, updateTableCell, insertTableRow, deleteTableRows } from '$lib/api.js'
import { odbcFlat } from '$lib/sql-ident.js'
import { studioEngine, studioQuote, studioPageSql } from '$lib/studio-sql.js'
import { engineLabel } from '$lib/db-capabilities.js'

export const STUDIO_ORIGIN = 'https://stroke.click'
const VERSION = '0.2.0'
const FEATURES = ['structure', 'where', 'edit', 'rowCounts']

export { studioEngine }

const SYSTEM_SCHEMAS = new Set(['pg_catalog', 'information_schema', 'pg_toast', 'mysql', 'performance_schema', 'sys', 'crdb_internal', 'pg_extension'])

// ── crypto ──────────────────────────────────────────────────────────────────

/** @param {Uint8Array} bytes */
function toB64u(bytes) {
  let s = ''
  for (const b of bytes) s += String.fromCharCode(b)
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}
/** @param {string} text */
function fromB64u(text) {
  const b = atob(text.replace(/-/g, '+').replace(/_/g, '/'))
  const out = new Uint8Array(b.length)
  for (let i = 0; i < b.length; i++) out[i] = b.charCodeAt(i)
  return out
}
const rand = (/** @type {number} */ n) => crypto.getRandomValues(new Uint8Array(n))
/** @param {CryptoKey} key @param {unknown} value */
async function seal(key, value) {
  const iv = rand(12)
  const ct = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, new TextEncoder().encode(JSON.stringify(value))))
  return { iv: toB64u(iv), ct: toB64u(ct) }
}
/** @param {CryptoKey} key @param {{ iv: string, ct: string }} s */
async function unseal(key, s) {
  const pt = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: fromB64u(s.iv) }, key, fromB64u(s.ct))
  return JSON.parse(new TextDecoder().decode(pt))
}

// ── the studio's routes, on the app's connection ────────────────────────────

/**
 * @param {{
 *   engine: string, label?: string,
 *   name: string, database: string, host: string,
 *   write: boolean,
 *   isLive: () => boolean,
 * }} o
 */
function makeRoutes({ engine, label, name, database, host, write, isLive }) {
  const q = (/** @type {string} */ id) => studioQuote(engine, id)
  // Bare names where the engine has no schema to put in front: SQLite and its
  // kin, GoogleSQL Spanner's unnamed schema, a flat ODBC database.
  const bare = (/** @type {string} */ schema) =>
    engine === 'sqlite' || (engine === 'spanner' && schema === 'default') || (engine === 'odbc' && odbcFlat())
  const from = (/** @type {string} */ schema, /** @type {string} */ table) => (bare(schema) ? q(table) : `${q(schema)}.${q(table)}`)
  const str = (/** @type {unknown} */ v) => {
    if (typeof v !== 'string') throw new Error('Expected a string')
    return v
  }
  /** @param {string} sql @returns {Promise<{ columns: string[], rows: unknown[][], rowCount: number }>} */
  const run = async (sql) => {
    const r = JSON.parse(/** @type {string} */ (await invoke('share_execute_sql', { sql, readOnly: !write })))
    const columns = /** @type {string[]} */ (r.columns ?? [])
    const rows = (r.rows ?? []).map((/** @type {any} */ row) => (Array.isArray(row) ? row : columns.map((c) => row?.[c])))
    return { columns, rows, rowCount: Number(r.row_count ?? rows.length) }
  }
  const guard = () => {
    if (!isLive()) throw new Error('The shared database isn\'t open in Stroke right now.')
  }

  /** @type {Record<string, (b: any) => Promise<unknown>>} */
  const routes = {
    '/info': async () => ({
      engine,
      database,
      host,
      version: '',
      label: label || engineLabel(engine),
      quote: studioQuote(engine, 'x').slice(0, 1),
      name,
      bridge: VERSION,
      features: write ? FEATURES : [...FEATURES.filter((f) => f !== 'edit'), 'readonly'],
      shared: true,
      readonly: !write,
    }),
    '/tables': async () => {
      const schemas = engine === 'sqlite' ? ['main'] : (/** @type {string[]} */ (await listSchemas())).filter((s) => !SYSTEM_SCHEMAS.has(s))
      const out = []
      for (const schema of schemas) {
        const list = /** @type {any[]} */ (await listTables(schema))
        for (const t of list) {
          out.push({
            schema,
            name: t.name,
            kind: /view/.test(String(t.kind)) ? 'view' : 'table',
            rows: Number(t.rowCount) >= 0 ? Number(t.rowCount) : null,
          })
        }
      }
      return out
    },
    '/columns': async (b) => {
      const cols = /** @type {any[]} */ (await getTableColumnStructure(str(b.schema), str(b.table)))
      return cols.map((c) => ({ name: c.name, type: c.dataType, nullable: !!c.isNullable, primaryKey: !!c.isPrimaryKey, default: c.columnDefault ?? null }))
    },
    '/structure': async (b) => {
      const schema = str(b.schema)
      const table = str(b.table)
      const cols = /** @type {any[]} */ (await getTableColumnStructure(schema, table))
      const idx = /** @type {any[]} */ (await listIndexes(schema, table).catch(() => []))
      return {
        columns: cols.map((c) => ({ name: c.name, type: c.dataType, nullable: !!c.isNullable, primaryKey: !!c.isPrimaryKey, default: c.columnDefault ?? null })),
        indexes: idx
          .filter((i) => !i.tableName || i.tableName === table)
          .map((i) => ({ name: i.name, columns: String(i.columns || '').split(',').map((s) => s.trim()).filter(Boolean), unique: !!i.isUnique, primary: !!i.isPrimary })),
        foreignKeys: cols
          .filter((c) => c.foreignKey)
          .map((c) => {
            const parts = String(c.foreignKey).split('.')
            const refColumn = parts.pop() ?? ''
            const refTable = parts.pop() ?? ''
            return { name: c.fkConstraintName || `fk_${c.name}`, columns: [c.name], refSchema: parts.join('.') || schema, refTable, refColumns: [refColumn] }
          }),
      }
    },
    '/rows': async (b) => {
      guard()
      const schema = str(b.schema)
      const table = str(b.table)
      const limit = Math.min(Math.max(Number(b.limit) || 100, 1), 1000)
      const offset = Math.max(Number(b.offset) || 0, 0)
      const where = typeof b.where === 'string' && b.where.trim() ? ` WHERE ${b.where}` : ''
      const order = b.sort && typeof b.sort.column === 'string' ? ` ORDER BY ${q(b.sort.column)} ${b.sort.dir === 'desc' ? 'DESC' : 'ASC'}` : ''
      const base = `SELECT * FROM ${from(schema, table)}${where}`
      // An ODBC database's paging isn't known here: LIMIT first, then OFFSET/FETCH.
      const page = await run(studioPageSql(engine, base, order, limit, offset)).catch((e) =>
        engine === 'odbc' ? run(studioPageSql(engine, base, order, limit, offset, true)) : Promise.reject(e),
      )
      const count = await run(`SELECT COUNT(*) AS n FROM ${from(schema, table)}${where}`)
      return { columns: page.columns, rows: page.rows, rowCount: page.rows.length, total: Number(count.rows[0]?.[0] ?? 0), estimated: false }
    },
    '/query': async (b) => {
      guard()
      const t = performance.now()
      const r = await run(str(b.sql))
      return { columns: r.columns, rows: r.rows, rowCount: r.rowCount, command: '', durationMs: Math.round(performance.now() - t) }
    },
  }
  if (write) {
    routes['/update'] = async (b) => {
      guard()
      for (const [column, value] of Object.entries(b.set ?? {})) await updateTableCell(str(b.schema), str(b.table), b.key, column, value)
      return { rowCount: 1 }
    }
    routes['/insert'] = async (b) => {
      guard()
      await insertTableRow(str(b.schema), str(b.table), b.values ?? {})
      return { rowCount: 1 }
    }
    routes['/delete'] = async (b) => {
      guard()
      const n = await deleteTableRows(str(b.schema), str(b.table), Array.isArray(b.keys) ? b.keys : [])
      return { rowCount: Number(n) || 0 }
    }
  }
  return routes
}

// ── the share ───────────────────────────────────────────────────────────────

/**
 * Start sharing. Resolves once the link exists; `onStatus` reports the relay
 * connection as it comes and goes (sleep, Wi-Fi), and it reconnects by itself.
 * @param {{
 *   engine: string, label?: string,
 *   name: string, database: string, host: string,
 *   write: boolean,
 *   isLive: () => boolean,
 *   onStatus?: (s: 'connecting' | 'online' | 'offline') => void,
 *   origin?: string,
 * }} o
 */
export async function startShare(o) {
  const origin = o.origin ?? STUDIO_ORIGIN
  const room = toB64u(rand(16))
  const keyText = toB64u(rand(32))
  const secret = toB64u(rand(32))
  const key = await crypto.subtle.importKey('raw', fromB64u(keyText), 'AES-GCM', false, ['encrypt', 'decrypt'])
  const routes = makeRoutes(o)
  const url = `${origin.replace(/^http/, 'ws')}/api/relay/${room}/host?${new URLSearchParams({ secret })}`
  const onStatus = o.onStatus ?? (() => {})
  let closed = false
  let delay = 1000
  /** @type {WebSocket | null} */
  let ws = null
  /** @type {ReturnType<typeof setInterval> | undefined} */
  let ping

  /** @param {{ id: number, iv: string, ct: string }} msg */
  const answer = async (msg) => {
    let reply
    try {
      const { path, body } = await unseal(key, msg)
      const route = typeof path === 'string' ? routes[path] : undefined
      reply = route ? { status: 200, data: await route(body ?? {}) } : { status: 404, error: 'Not available in this shared studio.' }
    } catch (e) {
      reply = { status: 400, error: e instanceof Error ? e.message : String(e) }
    }
    try {
      ws?.send(JSON.stringify({ id: msg.id, ...(await seal(key, reply)) }))
    } catch { /* the socket went; the page retries */ }
  }

  const connect = () => {
    if (closed) return
    onStatus('connecting')
    const sock = new WebSocket(url)
    ws = sock
    sock.onopen = () => {
      delay = 1000
      onStatus('online')
      clearInterval(ping)
      ping = setInterval(() => sock.readyState === 1 && sock.send('{"ping":1}'), 25_000)
    }
    sock.onmessage = (ev) => {
      let msg
      try { msg = JSON.parse(String(ev.data)) } catch { return }
      if (typeof msg?.id === 'number' && typeof msg.iv === 'string' && typeof msg.ct === 'string') void answer(msg)
    }
    sock.onclose = () => {
      clearInterval(ping)
      if (ws !== sock || closed) return
      onStatus('offline')
      setTimeout(connect, delay)
      delay = Math.min(delay * 2, 30_000)
    }
  }
  connect()

  return {
    link: `${origin}/studio#${new URLSearchParams({ relay: room, key: keyText })}`,
    write: o.write,
    stop() {
      closed = true
      clearInterval(ping)
      try { ws?.close(1000, 'Stopped sharing') } catch { /* closed */ }
      onStatus('offline')
    },
  }
}
