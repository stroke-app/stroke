import { expect, test } from 'vitest'
import ELK from 'elkjs/lib/elk.bundled.js'
import { buildElkGraph, cardsPerLayer, layoutWithElk } from './erd-elk.js'
import { pathHitsRect } from './erd-routing.js'

const W = 268, HDR = 42, ROW = 28
const card = (/** @type {string} */ id, rows = 3) => ({
  id, w: W, h: HDR + rows * ROW + 8,
  rowY: (/** @type {string | null} */ col) => (col === 'id' ? HDR + ROW / 2 : col ? HDR + ROW + ROW / 2 : HDR / 2),
})

test('cards per layer follows the schema size', () => {
  expect(cardsPerLayer(4)).toBe(4)
  expect(cardsPerLayer(30)).toBe(6)
  expect(cardsPerLayer(135)).toBe(12)
})

test('a hub schema lays out with clear cards and lines that miss every card', async () => {
  const cards = ['users', ...Array.from({ length: 9 }, (_, i) => `t${i}`)].map((id) => card(id))
  const links = cards.slice(1).map((c) => ({ id: `${c.id}__users`, source: c.id, target: 'users', sourceCol: 'user_id', targetCol: 'id' }))
  const { pos, routes } = await layoutWithElk(cards, links, { rankSep: 150, nodeSep: 52, perLayer: 4, elk: new ELK() })

  // Every card placed, none on another.
  const boxes = cards.map((c) => ({ ...c, x: pos.get(c.id)?.x ?? NaN, y: pos.get(c.id)?.y ?? NaN }))
  for (const b of boxes) expect(Number.isFinite(b.x) && Number.isFinite(b.y)).toBe(true)
  for (let i = 0; i < boxes.length; i++)
    for (let j = i + 1; j < boxes.length; j++) {
      const a = boxes[i], b = boxes[j]
      const clear = b.x >= a.x + a.w || a.x >= b.x + b.w || b.y >= a.y + a.h || a.y >= b.y + b.h
      expect(clear, `${a.id} overlaps ${b.id}`).toBe(true)
    }
  // Nine cards into layers of four: the hub cannot be one strip.
  const layers = new Set(boxes.map((b) => b.x))
  expect(layers.size).toBeGreaterThanOrEqual(3)

  // Every line: whole, orthogonal, on its rows, clear of every other card.
  const hub = boxes.find((b) => b.id === 'users')
  for (const l of links) {
    const pts = routes.get(l.id)
    expect(pts, l.id).toBeDefined()
    if (!pts || !hub) continue
    expect(pts.length).toBeGreaterThanOrEqual(2)
    for (let i = 1; i < pts.length; i++) {
      const a = pts[i - 1], b = pts[i]
      expect(Math.abs(a.x - b.x) < 0.01 || Math.abs(a.y - b.y) < 0.01, `${l.id} bend ${i} is diagonal`).toBe(true)
    }
    // Lands on the hub's `id` row, on the side facing the source.
    expect(Math.abs(pts[pts.length - 1].y - (hub.y + HDR + ROW / 2))).toBeLessThan(1)
    for (const b of boxes) {
      if (b.id === l.source || b.id === l.target) continue
      expect(pathHitsRect(pts, b, 0), `${l.id} crosses ${b.id}`).toBe(false)
    }
  }

  // Lines leaving the same layer get their own vertical track in the channel.
  /** @type {Map<number, Set<number>>} */
  const tracks = new Map()
  for (const l of links) {
    const pts = routes.get(l.id) ?? []
    const first = pts.findIndex((p, i) => i > 0 && Math.abs(p.x - pts[i - 1].x) < 0.01 && Math.abs(p.y - pts[i - 1].y) > 0.01)
    if (first < 0) continue
    const layerX = boxes.find((b) => b.id === l.source)?.x ?? 0
    const set = tracks.get(layerX) ?? new Set()
    expect(set.has(pts[first].x), `${l.id} shares a track`).toBe(false)
    set.add(pts[first].x)
    tracks.set(layerX, set)
  }
})

test('the first pass carries no ports and the second pins every line to a row', () => {
  const cards = [card('a'), card('b')]
  const links = [{ id: 'a__b', source: 'a', target: 'b', sourceCol: 'b_id', targetCol: 'id' }]
  const free = buildElkGraph(cards, links, { rankSep: 150, nodeSep: 52, perLayer: 4 })
  expect(free.children[0].ports).toBeUndefined()
  expect(free.edges[0].sources).toEqual(['a'])
  const pinned = buildElkGraph(cards, links, { rankSep: 150, nodeSep: 52, perLayer: 4, sides: new Map([['a__b', 1]]) })
  expect(pinned.children[0].ports?.[0].layoutOptions['elk.port.side']).toBe('EAST')
  expect(pinned.children[1].ports?.[0].layoutOptions['elk.port.side']).toBe('WEST')
  expect(pinned.edges[0].sources[0]).toContain('a#EAST#')
})
