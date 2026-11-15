// Building and publishing share links. The dialog collects the sharer's
// choices; this turns them into a payload, encrypts it (share-crypto.js),
// uploads the ciphertext and remembers the link on this machine.

import { writable } from 'svelte/store'
import { sealShare, maskValue } from '$lib/share-crypto.js'
import { shareCreate } from '$lib/api.js'

/** Expiries offered, in seconds (null = never, Pro). Mirrors the server. */
export const SHARE_EXPIRIES = /** @type {const} */ ([
  { label: '1 hour', seconds: 3600 },
  { label: '1 day', seconds: 86400 },
  { label: '7 days', seconds: 7 * 86400 },
  { label: '30 days', seconds: 30 * 86400 },
  { label: 'Never', seconds: null },
])

/**
 * What a share is made from, handed to the dialog by whatever opened it.
 * @typedef {{ kind: 'result' | 'table', title: string, source?: string, columns: { name: string, type?: string }[], rows: unknown[][], totalRows?: number, sql?: string }} TableSource
 * @typedef {{ kind: 'query', title: string, source?: string, sql: string, dialect?: string }} QuerySource
 * @typedef {{ kind: 'erd', title: string, source?: string, tables: { name: string, x: number, y: number, columns: { name: string, type: string, pk?: boolean, fk?: string | null }[] }[], links: { from: string, fromCol: string, to: string, toCol: string }[] }} ErdSource
 * @typedef {{ kind: 'mermaid', title: string, source?: string, code: string }} MermaidSource
 * @typedef {{ kind: 'chart', title: string, source?: string, chart: { type: string, x: string, y: string[], group?: string | null }, columns: { name: string, type?: string }[], rows: unknown[][] }} ChartSource
 * @typedef {TableSource | QuerySource | ErdSource | MermaidSource | ChartSource} ShareSource
 */

/** The share the dialog is open for, or null. */
export const shareRequest = writable(/** @type {ShareSource | null} */ (null))

/** Open the share dialog for `source`. @param {ShareSource} source */
export function requestShare(source) {
  shareRequest.set(source)
}

/**
 * The payload for a source and the sharer's choices: kept columns, masked
 * columns, and the plan's row cap.
 * @param {ShareSource} src
 * @param {{ title: string, keep?: Set<number>, mask?: Set<number>, rowCap?: number }} o
 */
export function buildPayload(src, o) {
  const base = { v: 1, title: o.title.trim() || src.title, createdAt: Date.now(), source: src.source }
  if (src.kind === 'result' || src.kind === 'table' || src.kind === 'chart') {
    const keep = src.columns.map((_, i) => i).filter((i) => !o.keep || o.keep.has(i))
    const mask = o.mask ?? new Set()
    const cap = o.rowCap ?? src.rows.length
    const rows = src.rows.slice(0, cap).map((r) => keep.map((i) => (mask.has(i) ? maskValue(r[i]) : r[i])))
    const columns = keep.map((i) => src.columns[i])
    const masked = keep.filter((i) => mask.has(i)).map((i) => src.columns[i].name)
    if (src.kind === 'chart') return { ...base, kind: 'chart', chart: src.chart, columns, rows, totalRows: src.rows.length, masked }
    return { ...base, kind: src.kind, columns, rows, totalRows: src.totalRows ?? src.rows.length, masked, sql: src.sql }
  }
  if (src.kind === 'query') return { ...base, kind: 'query', sql: src.sql, dialect: src.dialect }
  if (src.kind === 'erd') return { ...base, kind: 'erd', tables: src.tables, links: src.links }
  return { ...base, kind: 'mermaid', code: src.code }
}

// ── Links made on this machine ──────────────────────────────────────────────
// The server never sees a key, so it can't give a link back. Each link (with
// its key) is kept here so My shares can copy it again; nothing leaves the
// device.

const KEY = 'stroke:shares'

/** @typedef {{ id: string, url: string, title: string, kind: string, createdAt: number }} LocalShare */

/** @returns {Record<string, LocalShare>} */
export function localShares() {
  try { return JSON.parse(localStorage.getItem(KEY) || '{}') } catch { return {} }
}

/** @param {LocalShare} s */
function rememberShare(s) {
  try {
    const all = localShares()
    all[s.id] = s
    localStorage.setItem(KEY, JSON.stringify(all))
  } catch { /* storage full or blocked: the link was still shown once */ }
}

/** @param {string} id */
export function forgetShare(id) {
  try {
    const all = localShares()
    delete all[id]
    localStorage.setItem(KEY, JSON.stringify(all))
  } catch {}
}

/**
 * Encrypt, upload and remember. Returns the full link, key included.
 * @param {object} payload
 * @param {{ expiresIn: number | null, maxViews: number | null, password?: string | null }} o
 */
export async function publishShare(payload, o) {
  const p = /** @type {any} */ (payload)
  const sealed = await sealShare(payload, { password: o.password })
  const res = await shareCreate(
    { kind: p.kind, expiresIn: o.expiresIn, maxViews: o.maxViews, passwordSalt: sealed.passwordSalt },
    sealed.blob,
  )
  const url = `${res.url}#${sealed.linkKey}`
  rememberShare({ id: res.id, url, title: p.title, kind: p.kind, createdAt: Date.now() })
  return { id: res.id, url, bytes: sealed.blob.length }
}
