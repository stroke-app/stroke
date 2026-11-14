import { invoke, Channel } from '@tauri-apps/api/core'
import { loadSettings } from '$lib/stores/settings.js'
import { traceIpc, traceRows } from '$lib/perf.js'
import { recordQuery } from '$lib/stores/query-log.js'
import { assertWritable } from '$lib/stores/read-only.js'
import { getLastConnection } from '$lib/stores/connections.js'
import { isWriteSql } from '$lib/sql-write.js'

/** @typedef {{ name: string, host: string, port: number, database: string, user: string, password: string, ssl: boolean }} PgConnectionConfig */
/** @typedef {{ name: string, filePath: string }} SqliteConnectionConfig */
/** @typedef {{ name: string, accountId: string, databaseId: string, apiToken: string }} D1ConnectionConfig */
/** @typedef {PgConnectionConfig} ConnectionConfig */

/** @param {Record<string, unknown>} raw */
export function normalizeConnectionConfig(raw) {
  return {
    name: String(raw.name ?? '').trim() || 'Connection',
    host: String(raw.host ?? '127.0.0.1').trim(),
    port: Math.min(65535, Math.max(1, Number(raw.port) || 5432)),
    database: String(raw.database ?? 'postgres').trim(),
    user: String(raw.user ?? 'postgres').trim(),
    password: String(raw.password ?? ''),
    ssl: Boolean(raw.ssl),
  }
}

/**
 * True when the Tauri IPC bridge isn't present (e.g. `npm run dev` in a plain
 * browser). In that case *every* `invoke` fails structurally, so we surface a
 * dev hint. When the bridge IS present, we must pass the backend's real error
 * through untouched - matching on substrings like "invoke"/"Tauri" used to
 * swallow genuine DB failures whose text happened to mention those words.
 */
function tauriBridgeMissing() {
  return typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)
}

/** @param {unknown} err */
function formatInvokeError(err) {
  if (tauriBridgeMissing()) {
    return 'Database API unavailable. Run the app with: npm run tauri dev'
  }
  return err instanceof Error ? err.message : String(err)
}

// ── D1 auth self-heal ────────────────────────────────────────────────────────
// A Cloudflare OAuth token is short-lived, and the backend keeps whichever one it
// was handed at connect time. Once that one expires, EVERY later call - queries,
// table lists, row counts - comes back 401 while the app still shows a live
// connection, so the session is permanently broken until the user reconnects by
// hand. `connect_d1_db` and `test_d1` already refresh; the query path did not.
// Replaying is safe: D1 rejects an unauthorized request at the edge, before any
// SQL runs, so a retried write cannot apply twice.
let _d1Healing = false

/**
 * Rebuild the D1 pool with a freshly minted token.
 * @param {string} msg the failure text from the call that just 401'd
 * @returns {Promise<boolean>} true when the caller should retry
 */
async function healD1Auth(msg) {
  if (_d1Healing || !D1_UNAUTHORIZED.test(msg)) return false
  const conn = /** @type {any} */ (getLastConnection())
  if (conn?.type !== 'd1') return false
  _d1Healing = true
  try {
    const fresh = await d1WithFreshToken(conn)
    // Same object back means there was no OAuth session to refresh from (a manually
    // pasted token) - the 401 is real and belongs to the user, not to us.
    if (fresh === conn) return false
    await invoke('connect_d1_db', { config: fresh })
    return true
  } catch {
    return false
  } finally {
    _d1Healing = false
  }
}

async function inv(command, args = {}) {
  const t0 = performance.now()
  try {
    const r = await invoke(command, args)
    traceIpc(command, performance.now() - t0, true)
    return r
  } catch (err) {
    traceIpc(command, performance.now() - t0, false)
    const msg = formatInvokeError(err)
    if (await healD1Auth(msg)) {
      try {
        return await invoke(command, args)
      } catch (retryErr) {
        throw new Error(formatInvokeError(retryErr))
      }
    }
    throw new Error(msg)
  }
}

// ── Connect retry ───────────────────────────────────────────────────────────
// A connect attempt fails for one of two reasons:
//   • TRANSIENT - a timeout, the server briefly unreachable, a momentarily
//     exhausted pool, a DNS/network blip. These usually succeed on an immediate
//     second try, so we retry a few times with a short backoff for a smoother,
//     self-healing connect.
//   • PERMANENT - bad credentials (401/403), wrong host/database, a missing
//     file. These NEVER succeed on retry, so we surface them at once - the user
//     sees the real error (e.g. "Authentication error") with no extra delay.
// Anything we can't confidently classify as transient is treated as permanent
// (fail fast), so we never spin on an error the user needs to see and act on.
const CONNECT_MAX_ATTEMPTS = 3
/**
 * Wall-clock ceiling on the whole retry sequence. Each backend attempt is itself
 * bounded (probe + handshake deadline), and multiplying a slow attempt by the
 * retry count is what turned one unreachable host into ~20 s of spinner before
 * the user saw any message. No new attempt starts past this mark.
 */
const CONNECT_TOTAL_BUDGET_MS = 12_000

/** Auth / config errors that will never succeed on retry. */
function isPermanentConnError(msg) {
  return /\b(401|403|407)\b|unauthor|authenticat|password authentication|access denied|permission denied|invalid[\s_-]*(password|credential|token|api[\s_-]?key|key|secret)|bad credentials|forbidden|no such (database|file|host)|does not exist|not found|unknown database|unknown host|certificate|self[\s-]?signed|\btls\b|\bssl\b/i.test(msg)
}

/**
 * Errors worth a fast retry: a server mid-restart, an exhausted pool, a socket
 * that died between attempts. Deliberately EXCLUDES the "nothing is there"
 * family - connection refused, no route, host unreachable, DNS failure, and the
 * backend's own "Can't reach …" preflight verdict. Those are definitive: the
 * backend already probed every resolved address concurrently, so a retry buys
 * nothing and only multiplies the delay before the user sees the real problem.
 */
function isTransientConnError(msg) {
  if (/can't reach|can't resolve|connection refused|econnrefused|enetunreach|ehostunreach|no route|unreachable|getaddrinfo|eai_again|did not resolve|didn't resolve/i.test(msg)) return false
  return /pool timed out|reset|broken pipe|econnreset|epipe|temporarily unavailable|\b(502|503|504)\b|too many connections|server closed|connection terminated|starting up|i\/o error/i.test(msg)
}

/**
 * Like inv(), but for connect commands: retries a TRANSIENT failure with fast
 * backoff (200ms → 400ms), bounded by CONNECT_TOTAL_BUDGET_MS. Permanent
 * failures (auth/config) and definitively-unreachable hosts are thrown at once
 * so the real error reaches the UI without delay.
 */
async function connectInv(command, args = {}) {
  const startedAt = Date.now()
  let lastErr
  for (let attempt = 1; attempt <= CONNECT_MAX_ATTEMPTS; attempt++) {
    try {
      return await inv(command, args)
    } catch (e) {
      lastErr = e
      const msg = String(e?.message ?? e ?? '')
      const worthRetry = !isPermanentConnError(msg) && isTransientConnError(msg)
      if (attempt === CONNECT_MAX_ATTEMPTS || !worthRetry) throw e
      const backoff = 200 * 2 ** (attempt - 1)
      if (Date.now() - startedAt + backoff >= CONNECT_TOTAL_BUDGET_MS) throw e
      await new Promise((r) => setTimeout(r, backoff))
    }
  }
  throw lastErr
}

// ── Live mode ───────────────────────────────────────────────────────────────

/**
 * Start live updates for a table on the active connection (Postgres / SQLite).
 * The backend emits a `live-change` event when the table changes.
 * @param {string} schema @param {string} table
 */
export async function liveStart(schema, table) {
  return await inv('live_start', { schema, table })
}

/** Stop live updates and clean up any installed triggers. */
export async function liveStop() {
  return await inv('live_stop', {})
}

// ── PostgreSQL ────────────────────────────────────────────────────────────────

/** @param {{ host: string, port?: number, username: string, privateKeyPath?: string }} ssh */
function normalizeSshConfig(ssh) {
  return {
    host: String(ssh.host || '').trim(),
    port: Math.min(65535, Math.max(1, Number(ssh.port) || 22)),
    username: String(ssh.username || '').trim(),
    privateKeyPath: String(ssh.privateKeyPath || '').trim(),
  }
}

/** Attach SSH config to a connection payload when the user has configured a tunnel. */
function withSsh(payload, config) {
  if (config.ssh && config.ssh.host && config.ssh.username) {
    return { ...payload, ssh: normalizeSshConfig(config.ssh) }
  }
  return payload
}

/**
 * Attach the session time zone from Settings → Database to a Postgres/MySQL
 * connect payload. Applied per pooled connection by the backend; "SYSTEM" (or
 * blank) is treated as "leave the server default" and omitted.
 */
function withTimezone(payload) {
  try {
    const tz = String(loadSettings().sessionTimezone ?? '').trim()
    if (tz && tz.toUpperCase() !== 'SYSTEM') return { ...payload, timezone: tz }
  } catch {
    // Settings unavailable - connect without a timezone override.
  }
  return payload
}

export async function testPostgresConnection(config) {
  return inv('test_postgres_connection', { config: withSsh(withTimezone(normalizeConnectionConfig(config)), config) })
}

export async function connectPostgres(config) {
  return connectInv('connect_postgres', { config: withSsh(withTimezone(normalizeConnectionConfig(config)), config) })
}

/** Toggle the WebView DevTools (no-op in release builds). */
export async function toggleDevtools() {
  return inv('toggle_devtools')
}

// ── SQLite ────────────────────────────────────────────────────────────────────

/** @param {{ name: string, filePath: string }} config */
export async function testSqliteConnection(config) {
  return inv('test_sqlite', { config: { name: String(config.name || 'SQLite'), filePath: String(config.filePath || '') } })
}

/** @param {{ name: string, filePath: string }} config */
export async function connectSqlite(config) {
  return connectInv('connect_sqlite_db', { config: { name: String(config.name || 'SQLite'), filePath: String(config.filePath || '') } })
}

/**
 * Creates the sample SQLite database in the app data directory if it doesn't
 * exist yet, then returns its absolute file path.
 * @returns {Promise<string>}
 */
export async function initSampleDb() {
  return inv('init_sample_db')
}

// ── MySQL ─────────────────────────────────────────────────────────────────────

/** @param {{ name: string, host: string, port: number, database: string, user: string, password: string, ssl: boolean, ssh?: object }} config */
export async function testMysqlConnection(config) {
  const base = {
    name: String(config.name || 'MySQL'),
    host: String(config.host || '127.0.0.1'),
    port: Math.min(65535, Math.max(1, Number(config.port) || 3306)),
    database: String(config.database || ''),
    user: String(config.user || 'root'),
    password: String(config.password || ''),
    ssl: Boolean(config.ssl),
  }
  return inv('test_mysql', { config: withSsh(withTimezone(base), config) })
}

/** @param {{ name: string, host: string, port: number, database: string, user: string, password: string, ssl: boolean, ssh?: object }} config */
export async function connectMysql(config) {
  const base = {
    name: String(config.name || 'MySQL'),
    host: String(config.host || '127.0.0.1'),
    port: Math.min(65535, Math.max(1, Number(config.port) || 3306)),
    database: String(config.database || ''),
    user: String(config.user || 'root'),
    password: String(config.password || ''),
    ssl: Boolean(config.ssl),
  }
  return connectInv('connect_mysql_db', { config: withSsh(withTimezone(base), config) })
}

// ── Cloudflare D1 ─────────────────────────────────────────────────────────────

/**
 * A D1 connection saved from the Cloudflare sign-in carries a *snapshot* of an
 * OAuth access token, and those live about ten minutes. So reconnecting to a
 * saved D1 database later fails with `D1 API error 401 Unauthorized` while the
 * app still shows Cloudflare as connected - and it starts working the moment
 * anything else (the account or database dropdown) asks for a fresh token.
 *
 * The keychain can always mint a valid one, so D1 asks for it here rather than
 * trusting the stored copy. A manually pasted API token doesn't expire and has
 * no OAuth session behind it, so it is left exactly as saved.
 */
const D1_UNAUTHORIZED = /\b40[13]\b|unauthorized|invalid api token|authentication error/i

/**
 * @param {any} config
 * @returns {Promise<any>} the config, with a live token when one is available
 */
async function d1WithFreshToken(config) {
  try {
    const { cfGetValidToken } = await import('$lib/cloudflare.js')
    const token = await cfGetValidToken()
    if (token && token !== config?.apiToken) return { ...config, apiToken: token }
  } catch {
    // No OAuth session (manual token), or refresh failed - use what we were given.
  }
  return config
}

/**
 * Run a D1 call, and if it comes back unauthorized, try once more with a token
 * refreshed from the keychain. Covers connections saved before they carried the
 * `oauth` marker, and any token that expires mid-session.
 * @param {any} config
 * @param {(cfg: any) => Promise<any>} run
 */
async function d1Call(config, run) {
  // Connections created through the sign-in flow are known to hold a short-lived
  // token: refresh up front instead of paying for a doomed request first.
  const first = config?.oauth ? await d1WithFreshToken(config) : config
  try {
    return await run(first)
  } catch (err) {
    const msg = String(/** @type {any} */ (err)?.message ?? err ?? '')
    if (!D1_UNAUTHORIZED.test(msg)) throw err
    const retry = await d1WithFreshToken(first)
    if (retry === first) throw err
    return run(retry)
  }
}

/** @param {{ name: string, accountId: string, databaseId: string, apiToken: string, oauth?: boolean }} config */
export async function testD1Connection(config) {
  return d1Call(config, (cfg) => inv('test_d1', { config: cfg }))
}

/** @param {{ name: string, accountId: string, databaseId: string, apiToken: string, oauth?: boolean }} config */
export async function connectD1(config) {
  return d1Call(config, (cfg) => connectInv('connect_d1_db', { config: cfg }))
}

/**
 * List all Cloudflare accounts accessible with the given API token.
 * @param {string} apiToken
 * @returns {Promise<Array<{id: string, name: string}>>}
 */
export async function cloudflareListAccounts(apiToken) {
  return inv('cloudflare_list_accounts', { apiToken })
}

/**
 * List all D1 databases for a given Cloudflare account.
 * @param {string} apiToken
 * @param {string} accountId
 * @returns {Promise<Array<{uuid: string, name: string, created_at?: string, num_tables?: number}>>}
 */
export async function cloudflareListD1Databases(apiToken, accountId) {
  return inv('cloudflare_list_d1_databases', { apiToken, accountId })
}

/**
 * Create a D1 database in an account. `locationHint` is one of Cloudflare's
 * region hints (wnam, enam, weur, eeur, apac, oc), or null to let it choose.
 * @param {string} apiToken @param {string} accountId @param {string} name @param {string | null} [locationHint]
 * @returns {Promise<{ uuid: string, name: string, created_at?: string, num_tables?: number }>}
 */
export async function cloudflareCreateD1Database(apiToken, accountId, name, locationHint = null) {
  return inv('cloudflare_create_d1_database', { apiToken, accountId, name, locationHint })
}

/** @param {{ name: string, url: string, authToken?: string }} config */
export async function testLibSqlConnection(config) {
  return inv('test_libsql', { config })
}

/** @param {{ name: string, url: string, authToken?: string }} config */
export async function connectLibSql(config) {
  return connectInv('connect_libsql_db', { config })
}

// ── PostHog ───────────────────────────────────────────────────────────────────

/**
 * A PostHog project, queried with HogQL over PostHog's query API.
 * @param {{ name?: string, host: string, projectId: string|number, apiKey: string }} config
 */
function normalizePosthog(config) {
  return {
    name: String(config.name || 'PostHog'),
    host: String(config.host || 'https://us.posthog.com'),
    projectId: String(config.projectId ?? ''),
    apiKey: String(config.apiKey || ''),
  }
}

/** @param {{ name?: string, host: string, projectId: string|number, apiKey: string }} config */
export async function testPosthogConnection(config) {
  return inv('test_posthog', { config: normalizePosthog(config) })
}

/** @param {{ name?: string, host: string, projectId: string|number, apiKey: string }} config */
export async function connectPosthog(config) {
  return connectInv('connect_posthog_db', { config: normalizePosthog(config) })
}

// ── ClickHouse ────────────────────────────────────────────────────────────────

/** @param {{ name: string, host: string, port: number|string, database: string, user: string, password: string, secure?: boolean }} config */
function normalizeClickhouse(config) {
  return {
    name: String(config.name || 'ClickHouse'),
    host: String(config.host || '127.0.0.1'),
    port: Math.min(65535, Math.max(1, Number(config.port) || 8123)),
    database: String(config.database || 'default'),
    user: String(config.user || 'default'),
    password: String(config.password || ''),
    secure: Boolean(config.secure),
  }
}

/** @param {{ name: string, host: string, port: number|string, database: string, user: string, password: string, secure?: boolean }} config */
export async function testClickhouseConnection(config) {
  return inv('test_clickhouse', { config: normalizeClickhouse(config) })
}

/** @param {{ name: string, host: string, port: number|string, database: string, user: string, password: string, secure?: boolean }} config */
export async function connectClickhouse(config) {
  return connectInv('connect_clickhouse_db', { config: normalizeClickhouse(config) })
}

// ── Redis ─────────────────────────────────────────────────────────────────────

/** @param {{ name: string, host: string, port: number|string, password?: string, db?: number|string, tls?: boolean }} config */
function normalizeRedis(config) {
  return {
    type: 'redis',
    name: String(config.name || 'Redis'),
    host: String(config.host || '127.0.0.1'),
    port: Number(config.port) || 6379,
    password: config.password || null,
    db: Number(config.db) || 0,
    tls: !!config.tls,
  }
}

/** @param {{ name: string, host: string, port: number|string, password?: string, db?: number|string, tls?: boolean }} config */
export async function testRedis(config) {
  return inv('test_redis', { config: normalizeRedis(config) })
}

/** @param {{ name: string, host: string, port: number|string, password?: string, db?: number|string, tls?: boolean }} config */
export async function connectRedis(config) {
  return connectInv('connect_redis_db', { config: normalizeRedis(config) })
}

/**
 * One page of a non-blocking `SCAN` over the active Redis keyspace.
 * @param {string} cursor opaque cursor ("0" to start)
 * @param {string} [pattern] glob MATCH pattern (omit/"*" for all)
 * @param {number} [count] COUNT hint per page
 * @returns {Promise<{ cursor: string, keys: string[] }>}
 */
export async function redisScan(cursor, pattern, count = 1000) {
  return inv('redis_scan', { cursor: String(cursor), pattern: pattern || null, count })
}

/**
 * @typedef {{ key: string, binary: boolean, type: string, ttl_ms: number, memory: number | null,
 *   length: number | null, preview: string, truncated: boolean }} RedisKeyRow
 * @typedef {{ key: string, binary?: boolean }} RedisKeyRef
 * @typedef {{ field: string | null, value: string, score: string | null, index: number | null }} RedisValueEntry
 * @typedef {{ type: string, ttl_ms: number, memory: number | null, encoding: string, length: number | null,
 *   text: string | null, text_truncated: boolean, entries: RedisValueEntry[], next: string | null }} RedisKeyValue
 */

/**
 * One page of the keys table: SCAN until ~`target` keys match, then type, TTL,
 * memory, length and preview for each, in two pipelined round trips.
 * @param {string} cursor "0" to start
 * @param {{ pattern?: string, type?: string, target?: number }} [opts]
 * @returns {Promise<{ cursor: string, rows: RedisKeyRow[], scanned: number }>}
 */
export async function redisKeyPage(cursor, opts = {}) {
  return inv('redis_key_page', {
    cursor: String(cursor),
    pattern: opts.pattern || null,
    keyType: opts.type || null,
    target: opts.target ?? 200,
  })
}

/** @param {RedisKeyRef[]} keys @returns {Promise<RedisKeyRow[]>} */
export async function redisKeyRows(keys) {
  return inv('redis_key_rows', { keys })
}

/**
 * @param {RedisKeyRef} key
 * @param {string | null} [cursor] next-page cursor from a previous call
 * @param {number} [count]
 * @returns {Promise<RedisKeyValue>}
 */
export async function redisKeyValue(key, cursor = null, count = 200) {
  return inv('redis_key_value', { key, cursor, count })
}

/**
 * Apply staged writes in one MULTI/EXEC. Returns each op's outcome.
 * @param {{ args: string[], binary_key?: boolean }[]} ops
 * @returns {Promise<{ ok: boolean, error: string | null }[]>}
 */
/** One Redis command, reply as JSON (nested arrays kept). @param {string[]} args */
export async function redisCommand(args) {
  return inv('redis_command', { args: args.map(String) })
}

/**
 * Subscribe to channels and/or patterns. Calls onmessage for each message until
 * the returned stop function runs.
 * @param {string[]} channels @param {string[]} patterns
 * @param {(m: { channel: string, pattern: string | null, payload: string, at: number }) => void} onmessage
 */
export async function redisSubscribe(channels, patterns, onmessage) {
  const id = `sub-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
  const { listen } = await import('@tauri-apps/api/event')
  const off = await listen(`redis-pubsub-${id}`, (e) => onmessage(/** @type {any} */ (e.payload)))
  try {
    await inv('redis_subscribe', { id, channels, patterns })
  } catch (e) {
    off()
    throw e
  }
  return async () => {
    off()
    await inv('redis_unsubscribe', { id }).catch(() => {})
  }
}

export async function redisApply(ops) {
  return inv('redis_apply', { ops })
}

// ── DynamoDB ──────────────────────────────────────────────────────────────────

/**
 * @typedef {{ name?: string, region?: string, profile?: string, accessKeyId?: string,
 *   secretAccessKey?: string, sessionToken?: string, endpoint?: string }} DynamoConnConfig
 */

const DYNAMO_WRITES = new Set(['PutItem', 'UpdateItem', 'DeleteItem', 'BatchWriteItem', 'TransactWriteItems'])

/** @param {DynamoConnConfig} config */
function normalizeDynamo(config) {
  /** @param {string | undefined} s */
  const opt = (s) => (s && String(s).trim()) || null
  return {
    type: 'dynamodb',
    name: String(config.name || 'DynamoDB'),
    region: String(config.region || '').trim(),
    profile: opt(config.profile),
    accessKeyId: opt(config.accessKeyId),
    secretAccessKey: opt(config.secretAccessKey),
    sessionToken: opt(config.sessionToken),
    endpoint: opt(config.endpoint),
  }
}

/** @param {DynamoConnConfig} config */
export async function testDynamo(config) {
  return inv('test_dynamodb', { config: normalizeDynamo(config) })
}

/** @param {DynamoConnConfig} config */
export async function connectDynamo(config) {
  return connectInv('connect_dynamodb_db', { config: normalizeDynamo(config) })
}

/** Profile names from ~/.aws/config and ~/.aws/credentials. @returns {Promise<string[]>} */
export async function dynamoProfiles() {
  return inv('dynamodb_profiles')
}

/**
 * One DynamoDB API call on the active connection, e.g. `('Scan', { TableName })`.
 * Only read/item-write operations are allowed by the backend.
 * @param {string} op
 * @param {Record<string, any>} [body]
 * @returns {Promise<any>}
 */
export async function dynamoRequest(op, body = {}) {
  if (DYNAMO_WRITES.has(op)) assertWritable('change items')
  return inv('dynamodb_request', { op, body })
}

// ── CouchDB ───────────────────────────────────────────────────────────────────

/** @typedef {{ name?: string, url?: string, user?: string, password?: string }} CouchConnConfig */

/** @param {CouchConnConfig} config */
function normalizeCouch(config) {
  return {
    type: 'couchdb',
    name: String(config.name || 'CouchDB'),
    url: String(config.url || 'http://localhost:5984').trim(),
    user: (config.user && String(config.user).trim()) || null,
    password: config.password || null,
  }
}

/** @param {CouchConnConfig} config */
export async function testCouch(config) {
  return inv('test_couchdb', { config: normalizeCouch(config) })
}

/** @param {CouchConnConfig} config */
export async function connectCouch(config) {
  return connectInv('connect_couchdb_db', { config: normalizeCouch(config) })
}

/**
 * One CouchDB HTTP call on the active connection (`path` relative to the
 * server). Server administration and dropping databases are refused.
 * @param {'GET'|'POST'|'PUT'|'DELETE'} method @param {string} path @param {any} [body]
 * @returns {Promise<any>}
 */
export async function couchRequest(method, path, body) {
  if (method !== 'GET' && !path.endsWith('/_find')) assertWritable('change documents')
  return inv('couchdb_request', { method, path, body: body ?? null })
}

// ── MongoDB ───────────────────────────────────────────────────────────────────

/** @typedef {{ name?: string, uri?: string, database?: string }} MongoConnConfig */

/** @param {MongoConnConfig} config */
function normalizeMongo(config) {
  return {
    type: 'mongodb',
    name: String(config.name || 'MongoDB'),
    uri: String(config.uri || 'mongodb://localhost:27017').trim(),
    database: (config.database && String(config.database).trim()) || null,
  }
}

/** @param {MongoConnConfig} config */
export async function testMongo(config) {
  return inv('test_mongo', { config: normalizeMongo(config) })
}

/** @param {MongoConnConfig} config */
export async function connectMongo(config) {
  return connectInv('connect_mongo_db', { config: normalizeMongo(config) })
}

/** Every user collection as `db.collection`. @returns {Promise<string[]>} */
export async function mongoCollections() {
  return inv('mongo_collections')
}

/** @param {string} collection @returns {Promise<{ count: number, indexes: string[], size: number | null, capped: boolean }>} */
export async function mongoDescribe(collection) {
  return inv('mongo_describe', { collection })
}

/**
 * One page of documents. `filter`, `sort` and `projection` are objects in shell syntax.
 * @param {string} collection @param {string} filter @param {number} skip
 * @param {{ limit?: number, sort?: string, projection?: string }} [opts]
 * @returns {Promise<{ docs: Record<string, any>[], next: number | null }>}
 */
export async function mongoFind(collection, filter, skip, opts = {}) {
  return inv('mongo_find', {
    collection,
    filter,
    sort: opts.sort || null,
    projection: opts.projection || null,
    skip,
    limit: opts.limit ?? 20,
  })
}

/** Documents matching `filter` (the cheap estimate when it is empty). @param {string} collection @param {string} filter @returns {Promise<number>} */
export async function mongoCount(collection, filter) {
  return inv('mongo_count', { collection, filter })
}

/**
 * Point the query editor at the sidebar's schema (MongoDB database, Cassandra
 * keyspace). A no-op for engines without one.
 * @param {string} schema
 */
export async function docUseSchema(schema) {
  return inv('doc_use_schema', { schema })
}

/** @param {string} collection @param {Record<string, any>} doc @param {any} prevId @returns {Promise<Record<string, any>>} */
export async function mongoSave(collection, doc, prevId) {
  assertWritable('change documents')
  return inv('mongo_save', { collection, doc, prevId: prevId ?? null })
}

/** @param {string} collection @param {any} id */
export async function mongoDelete(collection, id) {
  assertWritable('delete documents')
  return inv('mongo_delete', { collection, id })
}

/**
 * Explain a find (executionStats): the stage tree, totals and a one-line verdict.
 * @param {string} collection @param {string} filter @param {{ sort?: string, projection?: string }} [opts]
 * @returns {Promise<import('$lib/doc-adapters.js').MongoExplain>}
 */
export async function mongoExplain(collection, filter, opts = {}) {
  return inv('mongo_explain', { collection, filter, sort: opts.sort || null, projection: opts.projection || null })
}

/**
 * Field paths inferred from a `$sample` of up to `size` documents.
 * @param {string} collection @param {string} filter @param {number} [size]
 * @returns {Promise<import('$lib/doc-adapters.js').MongoSchema>}
 */
export async function mongoSchema(collection, filter, size = 1000) {
  return inv('mongo_schema', { collection, filter, size })
}

/** @param {string} collection @returns {Promise<import('$lib/doc-adapters.js').MongoIndex[]>} */
export async function mongoIndexes(collection) {
  return inv('mongo_indexes', { collection })
}

/**
 * @param {string} collection @param {string} keys `{field: 1, other: -1}` in shell syntax
 * @param {{ name?: string, unique?: boolean, sparse?: boolean, hidden?: boolean, ttlSeconds?: number | null, partialFilter?: string }} options
 * @returns {Promise<string>} the index name
 */
export async function mongoCreateIndex(collection, keys, options) {
  assertWritable('create indexes')
  return inv('mongo_create_index', { collection, keys, options })
}

/** @param {string} collection @param {string} name */
export async function mongoDropIndex(collection, name) {
  assertWritable('drop indexes')
  return inv('mongo_drop_index', { collection, name })
}

/**
 * `$set` one field (or `$unset` it); returns the stored document.
 * @param {string} collection @param {any} id @param {string} path @param {any} value
 * @param {{ typeHint?: string, unset?: boolean }} [opts]
 * @returns {Promise<Record<string, any>>}
 */
export async function mongoUpdateField(collection, id, path, value, opts = {}) {
  assertWritable('change documents')
  return inv('mongo_update_field', { collection, id, path, value: value ?? null, typeHint: opts.typeHint || null, unset: !!opts.unset })
}

/**
 * Run a pipeline (shell syntax) and return at most `limit` documents.
 * @param {string} collection @param {string} pipeline @param {number} [limit]
 * @returns {Promise<Record<string, any>[]>}
 */
export async function mongoAggregate(collection, pipeline, limit = 10) {
  return inv('mongo_aggregate', { collection, pipeline, limit })
}

/** Native picker for a JSON / JSON Lines / CSV file. @returns {Promise<string | null>} */
export async function pickImportFile() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const path = await open({ multiple: false, filters: [{ name: 'Data', extensions: ['json', 'jsonl', 'ndjson', 'csv'] }] })
  return typeof path === 'string' ? path : null
}

/**
 * @param {string} path @param {{ format?: string, infer?: boolean }} [opts]
 * @returns {Promise<{ fileName: string, format: string, total: number, preview: Record<string, any>[] }>}
 */
export async function mongoImportPreview(path, opts = {}) {
  return inv('mongo_import_preview', { path, format: opts.format || null, infer: opts.infer ?? true })
}

/**
 * @param {string} collection @param {string} path @param {{ format?: string, infer?: boolean }} [opts]
 * @returns {Promise<{ inserted: number, failed: number, errors: string[] }>}
 */
export async function mongoImport(collection, path, opts = {}) {
  assertWritable('import documents')
  return inv('mongo_import', { collection, path, format: opts.format || null, infer: opts.infer ?? true })
}

// ── Firestore ─────────────────────────────────────────────────────────────────

/**
 * @typedef {{ name?: string, projectId?: string, database?: string, auth?: 'emulator' | 'serviceAccount' | 'gcloud',
 *   keyFile?: string, emulatorHost?: string }} FirestoreConnConfig
 */

/** @param {FirestoreConnConfig} config */
function normalizeFirestore(config) {
  /** @param {string | undefined} s */
  const opt = (s) => (s && String(s).trim()) || null
  return {
    type: 'firestore',
    name: String(config.name || 'Firestore'),
    projectId: String(config.projectId || '').trim(),
    database: opt(config.database),
    auth: config.auth || 'serviceAccount',
    keyFile: opt(config.keyFile),
    emulatorHost: opt(config.emulatorHost),
  }
}

/** @param {FirestoreConnConfig} config */
export async function testFirestore(config) {
  return inv('test_firestore', { config: normalizeFirestore(config) })
}

/** @param {FirestoreConnConfig} config */
export async function connectFirestore(config) {
  return connectInv('connect_firestore_db', { config: normalizeFirestore(config) })
}

/**
 * One Firestore REST call on the active connection; `path` starts with
 * `projects/...`. Only this database's documents are reachable.
 * @param {'GET'|'POST'|'PATCH'|'DELETE'} method @param {string} path @param {any} [body]
 */
export async function firestoreRequest(method, path, body) {
  const read = method === 'GET' || (method === 'POST' && /:(runQuery|listCollectionIds|runAggregationQuery)$/.test(path))
  if (!read) assertWritable('change documents')
  return inv('firestore_request', { method, path, body: body ?? null })
}

// ── Cassandra ─────────────────────────────────────────────────────────────────

/** @typedef {{ name?: string, hosts?: string, user?: string, password?: string, keyspace?: string }} CassandraConnConfig */

/** @param {CassandraConnConfig} config */
function normalizeCassandra(config) {
  /** @param {string | undefined} s */
  const opt = (s) => (s && String(s).trim()) || null
  return {
    type: 'cassandra',
    name: String(config.name || 'Cassandra'),
    hosts: String(config.hosts || '127.0.0.1').trim(),
    user: opt(config.user),
    password: config.password || null,
    keyspace: opt(config.keyspace),
  }
}

/** @param {CassandraConnConfig} config */
export async function testCassandra(config) {
  return inv('test_cassandra', { config: normalizeCassandra(config) })
}

/** @param {CassandraConnConfig} config */
export async function connectCassandra(config) {
  return connectInv('connect_cassandra_db', { config: normalizeCassandra(config) })
}

/** `keyspace.table` for every user table. @returns {Promise<string[]>} */
export async function cassandraTables() {
  return inv('cassandra_tables')
}

/**
 * @param {string} table keyspace.table
 * @returns {Promise<{ keyspace: string, table: string, keys: string[], partition_keys: string[], columns: [string, string][] }>}
 */
export async function cassandraDescribe(table) {
  return inv('cassandra_describe', { table })
}

/**
 * One page of rows as JSON (`SELECT JSON`), optionally filtered by a CQL WHERE clause.
 * @param {string} table @param {string} filter @param {string | null} cursor @param {number} [size]
 * @returns {Promise<{ rows: Record<string, any>[], next: string | null }>}
 */
export async function cassandraPage(table, filter, cursor, size = 100) {
  return inv('cassandra_page', { table, filter, cursor, size })
}

/** @param {string} table @param {Record<string, any>} row @param {Record<string, any> | null} prev */
export async function cassandraSave(table, row, prev) {
  assertWritable('change rows')
  return inv('cassandra_save', { table, row, prev })
}

/** @param {string} table @param {Record<string, any>} row */
export async function cassandraDelete(table, row) {
  assertWritable('delete rows')
  return inv('cassandra_delete', { table, row })
}

// ── DuckDB ──────────────────────────────────────────────────────────────────────

/** @param {{ name: string, filePath: string }} config */
function normalizeDuckdb(config) {
  return {
    name: String(config.name || 'DuckDB'),
    filePath: String(config.filePath || ':memory:'),
  }
}

/** @param {{ name: string, filePath: string }} config */
export async function testDuckdbConnection(config) {
  return inv('test_duckdb', { config: normalizeDuckdb(config) })
}

/** @param {{ name: string, filePath: string }} config */
export async function connectDuckdb(config) {
  return connectInv('connect_duckdb_db', { config: normalizeDuckdb(config) })
}

// ── MS SQL Server ─────────────────────────────────────────────────────────────

/** @param {{ name: string, host: string, port: number|string, database: string, user: string, password: string, encrypt?: boolean, trustCert?: boolean }} config */
function normalizeMssql(config) {
  return {
    name: String(config.name || 'SQL Server'),
    host: String(config.host || '127.0.0.1'),
    port: Math.min(65535, Math.max(1, Number(config.port) || 1433)),
    database: String(config.database || 'master'),
    user: String(config.user || 'sa'),
    password: String(config.password || ''),
    encrypt: Boolean(config.encrypt),
    trustCert: Boolean(config.trustCert),
  }
}

/** @param {{ name: string, host: string, port: number|string, database: string, user: string, password: string, encrypt?: boolean, trustCert?: boolean }} config */
export async function testMssqlConnection(config) {
  return inv('test_mssql', { config: normalizeMssql(config) })
}

/** @param {{ name: string, host: string, port: number|string, database: string, user: string, password: string, encrypt?: boolean, trustCert?: boolean }} config */
export async function connectMssql(config) {
  return connectInv('connect_mssql_db', { config: normalizeMssql(config) })
}

// ── Oracle ────────────────────────────────────────────────────────────────────

/**
 * @typedef {{ name?: string, host?: string, port?: number|string, serviceName?: string, sid?: string,
 *   connectString?: string, user?: string, password?: string, clientLibDir?: string }} OracleConnConfig
 */

/** @param {OracleConnConfig} config */
function normalizeOracle(config) {
  return {
    name: String(config.name || 'Oracle'),
    host: String(config.host || '127.0.0.1').trim(),
    port: Math.min(65535, Math.max(1, Number(config.port) || 1521)),
    serviceName: String(config.serviceName ?? '').trim(),
    sid: String(config.sid ?? '').trim(),
    connectString: String(config.connectString ?? '').trim(),
    user: String(config.user ?? '').trim(),
    password: String(config.password ?? ''),
    clientLibDir: String(config.clientLibDir ?? '').trim(),
  }
}

/** @param {OracleConnConfig} config */
export async function testOracleConnection(config) {
  return inv('test_oracle', { config: normalizeOracle(config) })
}

/** @param {OracleConnConfig} config */
export async function connectOracle(config) {
  return connectInv('connect_oracle_db', { config: normalizeOracle(config) })
}

/** The container the Oracle session is in: a pluggable database (FREEPDB1) or CDB$ROOT. */
export async function oracleContainerName() {
  return /** @type {Promise<string>} */ (inv('oracle_container_name'))
}

/**
 * Resolve saved-connection hostnames into the OS resolver cache ahead of time.
 *
 * A cold lookup measured 4147ms against a cached 58ms, and the connect tracked
 * it almost exactly - so paying it while the user is still choosing is the
 * difference between a 4s connect and a 300ms one. Fire-and-forget.
 * @param {string[]} hosts
 */
export function prewarmDns(hosts) {
  return inv('prewarm_dns', { hosts }).catch(() => {})
}

// ── OmniRoute (local OpenAI-compatible proxy) ────────────────────────────────

/** What's on this machine: `{ node, npm, omniroute }`, each a version or null. */
export async function omniRouteEnv() {
  return inv('omniroute_env')
}

/** `npm i -g omniroute`. Streams progress as `omniroute-log` events. */
export async function omniRouteInstall() {
  return inv('omniroute_install')
}

/** Start the proxy and wait until the port serves. Returns its base URL. */
export async function omniRouteStart(port) {
  return inv('omniroute_start', { port })
}

export async function omniRouteStop() {
  return inv('omniroute_stop')
}

/** True when something is serving on the port - ours or the user's own. */
export async function omniRouteRunning(port) {
  return inv('omniroute_running', { port })
}

// ── Docker ────────────────────────────────────────────────────────────────────

/** Returns Docker server version string, or throws a user-facing error. */
export async function dockerCheck() {
  return inv('docker_check')
}

/**
 * Pull + run a database container. Streams log events as `docker-log:{eventId}`.
 * @param {'postgres'|'mysql'} dbType
 * @param {string} eventId
 */
export async function dockerRunDb(dbType, eventId) {
  return inv('docker_run_db', { dbType, eventId })
}

// ── Local ORM studios ─────────────────────────────────────────────────────────

/**
 * Prisma Studio / Drizzle Studio instances running on this machine, each with
 * the database it is pointed at - read from that project's own schema/config, so
 * a row can be connected to without anyone typing a connection string.
 *
 * @typedef {{
 *   id: string, tool: 'prisma'|'drizzle', toolLabel: string, pid: number,
 *   port: number, listening: boolean, projectDir: string, projectName: string,
 *   engine: string | null, url: string | null, filePath: string | null,
 *   authToken: string | null, accountId: string | null, databaseId: string | null,
 *   apiToken: string | null, target: string, source: string, reason: string | null,
 * }} DetectedStudio
 *
 * @returns {Promise<DetectedStudio[]>}
 */
export async function scanLocalStudios() {
  return inv('scan_local_studios')
}

/**
 * What the Docker launcher can start: one row per image, grouped by kind.
 * @typedef {{ id: string, label: string, engine: string, icon: string, group: string, image: string, note: string, port: number }} DockerTemplate
 * @returns {Promise<DockerTemplate[]>}
 */
export async function dockerTemplates() {
  return inv('docker_templates')
}

/**
 * Database containers running in Docker, with the credentials they were started
 * with - read from the container's own environment, so nothing has to be typed.
 * Docker missing or stopped comes back as an empty list, never an error.
 *
 * @typedef {{
 *   name: string, containerId: string, image: string, engine: string,
 *   host: string, port: number, user: string, password: string,
 *   database: string, target: string, reason: string | null,
 * }} DockerDatabase
 *
 * @returns {Promise<DockerDatabase[]>}
 */
export async function scanDockerDatabases() {
  return inv('scan_docker_databases')
}

/**
 * Start, stop or restart a container (the Docker card's right-click menu).
 * @param {string} container name or id
 * @param {'start' | 'stop' | 'restart'} action
 */
export async function dockerContainerAction(container, action) {
  return inv('docker_container_action', { container, action })
}

/**
 * Database servers installed natively on this machine. There is no password to
 * recover for these - the row carries the engine's conventional local superuser.
 *
 * @typedef {{
 *   id: string, name: string, pid: number, engine: string, host: string,
 *   port: number, user: string, database: string, target: string,
 * }} MachineDatabase
 *
 * @returns {Promise<MachineDatabase[]>}
 */
export async function scanMachineDatabases() {
  return inv('scan_machine_databases')
}

// ── Shared disconnect ─────────────────────────────────────────────────────────

export async function disconnectPostgres() {
  return inv('disconnect_postgres')
}

export async function listSchemas() {
  return inv('pg_list_schemas')
}

/** @param {string} schema */
export async function listTables(schema) {
  return inv('pg_list_tables', { schema })
}

/**
 * Exact row counts for tables that `listTables` returned with an unknown
 * (null/-1) count. Called in a background pass so the sidebar renders
 * immediately and counts fill in as they resolve.
 * @param {string} schema
 * @param {string[]} tables
 * @returns {Promise<{ name: string, rowCount: number }[]>}
 */
export async function getTableRowCounts(schema, tables) {
  return inv('pg_table_row_counts', { schema, tables })
}

/**
 * A schema's indexes. MongoDB, CouchDB and DynamoDB keep indexes per
 * collection and answer only when `table` is given; the SQL engines answer for
 * the whole schema either way. Rows with `indexType: 'key'` are an engine's key
 * columns (Cassandra partition / clustering, DynamoDB partition / sort).
 * @param {string} schema @param {string} [table]
 */
export async function listIndexes(schema, table) {
  return inv('pg_list_indexes', table ? { schema, table } : { schema })
}

/**
 * @param {string} schema
 * @param {string} table
 */
export async function getTableColumnStructure(schema, table) {
  return inv('pg_get_table_column_structure', { schema, table })
}

/**
 * Column structure for every table in a schema, in one call. The ER diagram
 * needs the whole schema before it can lay anything out, and asking per table
 * put an IPC round trip in front of each one.
 * @param {string} schema
 * @returns {Promise<{ table: string, columns: any[] }[]>}
 */
export async function getSchemaColumnStructure(schema) {
  return inv('pg_get_schema_column_structure', { schema })
}

/**
 * Returns tables that have a FK pointing TO the given table (reverse/incoming relationships).
 * @param {string} schema @param {string} table
 * @returns {Promise<Array<{ fromSchema:string, fromTable:string, fromColumns:string[], toColumns:string[], constraintName:string }>>}
 */
export async function getIncomingForeignKeys(schema, table) {
  return inv('pg_get_incoming_foreign_keys', { schema, table })
}

/** @param {string} schema */
export async function listEnums(schema) {
  return inv('pg_list_enums', { schema })
}

/** @param {string} schema */
export async function listTriggers(schema) {
  return inv('pg_list_triggers', { schema })
}

/** @param {string} schema */
export async function listSequences(schema) {
  return inv('pg_list_sequences', { schema })
}

/** @param {string} schema */
export async function listFunctions(schema) {
  return inv('pg_list_functions', { schema })
}

/**
 * @typedef {{ kind: string, name: string, args: string, table: string, detail: string, subtype: string, comment: string | null }} DbObject
 * @typedef {{ kinds: string[], cascade: boolean, objects: DbObject[] }} ObjectListing
 */

/**
 * Functions, procedures, triggers, sequences, types and events in a schema,
 * and which of those kinds the engine has. Views come from `listTables`.
 * @param {string} schema
 * @returns {Promise<ObjectListing>}
 */
export async function listDbObjects(schema) {
  return inv('list_db_objects', { schema })
}

/**
 * The object as a statement that recreates it (CREATE OR REPLACE where the
 * engine has one, a DROP ... IF EXISTS in front where it does not).
 * @param {string} schema @param {Pick<DbObject, 'kind' | 'name'> & Partial<DbObject>} obj
 * @returns {Promise<string>}
 */
export async function getObjectDefinition(schema, obj) {
  return inv('get_object_definition', { kind: obj.kind, schema, name: obj.name, args: obj.args ?? '', table: obj.table ?? '' })
}

/**
 * Drop one object. Resolves to the statement that ran.
 * @param {string} schema @param {Pick<DbObject, 'kind' | 'name'> & Partial<DbObject>} obj @param {boolean} [cascade]
 * @returns {Promise<string>}
 */
export async function dropDbObject(schema, obj, cascade = false) {
  assertWritable('drop this object')
  return inv('drop_db_object', {
    kind: obj.kind, schema, name: obj.name, args: obj.args ?? '', table: obj.table ?? '', subtype: obj.subtype ?? '', cascade,
  })
}

/**
 * Table and view comments in a schema, for the sidebar.
 * @param {string} schema
 * @returns {Promise<{ name: string, comment: string }[]>}
 */
export async function listObjectComments(schema) {
  return inv('list_object_comments', { schema })
}

/** @returns {Promise<void>} */
export async function pingConnection() {
  await inv('ping_db_connection')
}

/**
 * @param {string} schema
 * @param {string} table
 * @returns {Promise<string>}
 */
export async function getTableDdl(schema, table) {
  return /** @type {string} */ (await inv('get_table_ddl', { schema, table }))
}

/**
 * @param {import('$lib/stores/connections.js').SavedConnection} connectionConfig
 * @param {string} schema
 * @param {string} table
 * @returns {Promise<string>}
 */
export async function getTableDdlOnConnection(connectionConfig, schema, table) {
  return /** @type {string} */ (await inv('get_table_ddl_on_connection', { config: connectionConfig, schema, table }))
}

/**
 * @param {string} schema
 * @param {string} table
 */
export async function truncateTable(schema, table) {
  assertWritable('truncate a table')
  return inv('pg_truncate_table', { schema, table })
}

/**
 * @param {string} schema
 * @param {string} table
 * @param {boolean} [cascade]
 * @param {'table' | 'view' | 'materialized_view'} [kind] views need DROP VIEW
 */
export async function dropTable(schema, table, cascade = false, kind = 'table') {
  assertWritable('drop a table')
  return inv('pg_drop_table', { schema, table, cascade, kind })
}

/**
 * Copy a database on the current server under a new name, structure and rows.
 * The copy runs server-side; nothing passes through the app.
 * @param {string} source
 * @param {string} target
 * @returns {Promise<{ message: string, warnings: string[] }>}
 */
export async function cloneDatabase(source, target) {
  assertWritable('copy a database')
  return inv('pg_clone_database', { source, target })
}

/**
 * @param {string} schema
 * @param {string} table
 * @param {number} limit
 * @param {number} offset
 * @param {{
 *   search?: string
 *   searchIsRegex?: boolean
 *   sortColumn?: string
 *   sortDirection?: 'asc' | 'desc'
 *   filters?: { column: string, op: string, value?: string }[]
 *   includeMeta?: boolean
 * }} [query]
 *
 * `includeMeta` (default true): fetch column enums/nullability, primary key and
 * foreign keys. Pass false on repeat fetches of a table already loaded
 * (pagination, sort, filter, live refresh) to skip those catalog round-trips -
 * the caller keeps the metadata it already has.
 */
export async function getTableRows(schema, table, limit, offset, query = {}) {
  const _t0 = performance.now()
  const r = await inv('pg_get_table_rows', {
    schema,
    table,
    limit,
    offset,
    search: query.search?.trim() || null,
    searchIsRegex: query.searchIsRegex ?? false,
    searchCaseSensitive: query.searchCaseSensitive ?? false,
    sortColumn: query.sortColumn || null,
    sortDirection: query.sortDirection || null,
    // Multi-column sort keys (Postgres). Primary key stays in sortColumn above
    // so other engines still sort by it when they ignore `sorts`.
    sorts: query.sorts?.length ? query.sorts : null,
    filters: query.filters?.length ? query.filters : null,
    // Keyset (cursor) pagination anchor - null = classic OFFSET (Postgres only).
    keyset: query.keyset ?? null,
    includeMeta: query.includeMeta !== false,
    // Default true. Pass false to skip COUNT(*) (returns total = -1) and paint
    // rows immediately; fetch the total separately with countTableRows().
    includeCount: query.includeCount !== false,
    // Null placement for the ORDER BY (dialects that support it); unset → default.
    nullsOrder: (() => { try { const v = loadSettings().nullSortOrder; return v === 'first' || v === 'last' ? v : null } catch { return null } })(),
    // Fetch a wide column as a size and load its value per cell (Settings →
    // Data). Off means every value on the page, whatever it weighs.
    previewWide: (() => { try { return loadSettings().lazyWideColumns !== false } catch { return true } })(),
  })
  recordQuery({ sql: r?.sql, durationMs: r?.queryMs ?? Math.round(performance.now() - _t0), schema, table, source: 'browse', success: true })
  traceRows('pg_get_table_rows', r, performance.now() - _t0)
  return r
}

/**
 * Exact (or planner-estimated) row count for a table, respecting the same
 * search/filters as getTableRows. Fetched in the background after rows land so
 * the total ("… of N") never blocks the initial paint. Returns -1 when the
 * count is unavailable (non-Postgres engine); callers keep their current total.
 * @param {string} schema
 * @param {string} table
 * @param {{ search?: string, searchIsRegex?: boolean, searchCaseSensitive?: boolean, filters?: { column: string, op: string, value?: string }[] }} [query]
 * @returns {Promise<number>}
 */
export async function countTableRows(schema, table, query = {}) {
  const n = await inv('pg_count_table_rows', {
    schema,
    table,
    search: query.search?.trim() || null,
    searchIsRegex: query.searchIsRegex ?? false,
    // Mirrors getTableRows: a count taken under a different predicate than the
    // rows is a pager that disagrees with the page.
    searchCaseSensitive: query.searchCaseSensitive ?? false,
    filters: query.filters?.length ? query.filters : null,
    // Settings → SQL editor → Exact row count: COUNT(*) even where the
    // planner's estimate would answer instantly.
    exact: (() => { try { return loadSettings().sqlEditor?.exactRowCount === true } catch { return false } })(),
  })
  return Number(n)
}

/**
 * @param {string} schema
 * @param {string} table
 * @param {string} column
 */
export async function getColumnStats(schema, table, column) {
  return inv('pg_get_column_stats', { schema, table, column })
}

/**
 * @typedef {object} AdvisorFinding
 * @property {string} checkId
 * @property {'security'|'performance'|'schema'} category
 * @property {'error'|'warning'|'info'} severity
 * @property {string} title
 * @property {string} entity
 * @property {string} entityKind
 * @property {string} description
 * @property {string | null} detail
 * @property {string | null} remediation
 * @property {number} metric
 */

/**
 * @typedef {object} AdvisorReport
 * @property {string} engine
 * @property {AdvisorFinding[]} findings
 * @property {Array<{ id: string, title: string, category: string, status: string, findings: number, ms: number, error: string | null }>} checks
 * @property {boolean} unsupported
 */

/**
 * Run the Advisor's checks over the connected database. Catalog reads only - it
 * never touches user rows and never writes.
 * @returns {Promise<AdvisorReport>}
 */
export async function advisorScan() {
  return inv('advisor_scan')
}

/**
 * Cancel a running SQL query. With `queryId` only that run is cancelled - which
 * is what lets two editor tabs run at once without one's Cancel killing the
 * other's query. Without an id, everything in flight is cancelled.
 * @param {string} [queryId]
 */
export async function cancelQuery(queryId) {
  return inv('cancel_query', { queryId: queryId ?? null })
}

/** @param {string} sql @param {string} [queryId] */
export async function executeSql(sql, queryId) {
  if (isWriteSql(sql)) assertWritable('run that statement')
  const _t0 = performance.now()
  try {
    const r = await inv('pg_execute_sql', { sql, queryId: queryId ?? null })
    recordQuery({ sql: r?.sql || sql, durationMs: r?.queryMs ?? Math.round(performance.now() - _t0), source: 'sql', success: true })
    return r
  } catch (err) {
    recordQuery({ sql, durationMs: Math.round(performance.now() - _t0), source: 'sql', success: false, error: /** @type {Error} */ (err).message })
    throw err
  }
}

/** Execute one or more SQL statements and return each result as a separate entry.
 *  @param {string} sql @param {string} [queryId] cancel handle for this run */
export async function executeSqlMulti(sql, queryId) {
  if (isWriteSql(sql)) assertWritable('run that statement')
  return await inv('pg_execute_sql_multi', { sql, queryId: queryId ?? null })
}

/**
 * executeSqlMulti for the SQL console. A single Postgres statement's rows come
 * through `onRows` in chunks while they stream in, and the reply carries none
 * of them: one reply holding a 5M-row result closed the app. Other engines and
 * scripts answer as before, rows in the reply. Resolves once the reply and
 * every chunk have landed: the backend ends the channel with a `done` chunk
 * whatever happened, and channel messages arrive in order.
 *
 * `keepInStore`: the rows go to the backend's result store under `queryId`
 * (Settings → Database → Stream query results). Chunks then carry progress
 * (`count`, `stored: true`) and only the first one carries rows; the rest are
 * read with resultWindow.
 * @typedef {{ offset: number, columns?: any[], rows: any[][], count: number, stored: boolean, done: boolean }} RowChunk
 * @param {string} sql
 * @param {string | undefined} queryId
 * @param {(chunk: RowChunk) => void} onRows
 * @param {{ keepInStore?: boolean }} [opts]
 */
export async function executeSqlStream(sql, queryId, onRows, opts = {}) {
  if (isWriteSql(sql)) assertWritable('run that statement')
  /** @type {(v?: unknown) => void} */
  let landed = () => {}
  const allLanded = new Promise((resolve) => { landed = resolve })
  const channel = new Channel((/** @type {any} */ chunk) => {
    if (chunk.rows.length || chunk.columns || chunk.stored) onRows(chunk)
    if (chunk.done) landed()
  })
  let replied = false
  try {
    const results = await inv('pg_execute_sql_stream', {
      sql,
      queryId: queryId ?? null,
      onRows: channel,
      keepInStore: opts.keepInStore === true,
    })
    replied = true
    return results
  } finally {
    // A failed invoke may never have reached the backend, so it does not wait long.
    await (replied ? allLanded : Promise.race([allLanded, new Promise((r) => setTimeout(r, 2000))]))
  }
}

/**
 * Rows [start, start + count) of a stored result, in its current order.
 * @param {string} id @param {number} start @param {number} count
 * @returns {Promise<any[][]>}
 */
export async function resultWindow(id, start, count) {
  return await inv('result_window', { id, start, count })
}

/**
 * Sort a stored result by column index (typed by its database type), or back
 * to the order it arrived in with `column: null`. Waits for every row.
 * @param {string} id @param {number | null} column @param {boolean} desc
 */
export async function resultSort(id, column, desc) {
  return await inv('result_sort', { id, column, desc })
}

/** Forget a stored result and its file. Never throws. @param {string} id */
export function resultDrop(id) {
  void invoke('result_drop', { id }).catch(() => {})
}

// ── Instance Insights (PostgreSQL + MySQL monitoring) ───────────────────────
// ── Geo view (PostGIS) ────────────────────────────────────────────────────────

/**
 * Is this connection spatial, and what can be mapped? Never throws for a
 * non-spatial database - it answers `{ available: false, layers: [] }`, so the
 * caller can ask on every connect without special-casing the engine.
 * @returns {Promise<{ available: boolean, version: string|null, layers: any[] }>}
 */
export async function geoOverview() {
  return await inv('geo_overview')
}

/**
 * Features for one viewport of one spatial column.
 *
 * `bbox` is `{ minX, minY, maxX, maxY }` in WGS84 degrees, or null for the whole
 * layer. When the viewport holds more rows than `limit`, the server returns
 * grid clusters instead (`mode: 'clusters'`, each feature carrying a `count`)
 * rather than an arbitrary truncation of the real rows.
 *
 * @param {{
 *   schema: string, table: string, column: string, kind: string,
 *   srid: number, geomType: string,
 *   bbox?: { minX: number, minY: number, maxX: number, maxY: number }|null,
 *   limit?: number, simplify?: number, clusterCell?: number,
 *   filters?: any[]|null, includeExtent?: boolean,
 * }} opts
 */
export async function geoFeatures(opts) {
  return await inv('geo_features', {
    schema: opts.schema,
    table: opts.table,
    column: opts.column,
    kind: opts.kind,
    srid: opts.srid,
    geomType: opts.geomType,
    bbox: opts.bbox
      ? {
          minX: opts.bbox.minX,
          minY: opts.bbox.minY,
          maxX: opts.bbox.maxX,
          maxY: opts.bbox.maxY,
        }
      : null,
    limit: opts.limit ?? 4000,
    simplify: opts.simplify ?? 0,
    clusterCell: opts.clusterCell ?? 1,
    filters: opts.filters ?? null,
    includeExtent: opts.includeExtent ?? false,
  })
}

export async function instanceVersion() { return await inv('instance_version') }
export async function instanceActivity() { return await inv('instance_activity') }
export async function instanceState() { return await inv('instance_state') }
export async function instanceConfig() { return await inv('instance_config') }
export async function instanceReplication() { return await inv('instance_replication') }

/**
 * Change one server setting. Postgres persists it with `ALTER SYSTEM` and
 * reloads; MySQL uses `SET PERSIST` (falling back to `SET GLOBAL`).
 * @param {string} name
 * @param {string | null} value `null` resets the setting to its default
 * @returns {Promise<{ name: string, value: string, requiresRestart: boolean, reloaded: boolean, message: string }>}
 */
export async function instanceSetConfig(name, value) {
  assertWritable('change server configuration')
  // The command takes an Option<String>, and Tauri rejects the call outright on a
  // type mismatch rather than coercing. A caller holding a number is an easy
  // mistake to make - `bind:value` on a number input produces one - so the
  // contract is enforced here rather than trusted at each call site. `null` has
  // to survive: it is what resets the setting to its default.
  return await inv('instance_set_config', { name, value: value == null ? null : String(value) })
}

/**
 * Run EXPLAIN ANALYZE on `sql` and return the parsed plan tree.
 * Works for PostgreSQL (JSON plan), MySQL (FORMAT=JSON), and SQLite (QUERY PLAN).
 * @param {string} sql
 * @returns {Promise<{ plan: object, planningTime: number, executionTime: number, driver: string }>}
 */
export async function explainSql(sql) {
  return inv('pg_explain_sql', { sql })
}

/**
 * Execute SQL against a specific saved connection without switching the active connection.
 * @param {import('$lib/stores/connections.js').SavedConnection} connectionConfig
 * @param {string} sql
 */
export async function executeSqlOnConnection(connectionConfig, sql) {
  if (isWriteSql(sql)) assertWritable('run that statement')
  try {
    return await invoke('execute_sql_on_connection', { config: connectionConfig, sql })
  } catch (err) {
    throw new Error(formatInvokeError(err))
  }
}

/** @param {import('$lib/stores/connections.js').SavedConnection} connectionConfig */
export async function listSchemasOnConnection(connectionConfig) {
  try {
    return /** @type {string[]} */ (await invoke('list_schemas_on_connection', { config: connectionConfig }))
  } catch (err) {
    throw new Error(formatInvokeError(err))
  }
}

/**
 * @param {import('$lib/stores/connections.js').SavedConnection} connectionConfig
 * @param {string} schema
 */
export async function listTablesOnConnection(connectionConfig, schema) {
  try {
    return /** @type {string[]} */ (await invoke('list_tables_on_connection', { config: connectionConfig, schema }))
  } catch (err) {
    throw new Error(formatInvokeError(err))
  }
}

/** Execute a DDL statement outside a transaction (CREATE/DROP DATABASE, etc.). */
export async function executeDdl(sql) {
  assertWritable('run that statement')
  return inv('pg_execute_ddl', { sql })
}

/**
 * @param {string} schema
 * @param {string} table
 * @param {Record<string, unknown>} primaryKey
 * @param {string} column
 * @param {unknown} value
 */
export async function updateTableCell(schema, table, primaryKey, column, value) {
  assertWritable('edit a cell')
  return inv('pg_update_table_cell', { schema, table, primaryKey, column, value })
}

/**
 * Load one cell in full.
 *
 * A browse page deliberately does not fetch wide columns - they arrive as a
 * preview with the real size attached - so this is how the whole value is read
 * when someone asks for it. Postgres only for now; other engines answer with a
 * message that says so.
 *
 * @param {string} schema
 * @param {string} table
 * @param {Record<string, unknown>} primaryKey
 * @param {string} column
 * @param {number} [maxBytes] ceiling on the text returned (default 4MB, hard max 16MB)
 * @returns {Promise<{ text: string, bytes: number, truncated: boolean }>}
 */
export async function fetchCellValue(schema, table, primaryKey, column, maxBytes) {
  return inv('pg_fetch_cell_value', { schema, table, primaryKey, column, maxBytes: maxBytes ?? null })
}

/**
 * @param {string} schema
 * @param {string} table
 * @param {Record<string, unknown>} primaryKey
 */
export async function deleteTableRow(schema, table, primaryKey) {
  assertWritable('delete a row')
  return inv('pg_delete_table_row', { schema, table, primaryKey })
}

/**
 * @param {string} schema
 * @param {string} table
 * @param {Record<string, unknown>[]} primaryKeys
 */
export async function deleteTableRows(schema, table, primaryKeys) {
  assertWritable('delete rows')
  return inv('pg_delete_table_rows', { schema, table, primaryKeys })
}

/**
 * @param {string} schema
 * @param {string} table
 * @param {Record<string, unknown>} values
 * @returns {Promise<{ row: unknown[] }>}
 */
export async function insertTableRow(schema, table, values) {
  assertWritable('insert a row')
  return inv('pg_insert_table_row', { schema, table, values })
}

/**
 * Apply a grid's staged changes as one transaction (PostgreSQL, MySQL, SQLite): every update,
 * delete and the insert lands, or none of them does. Each update or delete must
 * hit exactly one row, so a row deleted or re-keyed since the page loaded fails
 * the whole apply instead of reporting a write that never happened. A MySQL
 * table on a non-transactional engine (MyISAM, MEMORY) is refused with an error
 * containing "does not support transactions", having written nothing.
 *
 * @param {string} schema
 * @param {string} table
 * @param {{ primaryKey: Record<string, unknown>, values: Record<string, unknown> }[]} updates one entry per edited row
 * @param {Record<string, unknown>[]} deletes primary keys of rows to delete
 * @param {Record<string, unknown> | null} insert
 * @returns {Promise<{ updated: number, deleted: number, inserted: unknown[] | null }>}
 */
export async function applyTableChanges(schema, table, updates, deletes, insert) {
  assertWritable('apply changes')
  return inv('pg_apply_table_changes', { schema, table, updates, deletes, insert })
}

// ── MCP Server ────────────────────────────────────────────────────────────────

/** @returns {Promise<{ running: boolean, port: number, url: string, token: string }>} */
export async function mcpStart() {
  return inv('mcp_start')
}

/**
 * Sync credential-free connection metadata to the MCP layer so AI tools can
 * call list_databases / current_database.
 * Strip passwords/tokens before calling - this data flows into the MCP server.
 * @param {import('$lib/stores/connections.js').SavedConnection[]} connections
 * @param {string | null} activeId
 */
export async function mcpUpdateConnections(connections, activeId) {
  const safe = connections.map(({ id, name, type: t, host, port, database, filePath }) => ({
    id,
    name,
    type: t ?? 'postgres',
    ...(host ? { host } : {}),
    ...(port ? { port } : {}),
    ...(database ? { database } : {}),
    ...(filePath ? { file_path: filePath } : {}),
  }))
  // The caller is an effect that re-runs whenever the saved list is re-read
  // (every connect, every upsert), mostly with an identical payload. Skip those.
  const args = { connections: safe, activeId: activeId ?? null }
  const sig = JSON.stringify(args)
  if (sig === _mcpConnSig) return
  _mcpConnSig = sig
  return inv('mcp_update_connections', args).catch((e) => {
    _mcpConnSig = ''
    throw e
  })
}
/** Last payload sent by mcpUpdateConnections. */
let _mcpConnSig = ''

export async function mcpStop() {
  return inv('mcp_stop')
}

/** @returns {Promise<{ running: boolean, port: number, url: string, token: string }>} */
export async function mcpStatus() {
  return inv('mcp_status')
}

/** @param {boolean} readonly */
export async function mcpSetReadonly(readonly) {
  return inv('mcp_set_readonly', { readonly })
}

// ── Backup / Restore ──────────────────────────────────────────────────────────

/**
 * @typedef {{ includeSchema?: boolean, includeData?: boolean, includeSequences?: boolean, includeEnums?: boolean, includeFunctions?: boolean, includeTriggers?: boolean, includeViews?: boolean }} BackupOptions
 * @typedef {{ sql: string, tableCount: number, rowCount: number, cancelled: boolean, path: string | null, bytes: number, failedTables: number, elapsedMs: number }} BackupExportResult
 * @typedef {{ statementsOk: number, statementsErr: number, statementsSkipped: number, cancelled: boolean, errors: string[], elapsedMs: number }} BackupImportResult
 * @typedef {{ size: number, engine: string | null, stroke: boolean, incomplete: boolean }} BackupFileInfo
 */

/**
 * Export the connected database. With `path` the dump streams straight to that
 * file from Rust (no 100 MB string crossing IPC twice) and `sql` comes back
 * empty; without it the script is returned as `sql`.
 *
 * Progress arrives as `backup-progress` events (`kind: 'plan' | 'table'`) and
 * notable lines as `backup-log`.
 * @param {string | null} schema Filter to one schema (PostgreSQL/MySQL only)
 * @param {string[] | null} tables
 * @param {BackupOptions | null} options
 * @param {string | null} [path]
 * @returns {Promise<BackupExportResult>}
 */
export async function backupExport(schema = null, tables = null, options = null, path = null) {
  return inv('backup_export', { schema, tables, options, path })
}

/**
 * Restore a dump into the connected database: a file `path` (read in Rust) or
 * a `sql` string. Progress arrives as `restore-progress` events.
 * @param {{ sql?: string | null, path?: string | null }} source
 * @returns {Promise<BackupImportResult>}
 */
export async function backupImport({ sql = null, path = null }) {
  assertWritable('restore a backup')
  return inv('backup_import', { sql, path })
}

/**
 * Size and origin (engine, Stroke or not, cancelled export) of a dump file.
 * @param {string} path
 * @returns {Promise<BackupFileInfo>}
 */
export async function backupInspectFile(path) {
  return inv('backup_inspect_file', { path })
}

/**
 * Native save dialog for a backup. Returns the chosen path, or null.
 * @param {string} defaultName
 * @returns {Promise<string | null>}
 */
export async function pickBackupSavePath(defaultName) {
  const { save } = await import('@tauri-apps/plugin-dialog')
  return (await save({ defaultPath: defaultName, filters: [{ name: 'SQL', extensions: ['sql'] }] })) || null
}

/**
 * Native open dialog for a backup file. Returns the chosen path, or null.
 * @returns {Promise<string | null>}
 */
export async function pickBackupFile() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const path = await open({ multiple: false, filters: [{ name: 'SQL', extensions: ['sql', 'txt'] }] })
  return typeof path === 'string' ? path : null
}

/**
 * Show a file in Finder / Explorer / the file manager.
 * @param {string} path
 */
export async function revealInFolder(path) {
  const { revealItemInDir } = await import('@tauri-apps/plugin-opener')
  await revealItemInDir(path)
}

/**
 * Request cancellation of the running backup/restore. The backend stops at the
 * next table/statement boundary and returns the work completed so far.
 * @returns {Promise<void>}
 */
export async function backupCancel() {
  return inv('backup_cancel')
}

// ── Explicit transactions ─────────────────────────────────────────────────────

/**
 * @typedef {object} TxStatus
 * @property {boolean} open
 * @property {number} statements statements run since BEGIN
 * @property {number} rowsAffected
 * @property {number} openMs how long the transaction has been held
 * @property {string|null} engine
 */

/**
 * Open a transaction and hold its connection until it is committed or rolled
 * back. `sessionId` scopes it to one editor tab.
 * @param {string} sessionId
 * @returns {Promise<TxStatus>}
 */
export async function txBegin(sessionId) {
  return inv('tx_begin', { sessionId })
}

/**
 * Run SQL inside an open transaction. Nothing is visible elsewhere until commit.
 * @param {string} sessionId
 * @param {string} sql
 */
export async function txExecute(sessionId, sql) {
  if (isWriteSql(sql)) assertWritable('run that statement')
  const _t0 = performance.now()
  try {
    const r = await inv('tx_execute', { sessionId, sql })
    recordQuery({ sql: r?.sql || sql, durationMs: r?.queryMs ?? Math.round(performance.now() - _t0), source: 'sql', success: true })
    return r
  } catch (err) {
    recordQuery({ sql, durationMs: Math.round(performance.now() - _t0), source: 'sql', success: false, error: /** @type {Error} */ (err).message })
    throw err
  }
}

/** @param {string} sessionId @returns {Promise<TxStatus>} */
export async function txCommit(sessionId) {
  assertWritable('commit a transaction')
  return inv('tx_commit', { sessionId })
}

/** @param {string} sessionId @returns {Promise<TxStatus>} */
export async function txRollback(sessionId) {
  return inv('tx_rollback', { sessionId })
}

/** @param {string} sessionId @returns {Promise<TxStatus>} */
export async function txStatus(sessionId) {
  return inv('tx_status', { sessionId })
}

/**
 * A read for a console write's undo copy (sql-undo.js), outside any
 * transaction. Not logged as a query of the user's, and stoppable through the
 * run's own `queryId`.
 * @param {string} sql @param {string} [queryId]
 */
export async function inspectSql(sql, queryId) {
  return inv('pg_execute_sql', { sql, queryId: queryId ?? null })
}

/**
 * txExecute for the undo copy's transaction: the same, without a query-log
 * entry for each of its reads. The run itself is recorded in history as usual.
 * @param {string} sessionId @param {string} sql
 */
export async function txExecuteQuiet(sessionId, sql) {
  return inv('tx_execute', { sessionId, sql })
}

// ── Terminal ──────────────────────────────────────────────────────────────────
// The connection's own CLI (psql, mysql, sqlite3, sqlcmd, redis-cli) in a
// pseudo-terminal. See src-tauri/src/db/terminal.rs.

/**
 * @typedef {object} TerminalClient
 * @property {string} name `psql`, `mysql`, `mariadb`... '' when the engine has no shell
 * @property {string | null} path the binary that runs; null when not installed
 * @property {string | null} version its `--version` line
 * @property {string} hint how to install it, or why the engine has none
 */

/**
 * @param {Record<string, unknown>} config the saved connection, `type` set to its engine
 * @returns {Promise<TerminalClient>}
 */
export async function terminalClient(config) {
  return inv('terminal_client', { config })
}

/**
 * Install the connection's CLI with the OS package manager (brew, winget, or
 * the Linux one through pkexec). Resolves to where it landed; rejects with
 * the reason, or with install advice when there is no one-line install.
 * @param {Record<string, unknown>} config
 * @returns {Promise<string>}
 */
export async function terminalInstallClient(config) {
  return inv('terminal_install_client', { config })
}

/**
 * Start the client. Its bytes travel over the returned WebSocket URL, not IPC:
 * binary frames both ways, text frames for control (`{"resize":[c,r]}` in,
 * `{"exit":code}` out).
 * @param {Record<string, unknown>} config
 * @param {number} cols
 * @param {number} rows
 * @returns {Promise<{ id: string, url: string }>}
 */
export async function terminalOpen(config, cols, rows) {
  return inv('terminal_open', { config, cols, rows })
}

/** @param {string} id */
export async function terminalClose(id) {
  return inv('terminal_close', { id })
}

// ── Data import ───────────────────────────────────────────────────────────────

/**
 * @typedef {object} ImportOptions
 * @property {'error'|'skip'|'update'} [conflict] what to do when a row already exists
 * @property {string[]} [conflictColumns] the columns that decide a conflict
 * @property {boolean} [abortOnError] true (default) imports all-or-nothing
 * @property {number} [batchSize] rows per INSERT statement
 */

/**
 * @typedef {object} ImportRowsResult
 * @property {number} inserted
 * @property {number} failed
 * @property {number} skipped rows never attempted
 * @property {boolean} cancelled
 * @property {boolean} rolledBack true when nothing was committed
 * @property {{ row: number, message: string }[]} errors
 */

/**
 * Bulk-insert parsed rows into an existing table.
 *
 * `rows` must be positional: one value per entry in `columns`, same order.
 * @param {string|null} schema
 * @param {string} table
 * @param {string[]} columns
 * @param {unknown[][]} rows
 * @param {ImportOptions|null} [options]
 * @returns {Promise<ImportRowsResult>}
 */
export async function importRows(schema, table, columns, rows, options = null) {
  assertWritable('import rows')
  return inv('import_rows', { schema, table, columns, rows, options })
}

/**
 * Request cancellation of the running import. The backend stops at the next
 * batch; an all-or-nothing import then rolls back what it had written.
 * @returns {Promise<void>}
 */
export async function importCancel() {
  return inv('import_cancel')
}

// ── Autostart ─────────────────────────────────────────────────────────────────

export async function enableAutostart() {
  return inv('enable_autostart')
}

export async function disableAutostart() {
  return inv('disable_autostart')
}

export async function getAutostartStatus() {
  return /** @type {Promise<boolean>} */ (inv('get_autostart_status'))
}

// ── File I/O ──────────────────────────────────────────────────────────────────

/** @param {string} path @returns {Promise<string>} */
export async function readFile(path) {
  return inv('read_file', { path })
}

/**
 * Show an open dialog filtered to .sqlnb files, then read the selected file.
 * @returns {Promise<{ path: string, content: string } | null>}
 */
export async function openNotebookFile() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const path = await open({
    filters: [{ name: 'SQL Notebook', extensions: ['sqlnb'] }],
    multiple: false,
  })
  if (!path || typeof path !== 'string') return null
  const content = await readFile(path)
  return { path, content }
}

/**
 * Show a save dialog for .sqlnb files, then write the content.
 * @param {string} content
 * @param {string} [defaultName]
 * @returns {Promise<string | null>} saved path or null if cancelled
 */
export async function saveNotebookAs(content, defaultName = 'notebook.sqlnb') {
  const { save } = await import('@tauri-apps/plugin-dialog')
  const path = await save({
    filters: [{ name: 'SQL Notebook', extensions: ['sqlnb'] }],
    defaultPath: defaultName,
  })
  if (!path) return null
  await inv('save_file', { path, content })
  return path
}

/**
 * Write notebook content to a known path (no dialog).
 * @param {string} path
 * @param {string} content
 */
export async function saveNotebook(path, content) {
  return inv('save_file', { path, content })
}

/**
 * Search the web for the AI agent.
 *
 * Runs in Rust, not the webview: no search endpoint sends CORS headers that
 * would let a `fetch()` from the app origin read the response.
 * @param {string} query
 * @param {number} [limit]
 * @returns {Promise<{ title: string, url: string, snippet: string }[]>}
 */
export async function aiWebSearch(query, limit = 5) {
  return inv('ai_web_search', { query, limit })
}

/**
 * Fetch one page and return its readable text (tags, script and style stripped).
 * @param {string} url
 * @returns {Promise<string>}
 */
export async function aiFetchPage(url) {
  return inv('ai_fetch_page', { url })
}

/**
 * Show a native save dialog and write the payload to the chosen path.
 *
 * `<a download>` is silently ignored inside WKWebView, so every export that has
 * to land on disk goes through the dialog + a Rust write instead. Returns the
 * saved path so the caller can name it in a toast, or null when cancelled.
 *
 * @param {Blob | string} payload
 * @param {string} defaultName file name pre-filled in the dialog
 * @param {{ name: string, extensions: string[] }} filter
 * @returns {Promise<string | null>}
 */
export async function saveExportAs(payload, defaultName, filter) {
  const { save } = await import('@tauri-apps/plugin-dialog')
  const path = await save({ defaultPath: defaultName, filters: [filter] })
  if (!path) return null
  if (typeof payload === 'string') {
    await inv('save_file', { path, content: payload })
  } else {
    await inv('save_file_bytes', { path, base64: await blobToBase64(payload) })
  }
  return path
}

/** @param {Blob} blob */
async function blobToBase64(blob) {
  const bytes = new Uint8Array(await blob.arrayBuffer())
  let binary = ''
  // Chunked: `apply` on a multi-megabyte array overflows the argument limit.
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode.apply(null, /** @type {any} */ (bytes.subarray(i, i + 0x8000)))
  }
  return btoa(binary)
}

// ── Updates ──────────────────────────────────────────────────────────────────
// Rust stages the download on disk and installs it as the app quits where the
// install can finish on its own (src-tauri/src/updates.rs).

/**
 * @typedef {{ version: string, currentVersion: string, notes: string, date: number | null, staged: boolean, applyOnQuit: boolean }} UpdateInfo
 * @typedef {{ event: 'started', data: { contentLength: number | null } } | { event: 'progress', data: { downloaded: number } } | { event: 'finished' }} UpdateDownloadEvent
 */

/**
 * Ask the release feed for a newer build. Returns the downloaded one when an
 * update is already waiting, so a second check never downloads twice.
 * @returns {Promise<UpdateInfo | null>}
 */
export async function updateCheck() {
  return inv('update_check')
}

/**
 * Download the update from the last check and stage it for install.
 * @param {(event: UpdateDownloadEvent) => void} onEvent
 * @returns {Promise<UpdateInfo>}
 */
export async function updateDownload(onEvent) {
  return inv('update_download', { onEvent: new Channel(onEvent) })
}

/**
 * The downloaded update waiting to install, if any.
 * @returns {Promise<UpdateInfo | null>}
 */
export async function updateStatus() {
  return inv('update_status')
}

/** Install the downloaded update now and relaunch. Does not return on success. */
export async function updateRestart() {
  return inv('update_restart')
}

// ── Local agent CLIs (Claude Code, Codex, OpenCode) ─────────────────────────

/**
 * @typedef {{ id: string, label: string, note: string }} AgentModel
 * @typedef {{ id: string, name: string, status: 'ready' | 'needs_auth' | 'missing', version: string | null, hint: string | null, setup: { verb: string, command: string } | null, models: AgentModel[] }} AgentInfo
 */

/** Which agent CLIs are installed. @returns {Promise<AgentInfo[]>} */
export function agentList() {
  return invoke('agent_list')
}

/**
 * Start (or restart) the agent session behind a conversation. Its events,
 * deck's `AgentEvent` serialized as `{ Variant: payload }`, go to `onEvent`.
 * @param {{ conversationId: string, agent: string, model?: string, systemPrompt: string, resume?: string | null, effort?: string | null, access?: string }} opts
 * @param {(ev: any) => void} onEvent
 */
export function agentStart(opts, onEvent) {
  return invoke('agent_start', { ...opts, onEvent: new Channel(onEvent) })
}

/** Rejects with `NO_SESSION` once the agent has exited. @param {string} conversationId @param {string} text */
export function agentSend(conversationId, text) {
  return invoke('agent_send', { conversationId, text })
}

/** @param {string} conversationId */
export function agentInterrupt(conversationId) {
  return invoke('agent_interrupt', { conversationId })
}

/** @param {string} conversationId */
export function agentStop(conversationId) {
  return invoke('agent_stop', { conversationId })
}
