import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => '') }))

/** Minimal in-memory localStorage for the node test environment. */
function installLocalStorage(seed = {}) {
  const map = new Map(Object.entries(seed))
  globalThis.localStorage = /** @type {any} */ ({
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k),
  })
  return map
}

const free = (id) => ({ id, name: 'stroke-free-fast', provider: 'stroke', baseUrl: 'https://stroke.click/api/ai', model: 'stroke-free-fast' })

describe('AI profile duplicates', () => {
  beforeEach(() => vi.resetModules())

  it('drops identical profiles on load and keeps the active one', async () => {
    const other = { id: 'b', name: 'best-coding', provider: 'omniroute', baseUrl: 'http://localhost:20128/v1', model: 'auto/best-coding' }
    const renamed = { ...other, id: 'c', name: 'newai' }
    const store = installLocalStorage({
      'stroke:ai-profiles': JSON.stringify([free('1'), other, renamed, free('2'), free('3')]),
      'stroke:ai-active-profile': '3',
    })
    const { aiProfiles } = await import('./ai-settings.js')
    let list = []
    aiProfiles.subscribe((v) => (list = v))()
    expect(list.map((p) => p.id)).toEqual(['b', 'c', '3'])
    expect(JSON.parse(store.get('stroke:ai-profiles')).length).toBe(3)
  })

  it('saving a twin of an existing profile updates it instead of adding one', async () => {
    installLocalStorage({ 'stroke:ai-profiles': JSON.stringify([free('1')]) })
    const { aiProfiles, saveProfile } = await import('./ai-settings.js')
    const id = await saveProfile(free('new-uuid'))
    let list = []
    aiProfiles.subscribe((v) => (list = v))()
    expect(id).toBe('1')
    expect(list.length).toBe(1)
  })
})
