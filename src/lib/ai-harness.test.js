import { describe, expect, it, test } from 'vitest'
import { buildSystemPrompt, compactToolHistory, detectPromptTopics, titleFromMessage, toolsForTurn } from './ai.js'

const ctx = {
  dbType: 'postgres', schemas: ['public'], activeSchema: 'public',
  tables: Array.from({ length: 135 }, (_, i) => ({ name: `table_${i}`, rowCount: 1000 + i })),
  activeTable: 'table_1', columns: [{ name: 'id', dataType: 'uuid', nullable: false }], primaryKey: ['id'], foreignKeys: [],
  allTableColumns: { 'public.table_1': [{ name: 'id', dataType: 'uuid', nullable: false }] }, sampleRows: {}, userSkills: [],
}
const tok = (/** @type {string} */ s) => Math.round(s.length / 4)

test('topics come from keywords and stick for the conversation', () => {
  expect([...detectPromptTopics('how many users signed up last week')]).toEqual([])
  expect(detectPromptTopics('plot signups per month').has('charts')).toBe(true)
  expect(detectPromptTopics('draw the ERD').has('diagrams')).toBe(true)
  expect(detectPromptTopics('make the bars red', ['charts']).has('charts')).toBe(true)
})

test('a plain turn advertises the core tools only', () => {
  expect(toolsForTurn(new Set()).map((t) => t.function.name)).toEqual(['execute_sql', 'describe_table', 'list_tables', 'get_schema'])
  const names = toolsForTurn(new Set(['charts', 'export']), true).map((t) => t.function.name)
  expect(names).toContain('render_chart')
  expect(names).toContain('export_data')
  expect(names).toContain('web_search')
  expect(names).not.toContain('render_diagram')
})

test('the lean prompt leaves reference material out until a topic asks for it', () => {
  const lean = buildSystemPrompt({ ...ctx, topics: new Set() })
  expect(lean).not.toContain('Mermaid')
  expect(lean).not.toContain('Chart Types')
  expect(lean).not.toContain('Quick Reference')
  expect(lean).not.toContain('render_chart')
  expect(lean).toContain('table_134')
  expect(tok(lean)).toBeLessThan(2000)
  const charts = buildSystemPrompt({ ...ctx, topics: new Set(['charts']) })
  expect(charts).toContain('render_chart')
  expect(charts).toContain('Chart Types')
  // No topics named: everything, as the sidebar and the palette expect.
  const full = buildSystemPrompt(ctx)
  expect(full).toContain('Mermaid')
  expect(full).toContain('Quick Reference')
  process.stdout.write(`\nlean prompt ~${tok(lean)} tok, with charts ~${tok(charts)} tok, full ~${tok(full)} tok, core tools ~${tok(JSON.stringify(toolsForTurn(new Set())))} tok\n`)
})

test('old tool results are elided, the recent turns stay whole', () => {
  const big = JSON.stringify({ columns: ['a'], rows: Array.from({ length: 200 }, (_, i) => ({ a: i })), total_rows: 200 })
  const turn = (/** @type {string} */ id) => [
    { role: 'user', content: `q${id}` },
    { role: 'assistant', content: '', tool_calls: [{ id }] },
    { role: 'tool', tool_call_id: id, content: big },
  ]
  const out = compactToolHistory([...turn('1'), ...turn('2'), ...turn('3')], 2)
  expect(out[2].content).toContain('elided')
  expect(out[2].content).toContain('"total_rows":200')
  expect(out[2].tool_call_id).toBe('1')
  expect(out[5].content).toBe(big)
  expect(out[8].content).toBe(big)
})

test('a title comes from the first message', () => {
  expect(titleFromMessage('  show me the top 10 customers by revenue this quarter please ')).toBe('Show me the top 10 customers by…')
  expect(titleFromMessage('hi')).toBe('Hi')
  expect(titleFromMessage('')).toBe('')
})

describe('the free tier carries less', () => {
  it('budgets history by endpoint', async () => {
    const { historyBudget } = await import('./ai.js')
    expect(historyBudget({ baseUrl: 'https://stroke.click/api/ai' })).toMatchObject({ maxChars: 24_000, keepLastN: 6 })
    expect(historyBudget({ baseUrl: 'https://api.openai.com/v1' })).toMatchObject({ maxChars: 60_000, keepLastN: 10 })
  })

  it('slides old turns out on the free tier instead of summarising them', async () => {
    const { manageHistory, historyBudget } = await import('./ai.js')
    const settings = { baseUrl: 'https://stroke.click/api/ai', model: 'stroke-free', apiKey: '' }
    const long = 'x'.repeat(3000)
    /** @type {any[]} */
    const history = []
    for (let i = 0; i < 20; i++) history.push({ role: 'user', content: `q${i} ${long}` }, { role: 'assistant', content: `a${i} ${long}` })
    const { history: kept, summarized } = await manageHistory(/** @type {any} */ (settings), history, historyBudget(settings))
    expect(summarized).toBe(false)
    expect(kept.filter((m) => m.role === 'user')).toHaveLength(6)
    expect(kept.some((m) => m.role === 'system')).toBe(false)
  })
})

describe('identity and small talk', () => {
  it('names the model it runs on and keeps greetings short', () => {
    const prompt = buildSystemPrompt({ ...ctx, modelLabel: 'Claude Haiku 4.5' })
    expect(prompt).toContain('You run on Claude Haiku 4.5.')
    expect(prompt).toMatch(/Asked which model or AI you are: one sentence - Stroke's assistant running on Claude Haiku 4.5/)
    expect(prompt).toMatch(/Do not introduce yourself, list tables or restate any of this/)
  })
})
