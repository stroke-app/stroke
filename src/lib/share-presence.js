/**
 * Live presence on a shared studio: who has the link open, where they are and
 * what they are typing, the same protocol as the web studio
 * (stroke-web src/lib/relay/presence.ts). One socket to the room's
 * `/presence`; every message is sealed with the link's key, so the relay
 * forwards cursors it cannot read.
 *
 * A cell is { table: 'schema.table', row, col }: `row` is the primary key
 * values as strings, JSON-encoded, or `#<n>` without a key. Rows, sorts and
 * pages differ per person; a key is the same everywhere.
 */
import { writable } from 'svelte/store'
import { seal, unseal } from './relay-seal.js'

export const PEER_COLORS = ['#e5484d', '#0090ff', '#30a46c', '#f76b15', '#8e4ec6', '#d6409f', '#12a594', '#ffb224']
/** A person who has sent nothing for this long has gone (a closed laptop). */
const STALE_MS = 45_000

/**
 * @typedef {{ table: string, row: string, col: string }} CellRef
 * @typedef {{
 *   tab: string | null,
 *   cell: CellRef | null,
 *   pointer: (CellRef & { fx: number, fy: number }) | null,
 *   editing: (CellRef & { value: string }) | null,
 * }} Presence
 * @typedef {Presence & { id: number, name: string, color: string, role: 'host' | 'viewer', seen: number }} Peer
 */

/** The row key the web studio uses: primary key values as strings. @param {unknown[]} values */
export function rowKey(values) {
  return JSON.stringify(values.map((v) => (v === null || v === undefined ? null : typeof v === 'object' ? JSON.stringify(v) : String(v))))
}

/**
 * Join a share's presence as `name`. Returns a store of the others, `publish`
 * to say where this side is (sent at most every 50 ms), `announceChanged` after
 * a write, `onChanged` to hear about the others' writes, and `stop`.
 * @param {{ origin: string, room: string, key: CryptoKey, name: string, color?: string, role?: 'host' | 'viewer' }} o
 */
export function joinPresence(o) {
  /** @type {import('svelte/store').Writable<Peer[]>} */
  const peers = writable([])
  const color = o.color ?? PEER_COLORS[Math.floor(Math.random() * PEER_COLORS.length)]
  const role = o.role ?? 'host'
  /** @type {Presence} */
  let state = { tab: null, cell: null, pointer: null, editing: null }
  /** @type {Set<(table: string) => void>} */
  const changed = new Set()
  /** @type {WebSocket | null} */
  let ws = null
  let closed = false
  let delay = 1000
  /** @type {ReturnType<typeof setInterval> | undefined} */
  let ping
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let retry
  /** @type {ReturnType<typeof setTimeout> | null} */
  let timer = null

  /** @param {unknown} msg */
  const send = async (msg) => {
    const sock = ws
    if (!sock || sock.readyState !== 1) return
    const sealed = await seal(o.key, msg)
    if (sock.readyState === 1) sock.send(JSON.stringify(sealed))
  }
  const sendState = () => void send({ t: 'state', name: o.name, color, role, ...state })

  const connect = () => {
    if (closed) return
    const sock = new WebSocket(`${o.origin.replace(/^http/, 'ws')}/api/relay/${o.room}/presence`)
    ws = sock
    sock.onopen = () => {
      delay = 1000
      sendState()
      void send({ t: 'hello' })
      clearInterval(ping)
      ping = setInterval(sendState, 20_000)
    }
    sock.onmessage = async (ev) => {
      /** @type {any} */
      let raw
      try { raw = JSON.parse(String(ev.data)) } catch { return }
      if (typeof raw?.gone === 'number') {
        peers.update((ps) => ps.filter((p) => p.id !== raw.gone))
        return
      }
      if (typeof raw?.from !== 'number' || typeof raw.iv !== 'string' || typeof raw.ct !== 'string') return
      /** @type {any} */
      let msg
      try { msg = await unseal(o.key, raw) } catch { return }
      if (msg?.t === 'hello') sendState()
      else if (msg?.t === 'changed' && typeof msg.table === 'string') for (const fn of changed) fn(msg.table)
      else if (msg?.t === 'state') {
        /** @type {Peer} */
        const peer = {
          id: raw.from,
          name: String(msg.name ?? '').slice(0, 40) || 'Someone',
          color: PEER_COLORS.includes(msg.color) ? msg.color : PEER_COLORS[0],
          role: msg.role === 'host' ? 'host' : 'viewer',
          tab: typeof msg.tab === 'string' ? msg.tab : null,
          cell: msg.cell ?? null,
          pointer: msg.pointer ?? null,
          editing: msg.editing ? { ...msg.editing, value: String(msg.editing.value ?? '').slice(0, 500) } : null,
          seen: Date.now(),
        }
        peers.update((ps) => [...ps.filter((p) => p.id !== peer.id), peer].sort((a, b) => a.id - b.id))
      }
    }
    sock.onclose = () => {
      clearInterval(ping)
      if (closed || ws !== sock) return
      peers.set([])
      retry = setTimeout(connect, delay)
      delay = Math.min(delay * 2, 30_000)
    }
  }
  connect()
  const sweep = setInterval(() => peers.update((ps) => {
    const live = ps.filter((p) => Date.now() - p.seen < STALE_MS)
    return live.length === ps.length ? ps : live
  }), 10_000)

  return {
    peers,
    color,
    /** @param {Partial<Presence>} patch */
    publish(patch) {
      state = { ...state, ...patch }
      if (timer) return
      timer = setTimeout(() => { timer = null; sendState() }, 50)
    },
    /** @param {string} table */
    announceChanged(table) { void send({ t: 'changed', table }) },
    /** @param {(table: string) => void} fn */
    onChanged(fn) {
      changed.add(fn)
      return () => { changed.delete(fn) }
    },
    stop() {
      closed = true
      clearInterval(ping)
      clearInterval(sweep)
      clearTimeout(retry)
      if (timer) clearTimeout(timer)
      peers.set([])
      try { ws?.close(1000, 'Stopped sharing') } catch { /* closed */ }
      ws = null
    },
  }
}
