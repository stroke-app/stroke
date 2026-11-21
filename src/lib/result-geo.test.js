import { describe, it, expect } from 'vitest'
import { insideRings, parseGeoJson, parseSpatial, resultLayer, spatialColumns, toWgs84, unplacedHint } from './result-geo.js'

const cols = [
  { name: 'urn', data_type: 'int4' },
  { name: 'postcode', data_type: 'text' },
  { name: 'geogr', data_type: 'geography' },
  { name: 'wkt', data_type: 'text' },
  { name: 'gj', data_type: 'json' },
]
const rows = [
  [1, 'GU28BP', 'SRID=4326;POINT(-0.6031643633 51.2517255521)', 'POINT(-0.6 51.25)', { type: 'Point', coordinates: [-0.6, 51.25] }],
  [2, 'GU12JD', 'SRID=4326;POINT(-0.5556232692 51.2428664139)', 'LINESTRING(0 0,1 1)', '{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]}'],
  [3, 'KT211ET', null, null, null],
]

describe('spatialColumns', () => {
  it('finds geometry by type, and WKT or GeoJSON by value', () => {
    expect(spatialColumns(cols, rows)).toEqual([2, 3, 4])
  })
  it('leaves ordinary text and empty columns alone', () => {
    expect(spatialColumns([{ name: 'note', data_type: 'text' }], [['POINT is a word'], ['x']])).toEqual([])
    expect(spatialColumns([{ name: 'wkt', data_type: 'text' }], [[null], [null]])).toEqual([])
  })
})

describe('parsing', () => {
  it('reads GeoJSON, Features included, as WGS 84', () => {
    const g = parseGeoJson({ type: 'Feature', geometry: { type: 'MultiPoint', coordinates: [[1, 2], [3, 4]] } })
    expect(g?.srid).toBe(4326)
    expect(g?.points.length).toBe(2)
    expect(g?.bbox).toEqual({ minX: 1, minY: 2, maxX: 3, maxY: 4 })
    expect(parseGeoJson({ type: 'Nope' })).toBeNull()
  })
  it('reads EWKT and WKT', () => {
    expect(parseSpatial('SRID=4326;POINT(1 2)')?.points[0][0]).toMatchObject({ x: 1, y: 2 })
    expect(parseSpatial('hello')).toBeNull()
  })
})

describe('reprojection', () => {
  // Reference values from PostGIS 3.5 ST_Transform.
  it('inverts British National Grid to within a few metres', () => {
    const f = toWgs84(27700)
    const p = f?.(497581.38644245657, 151210.6317289557)
    expect(p?.lon).toBeCloseTo(-0.6031643633, 4)
    expect(p?.lat).toBeCloseTo(51.2517255521, 4)
    const q = f?.(531728.8243136029, 151895.8307809377)
    expect(q?.lon).toBeCloseTo(-0.1138699211, 4)
    expect(q?.lat).toBeCloseTo(51.2510257354, 4)
  })
  it('inverts UTM north and south', () => {
    const n = toWgs84(32630)?.(670614.662386859, 5679664.566380563)
    expect(n?.lon).toBeCloseTo(-0.5556, 6)
    expect(n?.lat).toBeCloseTo(51.2428, 6)
    const s = toWgs84(32756)?.(334368.633648097, 6250948.345385009)
    expect(s?.lon).toBeCloseTo(151.2093, 6)
    expect(s?.lat).toBeCloseTo(-33.8688, 6)
  })
  it('knows what it can not invert', () => {
    expect(toWgs84(2193)).toBeNull()
  })
})

describe('resultLayer', () => {
  it('puts each row on the map once, and counts the rest', () => {
    const l = resultLayer(rows, 2)
    expect(l.features.map((f) => f.row)).toEqual([0, 1])
    expect(l.empty).toBe(1)
    expect(l.extent?.[0]).toBeCloseTo(-0.6031643633)
  })
  it('converts British National Grid rows, and explains systems it can not', () => {
    const l = resultLayer([['SRID=27700;POINT(497581.38644245657 151210.6317289557)'], ['SRID=2193;POINT(1748000 5427000)']], 0)
    expect(l.features.length).toBe(1)
    expect(l.features[0].shape.points[0][0]).toBeCloseTo(-0.60316, 4)
    expect(l.unplaced).toBe(1)
    expect(l.srids).toEqual([2193])
    expect(unplacedHint(l, 'geom')).toBe('1 value is in EPSG:2193. Select ST_Transform(geom, 4326) to see it here.')
  })
})

describe('insideRings', () => {
  it('tests a point against a polygon with a hole', () => {
    const sq = (/** @type {number} */ a, /** @type {number} */ b) => [{ px: a, py: a }, { px: b, py: a }, { px: b, py: b }, { px: a, py: b }]
    const rings = [sq(0, 10), sq(4, 6)]
    expect(insideRings(2, 2, rings)).toBe(true)
    expect(insideRings(5, 5, rings)).toBe(false)
    expect(insideRings(12, 5, rings)).toBe(false)
  })
})
