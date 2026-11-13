// Cell Transforms - on-demand conversions surfaced in the cell context menu,
// and (when chosen for a whole column) rendered live in the grid. Each
// transform is synchronous and returns a string; it never mutates the data.
//
// `appliesTo(value, type, name)` gates which transforms surface for a cell -
// detection-specific ones are listed first so the most relevant option is on
// top, with a few universal string ops last.
//
// Picking one from the cell menu writes the result back into the cell as a
// pending edit. `informational: true` opts out of that: those produce a report
// about the value ("45 chars · 8 words"), not a replacement for it, so they copy
// and show the result instead of overwriting the column with prose.

// ── epoch / time ──────────────────────────────────────────────────────────────
/** @param {unknown} value */
function asEpoch(value) {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string' && /^\d{10,13}$/.test(value.trim())) return Number(value.trim())
  return null
}
/** @param {unknown} value */
function isEpochLike(value) {
  const n = asEpoch(value)
  if (n === null) return false
  // Plausible ~2001-2100 in seconds (1e9-4e9) or milliseconds (1e12-4e12).
  return (n > 1e9 && n < 4e9) || (n > 1e12 && n < 4e12)
}
/** @param {unknown} v */
function epochToMs(v) {
  const n = /** @type {number} */ (asEpoch(v))
  return n < 1e12 ? n * 1000 : n
}
/** @param {number} ms */
function relativeFromNow(ms) {
  const diff = ms - Date.now()
  const abs = Math.abs(diff)
  /** @type {[Intl.RelativeTimeFormatUnit, number][]} */
  const units = [['year', 31536e6], ['month', 2592e6], ['day', 864e5], ['hour', 36e5], ['minute', 6e4], ['second', 1e3]]
  const rtf = new Intl.RelativeTimeFormat('en', { numeric: 'auto' })
  for (const [unit, u] of units) {
    if (abs >= u || unit === 'second') return rtf.format(Math.round(diff / u), unit)
  }
  return 'now'
}

// ── base64 (utf-8 safe) ─────────────────────────────────────────────────────────
/** @param {string} s */
function b64urlDecode(s) {
  const pad = s.length % 4 === 0 ? '' : '='.repeat(4 - (s.length % 4))
  return utf8FromBinary(atob(s.replace(/-/g, '+').replace(/_/g, '/') + pad))
}
/** @param {string} bin latin1 byte-string */
function utf8FromBinary(bin) {
  try {
    const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0))
    return new TextDecoder('utf-8', { fatal: false }).decode(bytes)
  } catch {
    return bin
  }
}
/** @param {string} str */
function b64Encode(str) {
  const bytes = new TextEncoder().encode(str)
  let bin = ''
  for (const b of bytes) bin += String.fromCharCode(b)
  return btoa(bin)
}

// ── detection helpers ───────────────────────────────────────────────────────────
/** @param {unknown} value */
function isJwtLike(value) {
  return typeof value === 'string' && /^[\w-]+\.[\w-]+\.[\w-]+$/.test(value.trim())
}
/** @param {unknown} v */
function jsonLike(v) {
  if (typeof v === 'object' && v !== null) return true
  if (typeof v !== 'string') return false
  const t = v.trim()
  return (t.startsWith('{') && t.endsWith('}')) || (t.startsWith('[') && t.endsWith(']'))
}
/** @param {unknown} v */
function asJson(v) {
  return typeof v === 'string' ? JSON.parse(v) : v
}
/** @param {unknown} v */
function str(v) {
  return typeof v === 'string' ? v : String(v)
}
/** Deep-sort object keys for stable JSON. @param {any} v */
function sortKeys(v) {
  if (Array.isArray(v)) return v.map(sortKeys)
  if (v && typeof v === 'object') {
    /** @type {Record<string, unknown>} */
    const out = {}
    for (const k of Object.keys(v).sort()) out[k] = sortKeys(v[k])
    return out
  }
  return v
}
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
/** @param {unknown} v */
function isHexString(v) {
  if (typeof v !== 'string') return false
  const s = v.trim().replace(/^0x/i, '')
  return s.length >= 2 && s.length % 2 === 0 && /^[0-9a-fA-F]+$/.test(s)
}
/** @param {unknown} v - looks like an image URL or data URI? */
export function isImageUrl(v) {
  if (typeof v !== 'string') return false
  const s = v.trim()
  // Accept data:image URIs and any http(s) URL - the transform is opt-in per
  // column, so being lenient means it works for extension-less avatar URLs
  // (Google/GitHub/CDNs). A non-image URL just shows a broken-image tile.
  return /^data:image\//i.test(s) || /^https?:\/\/\S+$/i.test(s) ||
    // A file name with spaces in it (`…/photo 1 (5).jpg`); encoded before loading.
    /^https?:\/\/\S+(?: +\S+)*\.(jpe?g|png|gif|webp|svg|avif|bmp|ico)(\?\S*)?$/i.test(s)
}
/** @param {number} bytes */
function humanBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB', 'PB']
  let n = bytes / 1024
  let i = 0
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++ }
  return `${n.toFixed(n < 10 ? 2 : 1)} ${units[i]}`
}

export const cellTransforms = {
  id: 'cell-transforms',
  name: 'Cell Transforms',
  description: 'Decode JWT/Base64/hex/URL, epoch → date, JSON tools, case & text ops: one-click, and applicable per-column.',
  kind: 'transforms',
  transforms: [
    // ── Media (rendered as a thumbnail when set on a whole column) ────────────
    {
      id: 'avatar',
      label: 'Show as avatar',
      // Only meaningful applied to a whole column (renders a thumbnail live);
      // as a one-off cell action it would just copy the URL, so it's hidden
      // from the per-cell menu.
      columnOnly: true,
      /** @param {unknown} v */
      appliesTo: (v) => isImageUrl(v),
      /** @param {unknown} v */
      run: (v) => str(v),
    },
    {
      id: 'image-thumb',
      label: 'Show as image',
      columnOnly: true,
      /** @param {unknown} v */
      appliesTo: (v) => isImageUrl(v),
      /** @param {unknown} v */
      run: (v) => str(v),
    },

    // ── Timestamps ───────────────────────────────────────────────────────────
    {
      id: 'epoch-to-iso',
      label: 'Epoch → ISO date',
      /** @param {unknown} v */
      appliesTo: (v) => isEpochLike(v),
      /** @param {unknown} v */
      run: (v) => new Date(epochToMs(v)).toISOString(),
    },
    {
      id: 'epoch-to-local',
      label: 'Epoch → local + relative',
      // Reads as prose ('… (in 3 months)'), not a value to store.
      informational: true,
      /** @param {unknown} v */
      appliesTo: (v) => isEpochLike(v),
      /** @param {unknown} v */
      run: (v) => {
        const ms = epochToMs(v)
        return `${new Date(ms).toLocaleString()}  (${relativeFromNow(ms)})`
      },
    },
    {
      id: 'iso-to-epoch',
      label: 'Date → epoch (ms)',
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && /^\d{4}-\d{2}-\d{2}[T ]/.test(v.trim()) && !Number.isNaN(Date.parse(v)),
      /** @param {unknown} v */
      run: (v) => String(Date.parse(str(v))),
    },

    // ── Tokens / structured ───────────────────────────────────────────────────
    {
      id: 'decode-jwt',
      label: 'Decode JWT',
      // The decoded claims describe the token; they do not replace it.
      informational: true,
      /** @param {unknown} v */
      appliesTo: (v) => isJwtLike(v),
      /** @param {unknown} v */
      run: (v) => {
        const [h, p] = str(v).split('.')
        const header = JSON.parse(b64urlDecode(h))
        const payload = JSON.parse(b64urlDecode(p))
        // Surface exp/iat as readable dates alongside the raw claims.
        const notes = {}
        for (const k of ['exp', 'iat', 'nbf']) {
          if (typeof payload?.[k] === 'number') notes[k] = new Date(payload[k] * 1000).toISOString()
        }
        return JSON.stringify(Object.keys(notes).length ? { header, payload, _dates: notes } : { header, payload }, null, 2)
      },
    },
    {
      id: 'uuid-inspect',
      label: 'Inspect UUID',
      // A report about the UUID, not a different UUID.
      informational: true,
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && UUID_RE.test(v.trim()),
      /** @param {unknown} v */
      run: (v) => {
        const s = str(v).trim()
        const version = s[14]
        const variantNibble = parseInt(s[19], 16)
        const variant = variantNibble >= 8 && variantNibble <= 11 ? 'RFC 4122' : variantNibble >= 12 ? 'Microsoft' : 'NCS/reserved'
        return JSON.stringify({ uuid: s.toLowerCase(), version: Number(version), variant }, null, 2)
      },
    },

    // ── JSON ───────────────────────────────────────────────────────────────────
    {
      id: 'json-pretty',
      label: 'Pretty-print JSON',
      /** @param {unknown} v */
      appliesTo: (v) => jsonLike(v),
      /** @param {unknown} v */
      run: (v) => JSON.stringify(asJson(v), null, 2),
    },
    {
      id: 'json-minify',
      label: 'Minify JSON',
      /** @param {unknown} v */
      appliesTo: (v) => jsonLike(v),
      /** @param {unknown} v */
      run: (v) => JSON.stringify(asJson(v)),
    },
    {
      id: 'json-sort-keys',
      label: 'JSON, sort keys',
      /** @param {unknown} v */
      appliesTo: (v) => jsonLike(v),
      /** @param {unknown} v */
      run: (v) => JSON.stringify(sortKeys(asJson(v)), null, 2),
    },

    // ── Encoding ─────────────────────────────────────────────────────────────
    {
      id: 'base64-decode',
      label: 'Decode Base64',
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && /^[A-Za-z0-9+/]{8,}={0,2}$/.test(v.trim()) && v.trim().length % 4 === 0,
      /** @param {unknown} v */
      run: (v) => utf8FromBinary(atob(str(v).trim())),
    },
    {
      id: 'url-decode',
      label: 'URL decode',
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && /%[0-9a-fA-F]{2}/.test(v),
      /** @param {unknown} v */
      run: (v) => decodeURIComponent(str(v)),
    },
    {
      id: 'html-unescape',
      label: 'Unescape HTML entities',
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && /&(#\d+|#x[0-9a-fA-F]+|[a-zA-Z]+);/.test(v),
      /** @param {unknown} v */
      run: (v) => {
        const doc = new DOMParser().parseFromString(str(v), 'text/html')
        return doc.documentElement.textContent ?? str(v)
      },
    },
    {
      id: 'hex-decode',
      label: 'Hex → text',
      /** @param {unknown} v */
      appliesTo: (v) => isHexString(v) && str(v).replace(/^0x/i, '').length <= 512,
      /** @param {unknown} v */
      run: (v) => {
        const s = str(v).trim().replace(/^0x/i, '')
        const bytes = new Uint8Array(s.length / 2)
        for (let i = 0; i < bytes.length; i++) bytes[i] = parseInt(s.substr(i * 2, 2), 16)
        return new TextDecoder('utf-8', { fatal: false }).decode(bytes)
      },
    },

    // ── Numbers ─────────────────────────────────────────────────────────────
    {
      id: 'bytes-to-human',
      label: 'Bytes → human size',
      // '1.5 MB' is a rendering of the number, not the number.
      informational: true,
      /** @param {unknown} v */
      appliesTo: (v) => {
        const n = typeof v === 'number' ? v : typeof v === 'string' && /^\d+$/.test(v.trim()) ? Number(v) : NaN
        return Number.isFinite(n) && n >= 1024 && !isEpochLike(v)
      },
      /** @param {unknown} v */
      run: (v) => humanBytes(Number(str(v).trim())),
    },
    {
      id: 'dec-to-hex',
      label: 'Decimal → hex',
      /** @param {unknown} v */
      appliesTo: (v) => {
        const n = typeof v === 'number' ? v : typeof v === 'string' && /^\d+$/.test(v.trim()) ? Number(v) : NaN
        return Number.isInteger(n) && n >= 0 && !isEpochLike(v)
      },
      /** @param {unknown} v */
      run: (v) => '0x' + Number(str(v).trim()).toString(16),
    },
    {
      id: 'hex-to-dec',
      label: 'Hex → decimal',
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && /^0x[0-9a-fA-F]+$/.test(v.trim()),
      /** @param {unknown} v */
      run: (v) => String(parseInt(str(v).trim(), 16)),
    },

    // ── Case & text (conditional so they only show when meaningful) ──────────
    {
      id: 'to-upper',
      label: 'UPPERCASE',
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && /[a-z]/.test(v) && v.length <= 5000,
      /** @param {unknown} v */
      run: (v) => str(v).toUpperCase(),
    },
    {
      id: 'to-lower',
      label: 'lowercase',
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && /[A-Z]/.test(v) && v.length <= 5000,
      /** @param {unknown} v */
      run: (v) => str(v).toLowerCase(),
    },
    {
      id: 'to-title',
      label: 'Title Case',
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && /[a-zA-Z]/.test(v) && /[\s_-]/.test(v) && v.length <= 5000,
      /** @param {unknown} v */
      run: (v) => str(v).toLowerCase().replace(/(^|[\s_-])([a-z])/g, (_, sep, c) => sep + c.toUpperCase()),
    },
    {
      id: 'slugify',
      label: 'Slugify',
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && /[a-zA-Z0-9]/.test(v) && /[\sA-Z_]/.test(v) && v.length <= 500,
      /** @param {unknown} v */
      run: (v) => str(v).toLowerCase().trim().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, ''),
    },
    {
      id: 'trim',
      label: 'Trim whitespace',
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && v !== v.trim(),
      /** @param {unknown} v */
      run: (v) => str(v).trim(),
    },

    // ── Universal string ops (listed last) ───────────────────────────────────
    {
      id: 'count',
      label: 'Count chars / words / bytes',
      // Pure measurement - there is nothing here to write back.
      informational: true,
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && v.length > 0,
      /** @param {unknown} v */
      run: (v) => {
        const s = str(v)
        const words = s.trim() ? s.trim().split(/\s+/).length : 0
        const bytes = new TextEncoder().encode(s).length
        return `${s.length} chars · ${words} words · ${bytes} bytes`
      },
    },
    {
      id: 'base64-encode',
      label: 'Encode Base64',
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && v.length > 0 && v.length <= 5000,
      /** @param {unknown} v */
      run: (v) => b64Encode(str(v)),
    },
    {
      id: 'url-encode',
      label: 'URL encode',
      /** @param {unknown} v */
      appliesTo: (v) => typeof v === 'string' && v.length > 0 && v.length <= 5000 && /[^A-Za-z0-9\-_.~]/.test(v) && !/%[0-9a-fA-F]{2}/.test(v),
      /** @param {unknown} v */
      run: (v) => encodeURIComponent(str(v)),
    },
  ],
}
