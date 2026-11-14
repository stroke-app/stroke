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
 * Windows Explorer's "Copy as path" wraps the path in double quotes.
 * @param {string} s
 */
function unquotePath(s) {
  const t = s.trim()
  return /^"[^"]+"$/.test(t) || /^'[^']+'$/.test(t) ? t.slice(1, -1).trim() : t
}

/**
 * A bare path to a database file, which is what a SQLite "connection string"
 * usually is in practice. Spaces are fine in an absolute path (C:\Users\Jane
 * Doe\app.db is the usual Windows home); a relative one with spaces is more
 * likely a sentence.
 * @param {string} s already trimmed and unquoted
 */
function isDatabaseFilePath(s) {
  const absolute = /^([a-z]:[\\/]|\\\\|\/|~[\\/])/i.test(s)
  return /\.(db|sqlite3?|db3|duckdb)$/i.test(s) && (absolute || !/\s/.test(s))
}

/**
 * @param {string} uri
 * @returns {ParsedSqliteUri | { error: string } | null}
 */
export function parseSqliteUri(uri) {
  const trimmed = unquotePath(uri)
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

  if (isDatabaseFilePath(trimmed)) return { filePath: trimmed }

  return null
}

/**
 * Oracle connection strings: `oracle://user:pass@host:1521/SERVICE`, and the
 * JDBC thin forms a DBA hands out - `jdbc:oracle:thin:@//host:1521/SERVICE`,
 * `jdbc:oracle:thin:@host:1521:SID`, `jdbc:oracle:thin:user/pass@...` - and a
 * `(DESCRIPTION=...)` descriptor, which is kept whole as the connect string.
 * @param {string} uri
 * @returns {{ oracle: true, host: string, port: string, serviceName: string, sid: string, connectString: string, user: string, password: string } | { error: string } | null}
 */
export function parseOracleUri(uri) {
  const trimmed = uri.trim()
  if (!trimmed) return null
  const out = { oracle: /** @type {true} */ (true), host: '', port: '1521', serviceName: '', sid: '', connectString: '', user: '', password: '' }
  const thin = /^(?:jdbc:)?oracle:thin:(.*)$/i.exec(trimmed)
  if (thin) {
    let rest = thin[1]
    const at = rest.lastIndexOf('@')
    if (at < 0) return { error: 'Expected jdbc:oracle:thin:@host:port/service' }
    const creds = rest.slice(0, at)
    rest = rest.slice(at + 1).trim()
    if (creds) {
      const slash = creds.indexOf('/')
      out.user = slash < 0 ? creds : creds.slice(0, slash)
      out.password = slash < 0 ? '' : creds.slice(slash + 1)
    }
    if (rest.startsWith('(')) {
      out.connectString = rest
      return out
    }
    // `//host:port/service` (service) or `host:port:SID` (the old SID form).
    const svc = /^(?:\/\/)?([^:/]+)(?::(\d+))?\/(.+)$/.exec(rest)
    if (svc) return { ...out, host: svc[1], port: svc[2] || '1521', serviceName: svc[3] }
    const sid = /^([^:/]+):(\d+):(.+)$/.exec(rest)
    if (sid) return { ...out, host: sid[1], port: sid[2], sid: sid[3] }
    return { ...out, connectString: rest }
  }
  if (!/^oracle:\/\//i.test(trimmed)) return { error: 'Expected an oracle:// or jdbc:oracle:thin: connection string' }
  try {
    const url = new URL(escapeUserinfo(trimmed))
    return {
      ...out,
      host: url.hostname || '127.0.0.1',
      port: url.port || '1521',
      serviceName: decodeURIComponent(url.pathname.replace(/^\//, '')),
      user: decodeURIComponent(url.username),
      password: decodeURIComponent(url.password),
    }
  } catch {
    return { error: 'Could not parse connection URI' }
  }
}

/**
 * @param {'postgres'|'sqlite'|'mysql'|'mssql'|'clickhouse'|'redis'|'libsql'|'oracle'} type
 * @param {string} uri
 * @returns {ParsedPostgresUri | ParsedSqliteUri | Record<string, any> | { error: string } | null}
 */
export function parseConnectionUri(type, uri) {
  if (type === 'couchdb') return parseCouchUri(uri)
  if (type === 'cassandra') return parseCassandraUri(uri)
  if (type === 'sqlite') return parseSqliteUri(uri)
  if (type === 'mysql') return parseMysqlUri(uri)
  if (type === 'mssql') return parseMssqlUri(uri)
  if (type === 'clickhouse') return parseClickhouseUri(uri)
  if (type === 'redis') return parseRedisUri(uri)
  if (type === 'libsql') return parseLibsqlUri(uri)
  if (type === 'oracle') return parseOracleUri(uri)
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
 * @returns {{ type: string, provider?: string, uriType: 'postgres'|'sqlite'|'mysql'|'mssql'|'clickhouse'|'oracle' } | null}
 */
export function detectConnectionUri(uri) {
  const s = cleanPastedConnection(String(uri ?? ''))
  if (!s) return null
  const hit = detectClean(s)
  if (!hit) return null
  // `uri`: the string to hand the parser, when it isn't what was pasted - the
  // .env wrapper, driver suffix or key/value form turned into a plain URL.
  const plain = hit.uri ?? s
  const { uri: _drop, ...rest } = hit
  return plain !== String(uri ?? '').trim() ? { ...rest, uri: plain } : rest
}

/**
 * What people paste is rarely just the URL: a `.env` line (`DATABASE_URL="…"`,
 * `export REDIS_URL=…`), a quoted value, a trailing semicolon. Keep the value.
 * @param {string} raw
 */
export function cleanPastedConnection(raw) {
  let s = String(raw ?? '').trim()
  s = s.replace(/^export\s+/i, '')
  // KEY=value or KEY: value, when the key is an identifier and the value
  // starts like a connection string (not `host=… dbname=…` key/value text).
  const kv = /^([A-Za-z_][A-Za-z0-9_]*)\s*[=:]\s*(.+)$/s.exec(s)
  if (kv && /^["'`]?[a-z][a-z0-9+.-]*:\/\//i.test(kv[2].trim())) s = kv[2].trim()
  s = s.replace(/;\s*$/, '').trim()
  return unquotePath(s)
}

/**
 * A libpq key/value string (`host=db port=5432 dbname=app user=me`) as a URL.
 * @param {string} s
 */
function libpqToUrl(s) {
  /** @type {Record<string, string>} */
  const o = {}
  for (const m of s.matchAll(/(\w+)\s*=\s*('(?:[^'\\]|\\.)*'|\S+)/g)) o[m[1].toLowerCase()] = m[2].replace(/^'|'$/g, '').replace(/\\(.)/g, '$1')
  if (!o.host && !o.dbname) return null
  const enc = encodeURIComponent
  const auth = o.user ? `${enc(o.user)}${o.password ? `:${enc(o.password)}` : ''}@` : ''
  const q = o.sslmode ? `?sslmode=${enc(o.sslmode)}` : ''
  return `postgresql://${auth}${o.host || 'localhost'}${o.port ? `:${o.port}` : ''}/${enc(o.dbname || '')}${q}`
}

/**
 * An ADO.NET string (`Server=tcp:host,1433;Database=app;User Id=sa;Password=…;`)
 * in the semicolon form the SQL Server parser reads.
 * @param {string} s
 */
function adoNetToUrl(s) {
  /** @type {Record<string, string>} */
  const o = {}
  for (const part of s.split(';')) {
    const i = part.indexOf('=')
    if (i > 0) o[part.slice(0, i).trim().toLowerCase().replace(/\s+/g, ' ')] = part.slice(i + 1).trim()
  }
  const server = o.server ?? o['data source'] ?? o.address ?? o.addr
  if (!server) return null
  const [hostPart, portPart] = server.replace(/^tcp:/i, '').split(',')
  const user = o['user id'] ?? o.uid ?? o.user ?? ''
  const password = o.password ?? o.pwd ?? ''
  const db = o.database ?? o['initial catalog'] ?? ''
  const params = [db && `database=${db}`, user && `user=${user}`, password && `password=${password}`,
    o.encrypt && `encrypt=${o.encrypt}`, o.trustservercertificate && `trustServerCertificate=${o.trustservercertificate}`].filter(Boolean)
  return `sqlserver://${hostPart.trim()}${portPart ? `:${portPart.trim()}` : ''};${params.join(';')}`
}

/** @param {string} s @returns {{ type: string, provider?: string, uriType: string, uri?: string } | null} */
function detectClean(s) {
  // Key/value forms first: no scheme to go by.
  if (!/^[a-z][a-z0-9+.-]*:/i.test(s) || /^(server|data source)\s*=/i.test(s)) {
    if (/(^|;)\s*(server|data source|address)\s*=/i.test(s) && s.includes(';')) {
      const url = adoNetToUrl(s)
      if (url) return { type: 'mssql', uriType: 'mssql', uri: url }
    }
    if (/\b(host|dbname)\s*=/i.test(s) && !s.includes(';')) {
      const url = libpqToUrl(s)
      if (url) return { type: 'postgres', uriType: 'postgres', uri: url }
    }
  }

  let scheme = /^([a-z][a-z0-9+.-]*):/i.exec(s)?.[1]?.toLowerCase() ?? ''
  // SQLAlchemy / ORM driver suffixes: `postgresql+psycopg2://`, `mysql+pymysql://`.
  // Not `mongodb+srv` (a real scheme) or `prisma+postgres` (a provider's).
  if (scheme.includes('+') && scheme !== 'mongodb+srv' && scheme !== 'prisma+postgres') {
    const base = scheme.split('+')[0]
    const inner = detectClean(base + s.slice(scheme.length))
    return inner ? { ...inner, uri: inner.uri ?? base + s.slice(scheme.length) } : null
  }
  const host = hostOf(s)
  const port = Number(/^[a-z][a-z0-9+.-]*:\/\/(?:[^@/]*@)?[^/:?#]+:(\d+)/i.exec(s)?.[1] ?? 0)

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
    if (/\.turso\.io$/i.test(host)) return { type: 'libsql', provider: 'turso', uriType: 'libsql' }
    if (/\.tidbcloud\.com$/i.test(host)) return { type: 'mysql', provider: 'tidb', uriType: 'mysql' }
    if (/\.upstash\.io$/i.test(host) && /^rediss?$/.test(scheme)) return { type: 'redis', provider: 'upstash', uriType: 'redis' }
    if (/\.thenile\.dev$/i.test(host)) return { type: 'postgres', provider: 'nile', uriType: 'postgres' }
    if (/\.(rlwy\.net|railway\.app|railway\.internal)$/i.test(host)) {
      const inner = detectByScheme(scheme, s, host, port)
      if (inner) return { ...inner, provider: 'railway' }
    }
  }
  return detectByScheme(scheme, s, host, port)
}

/** @param {string} scheme @param {string} s @param {string} host @param {number} port */
function detectByScheme(scheme, s, host, port) {

  switch (scheme) {
    case 'postgres':
    case 'postgresql':
      return { type: 'postgres', uriType: 'postgres' }
    case 'cockroachdb':
      return { type: 'cockroachdb', uriType: 'postgres' }
    case 'mysql':
    case 'mysqlx':
      return { type: 'mysql', uriType: 'mysql', uri: scheme === 'mysqlx' ? 'mysql' + s.slice(6) : undefined }
    case 'mariadb':
      return { type: 'mariadb', uriType: 'mysql' }
    case 'sqlserver':
    case 'mssql':
      return { type: 'mssql', uriType: 'mssql' }
    case 'oracle':
      return { type: 'oracle', uriType: 'oracle' }
    case 'clickhouse':
    case 'clickhouses':
      return { type: 'clickhouse', uriType: 'clickhouse' }
    case 'mongodb':
    case 'mongodb+srv':
      return { type: 'mongodb', uriType: 'mongodb' }
    case 'postgis':
    case 'timescale':
    case 'pgsql':
    case 'pg':
      return { type: 'postgres', uriType: 'postgres', uri: 'postgresql' + s.slice(scheme.length) }
    case 'redis':
    case 'rediss':
      return { type: 'redis', uriType: 'redis' }
    case 'valkey':
    case 'valkeys':
      return { type: 'valkey', uriType: 'redis', uri: (scheme === 'valkeys' ? 'rediss' : 'redis') + s.slice(scheme.length) }
    case 'libsql':
      return { type: 'libsql', uriType: 'libsql' }
    case 'couchdb':
    case 'couchdbs':
      return { type: 'couchdb', uriType: 'couchdb', uri: (scheme === 'couchdbs' ? 'https' : 'http') + s.slice(scheme.length) }
    case 'cassandra':
    case 'scylla':
    case 'cql':
      return { type: 'cassandra', uriType: 'cassandra' }
    case 'http':
    case 'https':
    case 'ws':
    case 'wss':
      // An HTTP endpoint says what it is by its host or its port.
      if (/\.turso\.io$/i.test(host) || scheme.startsWith('ws')) return { type: 'libsql', uriType: 'libsql' }
      if (/clickhouse/i.test(host) || port === 8123 || port === 8443) return { type: 'clickhouse', uriType: 'clickhouse' }
      if (/couch|cloudant/i.test(host) || port === 5984) return { type: 'couchdb', uriType: 'couchdb' }
      return null
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
  if (isDatabaseFilePath(s)) {
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

/**
 * CouchDB's server URL, with the login moved to its own fields.
 * @param {string} uri
 * @returns {{ couch: true, url: string, user: string, password: string } | { error: string } | null}
 */
export function parseCouchUri(uri) {
  const t = uri.trim().replace(/^couchdbs:/i, 'https:').replace(/^couchdb:/i, 'http:')
  if (!t) return null
  let u
  try {
    u = new URL(escapeUserinfo(t))
  } catch {
    return { error: 'Could not read that CouchDB URL' }
  }
  const user = decodePart(u.username)
  const password = decodePart(u.password)
  u.username = ''
  u.password = ''
  return { couch: true, url: u.toString().replace(/\/$/, ''), user, password }
}

/**
 * `cassandra://user:pass@h1:9042,h2:9042/keyspace` into the CQL form's fields.
 * @param {string} uri
 * @returns {{ cql: true, hosts: string, user: string, password: string, keyspace: string } | { error: string } | null}
 */
export function parseCassandraUri(uri) {
  const m = /^[a-z]+:\/\/(?:([^:@/]*)(?::([^@/]*))?@)?([^/?#]+)(?:\/([^?#]*))?/i.exec(uri.trim())
  if (!m) return { error: 'Expected cassandra://host:9042/keyspace' }
  return { cql: true, user: decodePart(m[1] ?? ''), password: decodePart(m[2] ?? ''), hosts: m[3], keyspace: decodePart(m[4] ?? '') }
}

/**
 * The connection string for a form's payload: the reverse of the paste box.
 * Copy URL hands it to `.env` files, ORMs and other tools. Credentials are
 * URL-encoded, so a password with `@`, `/` or `#` survives the round trip.
 * Returns '' for an engine with no URL form (D1, DynamoDB, Firestore).
 * @param {Record<string, any>} p
 * @returns {string}
 */
export function connectionUrl(p) {
  if (!p) return ''
  const enc = encodeURIComponent
  const auth = (/** @type {string} */ u, /** @type {string} */ pw) =>
    u || pw ? `${enc(u ?? '')}${pw ? `:${enc(pw)}` : ''}@` : ''
  const hp = (/** @type {string} */ h, /** @type {any} */ port) => `${h || 'localhost'}${port ? `:${port}` : ''}`
  switch (p.type) {
    case 'postgres':
    case 'cockroachdb': {
      const q = p.ssl ? `?sslmode=${p.sslMode || 'require'}` : ''
      return `postgresql://${auth(p.user, p.password)}${hp(p.host, p.port)}/${enc(p.database ?? '')}${q}`
    }
    case 'mysql':
    case 'mariadb':
      return `mysql://${auth(p.user, p.password)}${hp(p.host, p.port)}/${enc(p.database ?? '')}${p.ssl ? '?ssl-mode=REQUIRED' : ''}`
    case 'mssql': {
      const parts = [p.database && `database=${p.database}`, p.user && `user=${p.user}`, p.password && `password=${p.password}`, p.encrypt && 'encrypt=true'].filter(Boolean)
      return `sqlserver://${hp(p.host, p.port)}${parts.length ? `;${parts.join(';')}` : ''}`
    }
    case 'oracle':
      if (p.connectString) return p.connectString
      if (p.sid) return `jdbc:oracle:thin:${p.user ? `${p.user}${p.password ? `/${p.password}` : ''}` : ''}@${hp(p.host, p.port)}:${p.sid}`
      return `oracle://${auth(p.user, p.password)}${hp(p.host, p.port)}/${enc(p.serviceName ?? '')}`
    case 'clickhouse':
      return `${p.secure ? 'https' : 'http'}://${auth(p.user, p.password)}${hp(p.host, p.port)}/?database=${enc(p.database || 'default')}`
    case 'redis':
    case 'valkey': {
      const scheme = (p.tls ? 'rediss' : 'redis')
      const cred = p.user || p.password ? `${enc(p.user ?? '')}:${enc(p.password ?? '')}@` : ''
      return `${scheme}://${cred}${hp(p.host, p.port)}/${Number(p.db) || 0}`
    }
    case 'mongodb':
      return p.uri || ''
    case 'libsql':
      return p.url ? `${p.url}${p.authToken ? `${p.url.includes('?') ? '&' : '?'}authToken=${enc(p.authToken)}` : ''}` : ''
    case 'couchdb': {
      const m = /^(https?:\/\/)(.*)$/i.exec(p.url || '')
      return m ? `${m[1]}${auth(p.user, p.password)}${m[2]}` : p.url || ''
    }
    case 'cassandra':
      return `cassandra://${auth(p.user, p.password)}${p.hosts || 'localhost:9042'}${p.keyspace ? `/${enc(p.keyspace)}` : ''}`
    case 'sqlite':
    case 'duckdb':
      return p.filePath || ''
    default:
      return ''
  }
}
