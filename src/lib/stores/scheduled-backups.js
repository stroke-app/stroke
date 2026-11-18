import { invoke } from '@tauri-apps/api/core'
import { writable, get } from 'svelte/store'
import { Cron } from 'croner'
import { loadSavedConnections } from './connections.js'
import { hasPaidPro } from './license.js'
import { isSignedIn } from './account.js'

/**
 * Scheduled backups (Pro). Jobs live on stroke.click; this app runs them,
 * because only it has the database credentials. Every minute the scheduler
 * looks for a job whose schedule tick has passed (or a "Run now" from the
 * website) on a connection saved on this device, claims the tick on the
 * server (so two devices never both run it), and runs it in Rust
 * (src-tauri/src/cloud_backup.rs).
 *
 * @typedef {{ id: string, slot: string, status: 'running'|'success'|'failed', destination: 'stroke'|'s3',
 *   object_key: string|null, bytes: number, tables: number, rows: number, error: string|null,
 *   device_name: string|null, started_at: number, finished_at: number|null }} BackupRun
 * @typedef {{ id: string, connection_id: string, label: string, driver: string, cron: string, timezone: string,
 *   destination: 'stroke'|'s3', s3_bucket: string|null, s3_endpoint: string|null, s3_prefix: string|null,
 *   keep: number, enabled: boolean, run_requested_at: number|null, created_at: number,
 *   next_run_at: number|null, runs: BackupRun[] }} BackupJob
 * @typedef {{ jobs: BackupJob[], max_jobs: number, used_bytes: number, quota_bytes: number, pro: boolean }} BackupOverview
 */

/** Engines the backup engine can dump. Keep in sync with stroke-web's DRIVERS. */
export const SCHEDULE_DRIVERS = new Set(['postgres', 'mysql', 'sqlite', 'd1', 'libsql', 'duckdb', 'mssql'])

/** @type {import('svelte/store').Writable<BackupOverview|null>} */
export const backupOverview = writable(null)
/** @type {import('svelte/store').Writable<string|null>} */
export const backupLoadError = writable(null)
/** The job running on this device right now, if any. */
export const runningJobId = writable(/** @type {string|null} */ (null))

/** @param {BackupOverview} o */
const setOverview = (o) => {
  backupOverview.set(o)
  backupLoadError.set(null)
  return o
}

export async function refreshBackupJobs() {
  try {
    return setOverview(/** @type {BackupOverview} */ (await invoke('backup_jobs_list')))
  } catch (e) {
    backupLoadError.set(String(e))
    return null
  }
}

/** @param {Record<string, unknown>} job */
export async function saveBackupJob(job) {
  return setOverview(/** @type {BackupOverview} */ (await invoke('backup_job_save', { job })))
}

/** @param {string} id @param {{ enabled?: boolean, run_now?: boolean }} patch */
export async function patchBackupJob(id, patch) {
  const o = setOverview(/** @type {BackupOverview} */ (await invoke('backup_job_patch', { id, patch })))
  if (patch.run_now) void tick()
  return o
}

/** @param {string} id */
export async function deleteBackupJob(id) {
  return setOverview(/** @type {BackupOverview} */ (await invoke('backup_job_delete', { id })))
}

/**
 * @param {string} jobId
 * @param {{ accessKeyId: string, secretAccessKey: string, region: string }} keys
 */
export async function saveS3Keys(jobId, keys) {
  await invoke('backup_s3_save', { jobId, accessKeyId: keys.accessKeyId, secretAccessKey: keys.secretAccessKey, region: keys.region })
}

/** The region saved with this device's S3 keys for the job, or null when it has none. */
export async function s3KeysRegion(/** @type {string} */ jobId) {
  try {
    return /** @type {string|null} */ (await invoke('backup_s3_status', { jobId }))
  } catch {
    return null
  }
}

/**
 * The tick a job should run for now, or null. A "Run now" request wins.
 * Missed ticks (the app was closed) collapse into one run for the latest.
 * @param {BackupJob} job
 * @param {number} [now]
 */
export function dueSlot(job, now = Date.now()) {
  // "Run now" works on a paused job too: it's an explicit ask.
  if (job.run_requested_at) return `manual:${job.run_requested_at}`
  if (!job.enabled) return null
  /** @type {Cron} */
  let cron
  try {
    cron = new Cron(job.cron, { timezone: job.timezone || 'UTC', paused: true })
  } catch {
    return null
  }
  const lastTick = Math.max(0, ...job.runs.filter((r) => !r.slot.startsWith('manual:')).map((r) => Date.parse(r.slot) || 0))
  const baseline = Math.max(job.created_at, lastTick)
  let tick = cron.nextRun(new Date(baseline))
  if (!tick || tick.getTime() > now) return null
  for (let i = 0; i < 5000; i++) {
    const next = cron.nextRun(tick)
    if (!next || next.getTime() > now) break
    tick = next
  }
  return tick.toISOString()
}

/** Plain words for the common schedules; the cron itself otherwise. @param {string} cron */
export function describeCron(cron) {
  const [min, hour, dom, mon, dow] = String(cron).trim().split(/\s+/)
  const num = /^\d+$/
  const at = (/** @type {string} */ h, /** @type {string} */ m) => `${h.padStart(2, '0')}:${m.padStart(2, '0')}`
  const days = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']
  if (num.test(min) && hour === '*' && dom === '*' && mon === '*' && dow === '*') return `Every hour at :${min.padStart(2, '0')}`
  if (num.test(min) && /^\*\/\d+$/.test(hour ?? '') && dom === '*' && mon === '*' && dow === '*') return `Every ${hour.slice(2)} hours`
  if (num.test(min) && num.test(hour) && mon === '*') {
    if (dom === '*' && dow === '*') return `Every day at ${at(hour, min)}`
    if (dom === '*' && num.test(dow) && days[Number(dow) % 7]) return `Every ${days[Number(dow) % 7]} at ${at(hour, min)}`
    if (num.test(dom) && dow === '*') return `Day ${dom} of every month at ${at(hour, min)}`
  }
  return cron
}

/** @param {number} n */
export function formatBytes(n) {
  if (n < 1024) return `${n} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let v = n / 1024
  let i = 0
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++ }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`
}

// ── Scheduler ────────────────────────────────────────────────────────────────

const TICK_MS = 60_000
/** Re-read the jobs at most this often (a "Run now" from the website shows up within it). */
const REFRESH_MS = 2 * 60_000
/** Ticks this device already tried (ran, failed, or lost to another device). */
const handled = new Set()
let started = false
let busy = false
let lastRefresh = 0

async function tick() {
  if (busy || !get(isSignedIn) || !get(hasPaidPro)) return
  busy = true
  try {
    if (Date.now() - lastRefresh > REFRESH_MS || !get(backupOverview)) {
      lastRefresh = Date.now()
      await refreshBackupJobs()
    }
    const overview = get(backupOverview)
    if (!overview?.pro) return
    const saved = new Map(loadSavedConnections().map((c) => [c.id, c]))
    for (const job of overview.jobs) {
      const conn = saved.get(job.connection_id)
      // Another device's connection: that device runs it.
      if (!conn) continue
      const slot = dueSlot(job)
      if (!slot || handled.has(`${job.id}|${slot}`)) continue
      handled.add(`${job.id}|${slot}`)
      await runJob(job, conn, slot)
      lastRefresh = Date.now()
      await refreshBackupJobs()
    }
  } finally {
    busy = false
  }
}

/**
 * @param {BackupJob} job
 * @param {object} conn the saved connection, secrets included
 * @param {string} slot
 */
async function runJob(job, conn, slot) {
  const { toast } = await import('$lib/components/ui/sonner/toast.svelte.js')
  runningJobId.set(job.id)
  try {
    const out = /** @type {{ status: string, message: string, bytes: number }} */ (
      await invoke('backup_job_run', { job, config: conn, slot })
    )
    if (out.status === 'success') {
      toast.success(`Backed up ${job.label}`, { description: `${out.message} ${formatBytes(out.bytes)} compressed.` })
    } else if (out.status === 'failed') {
      toast.error(`Scheduled backup of ${job.label} failed`, { description: out.message, duration: 15000 })
    }
  } catch (e) {
    toast.error(`Scheduled backup of ${job.label} failed`, { description: String(e), duration: 15000 })
  } finally {
    runningJobId.set(null)
  }
}

/** Start the scheduler once, after sign-in has been settled. */
export function startBackupScheduler() {
  if (started) return
  started = true
  setTimeout(() => void tick(), 15_000)
  setInterval(() => void tick(), TICK_MS)
}
