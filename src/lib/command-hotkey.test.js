import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@tanstack/svelte-hotkeys', () => ({
  createHotkey: vi.fn(),
  // Enough of the real matcher for these tests: modifiers and the key.
  matchesKeyboardEvent: (/** @type {KeyboardEvent} */ e, /** @type {string} */ combo) => {
    const parts = combo.split('+')
    const key = parts.pop()?.toLowerCase()
    const want = (/** @type {string} */ m) => parts.includes(m)
    return e.key.toLowerCase() === key
      && e.ctrlKey === (want('Mod') || want('Control'))
      && e.altKey === want('Alt')
      && e.shiftKey === want('Shift')
  },
}))

const { firesInInputs, matchesCommand } = await import('./command-hotkey.js')
const { setBinding, resetAllBindings } = await import('./stores/keybindings.js')

/** @param {string} key @param {{ ctrl?: boolean, alt?: boolean, shift?: boolean }} [m] */
const keydown = (key, m = {}) => /** @type {KeyboardEvent} */ ({ key, ctrlKey: !!m.ctrl, metaKey: false, altKey: !!m.alt, shiftKey: !!m.shift })

describe('firesInInputs', () => {
  it('lets chords with Mod, Ctrl, Meta or Alt through in text fields', () => {
    for (const c of ['Mod+K', 'Control+K', 'Meta+K', 'Alt+K', 'Mod+Shift+P', 'Alt+Shift+K']) {
      expect(firesInInputs(c), c).toBe(true)
    }
  })
  it('leaves bare keys and Shift+key to typing', () => {
    for (const c of ['K', 'Shift+K', 'F5', '']) expect(firesInInputs(c), c).toBe(false)
  })
})

describe('matchesCommand follows a remap', () => {
  beforeEach(() => resetAllBindings())

  it('the palette moves from Ctrl+K to Alt+K', () => {
    expect(matchesCommand(keydown('k', { ctrl: true }), 'palette.open')).toBe(true)
    setBinding('palette.open', 'Alt+K')
    expect(matchesCommand(keydown('k', { alt: true }), 'palette.open')).toBe(true)
    expect(matchesCommand(keydown('k', { ctrl: true }), 'palette.open')).toBe(false)
  })
})
