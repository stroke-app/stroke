/**
 * Notes for a stable release: every CHANGELOG.md section since the previous
 * stable version, merged by heading. Stable ships far less often than
 * nightly (3.0.0 to 3.7.5 is 66 versions), so the one section for the new
 * version alone told someone updating from stable almost nothing.
 *
 * Used by scripts/release-notes.mjs for the GitHub release and latest.json.
 * The output keeps the changelog's shape (`### Heading`, `#### Area`, `-`
 * bullets), which is what the app's update dialog parses (changelog.js).
 */

/** @param {string} v @returns {number[] | null} */
function parse(v) {
  const m = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(String(v).trim())
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null
}

/** @param {number[]} a @param {number[]} b */
function compare(a, b) {
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] - b[i]
  return 0
}

/**
 * The newest stable tag below `version`, or null. Pre-release tags (`-…`) and
 * anything that isn't `vX.Y.Z` are ignored.
 * @param {string[]} tags @param {string} version
 */
export function previousStable(tags, version) {
  const cur = parse(version)
  if (!cur) return null
  let best = /** @type {number[] | null} */ (null)
  for (const t of tags) {
    const v = parse(t)
    if (v && compare(v, cur) < 0 && (!best || compare(v, best) > 0)) best = v
  }
  return best ? best.join('.') : null
}

/**
 * The changelog's version sections, newest first.
 * @param {string} changelog
 * @returns {{ version: string, body: string }[]}
 */
export function splitVersions(changelog) {
  /** @type {{ version: string, body: string[] }[]} */
  const out = []
  for (const line of changelog.split('\n')) {
    const m = /^## \[([^\]]+)\]/.exec(line)
    if (m) { out.push({ version: m[1], body: [] }); continue }
    out.at(-1)?.body.push(line)
  }
  return out.map((s) => ({ version: s.version, body: s.body.join('\n').trim() }))
}

/** Order of the merged headings; anything else follows in first-seen order. */
const HEADING_ORDER = ['New Features', 'Bug Fixes', 'Changes']

/**
 * Notes for `version`: its own section, plus every older one down to (not
 * including) `since`, merged under one `###` heading each and one `####` area
 * within it, newest bullets first. With no `since`, just this version.
 * @param {string} changelog
 * @param {string} version
 * @param {string | null} [since]
 */
export function collectNotes(changelog, version, since = null) {
  const versions = splitVersions(changelog)
  const start = versions.findIndex((s) => s.version === version)
  if (start === -1) return ''
  const stop = since ? versions.findIndex((s, i) => i > start && s.version === since) : -1
  const picked = since ? versions.slice(start, stop === -1 ? undefined : stop) : [versions[start]]
  if (picked.length === 1) return picked[0].body

  /** @type {Map<string, Map<string, string[]>>} heading -> area ('' = none) -> bullets */
  const merged = new Map()
  for (const { body } of picked) {
    let heading = 'Changes'
    let area = ''
    for (const raw of body.split('\n')) {
      const line = raw.trimEnd()
      const h3 = /^###\s+(.+)$/.exec(line)
      const h4 = /^####\s+(.+)$/.exec(line)
      if (h3) { heading = h3[1].trim(); area = ''; continue }
      if (h4) { area = h4[1].trim(); continue }
      if (!/^\s*[-*]\s/.test(line)) {
        // A wrapped bullet's continuation stays with it.
        const list = merged.get(heading)?.get(area)
        if (line.trim() && list?.length) list[list.length - 1] += `\n${line}`
        continue
      }
      let areas = merged.get(heading)
      if (!areas) merged.set(heading, (areas = new Map()))
      let list = areas.get(area)
      if (!list) areas.set(area, (list = []))
      list.push(line)
    }
  }

  const headings = [...merged.keys()].sort((a, b) => {
    const ia = HEADING_ORDER.indexOf(a), ib = HEADING_ORDER.indexOf(b)
    return (ia === -1 ? HEADING_ORDER.length : ia) - (ib === -1 ? HEADING_ORDER.length : ib)
  })
  /** @type {string[]} */
  const out = []
  for (const h of headings) {
    out.push(`### ${h}`)
    const areas = /** @type {Map<string, string[]>} */ (merged.get(h))
    // Bullets without an area first, then each area under its own heading.
    for (const [area, bullets] of [...areas].sort(([a], [b]) => (a === '' ? -1 : b === '' ? 1 : 0))) {
      if (area) out.push('', `#### ${area}`)
      out.push(...bullets)
    }
    out.push('')
  }
  return out.join('\n').trim()
}
