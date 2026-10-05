import { describe, it, expect } from 'vitest'
import { repairToolPairs } from './ai.js'

const call = (id) => ({ id, type: 'function', function: { name: 'execute_sql', arguments: '{}' } })
const asks = (...ids) => ({ role: 'assistant', content: null, tool_calls: ids.map(call) })
const answer = (id) => ({ role: 'tool', tool_call_id: id, content: '{"ok":true}' })

describe('repairToolPairs', () => {
  it('leaves a well-formed history as it is', () => {
    const h = [{ role: 'user', content: 'hi' }, asks('a', 'b'), answer('a'), answer('b'), { role: 'assistant', content: 'done' }]
    expect(repairToolPairs(h)).toEqual(h)
  })

  it('answers calls that Stop cut off', () => {
    const h = [{ role: 'user', content: 'go' }, asks('a', 'b'), answer('a'), { role: 'user', content: 'next' }]
    const out = repairToolPairs(h)
    expect(out.map((m) => m.role)).toEqual(['user', 'assistant', 'tool', 'tool', 'user'])
    expect(out[3]).toMatchObject({ tool_call_id: 'b' })
    expect(JSON.parse(out[3].content).cancelled).toBe(true)
  })

  it('drops an answer that landed after the next question', () => {
    const h = [asks('a'), { role: 'user', content: 'next' }, answer('a')]
    const out = repairToolPairs(h)
    expect(out.map((m) => m.role)).toEqual(['assistant', 'tool', 'user'])
  })

  it('drops answers to calls that were never made', () => {
    expect(repairToolPairs([{ role: 'user', content: 'x' }, answer('zz')])).toEqual([{ role: 'user', content: 'x' }])
  })
})
