import { writable, get } from 'svelte/store'
import { invoke } from '@tauri-apps/api/core'

// ── Provider catalogue ───────────────────────────────────────────────────────

/** Stroke's own zero-config gateway - see stroke-web/FREE_AI_TIER.md. */
export const STROKE_FREE_BASE_URL = 'https://stroke.click/api/ai'

export const PROVIDERS = [
  // First in the list and the default profile: a new install has a working
  // assistant before it has any credentials. Authenticated by device id, so
  // there is no key field and nothing to paste.
  { id: 'stroke', label: 'Stroke Free', url: STROKE_FREE_BASE_URL, keysUrl: null },
  { id: 'openrouter', label: 'OpenRouter',      url: 'https://openrouter.ai/api/v1', keysUrl: 'https://openrouter.ai/keys' },
  { id: 'openai',     label: 'OpenAI',           url: 'https://api.openai.com/v1', keysUrl: 'https://platform.openai.com/api-keys' },
  { id: 'anthropic',  label: 'Anthropic',        url: 'https://api.anthropic.com/v1', keysUrl: 'https://console.anthropic.com/settings/keys' },
  { id: 'copilot',    label: 'GitHub Copilot',   url: 'https://api.githubcopilot.com', keysUrl: null, deviceFlow: true },
  { id: 'mistral',    label: 'Mistral',          url: 'https://api.mistral.ai/v1', keysUrl: 'https://console.mistral.ai/api-keys' },
  { id: 'google',     label: 'Google',           url: 'https://generativelanguage.googleapis.com/v1beta/openai', keysUrl: 'https://aistudio.google.com/apikey' },
  { id: 'ollama',     label: 'Ollama',           url: 'http://localhost:11434/v1', keysUrl: null },
  // OmniRoute is a gateway that runs locally; Stroke installs and starts its own
  // copy (see src-tauri/src/omniroute.rs). It fronts many providers behind one
  // OpenAI-compatible endpoint on :20128, and its key comes from its own local
  // dashboard rather than a hosted console.
  { id: 'omniroute',  label: 'OmniRoute',        url: 'http://localhost:20128/v1', keysUrl: 'http://localhost:20128/dashboard' },
  { id: 'custom',     label: 'Custom',           url: '', keysUrl: null },
]

/**
 * The model a profile runs, as a person would name it: the picker's label where
 * there is one ("Stroke Free", "Claude Haiku 4.5"), else the model id.
 * @param {{ model?: string, baseUrl?: string }} settings
 */
export function modelDisplayName(settings) {
  const id = settings.model ?? ''
  if (!id) return ''
  for (const list of Object.values(PROVIDER_MODELS)) {
    const hit = list.find((m) => m.model === id)
    if (hit) return id.startsWith('stroke-free') ? `${hit.label}, a free model Stroke's gateway picks per request` : hit.label
  }
  return id
}

/** @type {Record<string, { label: string, model: string, tag: string }[]>} */
export const PROVIDER_MODELS = {
  // Aliases, not upstream model names: the gateway decides what actually serves
  // the request, so routing can change without invalidating a saved profile.
  stroke: [
    { label: 'Stroke Free', model: 'stroke-free', tag: 'free' },
    { label: 'Stroke Free · Fast', model: 'stroke-free-fast', tag: 'free' },
  ],
  openrouter: [
    { label: 'DeepSeek V3', model: 'deepseek/deepseek-chat-v3-0324:free', tag: 'free' },
    { label: 'Llama 3.3 70B', model: 'meta-llama/llama-3.3-70b-instruct:free', tag: 'free' },
    { label: 'Gemini 2.0 Flash', model: 'google/gemini-2.0-flash-exp:free', tag: 'free' },
    { label: 'Claude Haiku', model: 'anthropic/claude-haiku-4-5', tag: 'fast' },
    { label: 'Claude Sonnet', model: 'anthropic/claude-sonnet-4-5', tag: 'smart' },
    { label: 'GPT-4o mini', model: 'openai/gpt-4o-mini', tag: 'fast' },
  ],
  openai: [
    { label: 'GPT-4o mini', model: 'gpt-4o-mini', tag: 'fast' },
    { label: 'GPT-4o', model: 'gpt-4o', tag: 'smart' },
    { label: 'o1 mini', model: 'o1-mini', tag: 'smart' },
    { label: 'o3 mini', model: 'o3-mini', tag: 'smart' },
  ],
  anthropic: [
    { label: 'Claude Haiku 4.5', model: 'claude-haiku-4-5-20251001', tag: 'fast' },
    { label: 'Claude Sonnet 4.5', model: 'claude-sonnet-4-5', tag: 'smart' },
    { label: 'Claude Opus 4.5', model: 'claude-opus-4-5', tag: 'smart' },
  ],
  mistral: [
    { label: 'Mistral Small', model: 'mistral-small-latest', tag: 'fast' },
    { label: 'Mistral Medium', model: 'mistral-medium-latest', tag: 'smart' },
    { label: 'Codestral', model: 'codestral-latest', tag: 'fast' },
  ],
  google: [
    { label: 'Gemini 2.0 Flash', model: 'gemini-2.0-flash', tag: 'fast' },
    { label: 'Gemini 1.5 Pro', model: 'gemini-1.5-pro', tag: 'smart' },
    { label: 'Gemini 1.5 Flash', model: 'gemini-1.5-flash', tag: 'fast' },
  ],
  // Ollama models are discovered from the running server - it only accepts exact
  // installed tags (`llama3.1:8b`), so any hardcoded guess is a 404 waiting to happen.
  ollama: [],
  // Same story as Ollama: the catalogue depends on which upstream providers the
  // user connected in their dashboard, so it is read from the gateway at runtime.
  // Its routing aliases (`auto`, `auto/best-coding`, …) come back from /v1/models too.
  omniroute: [],
  // Copilot models are fetched dynamically; these are the static fallback
  copilot: [
    { label: 'GPT-4o',             model: 'gpt-4o',               tag: 'smart' },
    { label: 'GPT-4o mini',        model: 'gpt-4o-mini',          tag: 'fast'  },
    { label: 'o3-mini',            model: 'o3-mini',              tag: 'smart' },
    { label: 'Claude 3.5 Sonnet',  model: 'claude-3.5-sonnet',    tag: 'smart' },
    { label: 'Claude 3.7 Sonnet',  model: 'claude-3.7-sonnet',    tag: 'smart' },
    { label: 'Gemini 2.0 Flash',   model: 'gemini-2.0-flash-001', tag: 'fast'  },
  ],
  custom: [],
}

// ── Typedef ──────────────────────────────────────────────────────────────────

/**
 * @typedef {{
 *   id: string,
 *   name: string,
 *   provider: string,
 *   baseUrl: string,
 *   model: string,
 * }} ModelProfile
 */

/** @typedef {{ baseUrl: string, apiKey: string, model: string }} AiSettings */

// ── Storage keys ─────────────────────────────────────────────────────────────

const PROFILES_KEY = 'stroke:ai-profiles'
const ACTIVE_KEY   = 'stroke:ai-active-profile'
const LEGACY_KEY   = 'stroke:ai-settings'
// Set once, the first time the default profile is seeded. Without it, a user who
// deletes every profile gets Stroke Free handed back on the next launch.
const SEEDED_KEY   = 'stroke:ai-seeded'

// ── Default profile ──────────────────────────────────────────────────────────

/**
 * The profile a fresh install starts on.
 *
 * This used to be an OpenRouter model, which cannot answer without a key the new
 * user hasn't got - so the assistant was broken until they went and configured
 * one. Stroke Free needs no credentials, so the AI works on first launch.
 */
function makeDefaultProfile() {
  return /** @type {ModelProfile} */ ({
    id: 'default',
    name: 'Stroke Free',
    provider: 'stroke',
    baseUrl: STROKE_FREE_BASE_URL,
    // The fast alias by default. The gateway still routes any turn that carries
    // tools to the larger model, so the database agent keeps working while plain
    // conversation runs on the cheap one - the alias sets the floor, not the cap.
    model: 'stroke-free-fast',
  })
}

// ── Persistence helpers ───────────────────────────────────────────────────────

/** @returns {ModelProfile[]} */
function loadProfiles() {
  try {
    const raw = localStorage.getItem(PROFILES_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      if (Array.isArray(parsed) && parsed.length > 0) return parsed
    }
    // Migrate from legacy single-setting format
    const legacy = localStorage.getItem(LEGACY_KEY)
    if (legacy) {
      const s = JSON.parse(legacy)
      if (s.model) {
        const provider = detectProvider(String(s.baseUrl || ''))
        return [{
          id: 'default',
          name: modelShortName(String(s.model || '')),
          provider,
          baseUrl: String(s.baseUrl || 'https://openrouter.ai/api/v1'),
          model: String(s.model || ''),
        }]
      }
    }
  } catch { /* ignore */ }
  // Nothing stored and nothing to migrate: this is a fresh install, so seed the
  // credential-free profile rather than leaving the picker on "No model" - the
  // assistant has to work before the user has configured anything.
  try {
    if (localStorage.getItem(SEEDED_KEY)) return []
  } catch { /* ignore */ }
  const seed = [makeDefaultProfile()]
  try {
    localStorage.setItem(SEEDED_KEY, '1')
    localStorage.setItem(PROFILES_KEY, JSON.stringify(seed))
  } catch { /* ignore */ }
  return seed
}

/** @param {string} url */
function detectProvider(url) {
  if (url.includes('stroke.click')) return 'stroke'
  if (url.includes('openrouter.ai')) return 'openrouter'
  if (url.includes('openai.com')) return 'openai'
  if (url.includes('anthropic.com')) return 'anthropic'
  if (url.includes('mistral.ai')) return 'mistral'
  if (url.includes('googleapis.com')) return 'google'
  // Port first: both run on loopback, so the host alone can't tell them apart.
  if (url.includes(':20128')) return 'omniroute'
  if (url.includes(':11434')) return 'ollama'
  if (url.includes('localhost') || url.includes('127.0.0.1')) return 'ollama'
  return 'custom'
}

/** @param {string} model */
function modelShortName(model) {
  return model.split('/').pop()?.split(':')[0] ?? model
}

/** @param {ModelProfile[]} profiles */
function saveProfiles(profiles) {
  try { localStorage.setItem(PROFILES_KEY, JSON.stringify(profiles)) } catch { /* ignore */ }
}

function loadActiveId() {
  try { return localStorage.getItem(ACTIVE_KEY) ?? null } catch { return null }
}

/** @param {string} id */
function saveActiveId(id) {
  try { localStorage.setItem(ACTIVE_KEY, id) } catch { /* ignore */ }
}

// ── Stores ───────────────────────────────────────────────────────────────────

export const aiProfiles = writable(/** @type {ModelProfile[]} */ (loadProfiles()))
export const activeProfileId = writable(/** @type {string | null} */ (loadActiveId() ?? loadProfiles()[0]?.id ?? null))

/** Merged runtime settings (baseUrl + model from profile, apiKey from secure store). */
export const aiSettings = writable(/** @type {AiSettings} */ ({
  baseUrl: 'https://openrouter.ai/api/v1',
  apiKey: '',
  model: '',
}))

// ── Bootstrap: load API key from secure store ────────────────────────────────

async function loadKeyForProfile(profileId) {
  try {
    return await invoke('ai_load_key', { profileId })
  } catch {
    // Tauri not available (dev mode without backend) - fall back to legacy localStorage
    try {
      const legacy = localStorage.getItem(LEGACY_KEY)
      if (legacy) return JSON.parse(legacy).apiKey ?? ''
    } catch { /* ignore */ }
    return ''
  }
}

export async function refreshActiveSettings() {
  const profiles = get(aiProfiles)
  const activeId = get(activeProfileId)
  const profile = profiles.find((p) => p.id === activeId) ?? profiles[0]
  if (!profile) return
  const apiKey = await loadKeyForProfile(profile.id)
  aiSettings.set({ baseUrl: profile.baseUrl, apiKey, model: profile.model })
}

// Bootstrap after the first paint, not during module evaluation. This is the
// process's first read of the secrets vault, and the OS may put a "Stroke wants
// to use your confidential information" keychain prompt in front of it - firing
// it inline would land that prompt on screen before the window has composited a
// single frame, i.e. a system modal over an empty white window. One frame plus a
// macrotask is enough for the shell to be on screen behind it.
//
// Errors are swallowed so a failed secret-store read can't surface as an
// unhandled promise rejection; consumers read `aiSettings` and get the default
// until this resolves.
function bootstrapActiveSettings() {
  void refreshActiveSettings().catch(() => {})
}
if (typeof requestAnimationFrame === 'function') {
  requestAnimationFrame(() => setTimeout(bootstrapActiveSettings, 0))
} else {
  bootstrapActiveSettings()
}

// ── Actions ───────────────────────────────────────────────────────────────────

/**
 * Save a profile (create or update) and optionally store its API key.
 * @param {ModelProfile} profile
 * @param {string} [apiKey]
 */
export async function saveProfile(profile, apiKey) {
  const profiles = get(aiProfiles)
  const idx = profiles.findIndex((p) => p.id === profile.id)
  const next = idx >= 0
    ? profiles.map((p) => p.id === profile.id ? profile : p)
    : [...profiles, profile]
  aiProfiles.set(next)
  saveProfiles(next)

  if (apiKey !== undefined) {
    try { await invoke('ai_store_key', { profileId: profile.id, apiKey }) } catch { /* ignore in dev */ }
  }
}

/** @param {string} id */
export async function deleteProfile(id) {
  const profiles = get(aiProfiles)
  const next = profiles.filter((p) => p.id !== id)
  aiProfiles.set(next)
  saveProfiles(next)
  try { await invoke('ai_delete_key', { profileId: id }) } catch { /* ignore */ }

  const activeId = get(activeProfileId)
  if (activeId === id) {
    const nextId = next[0]?.id ?? null
    if (nextId) {
      await setActiveProfile(nextId)
    } else {
      activeProfileId.set(null)
      try { localStorage.removeItem(ACTIVE_KEY) } catch { /* ignore */ }
      aiSettings.set({ baseUrl: 'https://openrouter.ai/api/v1', apiKey: '', model: '' })
    }
  }
}

/** @param {string} id */
export async function setActiveProfile(id) {
  activeProfileId.set(id)
  saveActiveId(id)
  await refreshActiveSettings()
}

// ── Configured check ──────────────────────────────────────────────────────────

/** True when the given settings can reach a provider without further setup. */
export function isAiConfigured(s) {
  return Boolean(s.apiKey) ||
    /localhost|127\.0\.0\.1/.test(s.baseUrl) ||
    // The free tier authenticates with the device id, so "no key" is configured.
    (s.baseUrl ?? '').includes('stroke.click') ||
    (s.baseUrl ?? '').includes('githubcopilot.com')
}
