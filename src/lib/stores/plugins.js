// Persisted state for the extensions ("plugins") system: which extensions are
// enabled and their per-extension config.
//
// Two read paths intentionally coexist:
//  - `pluginState` (Svelte store) for reactive UI + canvas repaint triggers.
//  - `isPluginEnabled()` / `getPluginConfig()` read a plain module-level snapshot
//    so hot paths (per-cell canvas rendering) never pay store-subscription cost.
import { writable, get } from 'svelte/store'
import { debounce } from '$lib/utils.js'

const KEY = 'stroke:plugins'

/** @typedef {{ enabled: Record<string, boolean>, config: Record<string, Record<string, unknown>> }} PluginState */

/** @returns {PluginState} */
function load() {
  try {
    const raw = localStorage.getItem(KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      return {
        enabled: parsed?.enabled ?? {},
        config: parsed?.config ?? {},
      }
    }
  } catch {}
  return { enabled: {}, config: {} }
}

/** Reactive snapshot - subscribe in components/effects that must react. */
export const pluginState = writable(load())

/** Plain snapshot kept in sync for synchronous hot-path reads. */
let _snap = get(pluginState)
// The snapshot updates synchronously (hot-path reads need it); only the
// localStorage serialize is debounced so rapid toggles don't thrash storage.
const persistPlugins = debounce((/** @type {PluginState} */ v) => {
  try { localStorage.setItem(KEY, JSON.stringify(v)) } catch {}
}, 300)
pluginState.subscribe((v) => {
  _snap = v
  persistPlugins(v)
})
if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', () => persistPlugins.flush())
}

/**
 * Extensions that ship enabled. Workflow features (saved views, find &
 * replace) are core-adjacent - off-by-default would just hide them; the
 * Extensions page stays the place to switch them off. Nullish markers are on
 * because without them a real NULL and the text 'NULL' differ only by colour.
 */
const DEFAULT_ON = new Set(['saved-views', 'find-replace', 'nullish-values'])

/**
 * Enabled check against an explicit state snapshot (for reactive `$pluginState`
 * reads in components) - honors DEFAULT_ON when the user never toggled the id.
 * @param {PluginState} state @param {string} id
 */
export function pluginEnabledIn(state, id) {
  return state.enabled[id] ?? DEFAULT_ON.has(id)
}

/** @param {string} id */
export function isPluginEnabled(id) {
  return pluginEnabledIn(_snap, id)
}

/** @param {string} id @returns {Record<string, unknown>} */
export function getPluginConfig(id) {
  return _snap.config[id] ?? {}
}

/** @param {string} id @param {boolean} on */
export function setPluginEnabled(id, on) {
  pluginState.update((s) => ({ ...s, enabled: { ...s.enabled, [id]: on } }))
}

/** @param {string} id @param {Record<string, unknown>} patch */
export function setPluginConfig(id, patch) {
  pluginState.update((s) => ({
    ...s,
    config: { ...s.config, [id]: { ...(s.config[id] ?? {}), ...patch } },
  }))
}
