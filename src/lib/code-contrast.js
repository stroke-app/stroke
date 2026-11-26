/**
 * Make highlighted code readable on Stroke's surfaces.
 *
 * Shiki's vitesse themes were drawn for their own backgrounds. On a dialog or
 * popover in a dark theme their keyword green measured 3.42:1 and their
 * punctuation grey 1.68:1. Every token colour in the HTML is checked against
 * the worst surface the code can sit on for that theme (a light dark grey, or
 * a light grey) and, when it is under the target, mixed toward white or black
 * by the least amount that reaches it. The hue stays; colours that already
 * pass are left exactly as they were.
 */

const lin = (/** @type {number} */ c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4)
/** @param {[number, number, number]} rgb 0-255 */
const lum = ([r, g, b]) => 0.2126 * lin(r / 255) + 0.7152 * lin(g / 255) + 0.0722 * lin(b / 255)
/** @param {[number, number, number]} a @param {[number, number, number]} b */
export const contrastRatio = (a, b) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p)
  return (x + 0.05) / (y + 0.05)
}
/** @param {string} hex */
const parse = (hex) => /** @type {[number, number, number]} */ ([0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16)))
/** @param {[number, number, number]} rgb */
const toHex = (rgb) => rgb.map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, '0')).join('')

/** The surfaces checked against: the lightest a dark theme's popover gets, the darkest a light theme's. */
const WORST_BG = { dark: /** @type {[number, number, number]} */ ([0x2a, 0x2a, 0x2e]), light: /** @type {[number, number, number]} */ ([0xee, 0xef, 0xf2]) }
export const CODE_CONTRAST_MIN = 4.6

/** @type {Map<string, string>} */
const fixed = new Map()

/**
 * A 6-digit hex colour (no #) that reads at CODE_CONTRAST_MIN on that kind of
 * surface: the same colour when it already does.
 * @param {string} hex @param {'dark' | 'light'} mode
 */
export function readableHex(hex, mode) {
  const key = `${mode}:${hex.toLowerCase()}`
  const hit = fixed.get(key)
  if (hit) return hit
  const bg = WORST_BG[mode]
  const c = parse(hex)
  let out = hex
  if (contrastRatio(c, bg) < CODE_CONTRAST_MIN) {
    const toward = mode === 'dark' ? 255 : 0
    for (let t = 0.05; t <= 1.0001; t += 0.05) {
      const m = /** @type {[number, number, number]} */ (c.map((v) => v + (toward - v) * t))
      if (contrastRatio(m, bg) >= CODE_CONTRAST_MIN) { out = toHex(m); break }
    }
  }
  fixed.set(key, out)
  return out
}

/**
 * Rewrite every `color:#rrggbb` in highlighter HTML to its readable version.
 * @param {string} html @param {'dark' | 'light'} mode
 */
export function readableCodeHtml(html, mode) {
  return html.replace(/(?<![-\w])(color:\s*#)([0-9a-fA-F]{6})\b/g, (_, pre, hex) => pre + readableHex(hex, mode))
}
