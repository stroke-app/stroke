import { describe, it, expect } from 'vitest'
import { highlightsFor, latestHighlights } from './whats-new.js'

const SETS = [
  { after: '2.3.0', items: [{ id: 'b', icon: /** @type {const} */ ('tree'), title: 'B', body: '' }] },
  { after: '2.2.0', items: [{ id: 'a', icon: /** @type {const} */ ('tree'), title: 'A', body: '' }] },
]

describe('highlightsFor', () => {
  it('shows a set to anyone updating past the release it follows', () => {
    expect(highlightsFor('2.3.0', '2.3.1', SETS).map((h) => h.id)).toEqual(['b'])
    expect(highlightsFor('2.3.0', '2.4.0', SETS).map((h) => h.id)).toEqual(['b'])
  })
  it('collects every set skipped on the way, newest first', () => {
    expect(highlightsFor('2.1.5', '2.4.0', SETS).map((h) => h.id)).toEqual(['b', 'a'])
  })
  it('shows nothing to someone already past it, or without a previous version', () => {
    expect(highlightsFor('2.3.1', '2.3.2', SETS)).toEqual([])
    expect(highlightsFor('', '2.4.0', SETS)).toEqual([])
    expect(highlightsFor('2.3.0', '2.3.0', SETS)).toEqual([])
  })
})

describe('the 2.6 highlights', () => {
  it('show when updating from 2.6.0 or earlier, stable 2.5.5 included', () => {
    expect(highlightsFor('2.5.5', '2.7.3').map((h) => h.id)).toEqual(['ai-tab', 'docker', 'leader', 'connect'])
    expect(highlightsFor('2.6.0', '2.7.0')[0].id).toBe('ai-tab')
    expect(highlightsFor('2.7.0', '2.7.1')).toEqual([])
  })
  it('are what What\'s new shows on demand', () => {
    expect(latestHighlights('2.7.4').map((h) => h.id)).toEqual(['ai-tab', 'docker', 'leader', 'connect'])
    expect(latestHighlights('2.4.0').map((h) => h.id)[0]).toBe('tree-sidebar')
  })
})
