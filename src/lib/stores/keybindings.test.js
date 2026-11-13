import { describe, it, expect, beforeEach } from 'vitest'
import { get } from 'svelte/store'
import { COMMANDS, COMMAND_BY_ID } from '$lib/shortcuts.js'
import {
  keybindings,
  bindingFor,
  setBinding,
  unbind,
  resetBinding,
  resetAllBindings,
  assignReplacing,
  sanitizeOverrides,
  effectiveCombo,
  findConflict,
  canonicalCombo,
  recordKeydown,
  effectiveShortcutGroups,
  toAccelerator,
  nativeMenuAccelerators,
} from './keybindings.js'

beforeEach(() => resetAllBindings())

describe('registry', () => {
  it('takes its defaults from shortcuts.js', () => {
    expect(COMMAND_BY_ID.get('palette.open')?.combo).toBe('Mod+K')
    expect(bindingFor('palette.open')).toBe('Mod+K')
    expect(bindingFor('tab.close')).toBe('Mod+W')
  })

  it('expands numbered families into one command each', () => {
    expect(bindingFor('tab.goto.1')).toBe('Mod+1')
    expect(bindingFor('tab.goto.9')).toBe('Mod+9')
    expect(COMMAND_BY_ID.get('tab.goto.9')?.label).toBe('Go to last tab')
    expect(bindingFor('table.view.5')).toBe('Alt+5')
    expect(COMMAND_BY_ID.has('tab.goto.10')).toBe(false)
  })

  it('lists a command repeated in two groups once', () => {
    expect(COMMANDS.filter((c) => c.id === 'view.sidebar')).toHaveLength(1)
  })

  it('has no two remappable defaults on the same chord, on either platform', () => {
    for (const isMac of [true, false]) {
      const seen = new Map()
      for (const c of COMMANDS.filter((c) => c.combo)) {
        const key = canonicalCombo(c.combo, isMac)
        expect(seen.get(key), `${c.id} vs ${seen.get(key)}`).toBeUndefined()
        seen.set(key, c.id)
      }
    }
  })
})

describe('overrides store', () => {
  it('overrides, unbinds and resets one command', () => {
    setBinding('tab.close', 'Mod+Shift+Q')
    expect(bindingFor('tab.close')).toBe('Mod+Shift+Q')
    expect(get(keybindings)).toEqual({ 'tab.close': 'Mod+Shift+Q' })

    unbind('tab.close')
    expect(bindingFor('tab.close')).toBe('')

    resetBinding('tab.close')
    expect(bindingFor('tab.close')).toBe('Mod+W')
    expect(get(keybindings)).toEqual({})
  })

  it('drops the override when set back to the default', () => {
    setBinding('tab.close', 'Mod+J')
    setBinding('tab.close', 'Mod+W')
    expect(get(keybindings)).toEqual({})
  })

  it('ignores unknown commands', () => {
    setBinding('no.such.command', 'Mod+J')
    expect(get(keybindings)).toEqual({})
  })

  it('resets everything', () => {
    setBinding('tab.close', 'Mod+J')
    unbind('palette.open')
    resetAllBindings()
    expect(get(keybindings)).toEqual({})
    expect(bindingFor('palette.open')).toBe('Mod+K')
  })

  it('sanitizes stored data: unknown ids, non-strings, defaults, junk', () => {
    expect(sanitizeOverrides(null)).toEqual({})
    expect(sanitizeOverrides('x')).toEqual({})
    expect(sanitizeOverrides(['Mod+J'])).toEqual({})
    expect(sanitizeOverrides({
      'tab.close': 'Mod+J',
      'tab.new': '',
      'palette.open': 'Mod+K',
      'gone.command': 'Mod+L',
      'tab.closeAll': 5,
    })).toEqual({ 'tab.close': 'Mod+J', 'tab.new': '' })
  })

  it('reads override ?? default, with "" meaning unbound', () => {
    expect(effectiveCombo({}, 'tab.close')).toBe('Mod+W')
    expect(effectiveCombo({ 'tab.close': '' }, 'tab.close')).toBe('')
    expect(effectiveCombo({}, 'missing')).toBe('')
  })
})

describe('conflicts', () => {
  it('names the command already using the combo', () => {
    const c = findConflict({}, 'Mod+W', 'tab.new')
    expect(c).toMatchObject({ id: 'tab.close', label: 'Close tab', fixed: false })
  })

  it('does not report a command against itself', () => {
    expect(findConflict({}, 'Mod+W', 'tab.close')).toBeNull()
  })

  it('compares chords, not spellings', () => {
    // Off macOS Mod is Ctrl, so a literal Ctrl+W is the same key.
    expect(findConflict({}, 'Ctrl+W', null, false)?.id).toBe('tab.close')
    expect(findConflict({}, 'Control+w', null, false)?.id).toBe('tab.close')
    // On macOS it is ⌘, and Ctrl+W is a different key.
    expect(findConflict({}, 'Ctrl+W', null, true)).toBeNull()
    expect(canonicalCombo('Shift+Mod+Up', true)).toBe(canonicalCombo('Mod+Shift+ArrowUp', true))
  })

  it('follows overrides: a remapped command frees its old key', () => {
    const overrides = { 'tab.close': 'Mod+J' }
    expect(findConflict(overrides, 'Mod+W', 'tab.new')).toBeNull()
    expect(findConflict(overrides, 'Mod+J', 'tab.new')?.id).toBe('tab.close')
    expect(findConflict({ 'tab.close': '' }, 'Mod+W', null)).toBeNull()
  })

  it('reports built-in keys as fixed', () => {
    const c = findConflict({}, 'Mod+C', 'tab.new', false)
    expect(c).toMatchObject({ id: null, fixed: true, label: 'Copy cell value' })
  })

  it('finds nothing for a free chord', () => {
    expect(findConflict({}, 'Mod+Alt+Shift+F9', null)).toBeNull()
    expect(findConflict({}, '', null)).toBeNull()
  })

  it('replacing unbinds the command that held the combo', () => {
    assignReplacing('tab.new', 'Mod+W', 'tab.close')
    expect(bindingFor('tab.new')).toBe('Mod+W')
    expect(bindingFor('tab.close')).toBe('')
  })
})

/** A keydown as the recorder sees it. */
const kd = (key, code, mods = {}) => ({ key, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods })

describe('recorder normalisation', () => {
  it('rejects a bare modifier, so a binding can never be just "Shift"', () => {
    expect(recordKeydown(kd('Shift', 'ShiftLeft', { shiftKey: true }), true)).toBeNull()
    expect(recordKeydown(kd('Meta', 'MetaLeft', { metaKey: true }), true)).toBeNull()
    expect(recordKeydown(kd('Control', 'ControlLeft', { ctrlKey: true }), false)).toBeNull()
    expect(recordKeydown(kd('Alt', 'AltLeft', { altKey: true }), false)).toBeNull()
    expect(recordKeydown(kd('CapsLock', 'CapsLock'), false)).toBeNull()
    expect(recordKeydown(kd('Dead', 'Quote', { altKey: true }), true)).toBeNull()
  })

  it('records the combo once a real key arrives with the held modifiers', () => {
    expect(recordKeydown(kd('T', 'KeyT', { metaKey: true, shiftKey: true }), true)).toEqual({ type: 'combo', combo: 'Mod+Shift+T' })
    expect(recordKeydown(kd('k', 'KeyK', { ctrlKey: true }), false)).toEqual({ type: 'combo', combo: 'Mod+K' })
    expect(recordKeydown(kd('F5', 'F5'), false)).toEqual({ type: 'combo', combo: 'F5' })
    expect(recordKeydown(kd('/', 'Slash'), false)).toEqual({ type: 'combo', combo: '/' })
  })

  it('keeps Ctrl distinct from ⌘ on macOS and Meta distinct from Ctrl elsewhere', () => {
    expect(recordKeydown(kd('`', 'Backquote', { ctrlKey: true }), true)).toEqual({ type: 'combo', combo: 'Ctrl+`' })
    expect(recordKeydown(kd('k', 'KeyK', { metaKey: true }), false)).toEqual({ type: 'combo', combo: 'Meta+K' })
  })

  it('reads the physical key, not the character a modifier produced', () => {
    expect(recordKeydown(kd('å', 'KeyA', { altKey: true }), true)).toEqual({ type: 'combo', combo: 'Alt+A' })
    expect(recordKeydown(kd('!', 'Digit1', { shiftKey: true }), false)).toEqual({ type: 'combo', combo: 'Shift+1' })
    expect(recordKeydown(kd('?', 'Slash', { shiftKey: true, metaKey: true }), true)).toEqual({ type: 'combo', combo: 'Mod+Shift+/' })
  })

  it('names keys the hotkey grammar can parse', () => {
    expect(recordKeydown(kd(' ', 'Space', { altKey: true }), false)).toEqual({ type: 'combo', combo: 'Alt+Space' })
    expect(recordKeydown(kd('ArrowUp', 'ArrowUp', { metaKey: true }), true)).toEqual({ type: 'combo', combo: 'Mod+ArrowUp' })
  })

  it('Escape alone cancels; Backspace or Delete alone unbinds', () => {
    expect(recordKeydown(kd('Escape', 'Escape'), true)).toEqual({ type: 'cancel' })
    expect(recordKeydown(kd('Backspace', 'Backspace'), true)).toEqual({ type: 'clear' })
    expect(recordKeydown(kd('Delete', 'Delete'), false)).toEqual({ type: 'clear' })
  })

  it('treats Escape and Backspace with a modifier as ordinary keys', () => {
    expect(recordKeydown(kd('Backspace', 'Backspace', { metaKey: true }), true)).toEqual({ type: 'combo', combo: 'Mod+Backspace' })
    expect(recordKeydown(kd('Escape', 'Escape', { ctrlKey: true }), false)).toEqual({ type: 'combo', combo: 'Mod+Escape' })
  })
})

describe('lists and menus', () => {
  it('shows remapped keys in the reference list and drops unbound rows', () => {
    const find = (groups, desc) => groups.flatMap((g) => g.shortcuts).filter((s) => s.desc === desc)
    let groups = effectiveShortcutGroups({ 'tab.close': 'Mod+J', 'tab.new': '' })
    expect(find(groups, 'Close tab').map((s) => s.combo)).toEqual(['Mod+J'])
    expect(find(groups, 'New tab')).toHaveLength(0)

    // A family stays one row until one of its members changes.
    groups = effectiveShortcutGroups({})
    expect(find(groups, 'Go to tab (9 = last)').map((s) => s.combo)).toEqual(['Mod+1-9'])
    groups = effectiveShortcutGroups({ 'tab.goto.2': 'Alt+2' })
    expect(find(groups, 'Go to tab 2').map((s) => s.combo)).toEqual(['Alt+2'])
    expect(find(groups, 'Go to tab 3').map((s) => s.combo)).toEqual(['Mod+3'])
  })

  it('turns combos into native menu accelerators', () => {
    expect(toAccelerator('Mod+Shift+T')).toBe('CmdOrCtrl+Shift+T')
    expect(toAccelerator('Ctrl+`')).toBe('Ctrl+`')
    expect(toAccelerator('F11')).toBe('F11')
    expect(toAccelerator('Mod+ArrowUp')).toBe('CmdOrCtrl+Up')
    expect(toAccelerator('')).toBeNull()
    expect(toAccelerator('Mod+?')).toBeNull()
    const accels = nativeMenuAccelerators({ 'tab.close': 'Mod+J', 'palette.open': '' })
    expect(accels.closeTab).toBe('CmdOrCtrl+J')
    expect(accels.commandPalette).toBeNull()
    expect(accels.openSettings).toBe('CmdOrCtrl+,')
  })
})
