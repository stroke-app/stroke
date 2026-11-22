// Lightweight, dependency-free i18n. A reactive `locale` store drives a derived
// `t` store used as `$t('key', { vars })` in components; missing keys fall back
// to English, then to the key itself, so partial translation never breaks the UI.
//
// Migrating strings is incremental: wrap a literal in `$t('some.key')` and add
// the key to DICT.en (plus other locales as they're translated).

import { writable, derived } from 'svelte/store'
import en from './locales/en.js'
import es from './locales/es.js'
import fr from './locales/fr.js'
import de from './locales/de.js'
import pt from './locales/pt.js'
import ja from './locales/ja.js'

const STORAGE_KEY = 'stroke:locale'

/** Languages offered in the picker. */
export const LOCALES = /** @type {const} */ ([
  { id: 'en', label: 'English', native: 'English' },
  { id: 'es', label: 'Spanish', native: 'Español' },
  { id: 'fr', label: 'French', native: 'Français' },
  { id: 'de', label: 'German', native: 'Deutsch' },
  { id: 'pt', label: 'Portuguese', native: 'Português' },
  { id: 'ja', label: 'Japanese', native: '日本語' },
])

const LOCALE_IDS = new Set(LOCALES.map((l) => l.id))

function loadLocale() {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (saved && LOCALE_IDS.has(saved)) return saved
    const nav = (navigator.language || 'en').slice(0, 2)
    return LOCALE_IDS.has(nav) ? nav : 'en'
  } catch {
    return 'en'
  }
}

/** Current locale id (persisted). */
export const locale = writable(loadLocale())
locale.subscribe((v) => {
  try {
    localStorage.setItem(STORAGE_KEY, v)
    if (typeof document !== 'undefined') document.documentElement.lang = v
  } catch { /* ignore */ }
})

/** @param {(typeof LOCALES)[number]['id']} id */
export function setLocale(id) {
  if (LOCALE_IDS.has(id)) locale.set(id)
}

// ── Dictionaries ──────────────────────────────────────────────────────────────
// Keyed by dotted namespace. English is the source of truth; other locales may
// be partial and fall back to English per key.
// One file per language in ./locales; English is the source of truth and
// other languages fall back to it per key.
const DICT = { en, es, fr, de, pt, ja }

/**
 * @param {string} loc @param {string} key @param {Record<string, unknown>} [vars]
 */
function translate(loc, key, vars) {
  const s = DICT[loc]?.[key] ?? DICT.en[key] ?? key
  return vars ? s.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`)) : s
}

/**
 * Reactive translator. Usage in a component: `{$t('common.save')}` or
 * `{$t('greeting', { name })}`. Re-runs when the locale changes.
 * @type {import('svelte/store').Readable<(key: string, vars?: Record<string, unknown>) => string>}
 */
export const t = derived(locale, ($loc) => (key, vars) => translate($loc, key, vars))

/**
 * Translator with a fallback: `$tx('cmd.SQL editor', 'SQL editor')` is the
 * translation when this language has one, and the fallback otherwise (not the
 * key). For text keyed by its own English, like command names.
 * @type {import('svelte/store').Readable<(key: string, fallback: string, vars?: Record<string, unknown>) => string>}
 */
export const tx = derived(locale, ($loc) => (key, fallback, vars) => {
  const s = DICT[$loc]?.[key] ?? DICT.en[key] ?? fallback
  return vars ? s.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`)) : s
})
