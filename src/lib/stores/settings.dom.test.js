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

describe('AI chat in a tab is on by default', () => {
  beforeEach(() => vi.resetModules())

  it('is on for a new install', async () => {
    installLocalStorage()
    const { loadSettings } = await import('./settings.js')
    expect(loadSettings().aiInTab).toBe(true)
  })

  it('turns on once for settings saved before 3.0, then keeps a later choice', async () => {
    const store = installLocalStorage({ 'stroke:settings': JSON.stringify({ aiInTab: false, theme: 'dark' }) })
    let { loadSettings } = await import('./settings.js')
    expect(loadSettings().aiInTab).toBe(true)
    expect(JSON.parse(store.get('stroke:settings')).aiInTab).toBe(true)

    store.set('stroke:settings', JSON.stringify({ ...JSON.parse(store.get('stroke:settings')), aiInTab: false }))
    vi.resetModules()
    ;({ loadSettings } = await import('./settings.js'))
    expect(loadSettings().aiInTab).toBe(false)
  })
})
