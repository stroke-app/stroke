import { afterEach, describe, expect, it, vi } from 'vitest'
import { createSmoothScroll, isTrackpadWheel, wheelPixels } from './smooth-scroll.js'

/**
 * A scroll container that QUANTIZES its offset, the way a real element does.
 * This is the whole point of the fixture: an animation that measured its progress
 * by reading `scrollTop` back would stop making progress as soon as a frame's step
 * rounded away, and scrolling would stall part-way (or, for small deltas, never
 * start). Floor is the harshest realistic rounding.
 */
function fakeEl({ height = 500, content = 10_000 } = {}) {
  return {
    _top: 0,
    _left: 0,
    clientHeight: height,
    scrollHeight: content,
    clientWidth: height,
    scrollWidth: content,
    get scrollTop() { return this._top },
    set scrollTop(v) { this._top = Math.floor(v) },
    get scrollLeft() { return this._left },
    set scrollLeft(v) { this._left = Math.floor(v) },
  }
}

/** Run queued animation frames until the loop stops, or fail loudly if it can't. */
function runFrames(limit = 500) {
  let n = 0
  while (frames.length) {
    if (n++ > limit) throw new Error(`animation did not converge in ${limit} frames`)
    const batch = frames.splice(0, frames.length)
    for (const fn of batch) fn()
  }
  return n
}

/** @type {Array<() => void>} */
let frames = []
vi.stubGlobal('requestAnimationFrame', (/** @type {() => void} */ fn) => {
  frames.push(fn)
  return frames.length
})
vi.stubGlobal('cancelAnimationFrame', () => {})

afterEach(() => { frames = [] })

describe('createSmoothScroll', () => {
  it('reaches the exact target on a quantizing element', () => {
    const el = fakeEl()
    const s = createSmoothScroll(/** @type {any} */ (el))
    s.push(0, 300)
    runFrames()
    expect(el.scrollTop).toBe(300)
    expect(s.animating()).toBe(false)
  })

  it('converges for a delta small enough to round away per frame', () => {
    // 2px eased at 0.22 is 0.44px on the first frame - floored to 0 by the
    // element. The animation must still finish, and land on the target.
    const el = fakeEl()
    const s = createSmoothScroll(/** @type {any} */ (el))
    s.push(0, 2)
    runFrames()
    expect(el.scrollTop).toBe(2)
  })

  it('accumulates ticks that arrive mid-animation', () => {
    const el = fakeEl()
    const s = createSmoothScroll(/** @type {any} */ (el))
    s.push(0, 100)
    frames.splice(0, frames.length).forEach((f) => f()) // one frame only
    s.push(0, 100)
    runFrames()
    expect(el.scrollTop).toBe(200)
  })

  it('never scrolls past the end, or above the top', () => {
    const el = fakeEl({ height: 500, content: 1_000 })
    const s = createSmoothScroll(/** @type {any} */ (el))
    s.push(0, 5_000)
    runFrames()
    expect(el.scrollTop).toBe(500) // content 1000 - viewport 500
    s.push(0, -5_000)
    runFrames()
    expect(el.scrollTop).toBe(0)
  })

  it('caps how much travel one flick can queue', () => {
    // Three screens, so spinning the wheel can't wind up seconds of coasting.
    const el = fakeEl({ height: 500, content: 100_000 })
    const s = createSmoothScroll(/** @type {any} */ (el))
    s.push(0, 50_000)
    runFrames()
    expect(el.scrollTop).toBe(1_500)
  })

  it('adopts a position it did not set (keyboard, scrollbar, scrollIntoView)', () => {
    const el = fakeEl()
    const s = createSmoothScroll(/** @type {any} */ (el))
    el.scrollTop = 4_000
    s.sync()
    s.push(0, 100)
    runFrames()
    expect(el.scrollTop).toBe(4_100)
  })
})

/** @param {Partial<WheelEvent>} e */
const wheel = (e) => /** @type {WheelEvent} */ ({ deltaMode: 0, deltaX: 0, deltaY: 0, ...e })

describe('wheelPixels', () => {
  it('passes pixel deltas through unchanged', () => {
    expect(wheelPixels(wheel({ deltaY: 120 }), 800)).toEqual({ dx: 0, dy: 120 })
    expect(wheelPixels(wheel({ deltaX: -40, deltaY: 12 }), 800)).toEqual({ dx: -40, dy: 12 })
  })

  it('scales line deltas, so a mouse reporting lines still moves a sane distance', () => {
    // deltaMode 1 means "lines". Treating 3 as 3 pixels is the bug this exists to
    // prevent: the grid would creep by three pixels per notch.
    const { dy } = wheelPixels(wheel({ deltaMode: 1, deltaY: 3 }), 800)
    expect(dy).toBe(48)
  })

  it('scales page deltas by the scrollport', () => {
    expect(wheelPixels(wheel({ deltaMode: 2, deltaY: 1 }), 640).dy).toBe(640)
    // A degenerate viewport must not collapse the delta to zero.
    expect(wheelPixels(wheel({ deltaMode: 2, deltaY: 1 }), 0).dy).toBe(1)
  })

  it('keeps direction', () => {
    expect(wheelPixels(wheel({ deltaMode: 1, deltaY: -3 }), 800).dy).toBe(-48)
  })
})

describe('createSmoothScroll - horizontal', () => {
  it('eases the x axis to the exact target', () => {
    const el = fakeEl()
    const s = createSmoothScroll(/** @type {any} */ (el))
    s.push(300, 0)
    runFrames()
    expect(el.scrollLeft).toBe(300)
    expect(el.scrollTop).toBe(0)
  })

  it('does not disturb the other axis', () => {
    const el = fakeEl()
    const s = createSmoothScroll(/** @type {any} */ (el))
    s.push(0, 200)
    runFrames()
    s.push(150, 0)
    runFrames()
    expect(el.scrollTop).toBe(200)
    expect(el.scrollLeft).toBe(150)
  })

  it('clamps x at the right edge', () => {
    const el = fakeEl({ height: 500, content: 800 })
    const s = createSmoothScroll(/** @type {any} */ (el))
    s.push(5_000, 0)
    runFrames()
    expect(el.scrollLeft).toBe(300) // scrollWidth 800 - clientWidth 500
  })
})

describe('isTrackpadWheel', () => {
  it('treats smooth small, fractional, or diagonal deltas as trackpad gestures', () => {
    expect(isTrackpadWheel(wheel({ deltaY: 12 }))).toBe(true)
    expect(isTrackpadWheel(wheel({ deltaY: 53.5 }))).toBe(true)
    expect(isTrackpadWheel(wheel({ deltaX: 40, deltaY: 40 }))).toBe(true)
  })

  it('treats line-mode or large notched integer wheel ticks as discrete mouse wheels', () => {
    expect(isTrackpadWheel(wheel({ deltaMode: 1, deltaY: 3 }))).toBe(false)
    expect(isTrackpadWheel(wheel({ deltaMode: 0, deltaY: 100 }))).toBe(false)
    expect(isTrackpadWheel(wheel({ deltaMode: 0, deltaY: -120 }))).toBe(false)
  })
})
