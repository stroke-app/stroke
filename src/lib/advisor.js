/**
 * Advisor report shaping: severity order, grouping, counting, filtering.
 *
 * Pure functions, deliberately. The backend decides *what* is wrong; everything
 * about how the report reads - what counts as worse, what collapses into what -
 * lives here where it can be tested without a database.
 */

/** @typedef {import('$lib/api.js').AdvisorFinding} AdvisorFinding */

export const SEVERITIES = /** @type {const} */ (['error', 'warning', 'info'])
export const CATEGORIES = /** @type {const} */ (['security', 'performance', 'schema'])

/** Display metadata. Colours are semantic tokens, never raw hex. */
export const SEVERITY_META = {
  error: { label: 'Errors', dot: 'bg-destructive', text: 'text-destructive', icon: 'circle-alert' },
  warning: { label: 'Warnings', dot: 'bg-warning', text: 'text-warning', icon: 'triangle-alert' },
  info: { label: 'Info', dot: 'bg-info', text: 'text-info', icon: 'info' },
}

export const CATEGORY_META = {
  security: { label: 'Security', icon: 'shield-check' },
  performance: { label: 'Performance', icon: 'gauge' },
  schema: { label: 'Schema', icon: 'table-2' },
}

/** Worse first. Used for both sorting and tie-breaks. @param {string} s */
export function severityRank(s) {
  const i = SEVERITIES.indexOf(/** @type {any} */ (s))
  return i === -1 ? SEVERITIES.length : i
}

/**
 * How many findings sit at each severity.
 * @param {AdvisorFinding[]} findings
 * @returns {Record<string, number>}
 */
export function countBySeverity(findings) {
  const out = { error: 0, warning: 0, info: 0 }
  for (const f of findings) if (f.severity in out) out[f.severity] += 1
  return out
}

/**
 * How many findings sit in each category, plus an `all` total - what the category
 * tabs show. Counts reflect the *severity-filtered* set the caller passes in, so a
 * tab never promises rows that the active filter would hide.
 * @param {AdvisorFinding[]} findings
 * @returns {Record<string, number>}
 */
export function countByCategory(findings) {
  const out = { all: findings.length, security: 0, performance: 0, schema: 0 }
  for (const f of findings) if (f.category in out) out[f.category] += 1
  return out
}

/**
 * Narrow a report to the active severity and category.
 * @param {AdvisorFinding[]} findings
 * @param {{ severity?: string | null, category?: string | null, query?: string }} filter
 */
export function filterFindings(findings, { severity = null, category = null, query = '' } = {}) {
  const q = query.trim().toLowerCase()
  return findings.filter((f) => {
    if (severity && f.severity !== severity) return false
    if (category && category !== 'all' && f.category !== category) return false
    if (!q) return true
    // Entity first: searching an advisor report is almost always "what does it say
    // about THIS table".
    return (
      f.entity.toLowerCase().includes(q) ||
      f.title.toLowerCase().includes(q) ||
      f.description.toLowerCase().includes(q)
    )
  })
}

/**
 * Collapse findings into one group per check.
 *
 * A flat list is unusable at the sizes this produces - "235 warnings" is a wall,
 * while "Foreign key without an index (37)" is a decision. Groups keep the worst
 * severity any of their members carries, so nothing hides inside a collapsed row.
 * @param {AdvisorFinding[]} findings
 */
export function groupByCheck(findings) {
  /** @type {Map<string, { checkId: string, title: string, category: string, severity: string, findings: AdvisorFinding[] }>} */
  const groups = new Map()
  for (const f of findings) {
    const g = groups.get(f.checkId)
    if (!g) {
      groups.set(f.checkId, {
        checkId: f.checkId,
        title: f.title,
        category: f.category,
        severity: f.severity,
        findings: [f],
      })
      continue
    }
    g.findings.push(f)
    if (severityRank(f.severity) < severityRank(g.severity)) g.severity = f.severity
  }
  return [...groups.values()].sort(
    (a, b) =>
      severityRank(a.severity) - severityRank(b.severity) ||
      b.findings.length - a.findings.length ||
      a.title.localeCompare(b.title),
  )
}

/**
 * Rows for CSV/JSON/Markdown export - the same shape whatever the format, and
 * flat, because a spreadsheet can group but cannot ungroup.
 * @param {AdvisorFinding[]} findings
 */
export function exportRows(findings) {
  const columns = [
    { name: 'severity' },
    { name: 'category' },
    { name: 'check' },
    { name: 'entity' },
    { name: 'description' },
    { name: 'detail' },
    { name: 'remediation' },
  ]
  const rows = findings.map((f) => [
    f.severity,
    f.category,
    f.title,
    f.entity,
    f.description,
    f.detail ?? '',
    f.remediation ?? '',
  ])
  return { columns, rows }
}

/**
 * One-line verdict for the header. Deliberately not "0 issues found" when checks
 * failed or the engine has none - a report that can't distinguish "clean" from
 * "didn't look" is worse than no report.
 * @param {{ findings: AdvisorFinding[], checks: Array<{ status: string }>, unsupported: boolean } | null} report
 */
export function reportSummary(report) {
  if (!report) return 'Not scanned yet'
  if (report.unsupported) return 'No checks for this engine yet'
  const failed = report.checks.filter((c) => c.status !== 'ok').length
  const counts = countBySeverity(report.findings)
  const parts = []
  if (counts.error) parts.push(`${counts.error} error${counts.error === 1 ? '' : 's'}`)
  if (counts.warning) parts.push(`${counts.warning} warning${counts.warning === 1 ? '' : 's'}`)
  if (counts.info) parts.push(`${counts.info} suggestion${counts.info === 1 ? '' : 's'}`)
  const head = parts.length ? parts.join(' · ') : 'Nothing to report'
  if (failed) return `${head} · ${failed} check${failed === 1 ? '' : 's'} could not run`
  return head
}
