// `strokedb mcp`: an MCP server for AI coding agents (Claude Code, Codex,
// Cursor...). An agent working on a machine somewhere else (a cloud box, a
// container) adds it once and can then show the person the database: it gets
// a shared-studio link that opens a table in their browser or in the Stroke
// app, and switches everyone already on that link to the table it names.
//
//   claude mcp add strokedb -- npx -y strokedb mcp
//
// Stdio, newline-delimited JSON-RPC 2.0. stdout carries only protocol
// messages; anything for a human goes to stderr.

import readline from 'node:readline'
import { openEngine } from './engines.js'
import { makeRoutes, isReadingSql, STUDIO_ORIGIN } from './bridge.js'
import { newShare, shareUrl, startRelayHost, joinPresenceHost } from './relay.js'

const PROTOCOL = '2025-06-18'
/** Rows a query returns to the agent; the studio is for looking at more. */
const QUERY_ROWS = 100
/** Characters of tool output, so one wide table can't flood the agent's context. */
const MAX_TEXT = 24_000

const TOOLS = [
  {
    name: 'list_tables',
    title: 'List tables',
    description: 'List the tables and views in the database, with row counts where known.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    annotations: { readOnlyHint: true },
  },
  {
    name: 'describe_table',
    title: 'Describe a table',
    description: 'Columns of a table: name, type, nullability, primary key, default.',
    inputSchema: {
      type: 'object',
      properties: { table: { type: 'string', description: 'schema.table, or just the table name' } },
      required: ['table'],
      additionalProperties: false,
    },
    annotations: { readOnlyHint: true },
  },
  {
    name: 'query',
    title: 'Run a read-only query',
    description: `Run one read-only statement (SELECT, WITH, EXPLAIN, SHOW) and get up to ${QUERY_ROWS} rows back as JSON. Writes are refused.`,
    inputSchema: {
      type: 'object',
      properties: { sql: { type: 'string', description: 'One read-only SQL statement' } },
      required: ['sql'],
      additionalProperties: false,
    },
    annotations: { readOnlyHint: true },
  },
  {
    name: 'show_table',
    title: 'Show a table to the person',
    description:
      'Open a table for the person in the Stroke studio, wherever they are. Returns a link to the web studio and a stroke:// link for the desktop app, both opening on this table. Anyone who already has the studio open from an earlier link switches to this table at once. Use it when they ask to see data ("show me the users table").',
    inputSchema: {
      type: 'object',
      properties: { table: { type: 'string', description: 'schema.table, or just the table name' } },
      required: ['table'],
      additionalProperties: false,
    },
    annotations: { readOnlyHint: true, openWorldHint: true },
  },
]

/**
 * @param {{
 *   uri: string, engine: string, name: string, write?: boolean,
 *   studio?: string, input?: NodeJS.ReadableStream, output?: NodeJS.WritableStream,
 *   log?: (msg: string) => void,
 * }} o
 */
export async function runMcp({ uri, engine: engineId, name, write = false, studio = `${STUDIO_ORIGIN}/studio`, input = process.stdin, output = process.stdout, log = (m) => console.error(m) }) {
  // Read-only unless --write: the engine itself refuses writes.
  const db = await openEngine(engineId, uri, { readOnly: !write })
  /** @type {{ share: ReturnType<typeof newShare>, relay: { close(): void }, presence: Awaited<ReturnType<typeof joinPresenceHost>> } | null} */
  let shared = null
  let agentName = 'AI agent'

  /** @param {unknown} msg */
  const reply = (msg) => output.write(`${JSON.stringify(msg)}\n`)

  async function ensureShare() {
    if (shared) return shared
    const share = newShare()
    const base = new URL(studio).origin
    const relay = await startRelayHost({
      routes: makeRoutes({ engine: db, name, shared: true, write }),
      ...share,
      base,
      onStatus: (s, d) => log(`strokedb: sharing ${s}${d ? ` (${d})` : ''}`),
    })
    const presence = await joinPresenceHost({ room: share.room, key: share.key, base, name: agentName })
    shared = { share, relay, presence }
    return shared
  }

  /** @param {string} ref */
  async function resolveTable(ref) {
    const tables = await db.tables()
    const lower = ref.toLowerCase()
    return (
      tables.find((t) => `${t.schema}.${t.name}` === ref) ??
      tables.find((t) => t.name === ref) ??
      tables.find((t) => t.name.toLowerCase() === lower || `${t.schema}.${t.name}`.toLowerCase() === lower)
    )
  }

  /** @param {string} tool @param {any} args @returns {Promise<string>} */
  async function call(tool, args) {
    switch (tool) {
      case 'list_tables': {
        const tables = await db.tables()
        if (!tables.length) return 'No tables.'
        return tables
          .map((t) => `${t.schema}.${t.name}${t.kind === 'view' ? ' (view)' : ''}${t.rows != null ? `  ${t.rows} rows` : ''}`)
          .join('\n')
      }
      case 'describe_table': {
        const t = await resolveTable(String(args?.table ?? ''))
        if (!t) throw new Error(`No table called ${args?.table}. Call list_tables to see what there is.`)
        const cols = await db.columns(t.schema, t.name)
        return `${t.schema}.${t.name}\n${cols
          .map((c) => `  ${c.name} ${c.type}${c.primaryKey ? ' PRIMARY KEY' : ''}${c.nullable ? '' : ' NOT NULL'}${c.default != null ? ` DEFAULT ${c.default}` : ''}`)
          .join('\n')}`
      }
      case 'query': {
        const sql = String(args?.sql ?? '')
        if (!isReadingSql(sql)) throw new Error('Only read-only statements here: SELECT, WITH, EXPLAIN or SHOW.')
        const r = await db.query(sql)
        const rows = (r.rows ?? []).slice(0, QUERY_ROWS).map((row) => Object.fromEntries((r.columns ?? []).map((c, i) => [c, row[i]])))
        const more = (r.rows?.length ?? 0) > QUERY_ROWS ? `\n(${r.rows.length - QUERY_ROWS} more rows not shown; show_table opens them all for the person.)` : ''
        return JSON.stringify(rows, null, 1) + more
      }
      case 'show_table': {
        const t = await resolveTable(String(args?.table ?? ''))
        if (!t) throw new Error(`No table called ${args?.table}. Call list_tables to see what there is.`)
        const ref = `${t.schema}.${t.name}`
        const { share, presence } = await ensureShare()
        const watching = presence.here
        await presence.openTable(ref)
        const web = `${shareUrl({ studio, room: share.room, key: share.key })}&${new URLSearchParams({ open: ref })}`
        const app = `stroke://studio?${new URLSearchParams({ relay: share.room, key: share.key, open: ref })}`
        return [
          watching
            ? `Switched the ${watching === 1 ? 'person' : `${watching} people`} on the studio to ${ref}.`
            : `Ready to show ${ref}. Give the person this link:`,
          '',
          `Web: ${web}`,
          `Stroke app: ${app}`,
          '',
          `The link is the key to a live, end-to-end encrypted view of this database (${write ? 'read and write' : 'read-only'}), working while this MCP server runs. Share it only with the person you're working with. Later show_table calls reuse it and switch their view.`,
        ].join('\n')
      }
      default:
        throw new Error(`Unknown tool: ${tool}`)
    }
  }

  /** @param {any} msg */
  async function handle(msg) {
    const { id, method, params } = msg ?? {}
    const isRequest = id !== undefined && id !== null
    try {
      let result
      switch (method) {
        case 'initialize': {
          const client = String(params?.clientInfo?.name ?? '')
          agentName = client === 'claude-code' ? 'Claude Code' : client ? client.replace(/[-_]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase()).slice(0, 40) : 'AI agent'
          result = {
            protocolVersion: typeof params?.protocolVersion === 'string' ? params.protocolVersion : PROTOCOL,
            capabilities: { tools: {} },
            serverInfo: { name: 'strokedb', title: 'Stroke', version: '0.4.0' },
            instructions: `Database: ${name} (${engineId}). Use list_tables, describe_table and query to look at it. When the person wants to see data, call show_table: it returns a link that opens the table for them in the Stroke web studio or desktop app, wherever they are.`,
          }
          break
        }
        case 'ping':
          result = {}
          break
        case 'tools/list':
          result = { tools: TOOLS }
          break
        case 'tools/call': {
          try {
            let text = await call(String(params?.name ?? ''), params?.arguments ?? {})
            if (text.length > MAX_TEXT) text = `${text.slice(0, MAX_TEXT)}\n(cut at ${MAX_TEXT} characters)`
            result = { content: [{ type: 'text', text }] }
          } catch (e) {
            result = { content: [{ type: 'text', text: e instanceof Error ? e.message : String(e) }], isError: true }
          }
          break
        }
        default:
          if (!isRequest) return // notifications/initialized and friends
          return reply({ jsonrpc: '2.0', id, error: { code: -32601, message: `Method not found: ${method}` } })
      }
      if (isRequest) reply({ jsonrpc: '2.0', id, result })
    } catch (e) {
      if (isRequest) reply({ jsonrpc: '2.0', id, error: { code: -32603, message: e instanceof Error ? e.message : String(e) } })
    }
  }

  const rl = readline.createInterface({ input, crlfDelay: Infinity })
  /** @type {Promise<void>[]} */
  const pending = []
  rl.on('line', (line) => {
    if (!line.trim()) return
    let msg
    try {
      msg = JSON.parse(line)
    } catch {
      return void reply({ jsonrpc: '2.0', id: null, error: { code: -32700, message: 'Parse error' } })
    }
    pending.push(handle(msg))
  })
  await new Promise((resolve) => rl.once('close', resolve))
  await Promise.allSettled(pending)
  shared?.presence.close()
  shared?.relay.close()
  await db.close().catch(() => {})
}
