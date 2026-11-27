import { describe, expect, it } from 'vitest'
import { parseCombo, splitTipShortcut } from './tooltip-shortcut.js'

describe('parseCombo', () => {
  it('reads Windows and Linux spellings', () => {
    expect(parseCombo('Ctrl+R')).toEqual(['Ctrl', 'R'])
    expect(parseCombo('Ctrl+Shift+P')).toEqual(['Ctrl', '⇧', 'P'])
    expect(parseCombo('Ctrl+Enter')).toEqual(['Ctrl', '↵'])
    expect(parseCombo('Ctrl+↑')).toEqual(['Ctrl', '↑'])
    expect(parseCombo('Ctrl↵')).toEqual(['Ctrl', '↵'])
    expect(parseCombo('Alt+1')).toEqual(['Alt', '1'])
  })
  it('reads macOS glyph runs', () => {
    expect(parseCombo('⌘R')).toEqual(['⌘', 'R'])
    expect(parseCombo('⌘⇧P')).toEqual(['⌘', '⇧', 'P'])
    expect(parseCombo('⌘↵')).toEqual(['⌘', '↵'])
  })
  it('reads a single key', () => {
    expect(parseCombo('Esc')).toEqual(['Esc'])
    expect(parseCombo('Enter')).toEqual(['↵'])
    expect(parseCombo('/')).toEqual(['/'])
    expect(parseCombo('F11')).toEqual(['F11'])
    expect(parseCombo('s')).toEqual(['S'])
  })
  it('rejects words and phrases', () => {
    expect(parseCombo('Ctrl+scroll')).toBeNull()
    expect(parseCombo('read only')).toBeNull()
    expect(parseCombo('beta')).toBeNull()
    expect(parseCombo('Ctrl')).toBeNull()
    expect(parseCombo('3 rows')).toBeNull()
    expect(parseCombo('')).toBeNull()
  })
})

describe('splitTipShortcut', () => {
  it('splits a trailing combo', () => {
    expect(splitTipShortcut('Refresh (Ctrl+R)')).toEqual({ label: 'Refresh', keys: ['Ctrl', 'R'] })
  })
  it('splits a combo in the middle and keeps the rest', () => {
    expect(splitTipShortcut('Switch connection (Ctrl+K) · double-click to manage')).toEqual({
      label: 'Switch connection · double-click to manage',
      keys: ['Ctrl', 'K'],
    })
  })
  it('leaves non-combo groups alone', () => {
    expect(splitTipShortcut('Zoom in (Ctrl+scroll)')).toEqual({ label: 'Zoom in (Ctrl+scroll)', keys: null })
    expect(splitTipShortcut('Filter connections (Ctrl+F, or Shift+Tab from the list)').keys).toBeNull()
  })
  it('keeps the combo that sits beside a word group', () => {
    expect(splitTipShortcut('Save (read only) (Ctrl+S)')).toEqual({ label: 'Save (read only)', keys: ['Ctrl', 'S'] })
  })
  it('keeps two combos as text', () => {
    const t = 'Run (Ctrl+Enter) · run and go to the next cell (Shift+Enter)'
    expect(splitTipShortcut(t)).toEqual({ label: t, keys: null })
  })
  it('keeps a tooltip that is only a combo', () => {
    expect(splitTipShortcut('(Ctrl+R)')).toEqual({ label: '(Ctrl+R)', keys: null })
  })
  it('handles plain text', () => {
    expect(splitTipShortcut('Settings')).toEqual({ label: 'Settings', keys: null })
  })
})
