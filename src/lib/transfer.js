/**
 * Move this app's setup to another computer as one file, without an account:
 * connections (and their workspaces), saved queries, settings with shortcuts
 * and AI models, SQL snippets and the language.
 *
 * Passwords, tokens and AI keys are left out unless asked for, and then go in
 * encrypted with a passphrase: PBKDF2-SHA256 (310,000 rounds) to an AES-256-GCM
 * key, over one JSON object of every secret. The rest of the file is plain
 * JSON, so it can be read, diffed or trimmed by hand. The same pieces the
 * account sync moves (sync/runtime.js), through the same save paths, so
 * secrets read back land in the keychain, not in the file or localStorage.
 */
import { get } from 'svelte/store'
import { loadSavedConnections, upsertConnection, withSecrets, SECRET_FIELDS, splitSecrets } from '$lib/stores/connections.js'
import { workspaces, replaceWorkspaces } from '$lib/stores/workspaces.js'
import { loadSettings, updateSettings } from '$lib/stores/settings.js'
import { aiProfiles, activeProfileId, replaceProfiles, loadKeyForProfile, saveProfile } from '$lib/stores/ai-settings.js'
import { keybindings, replaceKeybindings } from '$lib/stores/keybindings.js'
import { savedSnapshot, loadSavedSnapshot, applySyncedQuery } from '$lib/stores/query-history.js'
import { userSnippets, importSnippets, normalizeSnippet } from '$lib/stores/sql-snippets.js'
import { locale, setLocale } from '$lib/i18n.js'
import { pickSyncedSettings } from '$lib/sync/policy.js'
import { FORMAT, seal, unseal, planImportAgainst } from '$lib/transfer-core.js'

export { FORMAT, seal, unseal }

// ── Export ───────────────────────────────────────────────────────────────────

/** Connections this machine found itself (Docker, ORM studios) stay here. */
const exportable = () => loadSavedConnections().filter((c) => !c.origin)

/** What an export with every part would hold, for the dialog's counts. */
export async function exportCounts() {
  await loadSavedSnapshot().catch(() => {})
  return {
    connections: exportable().length,
    queries: savedSnapshot().length,
    snippets: get(userSnippets).length,
    aiProfiles: get(aiProfiles).length,
  }
}

/**
 * The export file's text.
 * @param {import('$lib/transfer-core.js').Parts} parts @param {string} [passphrase] required when `parts.secrets`
 * @param {string} [appVersion]
 */
export async function buildExport(parts, passphrase = '', appVersion = '') {
  /** @type {Record<string, any>} */
  const out = { format: FORMAT, version: 1, exportedAt: new Date().toISOString(), app: appVersion || undefined }
  /** @type {{ connections: Record<string, Record<string, string>>, ai: Record<string, string> }} */
  const secrets = { connections: {}, ai: {} }

  if (parts.connections) {
    const list = exportable()
    out.workspaces = get(workspaces).map((w) => ({ id: w.id, name: w.name, color: w.color, createdAt: w.createdAt }))
    out.connections = []
    for (const c of list) {
      const whole = await withSecrets(c)
      const { pub, sec } = splitSecrets(whole)
      const { lastConnectedAt: _t, ...rest } = /** @type {Record<string, any>} */ (pub)
      out.connections.push(rest)
      if (Object.keys(sec).length) secrets.connections[c.id] = sec
    }
  }
  if (parts.queries) {
    await loadSavedSnapshot().catch(() => {})
    out.savedQueries = savedSnapshot()
  }
  if (parts.settings) {
    out.settings = {
      app: pickSyncedSettings(loadSettings()),
      keybindings: get(keybindings),
      ai: { profiles: get(aiProfiles), activeProfileId: get(activeProfileId) },
      locale: get(locale),
    }
    if (parts.secrets) {
      for (const p of get(aiProfiles)) {
        const key = await loadKeyForProfile(p.id).catch(() => '')
        if (key) secrets.ai[p.id] = key
      }
    }
  }
  if (parts.snippets) out.snippets = get(userSnippets).map(({ id: _id, ...s }) => s)
  if (parts.secrets && (Object.keys(secrets.connections).length || Object.keys(secrets.ai).length)) {
    if (!passphrase) throw new Error('A passphrase is needed to include passwords.')
    out.secrets = await seal(secrets, passphrase)
  }
  return JSON.stringify(out, null, 2)
}

// ── Import ───────────────────────────────────────────────────────────────────


/**
 * What a file would bring in, without changing anything.
 * @param {string} text
 */
export function planImport(text) {
  return planImportAgainst(text, new Set(loadSavedConnections().map((c) => c.id)))
}

/**
 * Apply a plan. A connection already here (same id) is replaced only with
 * `replaceExisting`; otherwise it is kept and the file's copy skipped.
 * @param {import('$lib/transfer-core.js').ImportPlan} plan
 * @param {{ connections: boolean, queries: boolean, settings: boolean, snippets: boolean, replaceExisting: boolean, passphrase?: string }} opts
 * @returns {Promise<{ connections: number, skipped: number, queries: number, settings: boolean, snippets: number, secrets: boolean } | { error: string }>}
 */
export async function applyImport(plan, opts) {
  const { file } = plan
  /** @type {{ connections?: Record<string, Record<string, string>>, ai?: Record<string, string> } | null} */
  let secrets = null
  if (file.secrets && opts.passphrase) {
    secrets = await unseal(file.secrets, opts.passphrase)
    if (!secrets) return { error: 'The passphrase is wrong.' }
  }
  const result = { connections: 0, skipped: 0, queries: 0, settings: false, snippets: 0, secrets: !!secrets }

  if (opts.connections && plan.connections.length) {
    if (Array.isArray(file.workspaces)) {
      const list = get(workspaces)
      const ids = new Set(list.map((w) => w.id))
      const extra = file.workspaces.filter((w) => w && typeof w.id === 'string' && !ids.has(w.id))
      if (extra.length) replaceWorkspaces([...list, ...extra])
    }
    for (const { conn, existing } of plan.connections) {
      if (existing && !opts.replaceExisting) { result.skipped++; continue }
      const sec = secrets?.connections?.[conn.id] ?? {}
      // Only known secret fields come out of the sealed block.
      const clean = Object.fromEntries(Object.entries(sec).filter(([k, v]) => SECRET_FIELDS.includes(k) && typeof v === 'string'))
      upsertConnection({ ...conn, ...clean }, { revive: true, exact: true })
      result.connections++
    }
  }
  if (opts.queries && Array.isArray(file.savedQueries)) {
    for (const q of file.savedQueries) {
      if (q?.id && q.connectionId) { await applySyncedQuery(q); result.queries++ }
    }
  }
  if (opts.settings && plan.settings) {
    const s = file.settings
    const app = pickSyncedSettings(s.app ?? {})
    if (Object.keys(app).length) updateSettings(app)
    if (s.keybindings && typeof s.keybindings === 'object') replaceKeybindings(s.keybindings)
    if (s.ai && Array.isArray(s.ai.profiles)) {
      await replaceProfiles(s.ai.profiles, s.ai.activeProfileId ?? null)
      for (const p of s.ai.profiles) {
        const key = secrets?.ai?.[p.id]
        if (typeof key === 'string' && key) await saveProfile(p, key)
      }
    }
    if (typeof s.locale === 'string') setLocale(/** @type {any} */ (s.locale))
    result.settings = true
  }
  if (opts.snippets && Array.isArray(file.snippets)) {
    const list = file.snippets.map((/** @type {any} */ s) => normalizeSnippet({ ...s, id: undefined })).filter((s) => s !== null)
    const { added, replaced } = importSnippets(list)
    result.snippets = added + replaced
  }
  return result
}
