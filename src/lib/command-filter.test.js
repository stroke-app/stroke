import { describe, expect, it } from 'vitest'
import { commandFilter, looseLabelScore } from './command-filter.js'

const score = (label, search, value = label) => commandFilter(value, search, [label])

describe('commandFilter', () => {
  it('keeps the old tiers', () => {
    expect(score('Settings', 'settings')).toBe(1)
    expect(score('Settings', 'set')).toBe(0.95)
    expect(score('Open settings', 'set')).toBe(0.9)
    expect(score('New table', 'newtable')).toBe(0.3)
  })
  it('finds letters typed with some left out', () => {
    expect(score('Change status', 'stts')).toBeGreaterThan(0)
    expect(score('New table', 'nwtb')).toBeGreaterThan(0)
    expect(score('Toggle sidebar', 'tgsb')).toBeGreaterThan(0)
  })
  it('ranks loose matches under every exact tier', () => {
    expect(score('Change status', 'stts')).toBeLessThan(0.3)
  })
  it('needs the first letter at a word start', () => {
    expect(score('Schema public', 'map')).toBe(0)
    expect(score('Change status', 'tts')).toBe(0)
  })
  it('rejects a match spread across the label', () => {
    expect(score('Open settings', 'ogs')).toBe(0)
  })
  it('does not search hidden keywords loosely', () => {
    expect(commandFilter('open settings preferences configuration', 'pcfg', ['Settings'])).toBe(0)
  })
  it('ignores one-letter loose queries', () => {
    expect(looseLabelScore('change status', 'c')).toBe(0)
  })
  it('breaks a tie toward the shorter label', () => {
    expect(score('Settings', 'stng')).toBeGreaterThan(score('Import connections and settings', 'stng'))
  })
  it('prefers the tighter of two loose matches', () => {
    expect(looseLabelScore('stats', 'stts')).toBeGreaterThan(looseLabelScore('status', 'stts'))
  })
})
