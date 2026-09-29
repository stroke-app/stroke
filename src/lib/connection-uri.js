/** @typedef {{ host: string, port: string, database: string, user: string, password: string, ssl: boolean }} ParsedPostgresUri */
/** @typedef {{ filePath: string }} ParsedSqliteUri */

/**
 * Make the user:password part of a connection URL safe for `new URL`.
 *
 * Passwords get pasted as typed, and a typed password is not URL-encoded: a
 * bare `%` that isn't an escape (`p%ss`) made `decodeURIComponent` throw and
 * the whole import fail, and a raw `#`, `?` or `/` ended the authority early,
 * so everything after it was read as the fragment, query or path and the
 * password came back cut short. The host can never contain `@`, so the LAST `@`
 * is the real split; everything before it is credentials, and every character
 * there that the URL grammar would misread is escaped. Escapes already present
 * (`%25`, `%40`) are left as they are, so a correctly encoded URL is unchanged.
 * @param {string} uri a string with a `scheme://` prefix
 */
function escapeUserinfo(uri) {
  const m = /^([a-z][a-z0-9+.-]*:\/\/)(.*)$/is.exec(uri)
  if (!m) return uri
  const [, scheme, rest] = m
  const at = rest.lastIndexOf('@')
  if (at < 0) return uri
  const fix = (/** @type {string} */ part) =>
    part
      .replace(/%(?![0-9a-f]{2})/gi, '%25')
      .replace(/[#?/@\[\] ]/g, (c) => encodeURIComponent(c))
  const userinfo = rest.slice(0, at)
  const colon = userinfo.indexOf(':')
  const safe = colon < 0 ? fix(userinfo) : `${fix(userinfo.slice(0, colon))}:${fix(userinfo.slice(colon + 1))}`
  return `${scheme}${safe}@${rest.slice(at + 1)}`
}

/** decodeURIComponent that never throws: a malformed escape stays as written. */
function decodePart(/** @type {string} */ s) {
  try {
    return decodeURIComponent(s)
  } catch {
    return s
  }
}

/**
 * @param {string} uri
 * @returns {ParsedPostgresUri | { error: string } | null}
 */
export function parsePostgresUri(uri) {
  const trimmed = uri.trim()
  if (!trimmed) return null

  let normalized = trimmed
  if (/^postgres:\/\//i.test(normalized)) {
    normalized = `postgresql://${normalized.slice(11)}`
  } else if (!/^postgresql:\/\//i.test(normalized)) {
    if (trimmed.includes('@') || /^[^/]+:\d+\//.test(trimmed)) {
      normalized = `postgresql://${trimmed}`
    } else {
      return { error: 'Expected a postgresql:// connection URI' }
    }
  }

  try {
    const url = new URL(escapeUserinfo(normalized))
    const sslmode = url.searchParams.get('sslmode')?.toLowerCase()
    const ssl =
      sslmode === 'require' ||
      sslmode === 'verify-ca' ||
      sslmode === 'verify-full' ||
      url.searchParams.get('ssl') === 'true'

    return {
      host: url.hostname || '127.0.0.1',
      port: url.port || '5432',
      database: decodePart(url.pathname.replace(/^\//, '')) || 'postgres',
      user: decodePart(url.username),
      password: decodePart(url.password),
      ssl,
    }
  } catch {
    return { error: 'Could not parse connection URI' }
  }
}

/**
 * MySQL / MariaDB connection URI, e.g. `mysql://user:pass@host:3306/db`.
 * @param {string} uri
 * @returns {ParsedPostgresUri | { error: string } | null}
 */
export function parseMysqlUri(uri) {
  const trimmed = uri.trim()
  if (!trimmed) return null

  let normalized = trimmed
  if (/^(mysql|mariadb):\/\//i.test(normalized)) {
    // Normalize any recognized scheme to mysql:// so the URL parser is happy.
    normalized = `mysql://${normalized.replace(/^[a-z]+:\/\//i, '')}`
  } else if (trimmed.includes('@') || /^[^/]+:\d+\//.test(trimmed)) {
    normalized = `mysql://${trimmed}`
  } else {
    return { error: 'Expected a mysql:// connection URI' }
  }

  try {
    const url = new URL(escapeUserinfo(normalized))
    const sslMode = url.searchParams.get('ssl-mode')?.toLowerCase()
    const ssl =
      sslMode === 'required' ||
      sslMode === 'verify_ca' ||
      sslMode === 'verify_identity' ||
      url.searchParams.get('ssl') === 'true'

    return {
      host: url.hostname || '127.0.0.1',
      port: url.port || '3306',
      database: decodePart(url.pathname.replace(/^\//, '')),
      user: decodePart(url.username),
      password: decodePart(url.password),
      ssl,
    }
  } catch {
    return { error: 'Could not parse connection URI' }
  }
}

/**
 * SQL Server URI, e.g. `sqlserver://sa:pass@host:1433/db?encrypt=true`.
 * @param {string} uri
 * @returns {{ host: string, port: string, database: string, user: string, password: string, encrypt: boolean, trustCert: boolean } | { error: string } | null}
 */
export function parseMssqlUri(uri) {
  const trimmed = uri.trim()
  if (!trimmed) return null

  // Prisma (and JDBC) write SQL Server as `sqlserver://host:1433;database=x;user=y`
  // - semicolon parameters, not a URL query string. `new URL` reads that as a
  // malformed port and throws, so it is parsed on its own terms.
  const semi = trimmed.match(/^(?:sqlserver|mssql):\/\/([^;]+);(.*)$/i)
  if (semi) {
    const [hostPart, rest] = [semi[1], semi[2]]
    const [host, port] = hostPart.split(':')
    /** @type {Record<string, string>} */
    const params = {}
    for (const pair of rest.split(';')) {
      const at = pair.indexOf('=')
      if (at <= 0) continue
      params[pair.slice(0, at).trim().toLowerCase()] = pair.slice(at + 1).trim()
    }
    const yes = (v) => v === 'true' || v === '1' || v === 'yes'
    const trustRaw = params.trustservercertificate
    return {
      host: host || '127.0.0.1',
      port: port || '1433',
      database: params.database ?? params.initialcatalog ?? '',
      user: params.user ?? params.username ?? params.userid ?? '',
      password: params.password ?? '',
      encrypt: params.encrypt == null ? true : yes(params.encrypt.toLowerCase()),
      trustCert: trustRaw == null ? true : yes(trustRaw.toLowerCase()),
    }
  }

  let normalized = trimmed
  if (/^(sqlserver|mssql):\/\//i.test(normalized)) {
    normalized = `mssql://${normalized.replace(/^[a-z]+:\/\//i, '')}`
  } else if (trimmed.includes('@') || /^[^/]+:\d+\//.test(trimmed)) {
    normalized = `mssql://${trimmed}`
  } else {
    return { error: 'Expected a sqlserver:// connection URI' }
  }

  const truthy = (v) => v === 'true' || v === '1' || v === 'yes'
  try {
    const url = new URL(escapeUserinfo(normalized))
    const trust = url.searchParams.get('trustservercertificate')?.toLowerCase()
    return {
      host: url.hostname || '127.0.0.1',
      port: url.port || '1433',
      database: decodePart(url.pathname.replace(/^\//, '')),
      user: decodePart(url.username),
      password: decodePart(url.password),
      encrypt: truthy(url.searchParams.get('encrypt')?.toLowerCase() ?? ''),
      // Default to trusting the cert (matches the form default; most local/dev
      // SQL Servers use a self-signed cert), unless the URI explicitly says false.
      trustCert: trust == null ? true : truthy(trust),
    }
  } catch {
    return { error: 'Could not parse connection URI' }
  }
}

/**
 * ClickHouse URI: `clickhouse://user:pass@host:8123/db` or an http(s):// URL.
 * @param {string} uri
 * @returns {{ host: string, port: string, database: string, user: string, password: string, secure: boolean } | { error: string } | null}
 */
export function parseClickhouseUri(uri) {
  const trimmed = uri.trim()
  if (!trimmed) return null

  const m = trimmed.match(/^(clickhouse|https?):\/\//i)
  const scheme = m?.[1]?.toLowerCase() ?? null
  let normalized
  if (scheme === 'clickhouse') {
    normalized = `http://${trimmed.replace(/^clickhouse:\/\//i, '')}`
  } else if (scheme) {
    normalized = trimmed
  } else if (trimmed.includes('@') || /^[^/]+:\d+\//.test(trimmed)) {
    normalized = `http://${trimmed}`
  } else {
    return { error: 'Expected a clickhouse:// or http(s):// URI' }
  }

  try {
    const url = new URL(escapeUserinfo(normalized))
    const secure = scheme === 'https' || url.port === '8443'
    return {
      host: url.hostname || '127.0.0.1',
      port: url.port || (secure ? '8443' : '8123'),
      database:
        decodePart(url.pathname.replace(/^\//, '')) || url.searchParams.get('database') || '',
      user: decodePart(url.username) || url.searchParams.get('user') || '',
      password: decodePart(url.password) || url.searchParams.get('password') || '',
      secure,
    }
  } catch {
    return { error: 'Could not parse connection URI' }
  }
}

/**
 * @param {string} uri
 * @returns {ParsedSqliteUri | { error: string } | null}
 */
export function parseSqliteUri(uri) {
  const trimmed = uri.trim()
  if (!trimmed) return null
  if (trimmed === ':memory:') return { filePath: ':memory:' }

  if (/^sqlite:/i.test(trimmed)) {
    let path = trimmed.replace(/^sqlite:\/\/?/i, '').replace(/^sqlite:/i, '')
    path = decodeURIComponent(path)
    if (!path) return { error: 'SQLite URI is missing a file path' }
    return { filePath: path }
  }

  if (/^file:\/\//i.test(trimmed)) {
    return { filePath: decodeURIComponent(trimmed.slice(7)) }
  }

  return null
}

/**
 * @param {'postgres'|'sqlite'|'mysql'|'mssql'|'clickhouse'|'redis'|'libsql'} type
 * @param {string} uri
 * @returns {ParsedPostgresUri | ParsedSqliteUri | Record<string, any> | { error: string } | null}
 */
export function parseConnectionUri(type, uri) {
  if (type === 'sqlite') return parseSqliteUri(uri)
  if (type === 'mysql') return parseMysqlUri(uri)
  if (type === 'mssql') return parseMssqlUri(uri)
  if (type === 'clickhouse') return parseClickhouseUri(uri)
  if (type === 'redis') return parseRedisUri(uri)
  if (type === 'libsql') return parseLibsqlUri(uri)
  return parsePostgresUri(uri)
}

/**
 * Which engine a pasted connection string is for.
 *
 * `parseConnectionUri` needs the engine told to it, which is right when the
 * form is already on one - but the fastest way to connect is to paste what your
 * hosting provider or `.env` gave you and be put on the right form. A URI
 * already names its engine in the scheme (and, for a few providers, in the
 * host), so asking the user to pick it first is asking them to repeat
 * themselves.
 *
 * Returns the app's own `DbType` id, plus the provider when the host identifies
 * one (the caller can then offer that provider's own sign-in instead of raw
 * credentials), or null when nothing in the string says.
 *
 * @param {string} uri
 * @returns {{ type: string, provider?: string, uriType: 'postgres'|'sqlite'|'mysql'|'mssql'|'clickhouse' } | null}
 */
export function detectConnectionUri(uri) {
  const s = String(uri ?? '').trim()
  if (!s) return null

  const scheme = /^([a-z][a-z0-9+.-]*):/i.exec(s)?.[1]?.toLowerCase() ?? ''
  const host = hostOf(s)

  // Providers first: a Neon or Supabase string is a Postgres string, and saying
  // so lets the caller offer the account flow rather than a bare host/port form.
  if (host) {
    if (/\.neon\.(tech|build)$/i.test(host)) return { type: 'postgres', provider: 'neon', uriType: 'postgres' }
    if (/\.supabase\.(co|com)$/i.test(host) || /\bpooler\.supabase\.com$/i.test(host)) {
      return { type: 'postgres', provider: 'supabase', uriType: 'postgres' }
    }
    if (/\.prisma-data\.(net|com)$/i.test(host) || scheme === 'prisma' || scheme === 'prisma+postgres') {
      return { type: 'postgres', provider: 'prisma-postgres', uriType: 'postgres' }
    }
    if (/\.psdb\.cloud$/i.test(host) || /\.planetscale\.(com|sh)$/i.test(host)) {
      return { type: 'mysql', provider: 'planetscale', uriType: 'mysql' }
    }
    if (/\.turso\.io$/i.test(host)) return { type: 'libsql', provider: 'turso', uriType: 'postgres' }
  }

  switch (scheme) {
    case 'postgres':
    case 'postgresql':
      return { type: 'postgres', uriType: 'postgres' }
    case 'cockroachdb':
      return { type: 'cockroachdb', uriType: 'postgres' }
    case 'mysql':
      return { type: 'mysql', uriType: 'mysql' }
    case 'mariadb':
      return { type: 'mariadb', uriType: 'mysql' }
    case 'sqlserver':
    case 'mssql':
      return { type: 'mssql', uriType: 'mssql' }
    case 'clickhouse':
    case 'clickhouses':
      return { type: 'clickhouse', uriType: 'clickhouse' }
    case 'redis':
    case 'rediss':
      return { type: 'redis', uriType: 'postgres' }
    case 'libsql':
      return { type: 'libsql', uriType: 'postgres' }
    case 'duckdb':
      return { type: 'duckdb', uriType: 'sqlite' }
    case 'sqlite':
    case 'file':
      return { type: 'sqlite', uriType: 'sqlite' }
    default:
      break
  }

  // `jdbc:postgresql://…` - the prefix a JDBC console hands out.
  const jdbc = /^jdbc:([a-z0-9]+):/i.exec(s)?.[1]?.toLowerCase()
  if (jdbc) {
    const inner = detectConnectionUri(s.slice(5))
    if (inner) return inner
  }

  if (s === ':memory:') return { type: 'sqlite-memory', uriType: 'sqlite' }
  // A bare path to a database file, which is what a SQLite "connection string"
  // usually is in practice.
  if (/\.(db|sqlite3?|duckdb)$/i.test(s) && !/\s/.test(s)) {
    return { type: /\.duckdb$/i.test(s) ? 'duckdb' : 'sqlite', uriType: 'sqlite' }
  }
  return null
}

/** Host of a URI, without credentials or port. Empty when it has none. */
function hostOf(/** @type {string} */ uri) {
  const m = /^[a-z][a-z0-9+.-]*:\/\/([^/?#]*)/i.exec(uri)
  if (!m) return ''
  const authority = m[1]
  const afterCreds = authority.includes('@') ? authority.slice(authority.lastIndexOf('@') + 1) : authority
  // Strip a port, and the brackets an IPv6 literal carries.
  const bare = afterCreds.replace(/^\[([^\]]+)\](?::\d+)?$/, '$1').replace(/:\d+$/, '')
  return bare.toLowerCase()
}

/**
 * `redis://` / `rediss://`, into the fields the Redis form holds.
 *
 * Redis puts the logical database in the path (`/0`) and, unusually, allows a
 * password with no username (`redis://:pw@host`), which is why this cannot go
 * through the Postgres parser - that one reads `:pw` as a username and loses the
 * database index.
 * @param {string} uri
 * @returns {{ host: string, port: string, db: string, user: string, password: string, tls: boolean } | { error: string } | null}
 */
export function parseRedisUri(uri) {
  const trimmed = uri.trim()
  if (!trimmed) return null
  if (!/^rediss?:\/\//i.test(trimmed)) {
    return { error: 'Expected a redis:// or rediss:// URL' }
  }
  let url
  try {
    url = new URL(escapeUserinfo(trimmed))
  } catch {
    return { error: 'Could not read that Redis URL' }
  }
  const tls = url.protocol.toLowerCase() === 'rediss:'
  const path = url.pathname.replace(/^\//, '')
  // `/0` is the database index; anything non-numeric is not one.
  const db = /^\d+$/.test(path) ? path : ''
  return {
    host: url.hostname || '127.0.0.1',
    port: url.port || '6379',
    db: db || '0',
    user: decodePart(url.username || ''),
    password: decodePart(url.password || ''),
    tls,
  }
}

/**
 * `libsql://` / Turso `https://`, into the URL + token the LibSQL form holds.
 *
 * A Turso string often carries the token as `?authToken=`, which belongs in its
 * own field rather than in the URL the driver is handed.
 * @param {string} uri
 * @returns {{ url: string, authToken: string } | { error: string } | null}
 */
export function parseLibsqlUri(uri) {
  const trimmed = uri.trim()
  if (!trimmed) return null
  if (!/^(libsql|wss?|https?):\/\//i.test(trimmed)) {
    return { error: 'Expected a libsql://, wss:// or https:// URL' }
  }
  let url
  try {
    url = new URL(escapeUserinfo(trimmed))
  } catch {
    return { error: 'Could not read that LibSQL URL' }
  }
  const token =
    url.searchParams.get('authToken') ?? url.searchParams.get('auth_token') ?? ''
  url.searchParams.delete('authToken')
  url.searchParams.delete('auth_token')
  // `toString()` re-appends a bare `?` once the params are gone.
  return { url: url.toString().replace(/\?$/, ''), authToken: token }
}
