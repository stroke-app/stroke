import { describe, it, expect } from 'vitest'
import { normalizePlan, annotatePlan, planInsights, describeNode, planText, planOutline, foldLabel } from './explain-plan.js'

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

  it('does not count parallel workers as repeated loops', () => {
    // Gather with 2 workers: the scan ran in 3 processes at once for ~90ms
    // each, so the plan's 100ms is mostly the scan, not 270ms of it.
    const a = annotatePlan({
      'Node Type': 'Gather', 'Actual Total Time': 100, 'Actual Loops': 1, 'Workers Planned': 2, 'Workers Launched': 2,
      Plans: [{ 'Node Type': 'Parallel Seq Scan', 'Relation Name': 't', 'Actual Total Time': 90, 'Actual Loops': 3, 'Actual Rows': 100, 'Plan Rows': 100 }],
    })
    const [gather, scan] = a.nodes
    expect(scan.totalTime).toBe(90)
    expect(gather.selfTime).toBe(10)
    expect(a.totalTime).toBe(100)
    expect(scan.actualRows).toBe(300)
  })

  it('counts a CTE once', () => {
    // The CTE Scan's 60ms includes the CTE's own 50ms of work.
    const a = annotatePlan({
      'Node Type': 'Hash Join', 'Actual Total Time': 80, 'Actual Loops': 1,
      Plans: [
        { 'Node Type': 'Seq Scan', 'Parent Relationship': 'InitPlan', 'Subplan Name': 'CTE recent', 'Actual Total Time': 50, 'Actual Loops': 1 },
        { 'Node Type': 'CTE Scan', 'CTE Name': 'recent', 'Parent Relationship': 'Outer', 'Actual Total Time': 60, 'Actual Loops': 1 },
        { 'Node Type': 'Seq Scan', 'Parent Relationship': 'Inner', 'Actual Total Time': 15, 'Actual Loops': 1 },
      ],
    })
    const [join, cte, scan] = a.nodes
    expect(join.selfTime).toBe(5)
    expect(cte.selfTime).toBe(50)
    expect(scan.selfTime).toBe(10)
    expect(a.totalTime).toBe(80)
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

describe('planOutline', () => {
  // An Append over 847 partitions where pruning left 3 to scan.
  const parts = Array.from({ length: 847 }, (_, i) => (i === 10 || i === 400 || i === 846
    ? { 'Node Type': 'Seq Scan', 'Relation Name': `p${i}`, 'Actual Total Time': i === 400 ? 30 : 5, 'Actual Loops': 1, 'Actual Rows': 10 }
    : { 'Node Type': 'Seq Scan', 'Relation Name': `p${i}`, 'Actual Total Time': 0, 'Actual Loops': 0, 'Actual Rows': 0 }))
  const a = annotatePlan({ 'Node Type': 'Append', 'Actual Total Time': 45, 'Actual Loops': 1, Plans: parts })

  it('shows the partitions that ran and folds the rest', () => {
    const { entries, childrenOf } = planOutline(a.root, new Set())
    const nodes = entries.filter((e) => e.kind === 'node')
    expect(nodes.map((e) => e.node.relation)).toEqual(['', 'p10', 'p400', 'p846'])
    const fold = entries.find((e) => e.kind === 'fold')
    expect(fold).toMatchObject({ hidden: 844, hiddenRan: 0, expanded: false })
    expect(foldLabel(fold)).toBe('844 more, none ran')
    expect(childrenOf.get(a.root.id)).toHaveLength(4)
  })

  it('opens a folded step in full', () => {
    const { entries } = planOutline(a.root, new Set([a.root.id]))
    expect(entries.filter((e) => e.kind === 'node')).toHaveLength(848)
    expect(foldLabel(entries.at(-1))).toBe('Show fewer')
  })

  it('leaves narrow steps alone', () => {
    const { entries } = planOutline(annotatePlan(pg).root, new Set())
    expect(entries.every((e) => e.kind === 'node')).toBe(true)
  })
})

describe('document engine plans', () => {
  // Shape built by mongo::plan_tree from a real MongoDB 7 explain.
  const mongo = {
    'Node Type': '$group', 'Actual Rows': 1, 'Actual Total Time': 40,
    Plans: [{
      'Node Type': 'COLLSCAN', 'Relation Name': 'orders', 'Actual Rows': 2000, 'Actual Total Time': 38,
      'Docs Examined': 200000, 'Rows Removed by Filter': 198000, Filter: '{"k":{"$eq":3}}',
    }],
  }

  it('flags a MongoDB collection scan that throws away most documents', () => {
    const a = annotatePlan(normalizePlan(mongo, 'mongodb'))
    expect(a.root?.type).toBe('$group')
    expect(a.nodes).toHaveLength(2)
    const insights = planInsights(a)
    expect(insights[0].title).toBe('Full scan on orders throws away most rows')
  })

  it('reads an Elasticsearch profile as a timed tree', () => {
    const es = {
      'Node Type': 'Search', 'Relation Name': 'logs', 'Actual Rows': 1, 'Actual Total Time': 3,
      Plans: [{ 'Node Type': 'Shard', 'Relation Name': 'logs shard 0', 'Actual Total Time': 0.4,
        Plans: [{ 'Node Type': 'BooleanQuery', Filter: '+level:error', 'Actual Total Time': 0.4 }] }],
    }
    const a = annotatePlan(normalizePlan(es, 'elasticsearch'))
    expect(a.nodes.map((n) => n.type)).toEqual(['Search', 'Shard', 'BooleanQuery'])
    expect(a.analyzed).toBe(true)
  })
})
