import { describe, it, expect, vi, afterEach } from 'vitest'

/** shortcuts.js reads the platform once, at import: load it fresh per platform. */
async function load(os) {
  vi.resetModules()
  vi.doMock('./platform.js', () => ({ detectOs: () => os }))
  return import('./shortcuts.js')
}

afterEach(() => { vi.doUnmock('./platform.js') })

describe('comboTitle', () => {
  it('prints macOS glyphs together', async () => {
    const { comboTitle } = await load('macos')
    expect(comboTitle('Mod+Shift+P')).toBe('⌘⇧P')
    expect(comboTitle('Mod+Enter')).toBe('⌘↵')
    expect(comboTitle('Mod+Alt+Left')).toBe('⌘⌥←')
    expect(comboTitle('Alt+Backspace')).toBe('⌥⌫')
  })

  it('spells the keys out elsewhere', async () => {
    const { comboTitle } = await load('windows')
    expect(comboTitle('Mod+Shift+P')).toBe('Ctrl+Shift+P')
    expect(comboTitle('Mod+Enter')).toBe('Ctrl+Enter')
    expect(comboTitle('Mod+Alt+Left')).toBe('Ctrl+Alt+←')
    expect(comboTitle('Mod+,')).toBe('Ctrl+,')
  })
})

describe('the shortcuts list', () => {
  it('names each chord once per group', async () => {
    const { SHORTCUT_GROUPS } = await load('macos')
    for (const g of SHORTCUT_GROUPS) {
      const seen = new Set()
      for (const s of g.shortcuts) {
        expect(seen.has(s.combo), `${g.label}: ${s.combo}`).toBe(false)
        seen.add(s.combo)
      }
    }
  })

  it('lists the tab bar under Alt+Shift+T everywhere', async () => {
    const { SHORTCUT_GROUPS } = await load('macos')
    const tabBar = SHORTCUT_GROUPS.flatMap((g) => g.shortcuts).filter((s) => /tab bar/i.test(s.desc))
    expect(tabBar.length).toBeGreaterThan(0)
    for (const s of tabBar) expect(s.combo).toBe('Alt+Shift+T')
  })
})
