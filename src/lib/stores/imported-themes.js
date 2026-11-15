// Themes added from a file, pasted JSON or the community gallery
// (stroke.click/themes). Each is kept whole; the one in use is applied under
// the `imported` theme id, injected as one <style> like the Custom theme.

import { writable, get } from 'svelte/store'
import { themeCss } from '$lib/themes/theme-file.js'

const KEY = 'stroke:imported-themes'
const ACTIVE_KEY = 'stroke:imported-theme-active'

/** @typedef {import('$lib/themes/theme-file.js').StrokeTheme} StrokeTheme */

function load() {
  try {
    const list = JSON.parse(localStorage.getItem(KEY) ?? '[]')
    return Array.isArray(list) ? list.filter((t) => t && t.id && t.colors) : []
  } catch {
    return []
  }
}

/** @type {import('svelte/store').Writable<StrokeTheme[]>} */
export const importedThemes = writable(typeof localStorage === 'undefined' ? [] : load())
/** @type {import('svelte/store').Writable<string>} */
export const activeImportedId = writable(typeof localStorage === 'undefined' ? '' : (localStorage.getItem(ACTIVE_KEY) ?? ''))

importedThemes.subscribe((list) => {
  try { localStorage.setItem(KEY, JSON.stringify(list)) } catch { /* full or blocked */ }
})
activeImportedId.subscribe((id) => {
  try { localStorage.setItem(ACTIVE_KEY, id) } catch { /* blocked */ }
})

/** The theme the `imported` id stands for, or null. */
export function activeImported() {
  const id = get(activeImportedId)
  const list = get(importedThemes)
  return list.find((t) => t.id === id) ?? list[0] ?? null
}

/** Whether the imported theme in use is dark (for the `imported` id). */
export function importedIsDark() {
  return (activeImported()?.appearance ?? 'dark') === 'dark'
}

/**
 * Add (or replace, by id) a theme and make it the active imported one.
 * @param {StrokeTheme} t
 */
export function addImportedTheme(t) {
  importedThemes.update((list) => [...list.filter((x) => x.id !== t.id), t])
  activeImportedId.set(t.id)
}

/** @param {string} id */
export function removeImportedTheme(id) {
  importedThemes.update((list) => list.filter((x) => x.id !== id))
  if (get(activeImportedId) === id) activeImportedId.set(get(importedThemes)[0]?.id ?? '')
}

/** Put the active imported theme's tokens on the page. */
export function applyImportedTheme() {
  if (typeof document === 'undefined') return
  const t = activeImported()
  let el = /** @type {HTMLStyleElement | null} */ (document.getElementById('stroke-imported-theme'))
  if (!t) { el?.remove(); return }
  if (!el) {
    el = document.createElement('style')
    el.id = 'stroke-imported-theme'
    document.head.appendChild(el)
  }
  el.textContent = themeCss(t)
}
