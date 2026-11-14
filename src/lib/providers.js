/**
 * Frontend bridge for database provider adapters (Neon, Supabase, PlanetScale,
 * Prisma Postgres, TiDB Cloud, Turso, Railway, Nile, Upstash, PostHog). Mirrors `cloudflare.js`: thin invoke wrappers plus a
 * metadata registry the UI renders from. The heavy lifting (OAuth, listing,
 * building a connectable spec) lives in Rust (`src-tauri/src/providers`).
 */
import { invoke } from '@tauri-apps/api/core'

/**
 * @typedef {{ db_ref: string, name: string, region: string|null, kind: string|null, host: string|null }} ProviderDatabase
 * @typedef {{ db_type: string, host: string, port: number, username: string, password: string, database: string, ssl: boolean, needs_password: boolean, name: string }} ProviderConnection
 */

/**
 * UI metadata. `mode: 'token'` providers paste a credential instead of OAuth.
 * `signIn` is the quiet line under the sign-in prompt: what kind of flow it is.
 */
export const PROVIDERS = [
  { id: 'neon',        name: 'Neon',            mode: 'oauth', engine: 'postgres', blurb: 'Serverless Postgres, one-click connect', signIn: 'Secure PKCE flow' },
  { id: 'supabase',    name: 'Supabase',        mode: 'oauth', engine: 'postgres', blurb: 'Postgres platform, asks for your DB password once', signIn: 'Secure PKCE flow' },
  { id: 'planetscale', name: 'PlanetScale',     mode: 'oauth', engine: 'mysql',    blurb: 'Serverless MySQL, mints fresh credentials on connect', signIn: 'Standard OAuth flow' },
  { id: 'prisma',      name: 'Prisma Postgres', mode: 'oauth', engine: 'postgres', blurb: 'Serverless Postgres, sign in with Prisma', signIn: 'Secure PKCE flow' },
  { id: 'tidb',        name: 'TiDB Cloud',      mode: 'oauth', engine: 'mysql',    blurb: 'Serverless MySQL, creates a SQL user on connect', signIn: 'Confirm a one-time code in the browser' },
  { id: 'turso',       name: 'Turso',           mode: 'oauth', engine: 'libsql',   blurb: 'Edge SQLite, creates a database token on connect', signIn: 'Same sign-in as the Turso CLI' },
  { id: 'railway',     name: 'Railway',         mode: 'oauth', engine: 'postgres', blurb: 'Postgres, MySQL and Redis services on Railway', signIn: 'Secure PKCE flow' },
  { id: 'nile',        name: 'Nile',            mode: 'oauth', engine: 'postgres', blurb: 'Multi-tenant Postgres, creates credentials on connect', signIn: 'Same sign-in as the Nile CLI' },
  {
    id: 'upstash', name: 'Upstash', mode: 'token', engine: 'redis',
    blurb: 'Serverless Redis, connect with a Developer API key',
    // Paste-a-credential providers describe their own fields. `join` builds the
    // one string the backend stores (and splits again) from the field values.
    token: {
      help: 'Create a Developer API key in the Upstash console under Account, Management API.',
      helpUrl: 'https://console.upstash.com/account/api',
      fields: [
        { key: 'email', label: 'Account email', type: 'email', autocomplete: 'email', placeholder: 'you@example.com' },
        { key: 'apiKey', label: 'API key', type: 'password', autocomplete: 'off', placeholder: 'Developer API key', mono: true },
      ],
      join: (/** @type {Record<string, string>} */ v) => `${v.email.trim()}:${v.apiKey.trim()}`,
    },
  },
  {
    id: 'posthog', name: 'PostHog', mode: 'token', engine: 'posthog',
    blurb: 'Product analytics, queried with HogQL. Read-only',
    token: {
      help: 'Create a personal API key with the Project Read and Query Read scopes. For EU Cloud use https://eu.posthog.com, or your own URL if self-hosted.',
      helpUrl: 'https://us.posthog.com/settings/user-api-keys',
      fields: [
        { key: 'host', label: 'PostHog URL', type: 'url', autocomplete: 'url', placeholder: 'https://us.posthog.com', default: 'https://us.posthog.com', mono: true },
        { key: 'apiKey', label: 'Personal API key', type: 'password', autocomplete: 'off', placeholder: 'phx_…', mono: true },
      ],
      join: (/** @type {Record<string, string>} */ v) => `${v.host.trim()}|${v.apiKey.trim()}`,
    },
  },
]

/** @param {string} id */
export function providerMeta(id) {
  return PROVIDERS.find((p) => p.id === id) ?? null
}

/** Start the browser OAuth flow. @param {string} provider */
export async function providerStartOAuth(provider) {
  return invoke('provider_start_oauth', { provider })
}

/** Abort an in-flight OAuth wait - frees the localhost callback port. */
export async function providerCancelOAuth() {
  return invoke('provider_cancel_oauth')
}

/** Store a pasted API token / connection string for a token-based provider. */
export async function providerStoreToken(provider, token) {
  return invoke('provider_store_token', { provider, token })
}

/** @param {string} provider @returns {Promise<{connected: boolean, email: string|null}>} */
export async function providerOAuthStatus(provider) {
  return invoke('provider_oauth_status', { provider })
}

/** @param {string} provider */
/** Open the HTTPS connection to each signed-in provider's API ahead of use. Fire and forget. */
export function providerWarm() {
  return invoke('provider_warm').catch(() => {})
}

export async function providerLogout(provider) {
  return invoke('provider_logout', { provider })
}

/** @param {string} provider @returns {Promise<ProviderDatabase[]>} */
export async function providerListDatabases(provider) {
  return invoke('provider_list_databases', { provider })
}

/** @param {string} provider @param {string} dbRef @returns {Promise<ProviderConnection>} */
export async function providerBuildConnection(provider, dbRef) {
  return invoke('provider_build_connection', { provider, dbRef })
}

/**
 * Where a new database can go: organization/workspace choices and the regions
 * for `scope` (the first scope when omitted).
 * @param {string} provider @param {string | null} [scope]
 * @returns {Promise<import('./provider-create.js').CreateOptions>}
 */
export async function providerCreateOptions(provider, scope = null) {
  return invoke('provider_create_options', { provider, scope })
}

/**
 * Create a database on the provider. `password` is only for providers that
 * need one up front (Supabase).
 * @param {string} provider
 * @param {{ name: string, scope?: string | null, region?: string | null, password?: string | null }} request
 * @returns {Promise<import('./provider-create.js').CreatedDatabase>}
 */
export async function providerCreateDatabase(provider, request) {
  return invoke('provider_create_database', { provider, request })
}

/** @param {string} provider @param {string} dbRef @returns {Promise<import('./provider-create.js').DatabaseStatus>} */
export async function providerDatabaseStatus(provider, dbRef) {
  return invoke('provider_database_status', { provider, dbRef })
}

/**
 * Turn a resolved provider spec into the SavedConnection shape for its engine,
 * reusing the id of an existing saved entry for the same database so a connect
 * or a switch upserts that entry instead of piling up duplicates (or, worse,
 * overwriting the connection you switched away from).
 * libsql gets its URL in `host` and token in `password`; PostHog its project id
 * in `database` and API key in `password`; Redis its db number in `database`.
 * @param {ProviderConnection & { providerRef?: string }} conn
 * @param {{ provider?: string, saved: any[], readOnly?: boolean, newId: () => string }} opts
 */
export function providerToSaved(conn, { provider, saved, readOnly, newId }) {
  const base = { name: conn.name, provider, providerRef: conn.providerRef }
  const ro = readOnly || undefined
  if (conn.db_type === 'libsql') {
    const existing = saved.find((s) => s.type === 'libsql' && s.url === conn.host)
    return { ...base, id: existing?.id ?? newId(), type: 'libsql', url: conn.host, authToken: conn.password || undefined, readOnly: ro }
  }
  if (conn.db_type === 'posthog') {
    const existing = saved.find((s) => s.type === 'posthog' && s.host === conn.host && String(s.projectId) === String(conn.database))
    return { ...base, id: existing?.id ?? newId(), type: 'posthog', host: conn.host, projectId: conn.database, apiKey: conn.password, readOnly: true }
  }
  if (conn.db_type === 'redis') {
    const existing = saved.find((s) => s.type === 'redis' && s.host === conn.host && s.port === conn.port)
    return { ...base, id: existing?.id ?? newId(), type: 'redis', host: conn.host, port: conn.port, password: conn.password, db: Number(conn.database) || 0, tls: conn.ssl, readOnly: ro }
  }
  const type = conn.db_type === 'mysql' ? 'mysql' : 'postgres'
  const existing = saved.find((s) => s.host === conn.host && s.user === conn.username && s.type === type && (s.database ?? '') === (conn.database ?? ''))
    ?? saved.find((s) => s.host === conn.host && s.user === conn.username && s.type === type)
  return { ...base, id: existing?.id ?? newId(), type, host: conn.host, port: conn.port, database: conn.database, user: conn.username, password: conn.password, ssl: conn.ssl, readOnly: ro }
}

/** A driver error that means "these credentials are no good", on any engine. */
export const AUTH_FAILURE =
  /password authentication failed|authentication failed|access denied|invalid password|28P01|\b1045\b|role ".*" does not exist|WRONGPASS|NOAUTH|unauthorized|\b401\b/i
