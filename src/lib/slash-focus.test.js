import { describe, expect, it } from 'vitest'
import { isEditable } from './slash-focus.js'

// The DOM-walking half (`slashTarget`) is checked in a real browser; Node has
// no layout to tell visible fields from hidden ones.
describe('isEditable', () => {
  globalThis.HTMLElement ??= /** @type {any} */ (class {})
  const el = (/** @type {Record<string, any>} */ props) => Object.assign(Object.create(globalThis.HTMLElement.prototype), props)
  it('treats text fields and editors as typing targets', () => {
    expect(isEditable(el({ tagName: 'INPUT', type: 'text' }))).toBe(true)
    expect(isEditable(el({ tagName: 'INPUT', type: 'search' }))).toBe(true)
    expect(isEditable(el({ tagName: 'TEXTAREA' }))).toBe(true)
    expect(isEditable(el({ tagName: 'DIV', isContentEditable: true }))).toBe(true)
  })
  it('lets / through on buttons, checkboxes and the page', () => {
    expect(isEditable(el({ tagName: 'INPUT', type: 'checkbox' }))).toBe(false)
    expect(isEditable(el({ tagName: 'BUTTON' }))).toBe(false)
    expect(isEditable(null)).toBe(false)
  })
})
