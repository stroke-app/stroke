import { describe, it, expect, vi } from 'vitest'

vi.mock('$lib/api.js', () => ({ agentList: vi.fn(async () => []) }))

const { strokeToolName, toolOutputText, parseSqlResult, profileAgent, agentSystemPrompt } = await import('./local-agents.js')

describe('local agents', () => {
  it('strips each CLI namespace from Stroke tool names', () => {
    expect(strokeToolName('mcp__stroke__execute_sql')).toBe('execute_sql')
    expect(strokeToolName('stroke_list_tables')).toBe('list_tables')
    expect(strokeToolName('describe_table')).toBe('describe_table')
  })

  it('reads tool output in every shape the CLIs report', () => {
    expect(toolOutputText({ Text: 'a' })).toBe('a')
    expect(toolOutputText({ Structured: { content: [{ type: 'text', text: 'b' }, { type: 'text', text: 'c' }] } })).toBe('bc')
    expect(toolOutputText({ Terminal: { text: 'd', exit_code: 0 } })).toBe('d')
    expect(toolOutputText({ Truncated: { preview: 'e', total_bytes: 9 } })).toBe('e')
    expect(toolOutputText(null)).toBe('')
  })

  it('turns an execute_sql reply into a result card, and nothing else', () => {
    const text = JSON.stringify({ columns: ['id', 'name'], rows: [[1, 'a'], [2, 'b']], row_count: 2, truncated: false })
    expect(parseSqlResult(text)).toEqual({ columns: [{ name: 'id' }, { name: 'name' }], rows: [[1, 'a'], [2, 'b']], total: 2 })
    expect(parseSqlResult('Read-only mode is enabled.')).toBeNull()
    expect(parseSqlResult('{"ok":true}')).toBeNull()
  })

  it('tells agent profiles from HTTP ones', () => {
    expect(profileAgent({ provider: 'agent:codex' })).toBe('codex')
    expect(profileAgent({ provider: 'openrouter' })).toBeNull()
    expect(profileAgent(null)).toBeNull()
  })

  it('names the engine and the read-only rule in the prompt', () => {
    const p = agentSystemPrompt({ dbType: 'postgres', activeSchema: 'public', tables: ['users'] })
    expect(p).toContain('postgres')
    expect(p).toContain('Tables: users.')
    expect(p).toContain('read only')
  })
})

const { stepLabel, cleanToolError } = await import('./local-agents.js')

describe('folded steps', () => {
  it('names each Stroke tool in plain words', () => {
    expect(stepLabel({ tool: 'describe_table', op: 'describe', detail: 'tenants' })).toEqual({ label: 'Described', detail: 'tenants' })
    expect(stepLabel({ tool: 'get_database_stats', op: 'schema', detail: 'get database stats' })).toEqual({ label: 'Read database stats', detail: '' })
    expect(stepLabel({ tool: 'some_new_tool', op: 'schema', detail: '' }).label).toBe('Some new tool')
  })

  it('keeps only the database message from a wrapped tool error', () => {
    const raw = 'tool call error: tool call failed for `stroke/get_database_stats` Caused by: Mcp error: -32000: Stats query failed: error returned from database: column s.heap_blks_hit does not exist'
    expect(cleanToolError(raw)).toBe('Stats query failed: error returned from database: column s.heap_blks_hit does not exist')
    expect(cleanToolError('relation "x" does not exist')).toBe('relation "x" does not exist')
  })
})
