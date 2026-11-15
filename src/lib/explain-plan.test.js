import { describe, it, expect } from 'vitest'
import { normalizePlan, annotatePlan, planInsights, describeNode, planText } from './explain-plan.js'

const pg = {
  'Node Type': 'Sort', 'Total Cost': 120, 'Actual Total Time': 50, 'Actual Loops': 1, 'Actual Rows': 10, 'Plan Rows': 10,
  'Sort Method': 'external merge', 'Sort Space Used': 4096, 'Sort Space Type': 'Disk',
  Plans: [{
    'Node Type': 'Seq Scan', 'Relation Name': 'users', 'Total Cost': 100, 'Actual Total Time': 40, 'Actual Loops': 1,
    'Actual Rows': 10, 'Plan Rows': 5000, 'Rows Removed by Filter': 90000, Filter: "(email ~~ '%x%')",
  }],
}

describe('annotatePlan', () => {
  it('splits time into each step\'s own share', () => {
    const a = annotatePlan(pg)
    expect(a.analyzed).toBe(true)
    expect(a.nodes.map((n) => n.selfTime)).toEqual([10, 40])
    expect(a.nodes[1].share).toBeCloseTo(0.8)
    expect(a.nodes[1].misestimate).toBe(-500)
  })

  it('falls back to cost when nothing ran', () => {
    const a = annotatePlan({ 'Node Type': 'Limit', 'Total Cost': 10, Plans: [{ 'Node Type': 'Index Scan', 'Total Cost': 8 }] })
    expect(a.analyzed).toBe(false)
    expect(a.nodes[0].selfCost).toBe(2)
    expect(a.nodes[1].share).toBeCloseTo(0.8)
  })

  it('counts loops', () => {
    const a = annotatePlan({ 'Node Type': 'Index Scan', 'Actual Total Time': 0.5, 'Actual Loops': 4, 'Actual Rows': 2, 'Plan Rows': 2 })
    expect(a.nodes[0].totalTime).toBe(2)
    expect(a.nodes[0].actualRows).toBe(8)
  })
})

describe('planInsights', () => {
  it('finds the wasteful scan, the bad estimate and the disk sort', () => {
    const titles = planInsights(annotatePlan(pg)).map((i) => i.title)
    expect(titles).toContain('Full scan on users throws away most rows')
    expect(titles.some((t) => t.startsWith('Row estimate off by 500×'))).toBe(true)
    expect(titles).toContain('Sort spilled to disk')
  })

  it('says nothing about a small, healthy plan', () => {
    expect(planInsights(annotatePlan({ 'Node Type': 'Index Scan', 'Relation Name': 't', 'Actual Total Time': 0.1, 'Actual Rows': 1, 'Plan Rows': 1 }))).toEqual([])
  })
})

describe('normalizePlan', () => {
  it('maps MySQL JSON onto plan nodes', () => {
    const plan = normalizePlan({
      query_block: {
        cost_info: { query_cost: '12.5' },
        ordering_operation: { using_filesort: true, table: { table_name: 'orders', access_type: 'ALL', rows_examined_per_scan: 20000, rows_produced_per_join: 2000, filtered: '10.00', cost_info: { prefix_cost: '12.5', read_cost: '10', eval_cost: '2.5' }, attached_condition: '(status = 1)' } },
      },
    }, 'mysql')
    const a = annotatePlan(plan)
    expect(a.nodes.map((n) => n.type)).toEqual(['Sort', 'Full Table Scan'])
    expect(a.nodes[1].relation).toBe('orders')
    const titles = planInsights(a).map((i) => i.title)
    expect(titles).toContain('Full scan on orders')
    expect(titles).toContain('Filesort')
  })
})

describe('words', () => {
  it('describes common steps and prints the tree', () => {
    expect(describeNode('Hash Join')).toMatch(/hash table/)
    expect(planText(annotatePlan(pg).root)).toMatch(/^Sort .*\n\s+Sort Method: external merge\n\s+-> Seq Scan on users/)
  })
})
