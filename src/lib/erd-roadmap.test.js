import { expect, test } from 'vitest'
import ELK from 'elkjs/lib/elk.bundled.js'
import { adjacency, layoutRoadmap, lineage, packRows, roadmapEdges, roundedPath } from './erd-roadmap.js'

const W = 170, H = 46
const card = (/** @type {string} */ id, w = W) => ({ id, w, h: H })
/** FK holder first, referenced table second. */
const rel = (/** @type {string} */ source, /** @type {string} */ target, sourceCol = `${target}_id`) => ({ source, target, sourceCol })

const SHOP = [
  rel('orders', 'customers'), rel('order_items', 'orders'), rel('order_items', 'products'),
  rel('products', 'categories'), rel('categories', 'categories', 'parent_id'), rel('payments', 'orders'),
  rel('addresses', 'customers'), rel('shipments', 'orders'), rel('shipments', 'addresses'),
  rel('reviews', 'products'), rel('reviews', 'customers', 'author_id'), rel('reviews', 'customers', 'editor_id'),
]
const SHOP_TABLES = [...new Set(SHOP.flatMap((r) => [r.source, r.target])), 'settings', 'audit_log']

test('one edge per pair, pointing down at the table holding the key', () => {
  const ids = new Set(SHOP_TABLES)
  const edges = roadmapEdges(SHOP, ids)
  // The self reference is gone, the two keys from reviews to customers are one edge.
  expect(edges.some((e) => e.from === e.to)).toBe(false)
  const rc = edges.filter((e) => e.from === 'customers' && e.to === 'reviews')
  expect(rc).toHaveLength(1)
  expect(rc[0].cols).toEqual(['author_id', 'editor_id'])
  expect(new Set(edges.map((e) => e.id)).size).toBe(edges.length)
  // Tables off the page and links into skipped tables are dropped.
  expect(roadmapEdges(SHOP, new Set(['orders', 'order_items'])).map((e) => `${e.from}>${e.to}`)).toEqual(['orders>order_items'])
  expect(roadmapEdges(SHOP, ids, new Set(['customers'])).some((e) => e.from === 'customers')).toBe(false)
})

test('a lineage runs all the way up and all the way down, and no further', () => {
  const adj = adjacency(SHOP)
  const l = lineage('orders', adj)
  for (const t of ['orders', 'customers', 'order_items', 'payments', 'shipments']) expect(l.has(t), t).toBe(true)
  // A sibling branch is not on the line: products feeds order_items, not orders.
  expect(l.has('products')).toBe(false)
  expect(l.has('settings')).toBe(false)
  // A cycle ends.
  const loop = adjacency([rel('a', 'b'), rel('b', 'a')])
  expect([...lineage('a', loop)].sort()).toEqual(['a', 'b'])
})

test('referenced tables sit above the tables that point at them, cards clear, lines whole', async () => {
  const ids = new Set(SHOP_TABLES)
  const edges = roadmapEdges(SHOP, ids)
  const cards = SHOP_TABLES.map((id) => card(id, id.length > 8 ? 200 : W))
  const out = await layoutRoadmap(cards, edges, { elk: new ELK() })

  for (const c of cards) expect(out.pos.has(c.id), c.id).toBe(true)
  for (const e of edges) {
    const a = /** @type {{x:number,y:number}} */ (out.pos.get(e.from)), b = /** @type {{x:number,y:number}} */ (out.pos.get(e.to))
    expect(a.y + H, `${e.from} above ${e.to}`).toBeLessThan(b.y)
    const pts = out.routes.get(e.id)
    expect(pts, e.id).toBeDefined()
    const p = /** @type {{x:number,y:number}[]} */ (pts)
    // Leaves the middle of the referenced card's bottom, lands on the middle of the other's top.
    const fw = cards.find((c) => c.id === e.from)?.w ?? 0, tw = cards.find((c) => c.id === e.to)?.w ?? 0
    expect(Math.abs(p[0].x - (a.x + fw / 2))).toBeLessThan(1.5)
    expect(Math.abs(p[0].y - (a.y + H))).toBeLessThan(1.5)
    expect(Math.abs(p[p.length - 1].x - (b.x + tw / 2))).toBeLessThan(1.5)
    expect(Math.abs(p[p.length - 1].y - b.y)).toBeLessThan(1.5)
    // Orthogonal throughout.
    for (let i = 1; i < p.length; i++) expect(p[i].x === p[i - 1].x || p[i].y === p[i - 1].y).toBe(true)
  }
  const boxes = cards.map((c) => ({ ...c, ...(/** @type {{x:number,y:number}} */ (out.pos.get(c.id))) }))
  for (let i = 0; i < boxes.length; i++)
    for (let j = i + 1; j < boxes.length; j++) {
      const a = boxes[i], b = boxes[j]
      expect(b.x >= a.x + a.w || a.x >= b.x + b.w || b.y >= a.y + a.h || a.y >= b.y + b.h, `${a.id} overlaps ${b.id}`).toBe(true)
    }

  // The two tables with no keys go in the grid under the drawing.
  expect(out.loose.sort()).toEqual(['audit_log', 'settings'])
  for (const id of out.loose) expect(/** @type {{y:number}} */ (out.pos.get(id)).y).toBeGreaterThan(out.looseY)
  for (const c of cards) {
    const p = /** @type {{x:number,y:number}} */ (out.pos.get(c.id))
    expect(p.x + c.w).toBeLessThanOrEqual(out.width + 0.5)
    expect(p.y + c.h).toBeLessThanOrEqual(out.height + 0.5)
  }
})

test('a page with no links is all grid', async () => {
  const cards = ['a', 'b', 'c'].map((id) => card(id))
  const out = await layoutRoadmap(cards, [], { elk: new ELK() })
  expect(out.routes.size).toBe(0)
  expect(out.loose).toEqual(['a', 'b', 'c'])
  expect(out.looseY).toBe(0)
})

test('rows wrap at the width and centre', () => {
  const cards = Array.from({ length: 5 }, (_, i) => card(`t${i}`, 100))
  const { pos, bottom } = packRows(cards, 10, 330, 400)
  // 100 + 28 + 100 + 28 + 100 = 356 > 330: two to a row.
  expect(pos.get('t0')?.y).toBe(10)
  expect(pos.get('t2')?.y).toBe(10 + H + 28)
  expect(pos.get('t4')?.y).toBe(10 + 2 * (H + 28))
  expect(bottom).toBe(10 + 3 * H + 2 * 28)
  // Two cards (228 wide) centred on 400.
  expect(pos.get('t0')?.x).toBe(86)
})

test('rounded paths: corners cut, straight runs untouched, short jogs bounded', () => {
  expect(roundedPath([{ x: 0, y: 0 }, { x: 0, y: 50 }])).toBe('M0,0L0,50')
  // A collinear middle point is not a corner.
  expect(roundedPath([{ x: 0, y: 0 }, { x: 0, y: 20 }, { x: 0, y: 50 }])).toBe('M0,0L0,50')
  expect(roundedPath([{ x: 0, y: 0 }, { x: 0, y: 40 }, { x: 60, y: 40 }, { x: 60, y: 80 }], 10))
    .toBe('M0,0L0,30Q0,40 10,40L50,40Q60,40 60,50L60,80')
  // A 6px jog takes 3px corners, never more than half a segment.
  expect(roundedPath([{ x: 0, y: 0 }, { x: 0, y: 40 }, { x: 6, y: 40 }, { x: 6, y: 80 }], 10))
    .toBe('M0,0L0,37Q0,40 3,40L3,40Q6,40 6,43L6,80')
  expect(roundedPath([{ x: 1, y: 1 }])).toBe('')
})
