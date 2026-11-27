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

describe('two-letter queries', () => {
  it('ignore letters in the middle of a hidden keyword', () => {
    expect(commandFilter('migrations prisma drizzle applied pending failed', 'ai', ['Migrations'])).toBe(0)
    expect(commandFilter('ai sidebar', 'ai', ['AI sidebar'])).toBeGreaterThan(0.9)
    expect(commandFilter('migrations prisma drizzle applied pending failed', 'ile', ['Migrations'])).toBe(0.5)
  })
})

describe('cachedCommandFilter', () => {
  it('scores like commandFilter and forgets on a new search', async () => {
    const { cachedCommandFilter } = await import('./command-filter.js')
    const f = cachedCommandFilter()
    expect(f('open settings', 'set', ['Settings'])).toBe(commandFilter('open settings', 'set', ['Settings']))
    expect(f('open settings', 'set', ['Settings'])).toBe(commandFilter('open settings', 'set', ['Settings']))
    expect(f('open settings', 'zzz', ['Settings'])).toBe(0)
    // Same value, different label: scored apart, not served from one cache entry.
    expect(f('x', 'alp', ['Alpha'])).toBeGreaterThan(0)
    expect(f('x', 'alp', ['Beta'])).toBe(0)
  })
})

describe('commandFilter keyword position', () => {
  it('ranks a row whose keywords start with the word above one that ends with it', () => {
    const history = commandFilter('queries query history saved queries past runs charts', 'queries', ['Query history'])
    const insights = commandFilter('open instance insights monitoring sessions processes activity locks replication config pg_settings slow queries', 'queries', ['Instance insights'])
    expect(history).toBeGreaterThan(insights)
    expect(insights).toBeGreaterThanOrEqual(0.7)
    // Still below any match in the visible label.
    expect(history).toBeLessThan(commandFilter('x', 'query', ['Query history']))
  })
})
