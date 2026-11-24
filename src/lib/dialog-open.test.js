import { describe, it, expect, beforeEach } from 'vitest'
import { beginDialogOpen, endDialogOpen } from './dialog-open.js'

describe('dialog-open marker', () => {
  /** @type {Set<string>} */
  let attrs
  beforeEach(() => {
    attrs = new Set()
    globalThis.document = /** @type {any} */ ({
      documentElement: {
        setAttribute: (/** @type {string} */ k) => void attrs.add(k),
        removeAttribute: (/** @type {string} */ k) => void attrs.delete(k),
      },
    })
    for (let i = 0; i < 4; i++) endDialogOpen()
  })

  it('stays set until the last nested dialog closes', () => {
    beginDialogOpen()
    beginDialogOpen()
    expect(attrs.has('data-dialog-open')).toBe(true)
    endDialogOpen()
    expect(attrs.has('data-dialog-open')).toBe(true)
    endDialogOpen()
    expect(attrs.has('data-dialog-open')).toBe(false)
    endDialogOpen()
    expect(attrs.has('data-dialog-open')).toBe(false)
  })
})
