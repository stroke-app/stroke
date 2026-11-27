import { describe, expect, it } from 'vitest'
import { matchesKeyboardEvent } from '@tanstack/hotkeys'
import { COMMAND_BY_ID, comboTitle, keycaps } from './shortcuts.js'

/** A real "?" keypress: the key arrives with Shift held. */
const question = (mods = {}) => /** @type {KeyboardEvent} */ (/** @type {unknown} */ ({ key: '?', code: 'Slash', shiftKey: true, ctrlKey: false, altKey: false, metaKey: false, ...mods }))

describe('shortcuts on a shifted symbol', () => {
  it('fire on the keypress that types them', () => {
    expect(matchesKeyboardEvent(question(), COMMAND_BY_ID.get('help.shortcutsKey')?.combo ?? '')).toBe(true)
    expect(matchesKeyboardEvent(question({ ctrlKey: true }), COMMAND_BY_ID.get('help.shortcutsKey')?.combo ?? '')).toBe(false)
  })
  it('read without the Shift', () => {
    expect(keycaps('Shift+?')).toEqual(['?'])
    expect(comboTitle('Shift+?')).toBe('?')
    expect(keycaps('Shift+K')).toEqual(['⇧', 'K'])
  })
})
