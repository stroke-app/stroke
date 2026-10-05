import { describe, it, expect } from 'vitest'
import { shortcutKeys } from './kbd-text.js'

describe('shortcutKeys', () => {
  it('splits a chord into its keys', () => {
    expect(shortcutKeys('Ctrl+Shift+P')).toEqual(['Ctrl', 'Shift', 'P'])
    expect(shortcutKeys('Cmd + Enter')).toEqual(['Cmd', 'Enter'])
    expect(shortcutKeys('Mod+/')).toEqual(['Mod', '/'])
    expect(shortcutKeys('Alt+F4')).toEqual(['Alt', 'F4'])
  })

  it('keeps a key that is also the separator', () => {
    expect(shortcutKeys('Cmd++')).toEqual(['Cmd', '+'])
    expect(shortcutKeys('Ctrl+-')).toEqual(['Ctrl', '-'])
  })

  it('keeps the macOS glyph form as one cap', () => {
    expect(shortcutKeys('⌘⇧P')).toEqual(['⌘⇧P'])
    expect(shortcutKeys('⌘K')).toEqual(['⌘K'])
    expect(shortcutKeys('⌘↵')).toEqual(['⌘↵'])
  })

  it('converts a lone key no SQL word shares', () => {
    expect(shortcutKeys('Esc')).toEqual(['Esc'])
    expect(shortcutKeys('Enter')).toEqual(['Enter'])
    expect(shortcutKeys('F5')).toEqual(['F5'])
  })

  it('leaves identifiers, SQL and expressions as code', () => {
    for (const text of ['EMPLOYEE', 'works_in', 'DELETE', 'END', 'Home', 'a+b', 'price + tax', 'x + 1', 'Shift', 'Cmd+', '+K', 'SELECT 1', 'Ctrl+Shift+Enter now']) {
      expect(shortcutKeys(text), text).toBeNull()
    }
  })
})
