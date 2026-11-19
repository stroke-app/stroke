// Local database servers (src-tauri/src/local_servers.rs): the IPC calls, the
// engines and versions on offer, and the connection a running server opens as.

import { invoke } from '@tauri-apps/api/core'

/**
 * @typedef {{ id: string, name: string, engine: string, version: string, port: number, runtime: 'docker' | 'native', createdAt: number, autostart: boolean, running: boolean, problem: string | null, startedAt: number | null, diskBytes: number | null, dataDir: string | null }} LocalServer
 * @typedef {{ engine: string, port: number, source: 'docker' | 'process', name: string, image: string | null }} FoundServer
 * @typedef {{ docker: boolean, dockerInstalled: boolean, brew: boolean, native: string[] }} LocalEnv
 * @typedef {{ name: string, status: string, running: boolean }} BrewService
 */

/** Engines, their versions (newest first) and default port. */
export const ENGINES = [
  { id: 'postgres', label: 'PostgreSQL', short: 'Pg', color: '#336791', port: 5432, versions: ['18', '17', '16', '15', '14', '13'] },
  { id: 'mysql', label: 'MySQL', short: 'My', color: '#f29111', port: 3306, versions: ['9.4', '8.4', '8.0'] },
  { id: 'mariadb', label: 'MariaDB', short: 'Ma', color: '#c0765a', port: 3306, versions: ['11.8', '11.4', '10.11'] },
  { id: 'redis', label: 'Redis', short: 'Re', color: '#d82c20', port: 6379, versions: ['8.2', '7.4', '7.2'] },
  { id: 'mongodb', label: 'MongoDB', short: 'Mo', color: '#47a248', port: 27017, versions: ['8.0', '7.0', '6.0'] },
]

/** @param {string} id */
export const engineOf = (id) => ENGINES.find((e) => e.id === id) ?? ENGINES[0]

/**
 * The first port from the engine's default up that no server here uses.
 * @param {string} engine @param {{ port: number }[]} servers
 */
export function nextPort(engine, servers) {
  const taken = new Set(servers.map((s) => s.port))
  let p = engineOf(engine).port
  while (taken.has(p)) p++
  return p
}

/**
 * The saved connection a server opens as: 127.0.0.1, its port, the engine's
 * default superuser and no password (local servers run with trust auth).
 * @param {LocalServer} s
 */
export function connectionFor(s) {
  const base = { id: `local-${s.id}`, name: s.name, host: '127.0.0.1', port: s.port, environment: /** @type {'dev'} */ ('dev') }
  switch (s.engine) {
    case 'postgres': return { ...base, type: 'postgres', user: 'postgres', password: '', database: 'postgres' }
    case 'mysql':
    case 'mariadb': return { ...base, type: s.engine, user: 'root', password: '', database: '' }
    case 'redis': return { ...base, type: 'redis', db: 0, password: '' }
    default: return { ...base, type: 'mongodb', uri: `mongodb://127.0.0.1:${s.port}/?directConnection=true`, database: '' }
  }
}

/** @returns {Promise<LocalEnv>} */
export const localEnv = () => invoke('local_servers_env')
/** @returns {Promise<LocalServer[]>} */
export const listServers = () => invoke('local_servers_list')
/** @param {{ name: string, engine: string, version: string, port: number, runtime: string }} spec */
export const createServer = (spec) => invoke('local_server_create', { spec })
/** @param {string} id */
export const startServer = (id) => invoke('local_server_start', { id })
/** @param {string} id */
export const stopServer = (id) => invoke('local_server_stop', { id })
/** @param {string} id */
export const deleteServer = (id) => invoke('local_server_delete', { id })
/** @param {string} id @param {{ name?: string, port?: number, autostart?: boolean }} patch */
export const updateServer = (id, patch) => invoke('local_server_update', { id, name: patch.name ?? null, port: patch.port ?? null, autostart: patch.autostart ?? null })
/** Database servers on this computer that Stroke didn't make. @returns {Promise<FoundServer[]>} */
export const discoverServers = () => invoke('local_servers_discover')
/** @param {string} id */
export const revealServer = (id) => invoke('local_server_reveal', { id })
/** Open the Local servers window; `from` is the window that connects. @param {string} from */
export const openLocalServersWindow = (from) => invoke('open_local_servers_window', { from })
/** Hand a connection to an app window. @param {string} target @param {any} connection */
export const connectIn = (target, connection) => invoke('local_server_connect_in', { target, connection })

/**
 * The URL a server is reached at, as an app's DATABASE_URL would hold it.
 * @param {{ engine: string, port: number }} s
 */
export function connectionUrl(s) {
  switch (s.engine) {
    case 'postgres': return `postgresql://postgres@127.0.0.1:${s.port}/postgres`
    case 'mysql':
    case 'mariadb': return `mysql://root@127.0.0.1:${s.port}/`
    case 'redis': return `redis://127.0.0.1:${s.port}`
    default: return `mongodb://127.0.0.1:${s.port}/?directConnection=true`
  }
}

/** The env var name apps usually read for an engine. @param {string} engine */
export const envName = (engine) => (engine === 'redis' ? 'REDIS_URL' : engine === 'mongodb' ? 'MONGODB_URI' : 'DATABASE_URL')

/** 1536 → "1.5 KB". @param {number | null | undefined} n */
export function formatBytes(n) {
  if (n == null) return ''
  const u = ['B', 'KB', 'MB', 'GB', 'TB']
  let i = 0
  let v = n
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++ }
  return `${v < 10 && i ? v.toFixed(1) : Math.round(v)} ${u[i]}`
}

/** How long since `ms`, short: "3 min", "2 h 5 min", "4 days". @param {number | null | undefined} ms @param {number} [now] */
export function uptime(ms, now = Date.now()) {
  if (!ms) return ''
  const s = Math.max(0, Math.round((now - ms) / 1000))
  if (s < 60) return 'under a minute'
  const m = Math.floor(s / 60)
  if (m < 60) return `${m} min`
  const h = Math.floor(m / 60)
  if (h < 24) return m % 60 ? `${h} h ${m % 60} min` : `${h} h`
  const d = Math.floor(h / 24)
  return `${d} day${d === 1 ? '' : 's'}`
}
/** @param {string} id @returns {Promise<string>} */
export const serverLogs = (id) => invoke('local_server_logs', { id })
/** @param {number} from @returns {Promise<number>} */
export const freePort = (from) => invoke('local_server_free_port', { from })
/** @returns {Promise<BrewService[]>} */
export const brewServices = () => invoke('brew_services_list')
/** @param {string} name @param {'start' | 'stop'} action */
export const brewAction = (name, action) => invoke('brew_service_action', { name, action })
