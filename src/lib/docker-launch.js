// Starting a database in Docker, independent of the window that asked for it.
// The pull and start-up run here; the Docker dialog shows the state while it is
// open, and a toast carries it once the dialog is closed, so a slow first pull
// never has to be watched. When the database is ready it connects if the
// dialog is still open, and otherwise the toast offers Connect.

import { writable, get } from 'svelte/store'
import { listen } from '@tauri-apps/api/event'
import { dockerCheck, dockerRunDb } from '$lib/api.js'
import { toast } from '$lib/components/ui/sonner/toast.svelte.js'

/**
 * @typedef {'checking' | 'pulling' | 'starting' | 'waiting' | 'ready' | 'error'} LaunchPhase
 * @typedef {{
 *   key: string,
 *   id: string,
 *   label: string,
 *   icon: string,
 *   image: string,
 *   phase: LaunchPhase,
 *   progress: number,
 *   line: string,
 *   logs: string[],
 *   layers: { total: number, done: number },
 *   error: string,
 *   conn: Record<string, any> | null,
 *   toastId: number | null,
 * }} Launch
 */

/** Every launch this session, newest first. @type {import('svelte/store').Writable<Launch[]>} */
export const dockerLaunches = writable([])

/** How far along each layer state is, for the overall pull fraction. */
const LAYER_WEIGHT = /** @type {Record<string, number>} */ ({
  pending: 0,
  waiting: 0,
  downloading: 0.35,
  verifying: 0.6,
  downloaded: 0.65,
  extracting: 0.85,
  done: 1,
})

/**
 * Read one line of `docker pull` (no TTY: one status per layer per line) into
 * the layer table. Returns true when the line was a layer status.
 * @param {Map<string, string>} layers
 * @param {string} line
 */
export function readPullLine(layers, line) {
  const m = /^([0-9a-f]{12}): (.+)$/.exec(line.trim())
  if (!m) return false
  const [, id, status] = m
  const s = status.toLowerCase()
  const state =
    s.startsWith('already exists') || s.startsWith('pull complete') ? 'done'
    : s.startsWith('extracting') ? 'extracting'
    : s.startsWith('download complete') ? 'downloaded'
    : s.startsWith('verifying') ? 'verifying'
    : s.startsWith('downloading') ? 'downloading'
    : s.startsWith('waiting') ? 'waiting'
    : 'pending'
  // Never step a layer backwards (a late "Waiting" after "Downloading").
  const prev = layers.get(id)
  if (!prev || LAYER_WEIGHT[state] >= LAYER_WEIGHT[prev]) layers.set(id, state)
  return true
}

/** The pull's fraction (0-1) from the layer table, or -1 while no layer is known. @param {Map<string, string>} layers */
export function pullFraction(layers) {
  if (!layers.size) return -1
  let sum = 0
  for (const st of layers.values()) sum += LAYER_WEIGHT[st] ?? 0
  return sum / layers.size
}

/** @param {string} key @param {Partial<Launch>} patch */
function patch(key, patch) {
  dockerLaunches.update((all) => all.map((l) => (l.key === key ? { ...l, ...patch } : l)))
}
/** @param {string} key */
function find(key) {
  return get(dockerLaunches).find((l) => l.key === key) ?? null
}

/** Overall progress: the pull is most of it, then start-up. @param {LaunchPhase} phase @param {number} pull */
function overall(phase, pull) {
  if (phase === 'checking') return -1
  if (phase === 'pulling') return pull < 0 ? -1 : pull * 0.8
  if (phase === 'starting') return 0.84
  if (phase === 'waiting') return 0.92
  return 1
}

const PHASE_TITLE = /** @type {Record<LaunchPhase, string>} */ ({
  checking: 'Checking Docker',
  pulling: 'Pulling',
  starting: 'Starting',
  waiting: 'Starting',
  ready: 'Ready',
  error: 'Failed',
})

/**
 * Show (or refresh) the launch's toast: only while the dialog is not showing it.
 * @param {string} key
 */
function syncToast(key) {
  const l = find(key)
  if (!l || l.toastId == null) return
  const title = `${PHASE_TITLE[l.phase]} ${l.label}`
  const description =
    l.phase === 'pulling' && l.layers.total
      ? `${l.layers.done} of ${l.layers.total} layers`
      : l.phase === 'waiting'
        ? 'Waiting for it to accept connections'
        : l.line
  toast.update(l.toastId, { title, description, progress: l.progress })
}

/**
 * Hand a running launch to a toast (the dialog closed while it runs).
 * @param {string} key
 * @param {(conn: Record<string, any>) => void} onconnect
 */
export function backgroundLaunch(key, onconnect) {
  const l = find(key)
  if (!l || l.toastId != null) return
  if (l.phase === 'ready' && l.conn) {
    readyToast(l, onconnect)
    return
  }
  if (l.phase === 'error') return
  const id = toast.loading(`${PHASE_TITLE[l.phase]} ${l.label}`, {
    description: l.line,
    progress: l.progress,
  })
  patch(key, { toastId: id })
  _onconnect.set(key, onconnect)
}

/** @type {Map<string, (conn: Record<string, any>) => void>} */
const _onconnect = new Map()

/** @param {Launch} l @param {(conn: Record<string, any>) => void} onconnect */
function readyToast(l, onconnect) {
  const action = { label: 'Connect', onClick: () => { if (l.conn) onconnect(l.conn) } }
  const description = `${l.image}, running. Its connection is saved.`
  if (l.toastId != null) {
    toast.update(l.toastId, { type: 'success', title: `${l.label} is ready`, description, progress: null, action, duration: 30000 })
  } else {
    toast.success(`${l.label} is ready`, { description, action, duration: 30000 })
  }
}

/**
 * Start a database from the catalog. Resolves with the connection, or throws
 * with Docker's error. While `inDialog()` is true the dialog shows progress;
 * after that a toast does (see backgroundLaunch).
 * @param {{ id: string, label: string, icon: string, image: string }} template
 * @param {{ inDialog: () => boolean, onconnect: (conn: Record<string, any>) => void }} o
 */
export async function launchDocker(template, o) {
  const key = crypto.randomUUID()
  /** @type {Launch} */
  const launch = {
    key, id: template.id, label: template.label, icon: template.icon, image: template.image,
    phase: 'checking', progress: -1, line: 'Checking Docker', logs: [], layers: { total: 0, done: 0 },
    error: '', conn: null, toastId: null,
  }
  dockerLaunches.update((all) => [launch, ...all.filter((l) => l.phase !== 'ready' && l.phase !== 'error')])
  /** @type {Map<string, string>} */
  const layers = new Map()
  let phase = /** @type {LaunchPhase} */ ('checking')
  const unlisten = await listen(`docker-log:${key}`, (event) => {
    const line = String(/** @type {{ line: string }} */ (event.payload).line ?? '')
    const isLayer = readPullLine(layers, line)
    if (/^Pulling /.test(line) || isLayer) phase = 'pulling'
    else if (/Starting container/.test(line)) phase = 'starting'
    else if (/Waiting for the database/.test(line)) phase = 'waiting'
    const cur = find(key)
    const done = [...layers.values()].filter((s) => s === 'done').length
    patch(key, {
      phase,
      progress: overall(phase, pullFraction(layers)),
      line: isLayer ? cur?.line ?? line : line,
      logs: [...(cur?.logs ?? []).slice(-400), line],
      layers: { total: layers.size, done },
    })
    syncToast(key)
  })
  try {
    await dockerCheck()
    phase = 'pulling'
    patch(key, { phase, line: `Pulling ${template.image}` })
    syncToast(key)
    const conn = await dockerRunDb(template.id, key)
    patch(key, { phase: 'ready', progress: 1, conn, line: 'Ready' })
    const l = find(key)
    if (o.inDialog() && l?.toastId == null) {
      o.onconnect(conn)
    } else if (l) {
      readyToast(l, _onconnect.get(key) ?? o.onconnect)
    }
    return conn
  } catch (err) {
    const msg = String(err).replace(/^Error:\s*/, '')
    patch(key, { phase: 'error', error: msg, progress: 0 })
    const l = find(key)
    if (l?.toastId != null) {
      toast.update(l.toastId, { type: 'error', title: `${template.label} didn't start`, description: msg, progress: null, duration: 15000 })
    }
    throw new Error(msg)
  } finally {
    unlisten()
    _onconnect.delete(key)
  }
}

/** The newest launch for a catalog id that is still going (or just finished). @param {string} id */
export function currentLaunch(id) {
  return get(dockerLaunches).find((l) => l.id === id) ?? null
}
