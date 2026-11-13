// Lat/lng pairs - two coordinate columns read as one point. The latitude cell
// shows `📍 37.7749, -122.4194` in the tooltip and links to a map; the
// longitude cell gets the same link. Also reads a single text column holding
// "lat,lng" (location, coords, position).
//
// The partner column is found by name in the same row (ctx.row + ctx.colIndex):
// lat/latitude ↔ lng/lon/long/longitude, with any shared prefix or suffix
// (`pickup_lat` ↔ `pickup_lng`).

const LAT_RE = /^(.*?)(lat|latitude)(.*)$/i
const LNG_RE = /^(.*?)(lng|lon|long|longitude)(.*)$/i
const PAIR_NAME_RE = /(^|_)(location|coords?|coordinates|position|latlng|lat_lng|latlon|geo|point|gps)$/i
const PAIR_RE = /^\s*\(?\s*(-?\d{1,2}(?:\.\d+)?)\s*[, ]\s*(-?\d{1,3}(?:\.\d+)?)\s*\)?\s*$/

const PROVIDERS = /** @type {Record<string, (lat: number, lng: number) => string>} */ ({
  openstreetmap: (lat, lng) => `https://www.openstreetmap.org/?mlat=${lat}&mlon=${lng}#map=15/${lat}/${lng}`,
  google: (lat, lng) => `https://www.google.com/maps/search/?api=1&query=${lat},${lng}`,
  apple: (lat, lng) => `https://maps.apple.com/?ll=${lat},${lng}&q=${lat},${lng}`,
})

const defaultConfig = { provider: 'openstreetmap' }

/** @param {number} lat @param {number} lng */
const valid = (lat, lng) => Number.isFinite(lat) && Number.isFinite(lng) && Math.abs(lat) <= 90 && Math.abs(lng) <= 180 && !(lat === 0 && lng === 0)

/**
 * The partner column's index for a lat or lng column, or -1.
 * @param {string} name @param {Map<string, number>} index
 * @returns {{ partner: number, isLat: boolean }}
 */
function partnerOf(name, index) {
  const lat = LAT_RE.exec(name)
  if (lat && !LNG_RE.test(name)) {
    for (const w of ['lng', 'lon', 'long', 'longitude']) {
      for (const cand of [lat[1] + w + lat[3], lat[1] + w.toUpperCase() + lat[3], lat[1] + w[0].toUpperCase() + w.slice(1) + lat[3]]) {
        const i = index.get(cand)
        if (i !== undefined) return { partner: i, isLat: true }
      }
    }
  }
  const lng = LNG_RE.exec(name)
  if (lng) {
    for (const w of ['lat', 'latitude']) {
      for (const cand of [lng[1] + w + lng[3], lng[1] + w.toUpperCase() + lng[3], lng[1] + w[0].toUpperCase() + w.slice(1) + lng[3]]) {
        const i = index.get(cand)
        if (i !== undefined) return { partner: i, isLat: false }
      }
    }
  }
  return { partner: -1, isLat: false }
}

export const geoPair = {
  id: 'geo-pair',
  name: 'Lat/Lng Pairs',
  description: 'Read latitude and longitude columns as one point, with a map link.',
  kind: 'formatter',
  defaultConfig,

  /** @param {string} _type @param {string} name */
  appliesTo(_type, name) {
    return !name || LAT_RE.test(name) || LNG_RE.test(name) || PAIR_NAME_RE.test(name)
  },

  /**
   * @param {unknown} value @param {string} _type @param {Record<string, unknown>} config @param {string} name
   * @param {{ row?: unknown[], colIndex?: Map<string, number> } | undefined} ctx
   */
  format(value, _type, config, name, ctx) {
    const provider = PROVIDERS[String(config?.provider ?? defaultConfig.provider)] ?? PROVIDERS.openstreetmap
    /** @param {number} lat @param {number} lng */
    const out = (lat, lng) => ({
      link: provider(lat, lng),
      title: `📍 ${lat.toFixed(5)}, ${lng.toFixed(5)} · click to open the map`,
    })
    if (typeof value === 'string' && name && PAIR_NAME_RE.test(name)) {
      const m = PAIR_RE.exec(value)
      if (!m) return null
      const lat = Number(m[1]), lng = Number(m[2])
      return valid(lat, lng) ? out(lat, lng) : null
    }
    if (!ctx?.row || !ctx.colIndex || !name) return null
    const { partner, isLat } = partnerOf(name, ctx.colIndex)
    if (partner < 0) return null
    const self = Number(value)
    const other = Number(ctx.row[partner])
    const lat = isLat ? self : other
    const lng = isLat ? other : self
    return valid(lat, lng) ? out(lat, lng) : null
  },
}
