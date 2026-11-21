// Spatial values in a query result, ready for the result map.
//
// A result's spatial column is found by its type (PostGIS `geometry` and
// `geography` arrive as EWKT) or, for text and JSON columns, by what the
// values look like: `ST_AsText` gives WKT and `ST_AsGeoJSON` gives GeoJSON,
// and both are common ways to select a shape. Each row then becomes at most
// one feature in WGS 84. Web Mercator, British National Grid and the UTM zones
// are converted here; a value in any other system is counted, not guessed at:
// placing a shape on a map is a claim about where it is.

import { geoShapeOf, isGeometryType, parseGeometry } from '$lib/geometry-cell.js'

/** @typedef {import('$lib/geometry-cell.js').GeoShape} GeoShape */
/** @typedef {import('$lib/geometry-cell.js').ParsedGeometry} ParsedGeometry */

/** How many non-null values decide whether an untyped column is spatial. */
const SAMPLE = 20

const WKT_HEAD = /^\s*(?:SRID=\d+;\s*)?(?:GEOMETRYCOLLECTION|MULTIPOLYGON|MULTILINESTRING|MULTIPOINT|POLYGON|LINESTRING|POINT)\b/i
const GEOJSON_TYPES = new Set(['Point', 'MultiPoint', 'LineString', 'MultiLineString', 'Polygon', 'MultiPolygon', 'GeometryCollection'])

/**
 * A GeoJSON geometry object (or a JSON string of one), else null.
 * @param {unknown} v
 * @returns {any | null}
 */
function geoJsonOf(v) {
  let o = v
  if (typeof v === 'string') {
    const t = v.trim()
    if (!t.startsWith('{') || !t.includes('"type"')) return null
    try { o = JSON.parse(t) } catch { return null }
  }
  if (!o || typeof o !== 'object' || Array.isArray(o)) return null
  const g = /** @type {any} */ (o)
  // A Feature carries its geometry one level down.
  if (g.type === 'Feature') return geoJsonOf(g.geometry)
  if (!GEOJSON_TYPES.has(g.type)) return null
  if (g.type === 'GeometryCollection') return Array.isArray(g.geometries) ? g : null
  return Array.isArray(g.coordinates) ? g : null
}

/** @param {unknown} v */
function looksSpatial(v) {
  return (typeof v === 'string' && WKT_HEAD.test(v)) || geoJsonOf(v) !== null
}

/**
 * Indices of the columns that hold shapes.
 * @param {{ name: string, data_type?: string, dataType?: string, type?: string }[]} columns
 * @param {unknown[][]} rows
 * @returns {number[]}
 */
export function spatialColumns(columns, rows) {
  const out = []
  for (let c = 0; c < columns.length; c++) {
    const col = columns[c]
    const type = col.data_type ?? col.dataType ?? col.type ?? ''
    if (isGeometryType(type)) {
      out.push(c)
      continue
    }
    // Numbers, booleans and dates can't be shapes; don't sample them.
    if (/^(int|float|double|numeric|decimal|real|bool|date|time|serial|big|small|uuid|money|interval)/i.test(String(type))) continue
    let seen = 0
    let spatial = true
    for (const r of rows) {
      const v = r?.[c]
      if (v == null || v === '') continue
      if (!looksSpatial(v)) { spatial = false; break }
      if (++seen >= SAMPLE) break
    }
    if (spatial && seen > 0) out.push(c)
  }
  return out
}

/** @param {any} ring @returns {{ x: number, y: number }[]} */
const verts = (ring) => (Array.isArray(ring) ? ring.filter((p) => Array.isArray(p) && p.length >= 2).map((p) => ({ x: Number(p[0]), y: Number(p[1]) })) : [])

/**
 * GeoJSON as the parser's shape. GeoJSON is WGS 84 by definition (RFC 7946).
 * @param {any} g
 * @returns {ParsedGeometry | null}
 */
export function parseGeoJson(g) {
  const gj = geoJsonOf(g)
  if (!gj) return null
  /** @type {ParsedGeometry} */
  const out = { srid: 4326, type: gj.type, dims: 'XY', empty: false, points: [], lines: [], polygons: [], vertexCount: 0, parts: 0, bbox: null }
  const add = (/** @type {any} */ x) => {
    switch (x?.type) {
      case 'Point': if (Array.isArray(x.coordinates) && x.coordinates.length >= 2) out.points.push(verts([x.coordinates])); break
      case 'MultiPoint': for (const p of x.coordinates ?? []) out.points.push(verts([p])); break
      case 'LineString': out.lines.push(verts(x.coordinates)); break
      case 'MultiLineString': for (const l of x.coordinates ?? []) out.lines.push(verts(l)); break
      case 'Polygon': out.polygons.push((x.coordinates ?? []).map(verts)); break
      case 'MultiPolygon': for (const p of x.coordinates ?? []) out.polygons.push(p.map(verts)); break
      case 'GeometryCollection': for (const y of x.geometries ?? []) add(y); break
      default: break
    }
  }
  add(gj)
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
  const track = (/** @type {{ x: number, y: number }} */ v) => {
    out.vertexCount++
    if (v.x < minX) minX = v.x
    if (v.x > maxX) maxX = v.x
    if (v.y < minY) minY = v.y
    if (v.y > maxY) maxY = v.y
  }
  for (const p of out.points) p.forEach(track)
  for (const l of out.lines) l.forEach(track)
  for (const poly of out.polygons) for (const r of poly) r.forEach(track)
  out.parts = out.points.length + out.lines.length + out.polygons.length
  out.empty = out.vertexCount === 0
  out.bbox = out.empty ? null : { minX, minY, maxX, maxY }
  return out
}

/**
 * One cell as a parsed geometry: EWKT/WKT or GeoJSON.
 * @param {unknown} v
 * @returns {ParsedGeometry | null}
 */
export function parseSpatial(v) {
  if (v == null) return null
  if (typeof v === 'string' && WKT_HEAD.test(v)) return parseGeometry(v)
  return parseGeoJson(v)
}

// ── Projected systems the map can invert ─────────────────────────────────────
//
// Inverse Transverse Mercator from the Ordnance Survey's "A guide to
// coordinate systems in Great Britain" (Annex C), and its OSGB36 to WGS 84
// Helmert transformation (Annex E, about 5 m). That covers British National
// Grid and every UTM zone on WGS 84, the projected systems that turn up most.

const RAD = Math.PI / 180

/**
 * @typedef {{ a: number, b: number, F0: number, lat0: number, lon0: number, E0: number, N0: number }} TmParams
 */

/** @param {number} E @param {number} N @param {TmParams} p @returns {[number, number]} radians */
function tmInverse(E, N, p) {
  const { a, b, F0, E0, N0 } = p
  const lat0 = p.lat0 * RAD
  const lon0 = p.lon0 * RAD
  const e2 = 1 - (b * b) / (a * a)
  const n = (a - b) / (a + b)
  const n2 = n * n
  const n3 = n2 * n
  const meridional = (/** @type {number} */ lat) => {
    const d = lat - lat0
    const s = lat + lat0
    return b * F0 * (
      (1 + n + 1.25 * n2 + 1.25 * n3) * d
      - (3 * n + 3 * n2 + (21 / 8) * n3) * Math.sin(d) * Math.cos(s)
      + ((15 / 8) * n2 + (15 / 8) * n3) * Math.sin(2 * d) * Math.cos(2 * s)
      - (35 / 24) * n3 * Math.sin(3 * d) * Math.cos(3 * s)
    )
  }
  let lat = lat0
  let M = 0
  for (let i = 0; i < 20; i++) {
    lat = (N - N0 - M) / (a * F0) + lat
    M = meridional(lat)
    if (Math.abs(N - N0 - M) < 0.00001) break
  }
  const sin = Math.sin(lat)
  const cos = Math.cos(lat)
  const nu = (a * F0) / Math.sqrt(1 - e2 * sin * sin)
  const rho = (a * F0 * (1 - e2)) / Math.pow(1 - e2 * sin * sin, 1.5)
  const eta2 = nu / rho - 1
  const t = Math.tan(lat)
  const t2 = t * t
  const t4 = t2 * t2
  const t6 = t4 * t2
  const sec = 1 / cos
  const VII = t / (2 * rho * nu)
  const VIII = (t / (24 * rho * nu ** 3)) * (5 + 3 * t2 + eta2 - 9 * t2 * eta2)
  const IX = (t / (720 * rho * nu ** 5)) * (61 + 90 * t2 + 45 * t4)
  const X = sec / nu
  const XI = (sec / (6 * nu ** 3)) * (nu / rho + 2 * t2)
  const XII = (sec / (120 * nu ** 5)) * (5 + 28 * t2 + 24 * t4)
  const XIIA = (sec / (5040 * nu ** 7)) * (61 + 662 * t2 + 1320 * t4 + 720 * t6)
  const dE = E - E0
  return [
    lat - VII * dE ** 2 + VIII * dE ** 4 - IX * dE ** 6,
    lon0 + X * dE - XI * dE ** 3 + XII * dE ** 5 - XIIA * dE ** 7,
  ]
}

const AIRY = { a: 6377563.396, b: 6356256.909 }
const WGS84 = { a: 6378137, b: 6356752.314245 }

/** OSGB36 lat/lon (radians, Airy 1830) to WGS 84 degrees. */
function osgb36ToWgs84(/** @type {number} */ lat, /** @type {number} */ lon) {
  // Geodetic to cartesian on Airy 1830.
  const e2a = 1 - (AIRY.b * AIRY.b) / (AIRY.a * AIRY.a)
  const nu = AIRY.a / Math.sqrt(1 - e2a * Math.sin(lat) ** 2)
  const x = nu * Math.cos(lat) * Math.cos(lon)
  const y = nu * Math.cos(lat) * Math.sin(lon)
  const z = nu * (1 - e2a) * Math.sin(lat)
  // Helmert, OSGB36 to WGS 84.
  const tx = 446.448, ty = -125.157, tz = 542.06
  const s = -20.4894e-6
  const sec = RAD / 3600
  const rx = 0.1502 * sec, ry = 0.247 * sec, rz = 0.8421 * sec
  const x2 = tx + x * (1 + s) - y * rz + z * ry
  const y2 = ty + x * rz + y * (1 + s) - z * rx
  const z2 = tz - x * ry + y * rx + z * (1 + s)
  // Cartesian to geodetic on WGS 84.
  const e2 = 1 - (WGS84.b * WGS84.b) / (WGS84.a * WGS84.a)
  const p = Math.sqrt(x2 * x2 + y2 * y2)
  let phi = Math.atan2(z2, p * (1 - e2))
  for (let i = 0; i < 10; i++) {
    const v = WGS84.a / Math.sqrt(1 - e2 * Math.sin(phi) ** 2)
    const next = Math.atan2(z2 + e2 * v * Math.sin(phi), p)
    if (Math.abs(next - phi) < 1e-12) { phi = next; break }
    phi = next
  }
  return { lon: Math.atan2(y2, x2) / RAD, lat: phi / RAD }
}

/**
 * A converter from this SRID to lon/lat degrees, or null when the map can't.
 * @param {number | null} srid
 * @returns {((x: number, y: number) => { lon: number, lat: number }) | null}
 */
export function toWgs84(srid) {
  if (srid === 27700) {
    const p = { ...AIRY, F0: 0.9996012717, lat0: 49, lon0: -2, E0: 400000, N0: -100000 }
    return (x, y) => {
      const [lat, lon] = tmInverse(x, y, p)
      return osgb36ToWgs84(lat, lon)
    }
  }
  if (srid != null && ((srid > 32600 && srid <= 32660) || (srid > 32700 && srid <= 32760))) {
    const south = srid > 32700
    const zone = srid % 100
    const p = { ...WGS84, F0: 0.9996, lat0: 0, lon0: zone * 6 - 183, E0: 500000, N0: south ? 10000000 : 0 }
    return (x, y) => {
      const [lat, lon] = tmInverse(x, y, p)
      return { lon: lon / RAD, lat: lat / RAD }
    }
  }
  return null
}

/**
 * The geometry with its vertices moved to WGS 84, when its SRID is one the
 * map can invert.
 * @param {ParsedGeometry} g
 * @returns {ParsedGeometry | null}
 */
function reprojected(g) {
  const f = toWgs84(g.srid)
  if (!f) return null
  const mv = (/** @type {{ x: number, y: number }} */ v) => {
    const ll = f(v.x, v.y)
    return { ...v, x: ll.lon, y: ll.lat }
  }
  const points = g.points.map((p) => p.map(mv))
  const lines = g.lines.map((l) => l.map(mv))
  const polygons = g.polygons.map((poly) => poly.map((r) => r.map(mv)))
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
  const track = (/** @type {{ x: number, y: number }} */ v) => {
    if (v.x < minX) minX = v.x
    if (v.x > maxX) maxX = v.x
    if (v.y < minY) minY = v.y
    if (v.y > maxY) maxY = v.y
  }
  for (const p of points) p.forEach(track)
  for (const l of lines) l.forEach(track)
  for (const poly of polygons) for (const r of poly) r.forEach(track)
  return { ...g, srid: 4326, points, lines, polygons, bbox: Number.isFinite(minX) ? { minX, minY, maxX, maxY } : null }
}

/**
 * @typedef {Object} ResultFeature
 * @property {number} row     Index into the rows given.
 * @property {GeoShape} shape In lon/lat.
 */

/**
 * @typedef {Object} ResultLayer
 * @property {ResultFeature[]} features
 * @property {[number, number, number, number] | null} extent  Of every feature.
 * @property {number} empty      Rows with no value, or an empty geometry.
 * @property {number} unplaced   Values in a coordinate system the map can't show.
 * @property {number[]} srids    Those systems' SRIDs.
 * @property {number} assumed    Values without an SRID taken as WGS 84.
 */

/**
 * Every row's shape in one column.
 * @param {unknown[][]} rows @param {number} col
 * @returns {ResultLayer}
 */
export function resultLayer(rows, col) {
  /** @type {ResultFeature[]} */
  const features = []
  let empty = 0
  let unplaced = 0
  let assumed = 0
  /** @type {Set<number>} */
  const srids = new Set()
  let minLon = Infinity, minLat = Infinity, maxLon = -Infinity, maxLat = -Infinity
  for (let i = 0; i < rows.length; i++) {
    const g = parseSpatial(rows[i]?.[col])
    if (!g || g.empty) { empty++; continue }
    let shape = geoShapeOf(g)
    if (!shape) {
      const moved = reprojected(g)
      if (moved) shape = geoShapeOf(moved)
    }
    if (!shape) {
      unplaced++
      if (g.srid) srids.add(g.srid)
      continue
    }
    if (shape.assumed) assumed++
    features.push({ row: i, shape })
    const [a, b, c, d] = shape.extent
    if (a < minLon) minLon = a
    if (b < minLat) minLat = b
    if (c > maxLon) maxLon = c
    if (d > maxLat) maxLat = d
  }
  return {
    features,
    extent: features.length ? [minLon, minLat, maxLon, maxLat] : null,
    empty,
    unplaced,
    srids: [...srids].sort((a, b) => a - b),
    assumed,
  }
}

/**
 * Why some values aren't on the map, and how to get them there.
 * @param {ResultLayer} layer @param {string} column
 * @returns {string}
 */
export function unplacedHint(layer, column) {
  if (!layer.unplaced) return ''
  const sys = layer.srids.length ? layer.srids.map((s) => `EPSG:${s}`).join(', ') : 'an unknown coordinate system'
  const n = layer.unplaced
  return `${n} ${n === 1 ? 'value is' : 'values are'} in ${sys}. Select ST_Transform(${column}, 4326) to see ${n === 1 ? 'it' : 'them'} here.`
}

/**
 * Whether a point (screen px) is inside a polygon's rings (even-odd).
 * @param {number} px @param {number} py @param {{ px: number, py: number }[][]} rings
 */
export function insideRings(px, py, rings) {
  let inside = false
  for (const ring of rings) {
    for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
      const a = ring[i]
      const b = ring[j]
      if ((a.py > py) !== (b.py > py) && px < ((b.px - a.px) * (py - a.py)) / (b.py - a.py) + a.px) inside = !inside
    }
  }
  return inside
}
