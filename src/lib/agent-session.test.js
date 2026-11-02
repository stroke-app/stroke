import { describe, it, expect, vi, beforeEach } from 'vitest'

/** What the fake backend saw, and the event callback of the live session. */
const starts = /** @type {any[]} */ ([])
const sends = /** @type {string[]} */ ([])
/** @type {((ev: any) => void) | null} */
let emit = null
/** Events the fake agent answers each message with. @type {any[][]} */
let script = []
let exitBeforeNextSend = false

vi.mock('$lib/api.js', () => ({
  agentStart: vi.fn(async (opts, onEvent) => { starts.push(opts); emit = onEvent }),
  agentSend: vi.fn(async (_key, text) => {
    if (exitBeforeNextSend) { exitBeforeNextSend = false; throw 'NO_SESSION' }
    sends.push(text)
    const events = script.shift() ?? []
    queueMicrotask(() => { for (const ev of events) emit?.(ev) })
  }),
  agentInterrupt: vi.fn(async () => {}),
  agentStop: vi.fn(async () => {}),
  agentList: vi.fn(async () => []),
}))
vi.mock('$lib/stores/read-only.js', () => ({ isReadOnly: () => false }))

const { createAgentChat } = await import('./agent-session.js')

const ctx = { dbType: 'postgres', activeSchema: 'public', tables: ['users'] }

describe('agent chat', () => {
  beforeEach(() => { starts.length = 0; sends.length = 0; script = []; exitBeforeNextSend = false })

  it('turns agent events into callbacks', async () => {
    script = [[
      { Started: { native_session_id: 'ses-1' } },
      { Text: { delta: 'Looking. ' } },
      { ToolCall: { id: 't1', name: 'mcp__stroke__execute_sql', input: { sql: 'select 1' } } },
      { ToolCall: { id: 't1', name: 'mcp__stroke__execute_sql', input: { sql: 'select 1' } } },
      { ToolResult: { id: 't1', output: { Text: '{"columns":["n"],"rows":[[1]],"row_count":1}' }, is_error: false } },
      { Text: { delta: 'Done.' } },
      { TurnEnd: { stop: 'EndTurn' } },
    ]]
    const seen = /** @type {string[]} */ ([])
    const chat = createAgentChat()
    await chat.runTurn({
      agent: 'claude', model: 'sonnet', text: 'hi', ctx,
      on: {
        text: (d) => seen.push(`text:${d}`),
        toolCall: (c) => seen.push(`call:${c.tool}`),
        toolResult: (r) => seen.push(`result:${r.tool}:${r.isError}`),
      },
    })
    // The repeated call (complete input arriving later) is reported once.
    expect(seen).toEqual(['text:Looking. ', 'call:execute_sql', 'result:execute_sql:false', 'text:Done.'])
    expect(starts[0].systemPrompt).toContain('postgres')
  })

  it('keeps one session across turns and resumes it when the model changes', async () => {
    const end = [{ TurnEnd: { stop: 'EndTurn' } }]
    script = [[{ Started: { native_session_id: 'ses-9' } }, ...end], end, end]
    const chat = createAgentChat()
    const turn = (model) => chat.runTurn({ agent: 'claude', model, text: 'q', ctx, on: {} })
    await turn('sonnet')
    await turn('sonnet')
    expect(starts.length).toBe(1)
    await turn('opus')
    expect(starts.length).toBe(2)
    expect(starts[1].resume).toBe('ses-9')
  })

  it('starts the agent again and resends when it exited between turns', async () => {
    script = [[{ TurnEnd: { stop: 'EndTurn' } }], [{ TurnEnd: { stop: 'EndTurn' } }]]
    const chat = createAgentChat()
    await chat.runTurn({ agent: 'claude', model: 'sonnet', text: 'one', ctx, on: {} })
    exitBeforeNextSend = true
    await chat.runTurn({ agent: 'claude', model: 'sonnet', text: 'two', ctx, on: {} })
    expect(sends).toEqual(['one', 'two'])
    expect(starts.length).toBe(2)
  })

  it('a failed turn reports its message and still ends', async () => {
    script = [[{ TurnFailed: { message: 'rate limited' } }, { TurnEnd: { stop: 'Error' } }]]
    const failures = /** @type {string[]} */ ([])
    await createAgentChat().runTurn({ agent: 'codex', model: 'x', text: 'q', ctx, on: { failed: (m) => failures.push(m) } })
    expect(failures).toEqual(['rate limited'])
  })
})
