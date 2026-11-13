import { writable } from 'svelte/store'
import { agentList } from '$lib/api.js'
import { queryLanguageFor } from '$lib/ai-query-languages.js'

/**
 * Local agent CLIs as AI providers. The backend (src-tauri/src/agents.rs)
 * runs each with its own tools off and Stroke's read-only MCP server attached;
 * this module holds what the chat needs to drive one and draw its events.
 */

/** @type {import('svelte/store').Writable<import('$lib/api.js').AgentInfo[]>} */
export const localAgents = writable([])

let probing = /** @type {Promise<void> | null} */ (null)

/** Probe the installed CLIs. Concurrent calls share one probe. */
export function refreshLocalAgents() {
  probing ??= agentList()
    .then((list) => localAgents.set(list))
    .catch(() => {})
    .finally(() => { probing = null })
  return probing
}

const PREFS_KEY = 'stroke:agent-prefs'

/**
 * How hard the agent thinks, and whether it may change data. Shared by every
 * chat, like the model.
 * @typedef {'read' | 'write' | 'modify' | 'dangerous'} AgentAccess
 * @typedef {{ effort: 'low' | 'medium' | 'high' | 'xhigh' | 'max', access: AgentAccess }} AgentPrefs
 */

/** @returns {AgentPrefs} */
function loadPrefs() {
  /** @type {AgentPrefs} */
  const prefs = { effort: 'medium', access: 'read' }
  try {
    const raw = JSON.parse(localStorage.getItem(PREFS_KEY) ?? '{}')
    if (EFFORTS.some((e) => e.id === raw.effort)) prefs.effort = raw.effort
    if (ACCESS_LEVELS.includes(raw.access)) prefs.access = raw.access
  } catch { /* ignore */ }
  return prefs
}

/** In increasing order of what the agent may run; the backend enforces it. */
export const ACCESS_LEVELS = /** @type {AgentAccess[]} */ (['read', 'write', 'modify', 'dangerous'])

export const EFFORTS = /** @type {const} */ ([
  { id: 'low', label: 'Low' },
  { id: 'medium', label: 'Medium' },
  { id: 'high', label: 'High' },
  { id: 'xhigh', label: 'Extra high' },
  { id: 'max', label: 'Max' },
])

/** Agents whose CLI takes an effort level. OpenCode encodes it in the model id. */
export const AGENTS_WITH_EFFORT = new Set(['claude', 'codex'])

export const agentPrefs = writable(loadPrefs())
agentPrefs.subscribe((p) => {
  try { localStorage.setItem(PREFS_KEY, JSON.stringify(p)) } catch { /* ignore */ }
})

/** Profile provider id for an agent: `agent:claude`. @param {string} agent */
export const agentProvider = (agent) => `agent:${agent}`

/** The agent a profile drives, or null for an HTTP provider. @param {{ provider?: string } | null | undefined} profile */
export function profileAgent(profile) {
  const p = profile?.provider ?? ''
  return p.startsWith('agent:') ? p.slice(6) : null
}

/**
 * Stroke's tool name without the CLI's namespacing: Claude Code says
 * `mcp__stroke__execute_sql`, OpenCode `stroke_execute_sql`.
 * @param {string} name
 */
export function strokeToolName(name) {
  return name.replace(/^mcp__stroke__/, '').replace(/^stroke_/, '')
}

/**
 * The text of a tool result, whichever shape the CLI reported it in.
 * @param {any} output deck's `ToolOutput`
 */
export function toolOutputText(output) {
  if (output == null) return ''
  if (typeof output === 'string') return output
  if ('Text' in output) return String(output.Text)
  if ('Terminal' in output) return String(output.Terminal.text)
  if ('Truncated' in output) return String(output.Truncated.preview)
  if ('FileExcerpt' in output) return String(output.FileExcerpt.text)
  if ('Structured' in output) {
    const s = output.Structured
    const content = Array.isArray(s?.content) ? s.content : Array.isArray(s) ? s : null
    if (content) return content.map((c) => (typeof c?.text === 'string' ? c.text : '')).join('')
    return JSON.stringify(s)
  }
  return ''
}

/**
 * An execute_sql result as the chat's result card wants it, or null when the
 * text is not one (an error message, a refused write).
 * @param {string} text
 * @returns {{ columns: { name: string }[], rows: unknown[][], total: number } | null}
 */
export function parseSqlResult(text) {
  let v
  try { v = JSON.parse(text) } catch { return null }
  if (!v || !Array.isArray(v.columns) || !Array.isArray(v.rows)) return null
  const names = v.columns.map((c) => String(typeof c === 'string' ? c : c?.name ?? ''))
  const rows = v.rows.map((r) => (Array.isArray(r) ? r : names.map((n) => r?.[n])))
  return { columns: names.map((name) => ({ name })), rows, total: Number(v.row_count ?? rows.length) }
}

/** @type {Record<AgentAccess, string>} */
const ACCESS_RULES = {
  read: 'Your access is read only: SELECT, EXPLAIN and SHOW.',
  write: 'Your access is read & write: you may INSERT, and UPDATE or DELETE rows with a WHERE clause. No schema changes.',
  modify: 'Your access includes schema changes: CREATE, ALTER (not dropping anything), indexes, and row changes with a WHERE clause.',
  dangerous: 'Your access is unrestricted, including DROP, TRUNCATE and changes without a WHERE clause. Before anything that deletes or overwrites data, say exactly what it will do and wait for the user to confirm.',
}

/**
 * The agent's standing instructions. Short on purpose: the agent reads the
 * schema through its tools instead of carrying it in the prompt.
 * @param {{ dbType?: string, activeSchema?: string, activeTable?: string | null, tables?: string[], connectionName?: string, access?: AgentAccess, webAccess?: boolean }} ctx
 */
export function agentSystemPrompt(ctx) {
  const tables = (ctx.tables ?? []).slice(0, 200)
  // NoSQL engines: execute_sql takes the engine's own language (mongosh, CQL, ...).
  const lang = queryLanguageFor(ctx.dbType)
  return [
    'You are the database assistant inside Stroke, a desktop database client.',
    `The connected database is ${ctx.dbType || 'unknown engine'}${ctx.connectionName ? ` ("${ctx.connectionName}")` : ''}, schema "${ctx.activeSchema || 'public'}".`,
    ctx.activeTable ? `The table open in Stroke is ${ctx.activeTable}.` : '',
    tables.length ? `Tables: ${tables.join(', ')}${(ctx.tables?.length ?? 0) > tables.length ? ', …' : ''}.` : '',
    '',
    lang
      ? `Use the stroke tools (list_tables, describe_table, execute_sql) to look at the real data before answering. execute_sql takes ${lang.name}, not SQL.\n\n${lang.guide}`
      : 'Use the stroke tools (list_tables, describe_table, sample_rows, execute_sql, explain_query, get_database_stats) to look at the real schema and data before answering. Write SQL in this engine\'s dialect.',
    ACCESS_RULES[ctx.access ?? 'read'],
    `Stroke refuses any statement above that level. When a change is needed that your level does not allow, do not try it: hand it to the user with open_in_stroke (query) - it opens in Stroke's editor for them to review and run - or put it in a \`\`\`${lang?.fence ?? 'sql'} block.`,
    'Answer in Markdown. Show query results as Markdown tables when they are small.',
    '',
    // Stroke draws these itself; without being told, an agent believed it
    // could only make text and drew bar charts out of block characters.
    'Stroke renders charts and diagrams in the chat. To chart data, run the query, then write a ```chart block holding JSON: {"type": "bar", "title": "...", "x_col": "<column>", "y_col": "<column>", "group_col": "<optional series column>", "data": [<the result rows as objects>]}. Types: bar, bar-horizontal, bar-grouped, bar-stacked, line, area, line-multi, pie, donut, scatter, heatmap, histogram, funnel, treemap, sankey, gauge, radar, box-plot. Stroke draws it as an interactive chart (tooltips, zoom, export).',
    'For a diagram (ERD, flow, sequence, state), write a ```mermaid block whose first line is the diagram type (erDiagram, flowchart TD, …); Stroke draws it.',
    'Never draw a chart with text characters, and never say you cannot make charts or images.',
    ctx.webAccess
      ? 'Web access is on: for facts the database cannot answer - current exchange rates and prices, outside facts, docs, error codes - use your web search and fetch tools, then cite the URL. Do not guess or say you cannot know.'
      : '',
  ].filter(Boolean).join('\n')
}

/** @type {Record<string, string>} */
const STEP_LABELS = {
  list_tables: 'Listed tables',
  describe_table: 'Described',
  execute_sql: 'Ran query',
  explain_query: 'Explained query',
  get_database_stats: 'Read database stats',
  check_migrations: 'Checked migrations',
  export_query: 'Exported query',
  list_databases: 'Listed connections',
  current_database: 'Checked connection',
  sample_rows: 'Sampled rows',
  open_in_stroke: 'Opened in Stroke',
  WebSearch: 'Searched the web',
  WebFetch: 'Read page',
  web_search: 'Searched the web',
  web_fetch: 'Read page',
  webfetch: 'Read page',
}

/** A web search or page fetch (the agent's own tools, allowed with web access). @param {string} tool */
export function isWebTool(tool) {
  return /^(websearch|webfetch|web_search|web_fetch)$/i.test(tool)
}

/**
 * What a folded step says: "Described tenants", "Read database stats".
 * @param {{ tool: string, op: string, detail: string }} step
 * @returns {{ label: string, detail: string }}
 */
export function stepLabel(step) {
  const label = STEP_LABELS[step.tool]
    ?? step.tool.replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase())
  return { label, detail: step.tool === 'describe_table' || isWebTool(step.tool) ? step.detail : '' }
}

/**
 * The database's own words from a failed tool call, without the CLI's and the
 * MCP layer's wrapping ("tool call error: … Caused by: Mcp error: -32000: …").
 * @param {string} text
 */
export function cleanToolError(text) {
  const t = String(text ?? '').trim()
  const mcp = /Mcp error:\s*-?\d+:\s*([\s\S]*)$/i.exec(t)
  const inner = mcp ? mcp[1] : t.replace(/^tool call (?:error|failed)[^:]*:\s*/i, '')
  return inner.replace(/^Caused by:\s*/i, '').trim() || t
}
