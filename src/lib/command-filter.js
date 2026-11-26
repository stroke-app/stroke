/**
 * Ranking for the command menu's root page. bits-ui calls this per item with
 * the item's value, the search and its keywords (the visible label first), and
 * sorts by the score. 0 hides the item.
 *
 * Tiers, high to low: the label exactly, the label's start, the start of a
 * label word, the start of any word, a substring, several words in any order,
 * a separator-free prefix, and last a loose subsequence of the label.
 */

const stripSep = (/** @type {string} */ s) => s.replace(/[\s_.\-]+/g, '')

/**
 * Letters typed in order with some left out: "stts" for "Change status",
 * "nwtb" for "New table". The first letter has to start a word of the label
 * and the match has to stay compact, or a short query would find something in
 * every long label (unanchored, "map" matched "sche(ma p)ublic"). Only the
 * visible label is searched: the hidden keywords are long lists of synonyms,
 * and a loose match inside them shows a row with no visible reason for it.
 * @param {string} label lowercased
 * @param {string} q lowercased, separators stripped, at least 2 letters
 * @returns {number} a score in (0, 0.25], or 0
 */
export function looseLabelScore(label, q) {
  if (q.length < 2 || !label) return 0
  const words = label.split(/[^a-z0-9]+/).filter(Boolean)
  const text = words.join(' ')
  let best = 0
  // Try each word start that begins with the first letter.
  for (let start = 0; start < text.length; start++) {
    if (text[start] !== q[0] || (start > 0 && text[start - 1] !== ' ')) continue
    let ti = start + 1, qi = 1, gaps = 0, last = start
    while (ti < text.length && qi < q.length) {
      if (text[ti] === q[qi]) {
        // A jump to a later word's first letter is free: "nwtb" reads n.w t.b.
        const atWordStart = text[ti - 1] === ' '
        if (!atWordStart) gaps += ti - last - 1
        last = ti
        qi++
      }
      ti++
    }
    if (qi < q.length) continue
    if (gaps > q.length * 2) continue
    best = Math.max(best, 0.25 - gaps * 0.01)
  }
  // Ties go to the shorter label: "stng" means Settings more than it means
  // Import connections and settings.
  return best > 0 ? Math.max(0.001, best - Math.min(text.length, 80) * 0.0001) : 0
}

/** @type {(value: string, search: string, keywords?: string[]) => number} */
export function commandFilter(value, search, keywords = []) {
  const s = search.trim().toLowerCase()
  if (!s) return 1
  const label = (keywords[0] ?? '').toLowerCase()
  const v = value.toLowerCase()
  const wordStart = (/** @type {string} */ text) => text.split(/[^a-z0-9]+/).some((w) => w.startsWith(s))
  if (label) {
    if (label === s) return 1
    if (label.startsWith(s)) return 0.95
    if (wordStart(label)) return 0.9
  }
  // A word of the hidden keywords: the earlier it comes, the more the row is
  // about it. "queries" leads Query history's keywords and closes Instance
  // insights' ("... slow queries"); level, Instance insights won on page order.
  const vi = v.split(/[^a-z0-9]+/).filter(Boolean).findIndex((w) => w.startsWith(s))
  if (vi >= 0) return 0.7 + 0.05 / (1 + vi)
  // Inside a word of the hidden keywords only from 3 letters: two letters sit
  // in the middle of half of them, so "ai" listed Migrations (f-ai-led) and
  // Schema diagram (merm-ai-d) above Ask AI.
  if (label.includes(s) || (s.length >= 3 && v.includes(s))) return 0.5
  // Several words in any order, each the start of a word: "ai chat" finds
  // Ask AI ("ask ai assistant chat query"), which as one phrase it never did.
  const tokens = s.split(/\s+/).filter(Boolean)
  if (tokens.length > 1) {
    const all = `${label} ${v}`.split(/[^a-z0-9]+/).filter(Boolean)
    if (tokens.every((t) => all.some((w) => w.startsWith(t)))) return 0.4
  }
  // Separator-insensitive fallback so "newtable"/"new-table" match "new table",
  // anchored at a word start.
  const words = `${label} ${v}`.split(/[^a-z0-9]+/).filter(Boolean)
  const q = stripSep(s)
  for (let i = 0; i < words.length; i++) {
    if (words.slice(i).join('').startsWith(q)) return 0.3
  }
  return looseLabelScore(label, q)
}

/**
 * `commandFilter` remembered for the current search. bits-ui scores every row
 * again for each row it unmounts (its unregister re-runs the whole filter), so
 * the first letter typed into the command menu scored ~110 rows ~70 times over:
 * thousands of calls for one keystroke. The score only depends on the row and
 * the search, so a new search starts a fresh cache.
 * @returns {(value: string, search: string, keywords?: string[]) => number}
 */
export function cachedCommandFilter() {
  let forSearch = ''
  /** @type {Map<string, number>} */
  let cache = new Map()
  return (value, search, keywords = []) => {
    if (search !== forSearch) { forSearch = search; cache = new Map() }
    const key = keywords.length ? `${value}\u0000${keywords.join('\u0001')}` : value
    let score = cache.get(key)
    if (score === undefined) { score = commandFilter(value, search, keywords); cache.set(key, score) }
    return score
  }
}

