import { describe, it, expect } from 'vitest'
import { extractTextToolCalls } from './ai.js'

// The free gateway's fast model, asked for five charts, wrote its calls as text.
const REPLY = `Here are five different chart diagrams:

**1. Bar Chart**
{"name": "render_chart", "parameters": {"type": "bar", "title": "Monthly Revenue", "data": "[{\\"month\\":\\"Jan\\",\\"revenue\\":1200}]", "x_col": "month", "y_col": "revenue"}}

**2. Pie Chart**
\`\`\`json
{"name": "render_chart", "parameters": {"type": "pie", "title": "Product Mix", "data": "[]", "x_col": "product", "y_col": "quantity"}}
\`\`\`

Let me know if you want more!`

describe('extractTextToolCalls', () => {
  it('turns tool calls written as JSON into real calls and takes them out of the text', () => {
    const { text, toolCalls } = extractTextToolCalls(REPLY, ['execute_sql', 'render_chart'])
    expect(toolCalls.map((c) => c.function.name)).toEqual(['render_chart', 'render_chart'])
    expect(JSON.parse(toolCalls[0].function.arguments)).toMatchObject({ type: 'bar', x_col: 'month', y_col: 'revenue' })
    expect(text).not.toContain('"name"')
    expect(text).not.toContain('```')
    expect(text).toContain('**1. Bar Chart**')
  })

  it('reads the OpenAI shape and leaves JSON that names no offered tool alone', () => {
    const openai = '{"type": "function", "function": {"name": "execute_sql", "arguments": "{\\"sql\\": \\"SELECT 1\\"}"}}'
    expect(extractTextToolCalls(openai, ['execute_sql']).toolCalls[0].function).toEqual({ name: 'execute_sql', arguments: '{"sql": "SELECT 1"}' })
    const data = 'The row is {"name": "Alice", "parameters": {"age": 3}} as stored.'
    expect(extractTextToolCalls(data, ['execute_sql'])).toEqual({ text: data, toolCalls: [] })
  })

  it('ignores quotes in the prose around the JSON', () => {
    const t = 'The "USERS" table: {"name": "describe_table", "parameters": {"table": "users"}}'
    const { text, toolCalls } = extractTextToolCalls(t, ['describe_table'])
    expect(toolCalls).toHaveLength(1)
    expect(text).toBe('The "USERS" table:')
  })
})
