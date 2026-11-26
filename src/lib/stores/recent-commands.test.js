import { beforeEach, describe, expect, it } from 'vitest'
import { loadRecentCommands, rememberCommand } from './recent-commands.js'

describe('recent commands', () => {
  beforeEach(() => {
    const map = new Map()
    globalThis.localStorage = /** @type {any} */ ({
      getItem: (k) => (map.has(k) ? map.get(k) : null),
      setItem: (k, v) => map.set(k, String(v)),
      removeItem: (k) => map.delete(k),
    })
  })
  it('keeps the newest first, without repeats, capped at three', () => {
    rememberCommand('SQL editor')
    rememberCommand('Charts')
    rememberCommand('SQL editor')
    rememberCommand('Map')
    rememberCommand('Backup & restore')
    expect(loadRecentCommands()).toEqual(['Backup & restore', 'Map', 'SQL editor'])
  })
  it('survives a corrupt entry', () => {
    localStorage.setItem('stroke:recent-commands', '{nope')
    expect(loadRecentCommands()).toEqual([])
  })
})
