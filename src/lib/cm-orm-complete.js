/**
 * Completion for the ORM runner (CodeMirror), Drizzle and Prisma.
 *
 * The runner used to hand Monaco's TypeScript service a generated .d.ts of the
 * schema; this reads the same model - tables, their columns, the builder and
 * model methods - straight off the text before the caret:
 *
 * - after a dot, the member chain in front of it (`db.select().from(t).`,
 *   `prisma.user.`, `user.`) decides what follows: builder methods, models,
 *   model methods, a table's columns
 * - inside the object of a Prisma call, the keys the caret is nested under
 *   decide the key that goes here: `where` takes the model's columns and
 *   AND / OR / NOT, a column inside it takes filter operators, `select` and
 *   `data` take columns, the call's own object takes its arguments
 * - anywhere else, the names in scope: `db` and the tables plus the condition
 *   helpers (Drizzle), `prisma` (Prisma)
 */
import { snippetCompletion } from '@codemirror/autocomplete'

/** @typedef {import('@codemirror/autocomplete').Completion} Completion */
/** @typedef {{ mode: 'drizzle' | 'prisma', tables: string[], columns: Record<string, string[]>, loadColumns?: (tables: string[]) => Promise<unknown> }} OrmModel */

/** How far back the caret's context is read. */
const LOOKBEHIND = 6000

const isIdent = (/** @type {string} */ s) => /^[A-Za-z_$][\w$]*$/.test(s)

// ── Drizzle ──────────────────────────────────────────────────────────────────

const DB_METHODS = [
  { label: 'select', snippet: 'select()', info: 'Start a SELECT; optionally pass the columns to pick' },
  { label: 'insert', snippet: 'insert(${table})', info: 'INSERT INTO a table' },
  { label: 'update', snippet: 'update(${table})', info: 'UPDATE a table' },
  { label: 'delete', snippet: 'delete(${table})', info: 'DELETE FROM a table' },
]
const SELECT_METHODS = [
  ['from', 'from(${table})'], ['where', 'where(${cond})'], ['orderBy', 'orderBy(${order})'], ['groupBy', 'groupBy(${col})'],
  ['having', 'having(${cond})'], ['limit', 'limit(${10})'], ['offset', 'offset(${0})'],
  ['leftJoin', 'leftJoin(${table}, ${on})'], ['innerJoin', 'innerJoin(${table}, ${on})'],
  ['rightJoin', 'rightJoin(${table}, ${on})'], ['fullJoin', 'fullJoin(${table}, ${on})'], ['toSQL', 'toSQL()'],
]
const INSERT_METHODS = [
  ['values', 'values({ ${} })'], ['returning', 'returning()'], ['onConflictDoNothing', 'onConflictDoNothing()'],
  ['onConflictDoUpdate', 'onConflictDoUpdate({ target: ${col}, set: { ${} } })'], ['toSQL', 'toSQL()'],
]
const UPDATE_METHODS = [['set', 'set({ ${} })'], ['where', 'where(${cond})'], ['returning', 'returning()'], ['toSQL', 'toSQL()']]
const DELETE_METHODS = [['where', 'where(${cond})'], ['returning', 'returning()'], ['toSQL', 'toSQL()']]
const BUILDER_METHODS = /** @type {Record<string, string[][]>} */ ({
  select: SELECT_METHODS, insert: INSERT_METHODS, update: UPDATE_METHODS, delete: DELETE_METHODS,
})
/** The condition and aggregate helpers in scope, with their arguments. */
const HELPERS = [
  ['eq', 'eq(${col}, ${value})'], ['ne', 'ne(${col}, ${value})'], ['gt', 'gt(${col}, ${value})'],
  ['gte', 'gte(${col}, ${value})'], ['lt', 'lt(${col}, ${value})'], ['lte', 'lte(${col}, ${value})'],
  ['like', "like(${col}, '${%}')"], ['ilike', "ilike(${col}, '${%}')"], ['notIlike', "notIlike(${col}, '${%}')"],
  ['isNull', 'isNull(${col})'], ['isNotNull', 'isNotNull(${col})'], ['inArray', 'inArray(${col}, [${}])'],
  ['notInArray', 'notInArray(${col}, [${}])'], ['between', 'between(${col}, ${min}, ${max})'],
  ['notBetween', 'notBetween(${col}, ${min}, ${max})'], ['and', 'and(${})'], ['or', 'or(${})'], ['not', 'not(${cond})'],
  ['asc', 'asc(${col})'], ['desc', 'desc(${col})'], ['count', 'count(${})'], ['sum', 'sum(${col})'],
  ['avg', 'avg(${col})'], ['max', 'max(${col})'], ['min', 'min(${col})'], ['sql', 'sql`${}`'],
]

// ── Prisma ───────────────────────────────────────────────────────────────────

/** Each model method, the arguments its object takes, and its snippet. */
const MODEL_METHODS = /** @type {Record<string, { args: string[], snippet: string }>} */ ({
  findMany: { args: ['where', 'orderBy', 'take', 'skip', 'select', 'cursor'], snippet: 'findMany({ ${} })' },
  findFirst: { args: ['where', 'orderBy', 'select'], snippet: 'findFirst({ where: { ${} } })' },
  findFirstOrThrow: { args: ['where', 'select'], snippet: 'findFirstOrThrow({ where: { ${} } })' },
  findUnique: { args: ['where', 'select'], snippet: 'findUnique({ where: { ${} } })' },
  findUniqueOrThrow: { args: ['where', 'select'], snippet: 'findUniqueOrThrow({ where: { ${} } })' },
  create: { args: ['data', 'select'], snippet: 'create({ data: { ${} } })' },
  createMany: { args: ['data', 'skipDuplicates'], snippet: 'createMany({ data: [{ ${} }] })' },
  update: { args: ['data', 'where', 'select'], snippet: 'update({ where: { ${} }, data: { } })' },
  updateMany: { args: ['data', 'where'], snippet: 'updateMany({ where: { ${} }, data: { } })' },
  delete: { args: ['where', 'select'], snippet: 'delete({ where: { ${} } })' },
  deleteMany: { args: ['where'], snippet: 'deleteMany({ where: { ${} } })' },
  count: { args: ['where'], snippet: 'count({ where: { ${} } })' },
  aggregate: { args: ['_count', '_sum', '_avg', '_min', '_max', 'where'], snippet: 'aggregate({ ${} })' },
  groupBy: {
    args: ['by', '_count', '_sum', '_avg', '_min', '_max', 'where', 'having', 'orderBy', 'take', 'skip'],
    snippet: "groupBy({ by: ['${}'] })",
  },
  upsert: { args: ['where', 'create', 'update'], snippet: 'upsert({ where: { ${} }, create: { }, update: { } })' },
})
/** Keys whose object holds the model's columns. */
const COLUMN_OBJECTS = new Set(['where', 'select', 'data', 'create', 'update', 'orderBy', 'cursor', 'having', '_count', '_sum', '_avg', '_min', '_max'])
const WHERE_LOGIC = ['AND', 'OR', 'NOT']
const FILTER_OPS = ['equals', 'not', 'in', 'notIn', 'lt', 'lte', 'gt', 'gte', 'contains', 'startsWith', 'endsWith', 'mode']

// ── Reading the text ─────────────────────────────────────────────────────────

/**
 * The member chain ending just before `end` (the dot): `db.select().from(t)`
 * is db, select(), from(). Calls are skipped over by their brackets.
 * @param {string} text @param {number} end exclusive, the index of the dot
 * @returns {{ name: string, call: boolean }[]}
 */
export function chainBefore(text, end) {
  /** @type {{ name: string, call: boolean }[]} */
  const out = []
  let i = end - 1
  for (;;) {
    while (i >= 0 && /\s/.test(text[i])) i--
    let call = false
    if (text[i] === ')') {
      let depth = 0
      for (; i >= 0; i--) {
        if (text[i] === ')') depth++
        else if (text[i] === '(' && --depth === 0) break
      }
      if (i < 0) return out.reverse()
      i--
      call = true
      while (i >= 0 && /\s/.test(text[i])) i--
    }
    const stop = i + 1
    while (i >= 0 && /[\w$]/.test(text[i])) i--
    const name = text.slice(i + 1, stop)
    if (!name) return out.reverse()
    out.push({ name, call })
    while (i >= 0 && /\s/.test(text[i])) i--
    if (text[i] !== '.') return out.reverse()
    i--
  }
}

/**
 * The open brackets around the end of `text`, innermost last, each with the
 * key it is the value of (`where: {` → 'where') or the call it opens
 * (`prisma.user.findMany(` → 'prisma.user.findMany'). Strings and comments are
 * skipped.
 * @param {string} text
 * @returns {{ ch: string, key: string, callee: string }[]}
 */
export function openBrackets(text) {
  /** @type {{ ch: string, key: string, callee: string }[]} */
  const stack = []
  const n = text.length
  for (let i = 0; i < n; i++) {
    const c = text[i]
    if (c === '/' && text[i + 1] === '/') { const nl = text.indexOf('\n', i); if (nl === -1) break; i = nl; continue }
    if (c === '/' && text[i + 1] === '*') { const e = text.indexOf('*/', i + 2); if (e === -1) break; i = e + 1; continue }
    if (c === '"' || c === "'" || c === '`') {
      let j = i + 1
      while (j < n && text[j] !== c) { if (text[j] === '\\') j++; j++ }
      if (j >= n) return stack
      i = j
      continue
    }
    if (c === '{' || c === '[' || c === '(') {
      const before = text.slice(Math.max(0, i - 200), i)
      const key = c === '(' ? '' : /([A-Za-z_$][\w$]*)\s*:\s*$/.exec(before)?.[1] ?? ''
      const callee = c === '(' ? /([\w$]+(?:\s*\.\s*[\w$]+)*)\s*$/.exec(before)?.[1]?.replace(/\s+/g, '') ?? '' : ''
      stack.push({ ch: c, key, callee })
    } else if (c === '}' || c === ']' || c === ')') {
      stack.pop()
    }
  }
  return stack
}

/** Snippet completions from [label, body] pairs. @param {string[][]} pairs @param {string} type @param {number} boost */
const snippets = (pairs, type, boost = 0) => pairs.map(([label, body]) => snippetCompletion(body, { label, type, boost }))

/**
 * @param {OrmModel} model @param {string} name
 */
const tableOf = (model, name) => model.tables.find((t) => t === name) ?? model.tables.find((t) => t.toLowerCase() === name.toLowerCase())

/**
 * The completions for the caret at the end of `text`, or null.
 * @param {string} text the document up to the caret (a bounded slice is fine)
 * @param {OrmModel} model
 * @returns {{ from: number, options: Completion[], needsColumns?: string } | null} `from`: offset in `text`
 */
export function ormCompletions(text, model) {
  const m = /[\w$]*$/.exec(text)
  const prefix = m ? m[0] : ''
  const from = text.length - prefix.length
  let before = from - 1
  while (before >= 0 && /[ \t]/.test(text[before])) before--
  const columns = (/** @type {string} */ t) => model.columns[t]

  // ── After a dot ────────────────────────────────────────────────────────
  if (text[before] === '.') {
    const chain = chainBefore(text, before)
    if (!chain.length) return null
    const head = chain[0].name
    if (model.mode === 'drizzle') {
      if (head === 'db' && chain.length === 1) {
        return { from, options: DB_METHODS.map((d) => snippetCompletion(d.snippet, { label: d.label, type: 'method', info: d.info })) }
      }
      if (head === 'db' && chain[1]?.call) {
        const methods = BUILDER_METHODS[chain[1].name]
        return methods ? { from, options: snippets(methods, 'method') } : null
      }
      const table = chain.length === 1 ? tableOf(model, head) : undefined
      if (table) {
        const cols = columns(table)
        if (!cols) return { from, options: [], needsColumns: table }
        return { from, options: cols.map((c) => ({ label: c, type: 'property', detail: table })) }
      }
      return null
    }
    // Prisma
    if (head === 'prisma' && chain.length === 1) {
      return { from, options: model.tables.filter(isIdent).map((t) => ({ label: t, type: 'class', detail: 'model' })) }
    }
    if (head === 'prisma' && chain.length === 2 && !chain[1].call) {
      return {
        from,
        options: Object.entries(MODEL_METHODS).map(([label, d]) => snippetCompletion(d.snippet, { label, type: 'method', detail: chain[1].name })),
      }
    }
    return null
  }

  // ── A key inside a Prisma call's object ───────────────────────────────
  if (model.mode === 'prisma') {
    const stack = openBrackets(text.slice(0, from))
    const call = stack.findLastIndex((f) => f.ch === '(' && /^prisma\.[\w$]+\.[\w$]+$/.test(f.callee))
    if (call >= 0) {
      const [, modelName, method] = stack[call].callee.split('.')
      const inner = stack.slice(call + 1)
      const last = inner.at(-1)
      // A key goes here: right after `{` or `,` inside an object.
      const keyHere = last?.ch === '{' && /[{,]\s*$/.test(text.slice(0, from))
      if (!keyHere) return null
      /** The object keys from the call's argument inwards. An object in an array
       *  is under the array's key: `AND: [{ | }]` is in AND. */
      const path = []
      for (let k = 0; k < inner.length; k++) {
        if (inner[k].ch !== '{') continue
        path.push(inner[k].key || (inner[k - 1]?.ch === '[' ? inner[k - 1].key : ''))
      }
      const table = tableOf(model, modelName)
      const cols = table ? columns(table) : undefined
      const colOptions = (/** @type {string} */ detail) => (cols ?? []).map((c) => ({ label: c, type: 'property', detail }))
      const needs = table && !cols ? table : undefined
      if (path.length === 1) {
        const args = MODEL_METHODS[method]?.args ?? []
        return { from, options: args.map((a) => ({ label: a, type: 'property', detail: method, apply: argApply(a) })) }
      }
      // Inside where / AND / OR / NOT: columns and logic; a column's own object: its filters.
      const objectKeys = path.slice(1)
      const lastKey = objectKeys.at(-1) ?? ''
      const inWhere = objectKeys.some((k) => k === 'where' || k === 'having')
      if (inWhere && !WHERE_LOGIC.includes(lastKey) && lastKey !== 'where' && lastKey !== 'having') {
        return { from, options: FILTER_OPS.map((op) => ({ label: op, type: 'property', detail: lastKey })) }
      }
      if (inWhere) {
        return {
          from,
          options: [...colOptions(table ?? ''), ...WHERE_LOGIC.map((k) => ({ label: k, type: 'keyword', boost: -1 }))],
          needsColumns: needs,
        }
      }
      if (COLUMN_OBJECTS.has(lastKey)) return { from, options: colOptions(table ?? ''), needsColumns: needs }
      return null
    }
  }

  // ── A name in scope ────────────────────────────────────────────────────
  if (!prefix) return null
  if (model.mode === 'drizzle') {
    return {
      from,
      options: [
        { label: 'db', type: 'variable', boost: 2 },
        ...model.tables.filter(isIdent).map((t) => ({ label: t, type: 'class', detail: 'table', boost: 1 })),
        ...snippets(HELPERS, 'function'),
      ],
    }
  }
  return { from, options: [{ label: 'prisma', type: 'variable', boost: 2 }] }
}

/** An argument key goes in with what its value opens. @param {string} key */
function argApply(key) {
  if (key === 'take' || key === 'skip') return `${key}: `
  if (key === 'skipDuplicates') return `${key}: true`
  if (key === 'by') return `${key}: []`
  return `${key}: {  }`
}

/**
 * A CodeMirror completion source over the ORM model.
 * @param {() => OrmModel} getModel read per query, so a new schema needs no reconfigure
 * @returns {import('@codemirror/autocomplete').CompletionSource}
 */
export function ormCompletionSource(getModel) {
  return async (context) => {
    const start = Math.max(0, context.pos - LOOKBEHIND)
    const text = context.state.sliceDoc(start, context.pos)
    let model = getModel()
    let result = ormCompletions(text, model)
    if (result?.needsColumns && model.loadColumns) {
      // A table whose columns are not loaded yet: fetch them once, briefly.
      await Promise.race([model.loadColumns([result.needsColumns]), new Promise((r) => setTimeout(r, 1500))]).catch(() => {})
      if (context.aborted) return null
      model = getModel()
      result = ormCompletions(text, model)
    }
    if (!result || (!result.options.length && !context.explicit)) return null
    if (!context.explicit && !/[\w$.]$/.test(text) && !/[{,]\s*$/.test(text)) return null
    return { from: start + result.from, options: result.options, validFor: /^[\w$]*$/ }
  }
}
