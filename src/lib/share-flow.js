// Building and publishing share links. The dialog collects the sharer's
// choices; this turns them into a payload, encrypts it (share-crypto.js),
// uploads the ciphertext and remembers the link on this machine.

import { writable } from 'svelte/store'
import { sealShare, maskValue } from '$lib/share-crypto.js'
import { shareCreate } from '$lib/api.js'
import { loadSettings, updateSettings } from '$lib/stores/settings.js'

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

// ── Defaults for new links (Settings → Account & Sync → Sharing) ────────────

const DEFAULTS_KEY = 'stroke:share-defaults'

/** @typedef {{ expiry: number | null, access: 'link' | 'once' | 'limit', viewLimit: number, maskPii: boolean }} ShareDefaults */

/** @type {ShareDefaults} */
export const SHARE_DEFAULTS = { expiry: 86400, access: 'link', viewLimit: 10, maskPii: true }

/**
 * Whatever was stored, as valid defaults: unknown values fall back one by one.
 * @param {any} raw @returns {ShareDefaults}
 */
export function normalizeShareDefaults(raw) {
  const r = raw && typeof raw === 'object' ? raw : {}
  const expiry = r.expiry === null || SHARE_EXPIRIES.some((e) => e.seconds === r.expiry) ? r.expiry : SHARE_DEFAULTS.expiry
  const access = ['link', 'once', 'limit'].includes(r.access) ? r.access : SHARE_DEFAULTS.access
  const viewLimit = Number.isInteger(r.viewLimit) && r.viewLimit >= 1 && r.viewLimit <= 10000 ? r.viewLimit : SHARE_DEFAULTS.viewLimit
  return { expiry: expiry === undefined ? SHARE_DEFAULTS.expiry : expiry, access, viewLimit, maskPii: r.maskPii !== false }
}

/**
 * The defaults live in the app settings, so they sync with the account. The
 * key they had before (this device only) is read once as the starting point.
 * @returns {ShareDefaults}
 */
export function loadShareDefaults() {
  const saved = loadSettings().shareDefaults
  if (saved) return normalizeShareDefaults(saved)
  try { return normalizeShareDefaults(JSON.parse(localStorage.getItem(DEFAULTS_KEY) || 'null')) } catch { return { ...SHARE_DEFAULTS } }
}

/** @param {Partial<ShareDefaults>} patch @returns {ShareDefaults} */
export function saveShareDefaults(patch) {
  const next = normalizeShareDefaults({ ...loadShareDefaults(), ...patch })
  updateSettings({ shareDefaults: next })
  try { localStorage.removeItem(DEFAULTS_KEY) } catch {}
  return next
}

/** `product_count` → `Product count`. @param {string} name */
function prettyName(name) {
  const t = String(name ?? '').replace(/[_-]+/g, ' ').replace(/([a-z])([A-Z])/g, '$1 $2').trim().toLowerCase()
  return t ? t[0].toUpperCase() + t.slice(1) : ''
}

/**
 * A title for a chart from what it plots: "Product count by category".
 * @param {{ x?: string, y?: string[], group?: string | null }} spec
 */
export function chartTitle(spec) {
  const y = (spec?.y ?? []).filter(Boolean).map(prettyName).join(' and ')
  const x = spec?.x ? prettyName(spec.x).toLowerCase() : ''
  const g = spec?.group ? ` per ${prettyName(spec.group).toLowerCase()}` : ''
  return y && x ? `${y} by ${x}${g}` : y || prettyName(spec?.x ?? '') || 'Chart'
}

/**
 * A title for query results: the table the query reads, or its first line.
 * @param {string} sql
 */
export function resultsTitle(sql) {
  const text = String(sql ?? '').replace(/--[^\n]*/g, ' ')
  const m = /\bfrom\s+((?:"[^"]+"|`[^`]+`|\[[^\]]+\]|[\w$]+)(?:\.(?:"[^"]+"|`[^`]+`|\[[^\]]+\]|[\w$]+))?)/i.exec(text)
  if (m) return m[1].split('.').pop()?.replace(/^["`[]|["`\]]$/g, '') ?? 'Query results'
  const line = text.split('\n').map((l) => l.trim()).find(Boolean) ?? 'Query results'
  return line.length > 60 ? line.slice(0, 57) + '…' : line
}

/** The share the dialog is open for, or null. */
export const shareRequest = writable(/** @type {ShareSource | null} */ (null))

/** @type {((url: string | null) => void) | null} */
let onShareSettled = null

/**
 * Open the share dialog for `source`. `onSettled` hears the link once it is
 * made, or null if the dialog closes without one (an agent's share_link waits
 * on this).
 * @param {ShareSource} source
 * @param {(url: string | null) => void} [onSettled]
 */
export function requestShare(source, onSettled) {
  settleShare(null)
  onShareSettled = onSettled ?? null
  shareRequest.set(source)
}

/** The dialog's answer: the link, or null. Only the first answer counts. @param {string | null} url */
export function settleShare(url) {
  const cb = onShareSettled
  onShareSettled = null
  cb?.(url)
}

/** Shared links as a tab (or an overlay without a connection). StudioShell opens it. */
export const sharesPageRequest = writable(0)
export function openSharesPage() {
  sharesPageRequest.update((n) => n + 1)
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
 * `onStage` hears each step, so the dialog can say which one it is on.
 * @param {object} payload
 * @param {{ expiresIn: number | null, maxViews: number | null, password?: string | null }} o
 * @param {(stage: 'encrypt' | 'upload') => void} [onStage]
 */
export async function publishShare(payload, o, onStage = () => {}) {
  const p = /** @type {any} */ (payload)
  onStage('encrypt')
  const sealed = await sealShare(payload, { password: o.password })
  onStage('upload')
  const res = await shareCreate(
    { kind: p.kind, expiresIn: o.expiresIn, maxViews: o.maxViews, passwordSalt: sealed.passwordSalt },
    sealed.blob,
  )
  const url = `${res.url}#${sealed.linkKey}`
  rememberShare({ id: res.id, url, title: p.title, kind: p.kind, createdAt: Date.now() })
  return { id: res.id, url, bytes: sealed.blob.length }
}
