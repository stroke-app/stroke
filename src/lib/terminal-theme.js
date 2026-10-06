/**
 * xterm.js colours from the app theme, so the terminal sits on the same surface
 * as the SQL editor and its ANSI palette is the theme's own: red is
 * `--destructive`, and green, yellow, blue and magenta are the colours the
 * editor gives strings, numbers, names and keywords. psql's prompt, the error
 * highlighter (terminal-highlight.js) and every client's own colours all draw
 * from these, so they change with the theme like the rest of the app.
 *
 * xterm needs concrete colours and the tokens are oklch, so each one is
 * resolved on a probe element and rasterized to sRGB.
 */

/** @type {CanvasRenderingContext2D | null} */
let ctx = null

/**
 * A CSS colour expression (a token, `var()`, `color-mix()`) as [r, g, b], or
 * null when it does not resolve here.
 * @param {string} expr
 * @param {HTMLElement} host where the probe goes, so scoped tokens apply
 * @returns {[number, number, number] | null}
 */
function rgbOf(expr, host) {
  const probe = document.createElement('span')
  probe.style.cssText = 'position:absolute;visibility:hidden;pointer-events:none'
  host.appendChild(probe)
  probe.style.color = 'var(--stroke-unset-token)'
  const unset = getComputedStyle(probe).color
  probe.style.color = expr
  const color = getComputedStyle(probe).color
  probe.remove()
  // An unknown token makes the declaration invalid at computed-value time, and
  // the probe then inherits: the same colour as the deliberately unset one.
  if (!color || color === unset) return null
  if (!ctx) {
    const canvas = document.createElement('canvas')
    canvas.width = canvas.height = 1
    ctx = canvas.getContext('2d', { willReadFrequently: true })
  }
  if (!ctx) return null
  ctx.clearRect(0, 0, 1, 1)
  ctx.fillStyle = color
  ctx.fillRect(0, 0, 1, 1)
  const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data
  return [r, g, b]
}

/** @param {[number, number, number]} rgb @param {number} [alpha] 0..1 */
function hex([r, g, b], alpha) {
  const h = (/** @type {number} */ n) => n.toString(16).padStart(2, '0')
  return `#${h(r)}${h(g)}${h(b)}${alpha === undefined ? '' : h(Math.round(alpha * 255))}`
}

/** Relative luminance, 0 (black) to 1 (white). */
function luminance(/** @type {[number, number, number]} */ [r, g, b]) {
  const lin = (/** @type {number} */ c) => {
    const s = c / 255
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/**
 * @param {HTMLElement} host the terminal's container
 * @returns {import('@xterm/xterm').ITheme}
 */
export function terminalTheme(host) {
  // The SQL editor's surface, so the two read as one family.
  const bg = rgbOf('var(--panel, var(--background))', host) ?? [10, 10, 10]
  const dark = luminance(bg) < 0.25
  /** @param {string} expr @param {[number, number, number]} fallback */
  const rgb = (expr, fallback) => rgbOf(expr, host) ?? fallback
  const fg = rgb('var(--foreground)', dark ? [229, 229, 229] : [26, 26, 26])
  const muted = rgb('var(--muted-foreground)', dark ? [160, 160, 160] : [100, 100, 100])
  const surface = rgb('var(--muted)', dark ? [38, 38, 38] : [235, 235, 235])
  const accent = rgb('var(--primary)', fg)

  const red = hex(rgb('var(--destructive)', [229, 72, 77]))
  const green = hex(rgb('var(--json-string)', [74, 180, 110]))
  const yellow = hex(rgb('var(--json-number)', [214, 160, 60]))
  const blue = hex(rgb('var(--json-key)', [90, 150, 230]))
  const magenta = hex(rgb('var(--json-boolean)', [180, 110, 220]))
  // No token is cyan; halfway between the success green and the info blue is.
  const cyan = hex(rgb('color-mix(in oklch, var(--success), var(--info))', [70, 180, 190]))

  return {
    background: hex(bg),
    foreground: hex(fg),
    cursor: hex(fg),
    cursorAccent: hex(bg),
    selectionBackground: hex(accent, dark ? 0.32 : 0.22),
    selectionInactiveBackground: hex(muted, 0.2),
    scrollbarSliderBackground: hex(muted, 0.18),
    scrollbarSliderHoverBackground: hex(muted, 0.32),
    scrollbarSliderActiveBackground: hex(muted, 0.45),
    // ANSI black and white are the ends of the theme's own range: on a dark
    // theme black is a raised surface and white the text; on a light one the
    // other way round, so "white" text stays readable on the light canvas.
    black: dark ? hex(surface) : hex(fg),
    white: dark ? hex(fg) : hex(muted),
    brightBlack: hex(muted),
    brightWhite: hex(fg),
    red, green, yellow, blue, magenta, cyan,
    brightRed: red,
    brightGreen: green,
    brightYellow: yellow,
    brightBlue: blue,
    brightMagenta: magenta,
    brightCyan: cyan,
  }
}

/**
 * The editor's font as xterm wants it: the family stack and a pixel size.
 * @param {HTMLElement} host styled with the editor's font-family and font-size
 */
export function terminalFont(host) {
  const style = getComputedStyle(host)
  return { fontFamily: style.fontFamily, fontSize: Math.round(parseFloat(style.fontSize)) || 13 }
}
