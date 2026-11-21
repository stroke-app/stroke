import { describe, it, expect } from 'vitest'
import { compileGeometry, screenTransform, onScreen, traceRings, hitCompiled, boundsWithin } from './geo-cache.js'
import { toWorld, toScreen } from './geo-map.js'

const square = { type: 'Polygon', coordinates: [[[0, 0], [10, 0], [10, 10], [0, 10], [0, 0]], [[4, 4], [6, 4], [6, 6], [4, 6], [4, 4]]] }

describe('compileGeometry', () => {
  it('projects once into flat world coordinates, polygons first', () => {
    const c = compileGeometry({ type: 'GeometryCollection', geometries: [{ type: 'LineString', coordinates: [[0, 0], [1, 1]] }, square, { type: 'Point', coordinates: [2, 3] }] })
    expect(c.polygons).toBe(2)
    expect(c.rings.length).toBe(3)
    expect(c.rings[2].closed).toBe(false)
    const w = toWorld(2, 3)
    expect([...c.points]).toEqual([w.x, w.y])
    expect(c.box[0]).toBeCloseTo(toWorld(0, 0).x)
  })
  it('survives junk', () => {
    expect(compileGeometry(null).rings).toEqual([])
    expect(compileGeometry({ type: 'Point', coordinates: [1] }).points.length).toBe(0)
  })
})

describe('drawing', () => {
  const view = { scale: 4096, x: toWorld(5, 5).x, y: toWorld(5, 5).y }
  const t = screenTransform(view, 800, 600)
  it('matches toScreen exactly', () => {
    const w = toWorld(3, 4)
    const s = toScreen(view, 800, 600, w.x, w.y)
    expect(w.x * t.s + t.ox).toBeCloseTo(s.px, 9)
    expect(w.y * t.s + t.oy).toBeCloseTo(s.py, 9)
  })
  it('culls by box and traces rings', () => {
    const c = compileGeometry(square)
    expect(onScreen(c, t, 800, 600)).toBe(true)
    expect(onScreen(compileGeometry({ type: 'Point', coordinates: [120, 40] }), t, 800, 600)).toBe(false)
    const calls = []
    traceRings({ moveTo: () => calls.push('m'), lineTo: () => calls.push('l'), closePath: () => calls.push('z') }, c, t)
    expect(calls.filter((x) => x === 'm').length).toBe(2)
    expect(calls.filter((x) => x === 'z').length).toBe(2)
  })
})

describe('hitCompiled', () => {
  const shapes = [compileGeometry(square), compileGeometry({ type: 'Point', coordinates: [5, 5] })]
  const at = (lon, lat) => toWorld(lon, lat)
  it('picks a point over the polygon it sits in, and the polygon inside but not in its hole', () => {
    const p = at(5, 5)
    expect(hitCompiled(shapes, p.x, p.y, 0.0001)).toBe(1)
    const q = at(2, 2)
    expect(hitCompiled(shapes, q.x, q.y, 0.00001)).toBe(0)
    const hole = at(5.5, 4.5)
    expect(hitCompiled([shapes[0]], hole.x, hole.y, 0.00001)).toBe(-1)
    const out = at(20, 20)
    expect(hitCompiled(shapes, out.x, out.y, 0.0001)).toBe(-1)
  })
})

it('boundsWithin', () => {
  const outer = { minLon: 0, minLat: 0, maxLon: 10, maxLat: 10 }
  expect(boundsWithin({ minLon: 1, minLat: 1, maxLon: 9, maxLat: 9 }, outer)).toBe(true)
  expect(boundsWithin({ minLon: -1, minLat: 1, maxLon: 9, maxLat: 9 }, outer)).toBe(false)
})
