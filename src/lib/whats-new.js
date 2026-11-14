/**
 * Highlights for the What's new dialog: a few features worth trying, each with
 * a button that does it, above the full release notes.
 *
 * A set is keyed by the release it follows (`after`), not by its own version
 * number: that is picked when the release is cut (auto-release labels), so a
 * set reads "for anyone updating from `after` or older to something newer".
 */
import { compareVersions } from '$lib/changelog.js'

/**
 * @typedef {'enable-tree-sidebar' | 'open-themes' | 'open-shortcuts' | 'open-connections'} HighlightAction
 * @typedef {{
 *   id: string,
 *   icon: 'tree' | 'themes' | 'keyboard' | 'database',
 *   title: string,
 *   body: string,
 *   beta?: boolean,
 *   action?: { id: HighlightAction, label: string },
 * }} Highlight
 */

/** Newest first. @type {{ after: string, items: Highlight[] }[]} */
export const HIGHLIGHT_SETS = [
  {
    after: '2.3.0',
    items: [
      {
        id: 'tree-sidebar',
        icon: 'tree',
        beta: true,
        title: 'Explorer tree',
        body: 'Every connection, database, schema, table, view and routine in one tree, with columns a click away.',
        action: { id: 'enable-tree-sidebar', label: 'Try the explorer tree' },
      },
      {
        id: 'themes',
        icon: 'themes',
        title: 'Theme builder',
        body: 'Make a theme from a few colours, or try Glass light and dark.',
        action: { id: 'open-themes', label: 'Pick a theme' },
      },
      {
        id: 'shortcuts',
        icon: 'keyboard',
        title: 'Custom shortcuts',
        body: 'Give any command the keys you want, in the editors too.',
        action: { id: 'open-shortcuts', label: 'Shortcuts' },
      },
      {
        id: 'nosql',
        icon: 'database',
        title: 'Document databases',
        body: 'MongoDB, CouchDB, Cassandra, DynamoDB and Firestore, plus a Redis key browser.',
        action: { id: 'open-connections', label: 'Connect' },
      },
    ],
  },
]

/** At most this many cards: past four the dialog stops being a glance. */
const MAX = 4

/**
 * The highlights for an update from `previous` to `current`: every set whose
 * `after` lies in [previous, current), newest set first.
 * @param {string} previous @param {string} current
 * @returns {Highlight[]}
 */
export function highlightsFor(previous, current, sets = HIGHLIGHT_SETS) {
  if (!previous || !current) return []
  /** @type {Highlight[]} */
  const out = []
  for (const set of sets) {
    const fromOk = compareVersions(previous, set.after) <= 0
    const toOk = compareVersions(current, set.after) > 0
    if (fromOk && toOk) out.push(...set.items)
  }
  return out.slice(0, MAX)
}
