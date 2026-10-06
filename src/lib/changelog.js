/**
 * Release notes for the update dialog and the What's new dialog.
 *
 * CHANGELOG.md and the notes in latest.json share one shape (the release
 * workflow writes both from .changeset/*.md): `## [x.y.z] - date` per version,
 * then `### New Features` / `### Bug Fixes` / `### Changes`, optional
 * `#### Area` sub-sections, and `-` bullets.
 */

/** @typedef {'feature'|'fix'|'change'|'other'} ChangeType */
/**
 * `group` is the `###` heading a section sits under; for a `###` heading with
 * bullets of its own it equals `title`.
 * @typedef {{ title: string, group: string, type: ChangeType, items: string[] }} ChangelogSection
 */
/** @typedef {{ version: string, date: string, sections: ChangelogSection[] }} ChangelogRelease */

/**
 * Parse one version's notes into typed sections. Sub-sections inherit their
 * parent's type, so "Canvas Table" under "New Features" is a feature.
 * @param {string} markdown
 * @returns {ChangelogSection[]}
 */
export function parseChangelog(markdown) {
  if (!markdown?.trim()) return []
  /** @type {ChangeType} */
  let currentType = 'other'
  let currentGroup = ''
  /** @type {ChangelogSection | null} */
  let current = null
  /** @type {ChangelogSection[]} */
  const sections = []

  for (const line of markdown.split('\n')) {
    const trimmed = line.trim()

    if (/^#{1,3}\s/.test(trimmed)) {
      // Top-level section (## or ###) - determines the type inherited by sub-sections
      const title = trimmed.replace(/^#+\s*/, '').trim()
      const lower = title.toLowerCase()
      currentType =
        /feat|feature|add|new|what.?s new|✨|🚀|⭐|🆕/.test(lower) ? 'feature'
        : /fix|bug|patch|issue|🐛|🔧|🩹/.test(lower)               ? 'fix'
        : /change|improve|update|refactor|perf|♻️|💄|🔄|⚡/.test(lower) ? 'change'
        : 'other'
      currentGroup = title
      current = { title, group: title, type: currentType, items: [] }
      sections.push(current)

    } else if (/^#{4,}\s/.test(trimmed)) {
      // Sub-section (#### Category) - inherits parent type
      const title = trimmed.replace(/^#+\s*/, '').trim()
      current = { title, group: currentGroup || title, type: currentType, items: [] }
      sections.push(current)

    } else if (/^[-*•]\s/.test(trimmed)) {
      if (!current) {
        current = { title: 'Changes', group: 'Changes', type: 'other', items: [] }
        sections.push(current)
      }
      const item = trimmed
        .replace(/^[-*•]\s*/, '')
        .replace(/\s*\(#\d+\)\s*$/, '')
        .trim()
      if (item) current.items.push(item)

    } else if (trimmed && current && current.items.length === 0 && !/^#+/.test(trimmed) && !/^([-*_])\1{2,}$/.test(trimmed)) {
      // Plain text before any bullet - treat as a description item
      current.items.push(trimmed)
    }
  }

  return sections.filter((s) => s.items.length > 0)
}

/**
 * `x.y.z` with an optional `-pre` tag, or null. A leading `v` is allowed.
 * @param {string} v
 */
function parseVersion(v) {
  const m = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?/.exec(String(v ?? '').trim())
  return m ? { nums: [Number(m[1]), Number(m[2]), Number(m[3])], pre: m[4] ?? '' } : null
}

/**
 * Semver order: negative when a < b, 0 when equal, positive when a > b, and
 * NaN when either is not a version.
 * @param {string} a @param {string} b
 */
export function compareVersions(a, b) {
  const pa = parseVersion(a)
  const pb = parseVersion(b)
  if (!pa || !pb) return NaN
  for (let i = 0; i < 3; i++) {
    if (pa.nums[i] !== pb.nums[i]) return pa.nums[i] - pb.nums[i]
  }
  // A release sorts after its own pre-releases.
  if (pa.pre === pb.pre) return 0
  if (!pa.pre) return 1
  if (!pb.pre) return -1
  return pa.pre.localeCompare(pb.pre, undefined, { numeric: true })
}

/**
 * Split CHANGELOG.md into releases, in file order.
 * @param {string} markdown
 * @returns {ChangelogRelease[]}
 */
export function parseReleases(markdown) {
  /** @type {{ version: string, date: string, lines: string[] }[]} */
  const raw = []
  for (const line of String(markdown ?? '').split('\n')) {
    const head = /^##\s+\[([^\]]+)\](?:\s*-\s*(.+))?\s*$/.exec(line.trim())
    if (head) {
      raw.push({ version: head[1].trim(), date: (head[2] ?? '').trim(), lines: [] })
    } else if (raw.length && !/^##\s/.test(line.trim())) {
      raw[raw.length - 1].lines.push(line)
    }
  }
  return raw.map((r) => ({ version: r.version, date: r.date, sections: parseChangelog(r.lines.join('\n')) }))
}

/**
 * The releases someone skipped over by updating from `previous` to `current`:
 * every version in (previous, current], newest first, up to `limit` of them.
 * `omitted` counts the older ones left out.
 * @param {string} markdown CHANGELOG.md
 * @param {string} previous @param {string} current
 * @param {number} [limit]
 * @returns {{ releases: ChangelogRelease[], omitted: number }}
 */
export function releasesBetween(markdown, previous, current, limit = 5) {
  if (!(compareVersions(current, previous) > 0)) return { releases: [], omitted: 0 }
  const inRange = parseReleases(markdown)
    .filter((r) => r.sections.length > 0)
    .filter((r) => compareVersions(r.version, previous) > 0 && compareVersions(r.version, current) <= 0)
    .sort((a, b) => compareVersions(b.version, a.version))
  return { releases: inRange.slice(0, limit), omitted: Math.max(0, inRange.length - limit) }
}

/**
 * Split a changelog line on backticks so `code` can render as a code span
 * without handing the text to {@html}. An unclosed backtick stays literal.
 * @param {string} text
 * @returns {{ text: string, code: boolean }[]}
 */
export function splitInlineCode(text) {
  /** @type {{ text: string, code: boolean }[]} */
  const parts = []
  const re = /`([^`]+)`/g
  let last = 0
  for (let m = re.exec(text); m; m = re.exec(text)) {
    if (m.index > last) parts.push({ text: text.slice(last, m.index), code: false })
    parts.push({ text: m[1], code: true })
    last = m.index + m[0].length
  }
  if (last < text.length) parts.push({ text: text.slice(last), code: false })
  return parts
}
