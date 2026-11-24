import { describe, it, expect, beforeEach, vi } from 'vitest'
import {
  beginPanelResize,
  endPanelResize,
  isPanelResizing,
  PANEL_RESIZE_END_EVENT,
} from './panel-motion.js'
import {
  loadLayout,
  saveLayout,
  clampTerminalDockHeight,
  TERMINAL_DOCK_MIN,
  TERMINAL_DOCK_DEFAULT,
} from './stores/layout.js'

describe('panel resize coordination', () => {
  /** @type {Map<string, string>} */
  let attrs
  /** @type {Array<{ type: string }>} */
  let events

  beforeEach(() => {
    attrs = new Map()
    events = []
    globalThis.document = /** @type {any} */ ({
      documentElement: {
        setAttribute: (/** @type {string} */ k, /** @type {string} */ v) => void attrs.set(k, v),
        removeAttribute: (/** @type {string} */ k) => void attrs.delete(k),
        hasAttribute: (/** @type {string} */ k) => attrs.has(k),
      },
    })
    globalThis.window = /** @type {any} */ ({
      dispatchEvent: (/** @type {{ type: string }} */ ev) => {
        events.push(ev)
        return true
      },
    })
    globalThis.CustomEvent = /** @type {any} */ (
      class {
        /** @param {string} type */
        constructor(type) {
          this.type = type
        }
      }
    )
    // Reset any leftover counter if a previous test threw.
    for (let i = 0; i < 4; i++) endPanelResize()
    events = []
  })

  it('tracks data-resizing and fires resize-end once on release', () => {
    expect(isPanelResizing()).toBe(false)

    beginPanelResize('y')
    expect(isPanelResizing()).toBe(true)
    expect(attrs.get('data-resizing')).toBe('y')
    expect(events).toHaveLength(0)

    endPanelResize()
    expect(isPanelResizing()).toBe(false)
    expect(attrs.has('data-resizing')).toBe(false)
    expect(events.map((e) => e.type)).toEqual([PANEL_RESIZE_END_EVENT])
  })

  it('ref-counts nested drags so ending one does not release early', () => {
    beginPanelResize('x')
    beginPanelResize('y')
    endPanelResize()
    expect(isPanelResizing()).toBe(true)
    expect(events).toHaveLength(0)

    endPanelResize()
    expect(isPanelResizing()).toBe(false)
    expect(events).toHaveLength(1)

    // Extra endPanelResize calls clamp cleanly at zero without throwing.
    const spy = vi.fn()
    globalThis.window.dispatchEvent = spy
    endPanelResize()
    expect(isPanelResizing()).toBe(false)
  })
})

describe('terminal dock height layout persistence', () => {
  /** @type {Map<string, string>} */
  let store

  beforeEach(() => {
    store = new Map()
    globalThis.localStorage = /** @type {any} */ ({
      getItem: (/** @type {string} */ k) => store.get(k) ?? null,
      setItem: (/** @type {string} */ k, /** @type {string} */ v) => void store.set(k, v),
    })
  })

  it('persists terminalDockHeight across reloads and unrelated saveLayout calls', () => {
    expect(loadLayout().terminalDockHeight).toBe(TERMINAL_DOCK_DEFAULT)

    saveLayout({ terminalDockHeight: 410 })
    expect(loadLayout().terminalDockHeight).toBe(410)

    // Saving another layout property must keep terminalDockHeight intact.
    saveLayout({ navSidebarWidth: 340 })
    expect(loadLayout().terminalDockHeight).toBe(410)
  })

  it('clamps terminalDockHeight against floor and container bounds', () => {
    expect(clampTerminalDockHeight(40, 800)).toBe(TERMINAL_DOCK_MIN)
    expect(clampTerminalDockHeight(780, 800)).toBe(680)
    expect(clampTerminalDockHeight(320.4, 800)).toBe(320)
  })
})
