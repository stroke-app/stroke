// Reading a query plan: one tree shape for every engine (Postgres JSON is the
// model; MySQL's JSON is mapped onto it), each node's own share of time and
// cost, how far the row estimates were off, and the findings worth acting on.
// Pure functions, so the view stays a view and this stays testable.

/**
 * @typedef {Record<string, any>} RawNode a Postgres-style plan node
 * @typedef {'high' | 'medium' | 'info'} Severity
 * @typedef {{ id: string, severity: Severity, title: string, detail: string, nodeId: string | null }} Insight
 * @typedef {{
 *   id: string, raw: RawNode, depth: number, parentId: string | null, children: PlanNode[],
 *   type: string, relation: string, index: string,
 *   loops: number, actualRows: number | null, planRows: number | null,
 *   totalTime: number | null, selfTime: number | null,
 *   totalCost: number | null, selfCost: number | null,
 *   share: number, misestimate: number | null, removed: number | null,
 *   warnings: string[],
 * }} PlanNode
 */

const num = (/** @type {any} */ v) => (typeof v === 'number' && Number.isFinite(v) ? v : typeof v === 'string' && v.trim() !== '' && Number.isFinite(Number(v)) ? Number(v) : null)

// ── MySQL ────────────────────────────────────────────────────────────────────

const MYSQL_ACCESS = /** @type {Record<string, string>} */ ({
  ALL: 'Full Table Scan',
  index: 'Full Index Scan',
  range: 'Index Range Scan',
  ref: 'Index Lookup',
  eq_ref: 'Unique Index Lookup',
  const: 'Constant Lookup',
  system: 'Constant Lookup',
  fulltext: 'Fulltext Index Lookup',
  ref_or_null: 'Index Lookup',
  index_merge: 'Index Merge',
})

/** @param {any} t MySQL `table` entry */
function mysqlTable(t) {
  return {
    'Node Type': MYSQL_ACCESS[t.access_type] ?? t.access_type ?? 'Table',
    'Relation Name': t.table_name,
    'Index Name': t.key,
    'Plan Rows': num(t.rows_produced_per_join) ?? num(t.rows_examined_per_scan),
    'Total Cost': num(t.cost_info?.prefix_cost),
    'Self Cost': num(t.cost_info?.read_cost) != null ? (num(t.cost_info.read_cost) ?? 0) + (num(t.cost_info.eval_cost) ?? 0) : null,
    Filter: t.attached_condition,
    'Rows Examined': num(t.rows_examined_per_scan),
    'Filtered %': num(t.filtered),
    'Possible Keys': Array.isArray(t.possible_keys) ? t.possible_keys.join(', ') : undefined,
    'Used Columns': Array.isArray(t.used_columns) ? t.used_columns.join(', ') : undefined,
  }
}

/** @param {any} b a query_block or an operation inside one */
function mysqlBlock(b, type = 'Query Block') {
  /** @type {RawNode[]} */
  const plans = []
  if (b.table) plans.push(mysqlTable(b.table))
  if (Array.isArray(b.nested_loop)) {
    const tables = b.nested_loop.map((/** @type {any} */ x) => (x.table ? mysqlTable(x.table) : mysqlBlock(x)))
    plans.push(tables.length > 1 ? { 'Node Type': 'Nested Loop', Plans: tables } : tables[0])
  }
  for (const [key, label] of /** @type {const} */ ([
    ['ordering_operation', 'Sort'],
    ['grouping_operation', 'Group'],
    ['duplicates_removal', 'Distinct'],
    ['windowing', 'Window'],
  ])) {
    if (b[key]) {
      const inner = mysqlBlock(b[key], label)
      if (b[key].using_filesort) inner['Sort Method'] = 'filesort'
      if (b[key].using_temporary_table) inner['Uses Temporary Table'] = true
      plans.push(inner)
    }
  }
  if (b.union_result?.query_specifications) {
    plans.push({ 'Node Type': 'Union', Plans: b.union_result.query_specifications.map((/** @type {any} */ q) => mysqlBlock(q.query_block)) })
  }
  const node = /** @type {RawNode} */ ({ 'Node Type': type, 'Total Cost': num(b.cost_info?.query_cost) ?? num(b.cost_info?.sort_cost) })
  if (plans.length === 1 && type === 'Query Block') return { ...plans[0], 'Total Cost': plans[0]['Total Cost'] ?? node['Total Cost'] }
  if (plans.length) node.Plans = plans
  return node
}

/**
 * Bring any engine's plan to the Postgres node shape.
 * @param {any} plan @param {string} driver
 * @returns {RawNode | null}
 */
export function normalizePlan(plan, driver) {
  if (!plan || typeof plan !== 'object') return null
  if (driver === 'mysql' && plan.query_block) return mysqlBlock(plan.query_block)
  return plan
}

// ── Annotating ───────────────────────────────────────────────────────────────

/** Time a node spent in total, across all its loops (Postgres reports per loop). @param {RawNode} n */
function totalTimeOf(n) {
  const t = num(n['Actual Total Time'])
  if (t == null) return null
  return t * (num(n['Actual Loops']) ?? 1)
}

/**
 * Walk the plan into a flat list with each node's own time and cost.
 * @param {RawNode | null} plan
 * @returns {{ root: PlanNode | null, nodes: PlanNode[], analyzed: boolean, totalTime: number, totalCost: number }}
 */
export function annotatePlan(plan) {
  /** @type {PlanNode[]} */
  const nodes = []
  if (!plan) return { root: null, nodes, analyzed: false, totalTime: 0, totalCost: 0 }

  /** @param {RawNode} raw @param {number} depth @param {string | null} parentId @param {string} id */
  function walk(raw, depth, parentId, id) {
    const loops = num(raw['Actual Loops']) ?? 1
    const actualRows = num(raw['Actual Rows'])
    const planRows = num(raw['Plan Rows'])
    /** @type {PlanNode} */
    const node = {
      id,
      raw,
      depth,
      parentId,
      children: [],
      type: String(raw['Node Type'] ?? 'Step'),
      relation: String(raw['Relation Name'] ?? raw['CTE Name'] ?? raw['Function Name'] ?? ''),
      index: String(raw['Index Name'] ?? ''),
      loops,
      actualRows: actualRows != null ? actualRows * loops : null,
      planRows: planRows != null ? planRows * (raw['Actual Loops'] != null ? loops : 1) : null,
      totalTime: totalTimeOf(raw),
      selfTime: null,
      totalCost: num(raw['Total Cost']),
      selfCost: num(raw['Self Cost']),
      share: 0,
      misestimate: null,
      removed: (num(raw['Rows Removed by Filter']) ?? 0) * loops + (num(raw['Rows Removed by Join Filter']) ?? 0) * loops || null,
      warnings: [],
    }
    nodes.push(node)
    const kids = Array.isArray(raw.Plans) ? raw.Plans : []
    node.children = kids.map((k, i) => walk(k, depth + 1, id, `${id}.${i}`))
    if (node.totalTime != null) {
      const childTime = node.children.reduce((a, c) => a + (c.totalTime ?? 0), 0)
      node.selfTime = Math.max(0, node.totalTime - childTime)
    }
    if (node.selfCost == null && node.totalCost != null) {
      const childCost = node.children.reduce((a, c) => a + (c.totalCost ?? 0), 0)
      node.selfCost = Math.max(0, node.totalCost - childCost)
    }
    if (node.actualRows != null && node.planRows != null) {
      const a = Math.max(1, node.actualRows)
      const p = Math.max(1, node.planRows)
      node.misestimate = a >= p ? a / p : -(p / a)
    }
    return node
  }

  const root = walk(plan, 0, null, '0')
  const analyzed = nodes.some((n) => n.totalTime != null)
  const totalTime = analyzed ? nodes.reduce((a, n) => a + (n.selfTime ?? 0), 0) : 0
  const totalCost = nodes.reduce((a, n) => a + (n.selfCost ?? 0), 0)
  for (const n of nodes) {
    n.share = analyzed ? (totalTime > 0 ? (n.selfTime ?? 0) / totalTime : 0) : totalCost > 0 ? (n.selfCost ?? 0) / totalCost : 0
  }
  return { root, nodes, analyzed, totalTime, totalCost }
}

// ── Findings ─────────────────────────────────────────────────────────────────

const fmt = (/** @type {number} */ n) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e4 ? `${Math.round(n / 1e3)}k` : n.toLocaleString('en-US'))
const isFullScan = (/** @type {string} */ t) => /^(Seq Scan|Full Table Scan|Parallel Seq Scan)$/i.test(t) || /^SCAN \w+$/i.test(t)

/**
 * What is worth fixing, worst first. Every node also gets its own warnings,
 * for the badge on its card.
 * @param {ReturnType<typeof annotatePlan>} a
 * @returns {Insight[]}
 */
export function planInsights(a) {
  /** @type {Insight[]} */
  const out = []
  const add = (/** @type {PlanNode | null} */ n, /** @type {Severity} */ severity, /** @type {string} */ title, /** @type {string} */ detail) => {
    out.push({ id: `${n?.id ?? 'plan'}:${out.length}`, severity, title, detail, nodeId: n?.id ?? null })
    if (n) n.warnings.push(title)
  }

  for (const n of a.nodes) {
    const r = n.raw
    const rows = n.actualRows ?? n.planRows ?? 0
    const where = n.relation ? ` on ${n.relation}` : ''

    if (isFullScan(n.type) && n.relation) {
      const filter = r.Filter ? ` filtered by ${String(r.Filter)}` : ''
      if (n.removed && n.removed > Math.max(1000, rows * 4)) {
        add(n, 'high', `Full scan${where} throws away most rows`, `Read every row and kept ${fmt(rows)}, discarding ${fmt(n.removed)}${filter}. An index on the filtered columns would let it read only the matches.`)
      } else if (rows > 10000 || (num(r['Rows Examined']) ?? 0) > 10000) {
        add(n, 'medium', `Full scan${where}`, `Reads all ${fmt(Math.max(rows, num(r['Rows Examined']) ?? 0))} rows${filter}. Fine for small tables or when most rows are needed; otherwise look at an index.`)
      }
    }
    if (n.misestimate != null && Math.abs(n.misestimate) >= 10 && Math.max(n.actualRows ?? 0, n.planRows ?? 0) >= 100) {
      const over = n.misestimate < 0
      add(n, Math.abs(n.misestimate) >= 100 ? 'high' : 'medium', `Row estimate off by ${Math.round(Math.abs(n.misestimate))}×${where}`,
        `Planned ${fmt(n.planRows ?? 0)} rows, got ${fmt(n.actualRows ?? 0)}. The planner ${over ? 'overestimated' : 'underestimated'}, which can pick the wrong join or scan. Run ANALYZE on the table${n.relation ? ` (ANALYZE ${n.relation})` : ''} to refresh statistics.`)
    }
    if (/sort/i.test(n.type) && /external|disk/i.test(String(r['Sort Method'] ?? '') + String(r['Sort Space Type'] ?? ''))) {
      add(n, 'high', 'Sort spilled to disk', `Used ${r['Sort Space Used'] ?? '?'} kB on disk. Raising work_mem, or an index that already returns rows in this order, keeps it in memory.`)
    }
    if (r['Sort Method'] === 'filesort' || r['Uses Temporary Table']) {
      add(n, 'medium', r['Uses Temporary Table'] ? 'Temporary table' : 'Filesort', 'MySQL sorts or groups outside an index here. An index matching the ORDER BY or GROUP BY avoids it.')
    }
    if (/hash/i.test(n.type) && (num(r['Hash Batches']) ?? 1) > 1) {
      add(n, 'medium', 'Hash split into batches', `${r['Hash Batches']} batches: the hash table didn't fit in work_mem and went to disk.`)
    }
    if (n.type === 'Nested Loop' && n.children[1] && n.children[1].loops > 1000) {
      add(n, 'medium', `Inner side ran ${fmt(n.children[1].loops)} times`, 'A nested loop repeats its inner step once per outer row. With this many rows a hash or merge join is often cheaper, if the estimates allow the planner to choose one.')
    }
    const read = num(r['Shared Read Blocks']) ?? 0
    const hit = num(r['Shared Hit Blocks']) ?? 0
    if (read > 1000 && read > hit) {
      add(n, 'info', `Read from disk${where}`, `${fmt(read)} blocks (${fmt(Math.round((read * 8) / 1024))} MB) came from disk, ${fmt(hit)} from cache. A second run is usually faster.`)
    }
  }

  const hottest = a.nodes.reduce((b, n) => (n.share > (b?.share ?? 0) ? n : b), /** @type {PlanNode | null} */ (null))
  if (hottest && hottest.share >= 0.5 && a.nodes.length > 1) {
    out.push({
      id: 'hottest',
      severity: 'info',
      title: `${Math.round(hottest.share * 100)}% of the ${a.analyzed ? 'time' : 'cost'} is in ${hottest.type}${hottest.relation ? ` on ${hottest.relation}` : ''}`,
      detail: 'Start here: speeding up any other step changes little.',
      nodeId: hottest.id,
    })
  }
  const rank = { high: 0, medium: 1, info: 2 }
  return out.sort((x, y) => rank[x.severity] - rank[y.severity])
}

// ── Words ────────────────────────────────────────────────────────────────────

const ABOUT = /** @type {[RegExp, string][]} */ ([
  [/^(Parallel )?Seq Scan$|^Full Table Scan$|^SCAN /i, 'Reads the whole table, row by row, and checks each one.'],
  [/^(Parallel )?Index Only Scan$/i, 'Answers from the index alone, without visiting the table. Usually the fastest way in.'],
  [/^(Parallel )?Index Scan$|^Index Lookup$|^Unique Index Lookup$|^SEARCH /i, 'Finds the matching rows through an index, then reads them from the table.'],
  [/^Bitmap Index Scan$/i, 'Collects the locations of matching rows from an index, for a Bitmap Heap Scan to read.'],
  [/^Bitmap Heap Scan$/i, 'Reads the table pages a bitmap index scan pointed at, in physical order.'],
  [/^Index Range Scan$/i, 'Reads a range of an index (between, <, >, LIKE prefix).'],
  [/^Full Index Scan$/i, 'Reads the entire index. Cheaper than the table, still every entry.'],
  [/^Nested Loop$/i, 'For each row from the first input, looks up matches in the second. Quick when the first side is small.'],
  [/^Hash Join$/i, 'Builds a hash table from one input, then probes it with every row of the other.'],
  [/^Hash$/i, 'Builds the in-memory hash table a Hash Join probes.'],
  [/^Merge Join$/i, 'Walks two inputs sorted on the join key side by side.'],
  [/^(Incremental )?Sort$/i, 'Orders rows for ORDER BY, a merge join or grouping.'],
  [/aggregate|^Group$/i, 'Combines rows into groups or totals (GROUP BY, count, sum).'],
  [/^Limit$/i, 'Stops once it has enough rows.'],
  [/^Materialize$/i, 'Keeps its input in memory so it can be read more than once.'],
  [/^Memoize$/i, 'Caches inner-side results by key, so repeated lookups are free.'],
  [/^Gather( Merge)?$/i, 'Collects rows from parallel workers.'],
  [/^CTE Scan$/i, 'Reads the result of a WITH query.'],
  [/^Subquery Scan$/i, 'Reads the output of a subquery.'],
  [/^Result$/i, 'Produces rows without reading a table (constants, a one-row SELECT).'],
  [/^Append$|^Union$/i, 'Concatenates several inputs (UNION ALL, partitions).'],
  [/^(Insert|Update|Delete|Merge|ModifyTable)$/i, 'Writes the rows it receives. Explained with Analyze, it ran and was rolled back.'],
  [/^Unique$|^Distinct$/i, 'Drops duplicate rows.'],
  [/^WindowAgg$|^Window$/i, 'Computes window functions (OVER ...).'],
  [/^Function Scan$/i, 'Reads rows a set-returning function produces.'],
  [/^Values Scan$/i, 'Reads a VALUES list.'],
])

/** One plain sentence for a node type. @param {string} type */
export function describeNode(type) {
  for (const [re, text] of ABOUT) if (re.test(type)) return text
  return ''
}

/** Keys shown as conditions in the details panel, in this order. */
export const CONDITION_KEYS = [
  'Index Cond', 'Recheck Cond', 'Filter', 'Join Filter', 'Hash Cond', 'Merge Cond', 'Join Type',
  'Sort Key', 'Group Key', 'Sort Method', 'Strategy', 'Possible Keys', 'Used Columns', 'Output',
]

/** @param {any} v */
export function conditionText(v) {
  if (Array.isArray(v)) return v.join(', ')
  if (v && typeof v === 'object') return JSON.stringify(v)
  return String(v)
}

/** Milliseconds for people. @param {number | null | undefined} ms */
export function fmtMs(ms) {
  if (ms == null) return '–'
  if (ms >= 1000) return `${(ms / 1000).toFixed(2)} s`
  if (ms >= 10) return `${ms.toFixed(0)} ms`
  if (ms >= 1) return `${ms.toFixed(1)} ms`
  return `${ms.toFixed(3)} ms`
}

/** @param {number | null | undefined} n */
export function fmtCount(n) {
  if (n == null) return '–'
  return fmt(Math.round(n))
}

/**
 * The plan as indented text (for copying into an issue or a chat).
 * @param {PlanNode | null} root
 */
export function planText(root) {
  if (!root) return ''
  const lines = /** @type {string[]} */ ([])
  const walk = (/** @type {PlanNode} */ n) => {
    const bits = [`${n.type}${n.relation ? ` on ${n.relation}` : ''}${n.index ? ` using ${n.index}` : ''}`]
    if (n.actualRows != null) bits.push(`rows=${n.actualRows}${n.planRows != null ? ` (est ${n.planRows})` : ''}`)
    else if (n.planRows != null) bits.push(`est rows=${n.planRows}`)
    if (n.selfTime != null) bits.push(`self ${fmtMs(n.selfTime)}`)
    if (n.totalCost != null) bits.push(`cost ${n.totalCost}`)
    lines.push(`${'  '.repeat(n.depth)}${n.depth ? '-> ' : ''}${bits.join('  ')}`)
    for (const k of CONDITION_KEYS) if (k !== 'Output' && k !== 'Used Columns' && n.raw[k] != null) lines.push(`${'  '.repeat(n.depth + 2)}${k}: ${conditionText(n.raw[k])}`)
    n.children.forEach(walk)
  }
  walk(root)
  return lines.join('\n')
}
