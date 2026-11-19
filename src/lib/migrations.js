// The Migrations page's logic: what Prisma or Drizzle recorded in the
// database, merged with the migration files in the project folder, as one
// list with a status per migration. Pure: no IPC, no Svelte.
//
// Prisma keeps a row per attempt in `_prisma_migrations`: finished_at set =
// applied; neither finished_at nor rolled_back_at = failed (Prisma refuses to
// go on until it's resolved); rolled_back_at = rolled back. Its checksum is the
// SHA-256 of migration.sql.
// Drizzle keeps `__drizzle_migrations` (id, hash, created_at): a migration
// counts as applied when its journal `when` is at or before the newest
// created_at, which is how Drizzle's migrator decides; `hash` is the SHA-256
// of the .sql file. Drizzle records no failures (a failed run rolls back).
// Cloudflare D1 projects apply the same files with `wrangler d1 migrations
// apply`, which keeps its own `d1_migrations` (id, name, applied_at), `name`
// being the file name: every .sql file in the folder, in the journal or not.

/**
 * @typedef {'applied' | 'pending' | 'failed' | 'rolled-back'} MigrationStatus
 * @typedef {{
 *   name: string,
 *   status: MigrationStatus,
 *   appliedAt: number | null,
 *   startedAt: number | null,
 *   durationMs: number | null,
 *   steps: number | null,
 *   logs: string | null,
 *   sql: string | null,
 *   changed: boolean,
 *   missingFile: boolean,
 *   attempts: number,
 * }} Migration
 * @typedef {{ name: string, sql: string, checksum: string, when: number | null }} MigrationFile
 */

/** @param {unknown} v @returns {number | null} */
export function toMs(v) {
  if (v == null || v === '') return null
  if (typeof v === 'number') return v < 1e11 ? v * 1000 : v
  if (typeof v === 'bigint') return Number(v)
  const s = String(v).trim()
  if (/^\d+$/.test(s)) return toMs(Number(s))
  // "2024-01-02 10:00:00.123" from MySQL/SQLite has no zone: Prisma writes UTC.
  const iso = /^\d{4}-\d{2}-\d{2} \d/.test(s) && !/[zZ]|[+-]\d{2}:?\d{2}$/.test(s) ? `${s.replace(' ', 'T')}Z` : s
  const t = Date.parse(iso)
  return Number.isNaN(t) ? null : t
}

/** Rows as objects keyed by column name. @param {{ columns?: any[], rows?: unknown[][] }} r */
export function rowsOf(r) {
  const cols = (r?.columns ?? []).map((c) => String(c?.name ?? c))
  return (r?.rows ?? []).map((row) => Object.fromEntries(cols.map((c, i) => [c, row[i]])))
}

/**
 * @param {Record<string, any>[]} rows `_prisma_migrations`
 * @param {MigrationFile[] | null} files null when no project folder is set
 * @returns {Migration[]}
 */
export function mergePrisma(rows, files) {
  /** @type {Map<string, Record<string, any>[]>} */
  const byName = new Map()
  for (const r of rows) {
    const n = String(r.migration_name ?? '')
    if (!byName.has(n)) byName.set(n, [])
    byName.get(n)?.push(r)
  }
  const fileByName = new Map((files ?? []).map((f) => [f.name, f]))
  const names = [...new Set([...byName.keys(), ...fileByName.keys()])].sort()
  return names.map((name) => {
    const attempts = (byName.get(name) ?? []).slice().sort((a, b) => (toMs(a.started_at) ?? 0) - (toMs(b.started_at) ?? 0))
    const last = attempts[attempts.length - 1]
    const file = fileByName.get(name) ?? null
    /** @type {MigrationStatus} */
    let status = 'pending'
    if (last) status = last.rolled_back_at ? 'rolled-back' : last.finished_at ? 'applied' : 'failed'
    const started = last ? toMs(last.started_at) : null
    const finished = last ? toMs(last.finished_at) : null
    return {
      name,
      status,
      appliedAt: status === 'applied' ? finished : null,
      startedAt: started,
      durationMs: started != null && finished != null ? Math.max(0, finished - started) : null,
      steps: last?.applied_steps_count != null ? Number(last.applied_steps_count) : null,
      logs: last?.logs ? String(last.logs) : null,
      sql: file?.sql ?? null,
      changed: !!(file && last && status === 'applied' && last.checksum && file.checksum && last.checksum !== file.checksum),
      missingFile: !!files && !file,
      attempts: attempts.length,
    }
  })
}

/**
 * @param {Record<string, any>[]} rows `__drizzle_migrations`
 * @param {MigrationFile[] | null} files in journal order; null when no folder is set
 * @returns {Migration[]}
 */
export function mergeDrizzle(rows, files) {
  const applied = rows
    .map((r) => ({ hash: String(r.hash ?? ''), at: toMs(r.created_at) }))
    .sort((a, b) => (a.at ?? 0) - (b.at ?? 0))
  const newest = applied.reduce((m, r) => Math.max(m, r.at ?? 0), 0)
  const hashes = new Set(applied.map((r) => r.hash))
  if (!files) {
    return applied.map((r, i) => ({
      name: `#${i + 1} · ${r.hash.slice(0, 12)}`,
      status: /** @type {MigrationStatus} */ ('applied'),
      appliedAt: r.at,
      startedAt: null,
      durationMs: null,
      steps: null,
      logs: null,
      sql: null,
      changed: false,
      missingFile: false,
      attempts: 1,
    }))
  }
  const fileHashes = new Set(files.map((f) => f.checksum))
  /** @type {Migration[]} */
  const out = files.map((f) => {
    const byHash = hashes.has(f.checksum)
    const byTime = f.when != null && f.when <= newest
    const ran = byHash || byTime
    const row = applied.find((r) => r.hash === f.checksum) ?? (byTime ? applied.find((r) => r.at === f.when) : undefined)
    return {
      name: f.name,
      status: ran ? 'applied' : 'pending',
      appliedAt: ran ? row?.at ?? f.when : null,
      startedAt: null,
      durationMs: null,
      steps: null,
      logs: null,
      sql: f.sql,
      // Ran (by its timestamp) but no row has this file's hash: edited since.
      changed: ran && !byHash && !!f.checksum,
      missingFile: !f.checksum,
      attempts: ran ? 1 : 0,
    }
  })
  // Rows whose file is no longer in the project.
  applied.filter((r) => !fileHashes.has(r.hash) && !files.some((f) => f.when != null && f.when === r.at)).forEach((r) => {
    out.push({ name: `Not in the project · ${r.hash.slice(0, 12)}`, status: 'applied', appliedAt: r.at, startedAt: null, durationMs: null, steps: null, logs: null, sql: null, changed: false, missingFile: true, attempts: 1 })
  })
  return out
}

/**
 * @param {Record<string, any>[]} rows `d1_migrations`
 * @param {MigrationFile[] | null} files every .sql file in the folder (name with `.sql`); null when no folder is set
 * @returns {Migration[]}
 */
export function mergeD1(rows, files) {
  const strip = (/** @type {string} */ n) => n.replace(/\.sql$/i, '')
  /** @type {Map<string, Record<string, any>>} */
  const ran = new Map(rows.map((r) => [strip(String(r.name ?? '')), r]))
  const fileByName = new Map((files ?? []).map((f) => [strip(f.name), f]))
  const names = [...new Set([...fileByName.keys(), ...ran.keys()])].filter(Boolean).sort()
  return names.map((name) => {
    const row = ran.get(name)
    const file = fileByName.get(name)
    return {
      name,
      status: /** @type {MigrationStatus} */ (row ? 'applied' : 'pending'),
      appliedAt: row ? toMs(row.applied_at) : null,
      startedAt: null,
      durationMs: null,
      steps: null,
      logs: null,
      sql: file?.sql ?? null,
      changed: false,
      missingFile: !!files && !file,
      attempts: row ? 1 : 0,
    }
  })
}

/** @param {Migration[]} list */
export function counts(list) {
  const c = { applied: 0, pending: 0, failed: 0, rolledBack: 0, changed: 0 }
  for (const m of list) {
    if (m.status === 'applied') c.applied++
    else if (m.status === 'pending') c.pending++
    else if (m.status === 'failed') c.failed++
    else c.rolledBack++
    if (m.changed) c.changed++
  }
  return c
}

/**
 * The SELECTs that read each tool's table, per engine family. Prisma's table
 * lives in the connection's schema; Drizzle's in `drizzle` on Postgres.
 * @param {'prisma' | 'drizzle' | 'd1'} tool @param {string} family @param {string | null} schema
 * @returns {string[]} tried in order until one works
 */
export function migrationQueries(tool, family, schema) {
  if (tool === 'd1') return family === 'd1' || family === 'sqlite' ? ['SELECT id, name, applied_at FROM "d1_migrations" ORDER BY id'] : []
  const q = family === 'mysql' ? (/** @type {string} */ s) => `\`${s.replace(/`/g, '``')}\`` : (/** @type {string} */ s) => `"${s.replace(/"/g, '""')}"`
  if (tool === 'prisma') {
    const cols = 'id, checksum, finished_at, migration_name, logs, rolled_back_at, started_at, applied_steps_count'
    const table = q('_prisma_migrations')
    const out = []
    if (family === 'postgres' && schema) out.push(`SELECT ${cols} FROM ${q(schema)}.${table} ORDER BY started_at`)
    out.push(`SELECT ${cols} FROM ${table} ORDER BY started_at`)
    return out
  }
  const table = q('__drizzle_migrations')
  const out = []
  if (family === 'postgres') out.push(`SELECT id, hash, created_at FROM ${q('drizzle')}.${table} ORDER BY created_at`)
  out.push(`SELECT id, hash, created_at FROM ${table} ORDER BY created_at`)
  return out
}

/** Whether an error just means "this table isn't there". @param {unknown} e */
export function isMissingTable(e) {
  return /does not exist|doesn't exist|no such table|not found|unknown table|invalid object name/i.test(String(e))
}
