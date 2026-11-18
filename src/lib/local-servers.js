// Local database servers (src-tauri/src/local_servers.rs): the IPC calls, the
// engines and versions on offer, and the connection a running server opens as.

import { invoke } from '@tauri-apps/api/core'

/**
 * @typedef {{ id: string, name: string, engine: string, version: string, port: number, runtime: 'docker' | 'native', createdAt: number, running: boolean, problem: string | null }} LocalServer
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
/** @param {string} id @param {{ name?: string, port?: number }} patch */
export const updateServer = (id, patch) => invoke('local_server_update', { id, name: patch.name ?? null, port: patch.port ?? null })
/** @param {string} id @returns {Promise<string>} */
export const serverLogs = (id) => invoke('local_server_logs', { id })
/** @param {number} from @returns {Promise<number>} */
export const freePort = (from) => invoke('local_server_free_port', { from })
/** @returns {Promise<BrewService[]>} */
export const brewServices = () => invoke('brew_services_list')
/** @param {string} name @param {'start' | 'stop'} action */
export const brewAction = (name, action) => invoke('brew_service_action', { name, action })
