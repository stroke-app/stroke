import { describe, it, expect } from 'vitest'
import { separateCards, cardsAreSeparated, wrapTallRanks } from './erd-layout.js'

/** @param {Array<[string, number, number, number, number]>} rows */
const cards = (rows) => rows.map(([id, x, y, w, h]) => ({ id, x, y, w, h }))

/** Apply the result back onto the cards, the way the page does. */
function applied(list, opts) {
  const pos = separateCards(list, opts)
  return list.map((c) => ({ ...c, ...pos.get(c.id) }))
}

describe('separateCards', () => {
  it('leaves a layout that is already clear alone', () => {
    const list = cards([
      ['a', 0, 0, 268, 200],
      ['b', 500, 0, 268, 200],
      ['c', 0, 400, 268, 200],
    ])
    const pos = separateCards(list, { gapX: 120, gapY: 80 })
    for (const c of list) expect(pos.get(c.id)).toEqual({ x: c.x, y: c.y })
  })

  it('pulls apart two cards drawn on top of each other', () => {
    // The failure from the diagram: a tall card sitting over its neighbour.
    const list = cards([
      ['offer', 680, 205, 268, 260],
      ['artworks', 690, 395, 268, 120],
    ])
    const out = applied(list, { gapX: 120, gapY: 80 })
    expect(cardsAreSeparated(out, 120, 80)).toBe(true)
  })

  it('separates cards stacked at the exact same point', () => {
    const list = cards([
      ['a', 100, 100, 268, 180],
      ['b', 100, 100, 268, 180],
      ['c', 100, 100, 268, 180],
    ])
    const out = applied(list, { gapX: 120, gapY: 80 })
    expect(cardsAreSeparated(out, 120, 80)).toBe(true)
  })

  it('opens a channel wide enough for lines between neighbours', () => {
    // Touching cards: there is nowhere for a relationship line to run.
    const list = cards([
      ['a', 0, 0, 268, 200],
      ['b', 268, 0, 268, 200],
    ])
    const out = applied(list, { gapX: 140, gapY: 84 })
    const [a, b] = out
    // Whichever side they ended up on, the gap between them is the channel.
    expect(Math.max(b.x - (a.x + a.w), a.x - (b.x + b.w))).toBeGreaterThanOrEqual(139)
  })

  it('never moves a pinned card', () => {
    const list = cards([
      ['focus', 500, 500, 268, 300],
      ['other', 520, 520, 268, 300],
    ])
    const pos = separateCards(list, { gapX: 120, gapY: 80, fixed: new Set(['focus']) })
    expect(pos.get('focus')).toEqual({ x: 500, y: 500 })
    expect(cardsAreSeparated(list.map((c) => ({ ...c, ...pos.get(c.id) })), 120, 80)).toBe(true)
  })

  it('is deterministic - the same input never jitters', () => {
    const list = cards([
      ['a', 10, 10, 268, 200],
      ['b', 30, 40, 268, 340],
      ['c', 60, 20, 268, 120],
      ['d', 15, 300, 268, 260],
    ])
    const first = separateCards(list, { gapX: 120, gapY: 80 })
    const second = separateCards(list, { gapX: 120, gapY: 80 })
    for (const c of list) expect(second.get(c.id)).toEqual(first.get(c.id))
  })

  it('resolves a dense pile-up of a hundred cards', () => {
    // Every card overlapping several others, the worst case a re-flow can leave.
    const list = cards(
      Array.from({ length: 100 }, (_, i) => [
        `t${i}`,
        (i % 10) * 60,
        Math.floor(i / 10) * 60,
        268,
        180 + (i % 5) * 40,
      ]),
    )
    const started = Date.now()
    const out = applied(list, { gapX: 120, gapY: 80 })
    expect(Date.now() - started).toBeLessThan(1500)
    expect(cardsAreSeparated(out, 120, 80)).toBe(true)
  })

  it('stays cheap on a schema-sized graph', () => {
    // 800 cards in overlapping columns - a whole-schema diagram. The pass used to
    // compare every pair, which is 60 sweeps over 320k pairs and seconds of
    // frozen window; the sweep only looks at horizontal neighbours.
    const list = cards(
      Array.from({ length: 800 }, (_, i) => [
        `t${i}`,
        (i % 25) * 200,
        Math.floor(i / 25) * 150,
        268,
        200 + (i % 6) * 30,
      ]),
    )
    const started = Date.now()
    const out = applied(list, { gapX: 140, gapY: 88 })
    expect(Date.now() - started).toBeLessThan(600)
    expect(cardsAreSeparated(out, 140, 88)).toBe(true)
  })

  it('keeps cards near where the layout put them', () => {
    const list = cards([
      ['a', 0, 0, 268, 200],
      ['b', 300, 10, 268, 200],
      ['c', 600, 20, 268, 200],
    ])
    const out = applied(list, { gapX: 120, gapY: 80 })
    // Left-to-right order is what Dagre decided; separation must not reshuffle it.
    expect(out.map((c) => c.id).sort((p, q) => out.find((o) => o.id === p).x - out.find((o) => o.id === q).x))
      .toEqual(['a', 'b', 'c'])
  })
})

describe('wrapTallRanks', () => {
  /** One hub in rank 0, `n` children stacked in rank 1 - the shape that turns a
   *  schema into a vertical strip. */
  const hubSchema = (n) => [
    { id: 'hub', x: 0, y: (n / 2) * 240, w: 268, h: 200 },
    ...Array.from({ length: n }, (_, i) => ({
      id: `t${i}`, x: 420, y: i * 240, w: 268, h: 200,
    })),
  ]
  const gaps = { gapX: 110, gapY: 52 }

  const boxes = (cards, pos) =>
    cards.map((c) => ({ ...c, ...(pos.get(c.id) ?? { x: c.x, y: c.y }) }))

  it('leaves a layout that already fits the page alone', () => {
    const cards = hubSchema(3)
    const out = wrapTallRanks(cards, gaps)
    expect(out.moved).toBe(false)
  })

  it('folds a rank taller than the page into columns', () => {
    const cards = hubSchema(90)
    const out = wrapTallRanks(cards, gaps)
    expect(out.moved).toBe(true)
    const xs = new Set([...out.pos.values()].map((p) => p.x))
    expect(xs.size).toBeGreaterThan(3)
  })

  it('brings the aspect ratio back towards the page', () => {
    const cards = hubSchema(90)
    const before = boxes(cards, new Map())
    const tall = Math.max(...before.map((c) => c.y + c.h)) / Math.max(...before.map((c) => c.x + c.w))
    const after = boxes(cards, wrapTallRanks(cards, gaps).pos)
    const wide = Math.max(...after.map((c) => c.y + c.h)) / Math.max(...after.map((c) => c.x + c.w))
    expect(tall).toBeGreaterThan(5)
    expect(wide).toBeLessThan(1.5)
  })

  it('never overlaps two cards', () => {
    for (const n of [8, 40, 90, 300]) {
      const cards = hubSchema(n)
      const out = wrapTallRanks(cards, gaps)
      expect(cardsAreSeparated(boxes(cards, out.pos))).toBe(true)
    }
  })

  it('keeps the order the layout chose inside each column', () => {
    const cards = hubSchema(40)
    const out = wrapTallRanks(cards, gaps)
    const rank = cards.filter((c) => c.id !== 'hub')
    /** @type {Map<number, {id: string, y: number}[]>} */
    const cols = new Map()
    for (const c of rank) {
      const p = out.pos.get(c.id)
      const at = cols.get(p.x)
      if (at) at.push({ id: c.id, y: p.y })
      else cols.set(p.x, [{ id: c.id, y: p.y }])
    }
    for (const col of cols.values()) {
      const byY = [...col].sort((a, b) => a.y - b.y).map((c) => c.id)
      const byIndex = [...col]
        .sort((a, b) => Number(a.id.slice(1)) - Number(b.id.slice(1)))
        .map((c) => c.id)
      expect(byY).toEqual(byIndex)
    }
  })

  it('agrees with itself on a second run', () => {
    const cards = hubSchema(60)
    const a = wrapTallRanks(cards, gaps).pos
    const b = wrapTallRanks(cards, gaps).pos
    for (const [id, p] of a) expect(b.get(id)).toEqual(p)
  })

  it('has nothing to do for a single card', () => {
    const out = wrapTallRanks([{ id: 'only', x: 5, y: 7, w: 268, h: 200 }], gaps)
    expect(out.moved).toBe(false)
    expect(out.pos.get('only')).toEqual({ x: 5, y: 7 })
  })
})

describe('packOrphans', () => {
  it('fills columns by height and aims at a landscape page', async () => {
    const { packOrphans, cardsAreSeparated } = await import('./erd-layout.js')
    const hs = [600, 120, 120, 120, 120, 400, 120, 120, 120, 120, 120, 120, 300, 120, 120, 120]
    const cards = hs.map((h, i) => ({ id: `t${i}`, w: 220, h }))
    const pos = packOrphans(cards, { gapX: 40, gapY: 30 })
    const placed = cards.map((c) => ({ ...c, ...pos.get(c.id) }))
    expect(cardsAreSeparated(placed, 40, 30)).toBe(true)
    const w = Math.max(...placed.map((c) => c.x + c.w)), h = Math.max(...placed.map((c) => c.y + c.h))
    expect(w / h).toBeGreaterThan(1)
    // The short cards fill in beside the tall first one instead of waiting under it.
    expect(placed[1].y).toBe(0)
  })

  it('handles none and one', async () => {
    const { packOrphans } = await import('./erd-layout.js')
    expect(packOrphans([], { gapX: 10, gapY: 10 }).size).toBe(0)
    expect(packOrphans([{ id: 'a', w: 100, h: 50 }], { gapX: 10, gapY: 10, x: 5, y: 7 }).get('a')).toEqual({ x: 5, y: 7 })
  })
})
