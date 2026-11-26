/**
 * Filtering for a command menu page done by its rows, not by bits-ui.
 *
 * bits-ui's Command filters by unmounting the rows that do not match, then
 * re-appends every remaining row in score order, and its List wraps the whole
 * list in `{#key search === ""}`: the first letter typed rebuilt every row and
 * group (53 rows, ~280 ms in a production build), and each later letter
 * re-sorted the DOM. With a page using this, bits-ui keeps an empty search and
 * never touches the rows: each row scores itself and draws nothing when it
 * does not match, a group hides when none of its rows match, and the page
 * orders its groups by their best row with a keyed `{#each}` (moving DOM nodes,
 * not rebuilding them).
 *
 * The palette provides one with setContext(LOCAL_FILTER, ...); the shared
 * command item and group components read it.
 */
import { SvelteMap } from 'svelte/reactivity'
import { untrack } from 'svelte'

export const LOCAL_FILTER = Symbol('command-local-filter')
export const LOCAL_GROUP = Symbol('command-local-group')

/**
 * @param {() => string} getSearch the search to filter by; '' shows every row
 * @param {(value: string, search: string, keywords?: string[]) => number} score
 */
export function createLocalFilter(getSearch, score) {
  /** Row value -> its group and score, for the rows mounted now. */
  const rows = new SvelteMap()
  return {
    get search() { return getSearch() },
    score,
    /** @param {string} value @param {string | undefined} group @param {number} s */
    report(value, group, s) {
      // Untracked: a row's effect calls this, and reading the map it writes
      // would make every row re-run whenever any row reports.
      untrack(() => {
        const prev = rows.get(value)
        if (!prev || prev.group !== group || prev.score !== s) rows.set(value, { group, score: s })
      })
    },
    /** @param {string} value */
    forget(value) { untrack(() => rows.delete(value)) },
    /** The best score in a group (0 when nothing in it matches). @param {string} group */
    best(group) {
      let b = 0
      for (const r of rows.values()) if (r.group === group && r.score > b) b = r.score
      return b
    },
    /** The highest-scoring row: ties go to the row mounted first, which is the
     *  first in the page's own order. @param {(group: string | undefined) => number} [groupRank] */
    top(groupRank) {
      /** @type {string | null} */
      let best = null
      let bs = 0
      let br = Infinity
      for (const [v, r] of rows) {
        if (r.score <= 0) continue
        const rank = groupRank ? groupRank(r.group) : 0
        if (r.score > bs || (r.score === bs && rank < br)) { best = v; bs = r.score; br = rank }
      }
      return best
    },
    /** How many mounted rows match. */
    get count() {
      let n = 0
      for (const r of rows.values()) if (r.score > 0) n++
      return n
    },
  }
}
