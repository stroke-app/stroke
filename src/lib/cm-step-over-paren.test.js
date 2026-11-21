import { describe, it, expect } from 'vitest'
import { stepsOverParen } from './cm-step-over-paren.js'

describe('stepsOverParen', () => {
  it('steps over the closing paren of a completed call', () => {
    const line = 'select st_astext(geom) from pings'
    expect(stepsOverParen(line, line.indexOf(')'))).toBe(true)
  })
  it('inserts when a nested call still needs its own', () => {
    const line = 'select f(g(x)'
    expect(stepsOverParen(line, line.length - 1)).toBe(false)
  })
  it('ignores parens in strings and needs a ) at the caret', () => {
    expect(stepsOverParen("select f(')') ", 12)).toBe(true)
    expect(stepsOverParen("select f('(')", 12)).toBe(true)
    expect(stepsOverParen('select f(x', 10)).toBe(false)
  })
})
