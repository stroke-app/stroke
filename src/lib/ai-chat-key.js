/**
 * Which connection an AI chat belongs to.
 *
 * Chats follow the connection, not the database: switching databases on the
 * same server keeps the same list, and each chat records the database and
 * schema it was working in. The key is the server and the login, never the
 * database (or a password).
 *
 * Before 2.6 the key was `host:port/database@user`, so a database switch
 * started an empty list, and every engine without a host (SQLite, MongoDB by
 * URI, D1, DynamoDB, Firestore) shared one `undefined:undefined/...` list.
 * `chatBelongsTo` still finds those chats.
 */

/** A URL without its credentials, path or query: the server it names. @param {string} url */
function serverOf(url) {
  const s = String(url ?? '').trim()
  const m = s.match(/^([a-z][a-z0-9+.-]*:\/\/)(?:[^@/]*@)?([^/?#]*)/i)
  return m ? `${m[1].toLowerCase()}${m[2].toLowerCase()}` : s
}

/**
 * @param {Record<string, any> | null | undefined} conn
 * @returns {string}
 */
export function aiChatKey(conn) {
  if (!conn) return ''
  const t = String(conn.type ?? 'postgres')
  if (conn.filePath) return `${t}:file:${conn.filePath}`
  if (t === 'd1') return `d1:${conn.accountId ?? ''}`
  if (t === 'posthog' || t === 'firestore') return `${t}:${conn.projectId ?? ''}:${conn.emulatorHost ?? ''}`
  if (t === 'dynamodb') return `dynamodb:${conn.endpoint || conn.region || ''}:${conn.profile ?? conn.accessKeyId ?? ''}`
  if (t === 'cassandra') return `cassandra:${[].concat(conn.hosts ?? []).join(',')}@${conn.user ?? ''}`
  const url = conn.uri || conn.url
  if (url && !conn.host) return `${t}:${serverOf(url)}@${conn.user ?? ''}`
  return `${t}:${conn.host ?? ''}:${conn.port ?? ''}@${conn.user ?? ''}`
}

/**
 * Whether a stored chat's key belongs to this connection: today's key, or the
 * per-database key it was saved under before 2.6.
 * @param {string} stored
 * @param {Record<string, any> | null | undefined} conn
 */
export function chatBelongsTo(stored, conn) {
  if (!conn || !stored) return false
  if (stored === aiChatKey(conn)) return true
  return stored.startsWith(`${conn.host}:${conn.port}/`) && stored.endsWith(`@${conn.user}`)
}
