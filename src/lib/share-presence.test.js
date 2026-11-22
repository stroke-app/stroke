import { describe, expect, it } from 'vitest'
import { rowKey } from './share-presence.js'

describe('rowKey', () => {
  it('matches the web studio whether a driver gives 1 or "1"', () => {
    expect(rowKey([1])).toBe(rowKey(['1']))
    expect(rowKey([1])).toBe('["1"]')
  })
  it('keeps composite keys in order and NULL as null', () => {
    expect(rowKey([7, 'a'])).toBe('["7","a"]')
    expect(rowKey([null, undefined])).toBe('[null,null]')
  })
  it('encodes object values as JSON text', () => {
    expect(rowKey([{ a: 1 }])).toBe('["{\\"a\\":1}"]')
  })
})

describe('relay sealing', () => {
  it('opens what it sealed, and refuses another key', async () => {
    const { seal, unseal, rand } = await import('./relay-seal.js')
    const mk = () => crypto.subtle.importKey('raw', rand(32), 'AES-GCM', false, ['encrypt', 'decrypt'])
    const key = await mk()
    const sealed = await seal(key, { t: 'changed', table: 'main.people' })
    expect(await unseal(key, sealed)).toEqual({ t: 'changed', table: 'main.people' })
    await expect(unseal(await mk(), sealed)).rejects.toThrow()
  })
})
