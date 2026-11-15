// Where the project keeps its database: an argument, the environment, a .env
// file, or a SQLite file sitting in the folder.

import fs from 'node:fs'
import path from 'node:path'

/** Checked in this order; the first one set wins. */
export const URL_KEYS = [
  'DATABASE_URL',
  'POSTGRES_URL',
  'POSTGRES_PRISMA_URL',
  'POSTGRES_URL_NON_POOLING',
  'DIRECT_URL',
  'PG_URL',
  'PGURL',
  'MYSQL_URL',
  'MYSQL_DATABASE_URL',
  'DB_URL',
  'SQLITE_URL',
  'DB_FILE_NAME',
]

/** Later files override earlier ones, as in Next.js and Vite. */
export const ENV_FILES = ['.env', '.env.development', '.env.local', '.env.development.local']

const DB_FILE = /\.(db|sqlite|sqlite3|db3)$/i

/**
 * A dotenv file's KEY=value pairs. Handles `export`, quotes, `#` comments and
 * `${VAR}` references to keys defined earlier in the same file set.
 * @param {string} text
 * @param {Record<string, string>} [into]
 */
export function parseEnv(text, into = {}) {
  for (const raw of text.split(/\r?\n/)) {
    const m = /^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$/.exec(raw)
    if (!m) continue
    let v = m[2].trim()
    const q = v[0]
    if ((q === '"' || q === "'" || q === '`') && v.lastIndexOf(q) > 0) {
      v = v.slice(1, v.lastIndexOf(q))
      if (q === '"') v = v.replace(/\\n/g, '\n')
    } else {
      v = v.replace(/\s+#.*$/, '')
    }
    if (q !== "'") v = v.replace(/\$\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (_, k) => into[k] ?? process.env[k] ?? '')
    into[m[1]] = v
  }
  return into
}

/** @param {string} dir */
export function readEnvFiles(dir) {
  /** @type {Record<string, string>} */
  const env = {}
  /** @type {string[]} */
  const read = []
  for (const f of ENV_FILES) {
    const p = path.join(dir, f)
    try {
      parseEnv(fs.readFileSync(p, 'utf8'), env)
      read.push(f)
    } catch {
      // not there
    }
  }
  return { env, read }
}

/**
 * @typedef {{ uri: string, source: string }} Found
 */

/**
 * The connection to open, and where it came from.
 * @param {{ arg?: string, cwd: string, env?: Record<string, string | undefined> }} o
 * @returns {Found | null}
 */
export function findConnection({ arg, cwd, env = process.env }) {
  if (arg) {
    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(arg)) return { uri: arg, source: 'argument' }
    const file = path.resolve(cwd, arg)
    if (fs.existsSync(file)) return { uri: file, source: 'argument' }
    throw new Error(`Not a connection URL or a file: ${arg}`)
  }
  for (const k of URL_KEYS) {
    const v = env[k]?.trim()
    if (v) return { uri: resolveFileUri(v, cwd), source: `$${k}` }
  }
  const { env: fileEnv, read } = readEnvFiles(cwd)
  for (const k of URL_KEYS) {
    const v = fileEnv[k]?.trim()
    if (v) {
      const from = [...read].reverse().find((f) => {
        try {
          return k in parseEnv(fs.readFileSync(path.join(cwd, f), 'utf8'))
        } catch {
          return false
        }
      })
      return { uri: resolveFileUri(v, cwd), source: `${k} in ${from ?? '.env'}` }
    }
  }
  const files = listDbFiles(cwd)
  if (files.length) return { uri: path.join(cwd, files[0]), source: files[0] }
  return null
}

/**
 * `file:./dev.db` (Prisma, Drizzle, libSQL) and bare relative paths become
 * absolute, so the app and the bridge read the same file from anywhere.
 * @param {string} v
 * @param {string} cwd
 */
export function resolveFileUri(v, cwd) {
  const m = /^(?:file|sqlite3?):(?:\/\/)?(.+)$/i.exec(v)
  if (m) return path.resolve(cwd, m[1].replace(/\?.*$/, ''))
  if (!/^[a-z][a-z0-9+.-]*:\/\//i.test(v) && DB_FILE.test(v)) return path.resolve(cwd, v)
  return v
}

/** @param {string} dir */
function listDbFiles(dir) {
  const out = []
  for (const sub of ['', 'prisma', 'data', 'db']) {
    try {
      for (const f of fs.readdirSync(path.join(dir, sub))) {
        if (DB_FILE.test(f)) out.push(path.join(sub, f))
      }
    } catch {
      // not there
    }
  }
  return out
}

/**
 * Which engine a connection string is for, from the bridge's point of view.
 * @param {string} uri
 * @returns {'postgres' | 'mysql' | 'sqlite' | null}
 */
export function engineOf(uri) {
  const scheme = /^([a-z][a-z0-9+.-]*):\/\//i.exec(uri)?.[1]?.toLowerCase()
  if (!scheme) return DB_FILE.test(uri) || fs.existsSync(uri) ? 'sqlite' : null
  if (['postgres', 'postgresql', 'pg', 'cockroachdb', 'neon'].includes(scheme)) return 'postgres'
  if (['mysql', 'mariadb', 'mysql2'].includes(scheme)) return 'mysql'
  if (['sqlite', 'sqlite3', 'file'].includes(scheme)) return 'sqlite'
  return null
}

/**
 * The URL with its password replaced, for printing.
 * @param {string} uri
 */
export function redact(uri) {
  // The host can't contain `@`, so the last one ends the login - a typed
  // password often has a raw `@` in it.
  const m = /^([a-z][a-z0-9+.-]*:\/\/)(.*)$/is.exec(uri)
  if (!m) return uri
  const at = m[2].lastIndexOf('@')
  const colon = m[2].indexOf(':')
  if (at < 0 || colon < 0 || colon > at) return uri
  return `${m[1]}${m[2].slice(0, colon)}:****${m[2].slice(at)}`
}
