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
