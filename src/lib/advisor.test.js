import { describe, expect, it, test } from 'vitest'
import {
  countByCategory,
  countBySeverity,
  exportRows,
  filterFindings,
  groupByCheck,
  reportSummary,
  severityRank,
  healthScore,
  combinedFix,
  ignoreKey,
} from './advisor.js'

/** @param {Partial<any>} f */
const finding = (f) => ({
  checkId: 'unused_index',
  category: 'performance',
  severity: 'warning',
  title: 'Index never used',
  entity: 'public.articles',
  entityKind: 'index',
  description: 'No scan has touched this index',
  detail: null,
  remediation: null,
  metric: 0,
  ...f,
})

describe('severityRank', () => {
  it('orders worse first, unknown last', () => {
    expect(severityRank('error')).toBeLessThan(severityRank('warning'))
    expect(severityRank('warning')).toBeLessThan(severityRank('info'))
    expect(severityRank('nonsense')).toBeGreaterThan(severityRank('info'))
  })
})

describe('counts', () => {
  const findings = [
    finding({ severity: 'error', category: 'security' }),
    finding({ severity: 'warning', category: 'performance' }),
    finding({ severity: 'warning', category: 'schema' }),
    finding({ severity: 'info', category: 'performance' }),
  ]

  it('counts severities', () => {
    expect(countBySeverity(findings)).toEqual({ error: 1, warning: 2, info: 1 })
  })

  it('counts categories with an all total', () => {
    expect(countByCategory(findings)).toEqual({ all: 4, security: 1, performance: 2, schema: 1 })
  })

  it('ignores a severity it does not know rather than inventing a bucket', () => {
    expect(countBySeverity([finding({ severity: 'critical' })])).toEqual({ error: 0, warning: 0, info: 0 })
  })
})

describe('filterFindings', () => {
  const findings = [
    finding({ severity: 'error', category: 'security', entity: 'public.events', title: 'Table without RLS' }),
    finding({ severity: 'warning', category: 'performance', entity: 'public.articles' }),
    finding({ severity: 'info', category: 'schema', entity: 'analytics.query_log' }),
  ]

  it('filters by severity and by category', () => {
    expect(filterFindings(findings, { severity: 'error' })).toHaveLength(1)
    expect(filterFindings(findings, { category: 'performance' })).toHaveLength(1)
  })

  it('treats the all category as no category filter', () => {
    expect(filterFindings(findings, { category: 'all' })).toHaveLength(3)
  })

  it('searches entity, title and description', () => {
    expect(filterFindings(findings, { query: 'articles' })).toHaveLength(1)
    expect(filterFindings(findings, { query: 'RLS' })).toHaveLength(1)
    expect(filterFindings(findings, { query: 'no scan has touched' })).toHaveLength(3)
    expect(filterFindings(findings, { query: 'nothing here' })).toHaveLength(0)
  })

  it('combines filters', () => {
    expect(filterFindings(findings, { severity: 'error', category: 'performance' })).toHaveLength(0)
  })
})

describe('groupByCheck', () => {
  it('groups by check and keeps the worst severity in the group', () => {
    const groups = groupByCheck([
      finding({ checkId: 'a', severity: 'info', entity: 't1' }),
      finding({ checkId: 'a', severity: 'error', entity: 't2' }),
      finding({ checkId: 'b', severity: 'warning', entity: 't3' }),
    ])
    expect(groups.map((g) => g.checkId)).toEqual(['a', 'b'])
    // 'a' holds an error, so the collapsed row must not read as info.
    expect(groups[0].severity).toBe('error')
    expect(groups[0].findings).toHaveLength(2)
  })

  it('orders by severity, then by how many there are', () => {
    const groups = groupByCheck([
      finding({ checkId: 'few', severity: 'warning' }),
      finding({ checkId: 'many', severity: 'warning' }),
      finding({ checkId: 'many', severity: 'warning' }),
      finding({ checkId: 'worst', severity: 'error' }),
    ])
    expect(groups.map((g) => g.checkId)).toEqual(['worst', 'many', 'few'])
  })
})

describe('exportRows', () => {
  it('flattens to columns a spreadsheet can group itself', () => {
    const { columns, rows } = exportRows([finding({ detail: '194 MB', remediation: 'DROP INDEX x;' })])
    expect(columns.map((c) => c.name)).toEqual(['severity', 'category', 'check', 'entity', 'description', 'detail', 'remediation'])
    expect(rows[0][3]).toBe('public.articles')
    expect(rows[0][6]).toBe('DROP INDEX x;')
  })

  it('writes empty strings, not nulls, for absent detail', () => {
    const { rows } = exportRows([finding()])
    expect(rows[0][5]).toBe('')
    expect(rows[0][6]).toBe('')
  })
})

describe('reportSummary', () => {
  it('distinguishes not-scanned from clean', () => {
    expect(reportSummary(null)).toBe('Not scanned yet')
    expect(reportSummary({ findings: [], checks: [{ status: 'ok' }], unsupported: false })).toBe('Nothing to report')
  })

  it('never claims a clean bill of health for an engine it cannot check', () => {
    expect(reportSummary({ findings: [], checks: [], unsupported: true })).toBe('No checks for this engine yet')
  })

  it('says so when a check could not run', () => {
    const s = reportSummary({
      findings: [finding({ severity: 'error' })],
      checks: [{ status: 'ok' }, { status: 'error' }],
      unsupported: false,
    })
    expect(s).toBe('1 error · 1 check could not run')
  })

  it('pluralises each severity', () => {
    const s = reportSummary({
      findings: [finding({ severity: 'error' }), finding({ severity: 'warning' }), finding({ severity: 'warning' })],
      checks: [{ status: 'ok' }],
      unsupported: false,
    })
    expect(s).toBe('1 error · 2 warnings')
  })
})

describe('healthScore', () => {
  const f = (checkId, severity) => ({ checkId, severity, entity: checkId })
  test('a clean report is 100 and healthy', () => {
    expect(healthScore([])).toEqual({ score: 100, label: 'Healthy', tone: 'good' })
  })
  test('one check is capped, so a widespread pattern is one problem', () => {
    const many = Array.from({ length: 40 }, () => f('rls_disabled', 'warning'))
    expect(healthScore(many).score).toBe(75)
  })
  test('errors weigh more than warnings and info barely moves it', () => {
    expect(healthScore([f('a', 'error')]).score).toBe(88)
    expect(healthScore([f('a', 'warning')]).score).toBe(96)
    expect(healthScore([f('a', 'info')]).score).toBe(99)
    expect(healthScore([f('a', 'error'), f('b', 'error'), f('c', 'error'), f('d', 'error')]).tone).toBe('poor')
  })
})

describe('combinedFix and ignoreKey', () => {
  test('joins fixes under their entity and skips findings without one', () => {
    expect(combinedFix([{ entity: 'a', remediation: 'X;' }, { entity: 'b', remediation: null }, { entity: 'c', remediation: 'Y;' }])).toBe('-- a\nX;\n\n-- c\nY;')
    expect(ignoreKey({ checkId: 'k', entity: 'public.t' })).toBe('k|public.t')
  })
})
