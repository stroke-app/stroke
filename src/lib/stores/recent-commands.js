/**
 * The command menu's last few picks from its page list, newest first, so the
 * menu opens with what was used last on top (an empty search otherwise starts
 * with the same alphabet-ish list every time). Kept by English label, which is
 * the page item's stable name.
 */
const KEY = 'stroke:recent-commands'
export const RECENT_COMMANDS_MAX = 3

/** @returns {string[]} */
export function loadRecentCommands() {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? '[]')
    return Array.isArray(v) ? v.filter((x) => typeof x === 'string').slice(0, RECENT_COMMANDS_MAX) : []
  } catch {
    return []
  }
}

/** @param {string} label @returns {string[]} the new list */
export function rememberCommand(label) {
  const next = [label, ...loadRecentCommands().filter((l) => l !== label)].slice(0, RECENT_COMMANDS_MAX)
  try { localStorage.setItem(KEY, JSON.stringify(next)) } catch { /* the list is a convenience */ }
  return next
}
