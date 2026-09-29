import { describe, it, expect, beforeEach, vi } from 'vitest'
import {
  readProviderList,
  writeProviderList,
  clearProviderLists,
  _resetProviderListCache,
} from './provider-list-cache.js'

beforeEach(() => {
  const store = new Map()
  vi.stubGlobal('localStorage', {
    getItem: (/** @type {string} */ k) => store.get(k) ?? null,
    setItem: (/** @type {string} */ k, /** @type {string} */ v) => store.set(k, String(v)),
  })
  _resetProviderListCache()
})

describe('provider list cache', () => {
  it('survives a reload from storage', () => {
    writeProviderList('provider:neon', [{ db_ref: 'p1', name: 'app' }])
    _resetProviderListCache()
    expect(readProviderList('provider:neon')).toEqual([{ db_ref: 'p1', name: 'app' }])
  })

  it('clears only the signed-out provider', () => {
    writeProviderList('cloudflare:accounts', { accounts: [{ id: 'a' }] })
    writeProviderList('cloudflare:dbs:a', [{ uuid: 'u' }])
    writeProviderList('provider:neon', [{ db_ref: 'p1' }])
    clearProviderLists('cloudflare:')
    expect(readProviderList('cloudflare:accounts')).toBeUndefined()
    expect(readProviderList('cloudflare:dbs:a')).toBeUndefined()
    expect(readProviderList('provider:neon')).toEqual([{ db_ref: 'p1' }])
  })

  it('treats corrupt storage as empty', () => {
    localStorage.setItem('stroke.providerListCache.v1', '{not json')
    expect(readProviderList('provider:neon')).toBeUndefined()
  })
})
