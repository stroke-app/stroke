/**
 * The AI chat's step timeline, as pure functions: the plan the model opens a
 * multi-step turn with, a one-line label for each SQL step, failed steps
 * grouped with their retries, plan ticking, and the end-of-turn summary.
 *
 * AiChat.svelte draws all of it; nothing here touches the DOM or the store.
 */
import { splitSqlStatements } from '$lib/sql-statements.js'
import { stripSqlComments } from '$lib/sql-write.js'

/**
 * @typedef {{ id: string, tool: string, op: string, detail: string, status: 'running' | 'done' | 'error', error?: string, startedAt?: number, endedAt?: number, affected?: number | null, rowCount?: number | null }} TimelineStep
 * @typedef {{ step: TimelineStep, retries: TimelineStep[], status: 'running' | 'done' | 'error' }} StepGroup
 * @typedef {'pending' | 'running' | 'done' | 'error'} PlanStatus
 */

const PLAN_RE = /<plan>([\s\S]*?)(?:<\/plan>|$)/i

/**
 * Take the `<plan>` block out of a reply. A block still streaming (no closing
 * tag yet) is taken too, so it never flashes as raw text.
 * @param {string} text
 * @returns {{ plan: string[] | null, text: string }}
 */
export function extractPlan(text) {
  const src = String(text ?? '')
  const m = PLAN_RE.exec(src)
  if (!m) return { plan: null, text: src }
  const plan = m[1]
    .split('\n')
    .map((l) => l.replace(/^\s*(?:\d+[.)]|[-*•]|\[[ x]\])\s*/i, '').trim())
    .filter(Boolean)
    .slice(0, 8)
  const rest = (src.slice(0, m.index) + src.slice(m.index + m[0].length)).replace(/\n{3,}/g, '\n\n').trim()
  return { plan: plan.length ? plan : null, text: rest }
}

/** An identifier as written, quotes and schema prefix taken off: `shop`.`orders` → orders. */
const IDENT = '(?:`[^`]+`|"[^"]+"|\\[[^\\]]+\\]|[\\w$]+)(?:\\s*\\.\\s*(?:`[^`]+`|"[^"]+"|\\[[^\\]]+\\]|[\\w$]+))*'

/** @param {string} raw */
function bareName(raw) {
  const parts = raw.split(/\s*\.\s*(?=[`"[\w$])/)
  const last = parts[parts.length - 1] ?? raw
  return last.replace(/^[`"[]|[`"\]]$/g, '')
}

/**
 * Top-level row tuples after VALUES: `VALUES (1,'a'), (2,'b')` is 2.
 * @param {string} sql
 */
export function countValueTuples(sql) {
  const at = /\bvalues\s*\(/i.exec(sql)
  if (!at) return null
  let depth = 0
  let count = 0
  let quote = ''
  for (let i = at.index + at[0].length - 1; i < sql.length; i++) {
    const ch = sql[i]
    if (quote) {
      if (ch === '\\') i++
      else if (ch === quote) quote = ''
      continue
    }
    if (ch === "'" || ch === '"' || ch === '`') quote = ch
    else if (ch === '(') {
      if (depth === 0) count++
      depth++
    } else if (ch === ')') depth--
    else if (depth === 0 && /[;]/.test(ch)) break
    else if (depth === 0 && /[a-z]/i.test(ch)) break // ON DUPLICATE KEY / RETURNING
  }
  return count || null
}

/**
 * What one statement does: the verb, the kind of object, its name, and a
 * one-line label ("Create table customers", "Insert 3 rows into orders").
 * @param {string} statement
 * @returns {{ verb: string, kind: string, name: string, rows: number | null, label: string }}
 */
export function describeStatement(statement) {
  const sql = stripSqlComments(String(statement ?? '')).trim()
  const kw = (/^[a-z]+/i.exec(sql)?.[0] ?? '').toLowerCase()
  /** @param {string} verb @param {string} kind @param {string} name @param {string} label @param {number | null} [rows] */
  const out = (verb, kind, name, label, rows = null) => ({ verb, kind, name, rows, label })
  let m
  if ((m = new RegExp(`^create\\s+(?:or\\s+replace\\s+)?(?:temporary\\s+|temp\\s+)?(table|database|schema|view|materialized\\s+view)\\s+(?:if\\s+not\\s+exists\\s+)?(${IDENT})`, 'i').exec(sql))) {
    const kind = m[1].toLowerCase().replace(/\s+/g, ' ') === 'schema' ? 'database' : m[1].toLowerCase().replace(/\s+/g, ' ')
    const name = bareName(m[2])
    return out('create', kind, name, `Create ${kind} ${name}`)
  }
  if ((m = new RegExp(`^create\\s+(?:unique\\s+)?index\\s+(?:concurrently\\s+)?(?:if\\s+not\\s+exists\\s+)?(${IDENT})?\\s*on\\s+(${IDENT})`, 'i').exec(sql))) {
    const table = bareName(m[2])
    return out('create', 'index', m[1] ? bareName(m[1]) : table, m[1] ? `Create index ${bareName(m[1])} on ${table}` : `Create index on ${table}`)
  }
  if ((m = new RegExp(`^drop\\s+(table|database|schema|view|index)\\s+(?:if\\s+exists\\s+)?(${IDENT})`, 'i').exec(sql))) {
    const kind = m[1].toLowerCase() === 'schema' ? 'database' : m[1].toLowerCase()
    return out('drop', kind, bareName(m[2]), `Drop ${kind} ${bareName(m[2])}`)
  }
  if ((m = new RegExp(`^alter\\s+table\\s+(?:if\\s+exists\\s+)?(${IDENT})`, 'i').exec(sql))) {
    return out('alter', 'table', bareName(m[1]), `Alter table ${bareName(m[1])}`)
  }
  if ((m = new RegExp(`^truncate\\s+(?:table\\s+)?(${IDENT})`, 'i').exec(sql))) {
    return out('truncate', 'table', bareName(m[1]), `Empty table ${bareName(m[1])}`)
  }
  if ((m = new RegExp(`^(?:insert|replace)\\s+(?:ignore\\s+)?(?:into\\s+)?(${IDENT})`, 'i').exec(sql))) {
    const name = bareName(m[1])
    const rows = countValueTuples(sql)
    return out('insert', 'table', name, rows ? `Insert ${rows} ${rows === 1 ? 'row' : 'rows'} into ${name}` : `Insert into ${name}`, rows)
  }
  if ((m = new RegExp(`^update\\s+(?:only\\s+)?(${IDENT})`, 'i').exec(sql))) {
    return out('update', 'table', bareName(m[1]), `Update ${bareName(m[1])}`)
  }
  if ((m = new RegExp(`^delete\\s+from\\s+(?:only\\s+)?(${IDENT})`, 'i').exec(sql))) {
    return out('delete', 'table', bareName(m[1]), `Delete from ${bareName(m[1])}`)
  }
  if ((m = new RegExp(`^use\\s+(${IDENT})`, 'i').exec(sql))) {
    return out('use', 'database', bareName(m[1]), `Switch to ${bareName(m[1])}`)
  }
  if (kw === 'select' || kw === 'with') {
    m = new RegExp(`\\bfrom\\s+(${IDENT})`, 'i').exec(sql)
    const name = m ? bareName(m[1]) : ''
    return out('query', 'table', name, name ? `Query ${name}` : 'Run query')
  }
  if (kw === 'show' || kw === 'describe' || kw === 'desc' || kw === 'explain' || kw === 'pragma') {
    return out('inspect', '', '', kw === 'explain' ? 'Explain query' : 'Inspect schema')
  }
  if (!kw) return out('', '', '', 'Run SQL')
  return out(kw, '', '', `Run ${kw.toUpperCase()}`)
}

/**
 * One line for a whole execute_sql call, which may hold a script.
 * @param {string} sql
 */
export function sqlStepLabel(sql) {
  const stmts = statementsOf(sql)
  if (stmts.length <= 1) return describeStatement(stmts[0] ?? sql).label
  const labels = stmts.map((s) => describeStatement(s).label)
  const shown = labels.slice(0, 2).join(', ')
  return labels.length > 2 ? `${shown} +${labels.length - 2} more` : shown
}

/** @param {string} sql */
function statementsOf(sql) {
  return splitSqlStatements(String(sql ?? ''))
    .map((s) => s.text.trim().replace(/;\s*$/, ''))
    .filter((s) => stripSqlComments(s).trim())
}

/**
 * The key a retry shares with the attempt it replaces: the same verb on the
 * same object. Two reads of different tables are not retries of each other.
 * @param {TimelineStep} step
 */
function retryKey(step) {
  if (step.op !== 'query') return `${step.tool}:${step.detail}`
  const d = describeStatement(statementsOf(step.detail)[0] ?? step.detail)
  return `${d.verb}:${d.kind}:${d.name.toLowerCase()}`
}

/**
 * Fold each failed step's retries under it. A later step with the same
 * verb and target as a still-failing step is its retry, wherever it falls
 * (the model often reads the schema between the failure and the fix).
 * @param {TimelineStep[]} steps
 * @returns {StepGroup[]}
 */
export function groupRetries(steps) {
  /** @type {StepGroup[]} */
  const groups = []
  /** @type {Map<string, StepGroup>} */
  const failing = new Map()
  for (const step of steps) {
    if (step.op === 'note') {
      groups.push({ step, retries: [], status: 'done' })
      continue
    }
    const key = retryKey(step)
    const open = failing.get(key)
    if (open) {
      open.retries.push(step)
      open.status = step.status
      if (step.status !== 'error') failing.delete(key)
      continue
    }
    const group = { step, retries: [], status: step.status }
    groups.push(group)
    if (step.status === 'error') failing.set(key, group)
  }
  return groups
}

/**
 * "Retried · succeeded", "Retried twice · still failing".
 * @param {StepGroup} group
 */
export function retryLabel(group) {
  const n = group.retries.length
  if (!n) return ''
  const times = n === 1 ? 'Retried' : n === 2 ? 'Retried twice' : `Retried ${n} times`
  const how = group.status === 'done' ? 'succeeded' : group.status === 'running' ? 'running' : 'still failing'
  return `${times} · ${how}`
}

/** Words of a plan line or a name, for loose matching. @param {string} s */
function words(s) {
  return String(s).toLowerCase().replace(/[`"'[\]]/g, '').split(/[^a-z0-9_$]+/).filter(Boolean)
}

/**
 * Does a plan line name this object? `order_items` matches "order items" and
 * "order_items"; `customers` matches "customer table" but not "order items"
 * for `orders`: a singular form only counts when no other noun follows it.
 * @param {string} line @param {string} name
 */
function mentions(line, name) {
  if (!name) return false
  const n = name.toLowerCase()
  const text = ` ${words(line).join(' ')} `
  for (const v of new Set([n, n.replace(/_/g, ' ')])) if (v.length > 1 && text.includes(` ${v} `)) return true
  const singular = n.replace(/_/g, ' ').replace(/s$/, '')
  if (singular === n.replace(/_/g, ' ') || singular.length < 2) return false
  const m = new RegExp(` ${singular.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')} (\\S*)`).exec(text)
  return !!m && /^(|table|tables|and|or|with|row|rows|data|record|records)$/.test(m[1])
}

/**
 * The status of each plan line, from the steps so far. A line is done when
 * every step on an object it names has succeeded, failed when one of them is
 * still failing. A line that names nothing the steps touched takes its place
 * in the order: done once a later line has started, or once the turn has
 * finished cleanly. While the turn runs, the first open line is the running one.
 * @param {string[]} plan
 * @param {TimelineStep[]} steps
 * @param {boolean} finished the turn has ended
 * @returns {PlanStatus[]}
 */
export function planProgress(plan, steps, finished) {
  const groups = groupRetries(steps).filter((g) => g.step.op !== 'note')
  /** @type {{ names: string[], status: StepGroup['status'] }[]} */
  const touched = groups.map((g) => {
    const last = g.retries.length ? g.retries[g.retries.length - 1] : g.step
    const names = g.step.op === 'query'
      ? statementsOf(last.detail).map((s) => describeStatement(s).name).filter(Boolean)
      : [g.step.detail]
    return { names, status: g.status }
  })
  /** @type {(PlanStatus | null)[]} */
  const statuses = plan.map((line) => {
    const hits = touched.filter((t) => t.names.some((n) => mentions(line, n)))
    if (!hits.length) return null
    if (hits.some((h) => h.status === 'error')) return 'error'
    if (hits.some((h) => h.status === 'running')) return 'running'
    return 'done'
  })
  const anyFailing = touched.some((t) => t.status === 'error')
  // Unmatched lines: done when something after them has started.
  let laterStarted = finished && !anyFailing
  for (let i = statuses.length - 1; i >= 0; i--) {
    const s = statuses[i]
    if (s === null) statuses[i] = laterStarted ? 'done' : 'pending'
    else laterStarted = true
  }
  const out = /** @type {PlanStatus[]} */ (statuses)
  if (!finished && !out.includes('running')) {
    const next = out.indexOf('pending')
    if (next >= 0) out[next] = 'running'
  }
  return out
}

/**
 * What the turn changed, from the steps that succeeded: "Created database
 * ecommerce · Created 3 tables · Inserted 9 rows into 3 tables".
 * @param {TimelineStep[]} steps
 * @returns {{ text: string, sql: string }} `sql` is every successful write, in order
 */
export function summarizeSteps(steps) {
  /** @type {Map<string, Set<string>>} */
  const created = new Map()
  /** @type {Map<string, Set<string>>} */
  const dropped = new Map()
  /** @type {Map<string, number>} */
  const inserted = new Map()
  const altered = new Set()
  const updated = new Set()
  const deleted = new Set()
  /** @type {string[]} */
  const writes = []
  for (const step of steps) {
    if (step.op !== 'query' || step.status !== 'done') continue
    const stmts = statementsOf(step.detail)
    for (const s of stmts) {
      const d = describeStatement(s)
      if (d.verb === 'query' || d.verb === 'inspect' || d.verb === '') continue
      writes.push(s)
      if (d.verb === 'create') {
        if (!created.has(d.kind)) created.set(d.kind, new Set())
        created.get(d.kind)?.add(d.name)
      } else if (d.verb === 'drop') {
        if (!dropped.has(d.kind)) dropped.set(d.kind, new Set())
        dropped.get(d.kind)?.add(d.name)
      } else if (d.verb === 'insert') {
        const rows = d.rows ?? (stmts.length === 1 ? step.affected ?? 0 : 0)
        inserted.set(d.name, (inserted.get(d.name) ?? 0) + rows)
      } else if (d.verb === 'alter') altered.add(d.name)
      else if (d.verb === 'update') updated.add(d.name)
      else if (d.verb === 'delete' || d.verb === 'truncate') deleted.add(d.name)
    }
  }
  /** @param {string} kind @param {number} n */
  const noun = (kind, n) => (n === 1 ? kind : kind === 'index' ? 'indexes' : `${kind}s`)
  /** @param {string} verb @param {Map<string, Set<string>>} map */
  const objects = (verb, map) =>
    [...map].map(([kind, names]) =>
      names.size === 1 ? `${verb} ${kind} ${[...names][0]}` : `${verb} ${names.size} ${noun(kind, names.size)}`,
    )
  const parts = [...objects('Created', created), ...objects('Dropped', dropped)]
  if (inserted.size) {
    const total = [...inserted.values()].reduce((a, b) => a + b, 0)
    const into = inserted.size === 1 ? [...inserted.keys()][0] : `${inserted.size} tables`
    parts.push(total ? `Inserted ${total} ${total === 1 ? 'row' : 'rows'} into ${into}` : `Inserted into ${into}`)
  }
  if (altered.size) parts.push(`Altered ${altered.size === 1 ? [...altered][0] : `${altered.size} tables`}`)
  if (updated.size) parts.push(`Updated ${updated.size === 1 ? [...updated][0] : `${updated.size} tables`}`)
  if (deleted.size) parts.push(`Deleted from ${deleted.size === 1 ? [...deleted][0] : `${deleted.size} tables`}`)
  return { text: parts.join(' · '), sql: writes.length ? `${writes.join(';\n\n')};` : '' }
}

/**
 * A step's duration, compact: 84ms, 1.2s, 1m 5s.
 * @param {number | null | undefined} ms
 */
export function formatStepDuration(ms) {
  if (ms == null || !Number.isFinite(ms) || ms < 0) return ''
  if (ms < 1000) return `${Math.round(ms)}ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)}s`
  const s = Math.round(ms / 1000)
  return `${Math.floor(s / 60)}m ${s % 60}s`
}
