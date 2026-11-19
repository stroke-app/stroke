// The Custom theme: built in the theme builder (ThemeBuilder.svelte) from a
// mode, one to three colours, an intensity and a grain amount, and turned into
// the same token set every hand-written theme in app-themes.css defines.
//
// The tokens are generated, injected as one <style> under
// html[data-theme="custom"], and cached in localStorage with the light/dark
// flag, so index.html can put them back before the bundle loads and the first
// frame is already the right colours.
//
// Colour roles, the way Arc reads its gradient stops:
//   colour 1 - the surfaces' tint and, alone, the accent
//   colour 2 - the accent (buttons, focus, links, selection)
//   colour 3 - the sidebar's tint, so the chrome and the canvas differ in hue
//
// Grain is separate from the colours and applies whatever theme is active: a
// fixed SVG-turbulence layer over the window at the chosen opacity.
import { writable, get } from 'svelte/store'

/** @typedef {'auto' | 'light' | 'dark'} CustomMode */
/** A colour on the builder's pad: hue in degrees, strength 0..1. @typedef {{ h: number, s: number }} PadColor */
/**
 * `alpha` 1 is a solid window; below it the window turns to glass (macOS).
 * `surface`: the ladder the surfaces sit on. `matte` is softer and flatter (less
 * contrast between layers, less colour); `black` drops the dark ladder to true
 * black for OLED screens and a high-contrast look.
 * @typedef {'default' | 'matte' | 'black'} CustomSurface
 * @typedef {{ mode: CustomMode, colors: PadColor[], intensity: number, noise: number, alpha: number, surface?: CustomSurface }} CustomTheme
 */

const KEY = 'stroke:custom-theme'
const CSS_KEY = 'stroke:custom-theme-css'
export const MAX_COLORS = 3

/** @type {CustomTheme} */
export const DEFAULT_CUSTOM = { mode: 'auto', colors: [{ h: 265, s: 0.55 }], intensity: 0.5, noise: 0, alpha: 1, surface: /** @type {CustomSurface} */ ('default') }

/** Below this the Custom theme is translucent (the window's glass material shows). */
export const ALPHA_GLASS_BELOW = 0.98
/** @param {CustomTheme} [t] */
export const customIsGlass = (t = get(customTheme)) => t.alpha < ALPHA_GLASS_BELOW

/** Preset swatches for the builder's row, as pad colours. */
export const PRESETS = [
  // Surfaces first: these set the ladder as well as the colour.
  { name: 'Black', h: 0, s: 0, surface: 'black' },
  { name: 'Matte', h: 250, s: 0.06, surface: 'matte' },
  { name: 'Graphite', h: 250, s: 0.04 },
  { name: 'Stone', h: 70, s: 0.08 },
  { name: 'Cream', h: 70, s: 0.18 },
  { name: 'Sand', h: 80, s: 0.3 },
  { name: 'Coffee', h: 55, s: 0.38 },
  { name: 'Rose', h: 10, s: 0.38 },
  { name: 'Pink', h: 350, s: 0.45 },
  { name: 'Mauve', h: 320, s: 0.32 },
  { name: 'Red', h: 22, s: 0.7 },
  { name: 'Orange', h: 48, s: 0.72 },
  { name: 'Yellow', h: 92, s: 0.65 },
  { name: 'Green', h: 152, s: 0.62 },
  { name: 'Mint', h: 165, s: 0.35 },
  { name: 'Olive', h: 115, s: 0.4 },
  { name: 'Teal', h: 190, s: 0.55 },
  { name: 'Sky', h: 232, s: 0.5 },
  { name: 'Indigo', h: 270, s: 0.45 },
  { name: 'Violet', h: 300, s: 0.6 },
  { name: 'Ocean', h: 215, s: 0.65 },
  { name: 'Slate', h: 255, s: 0.12 },
]

const clamp = (/** @type {number} */ v, lo = 0, hi = 1) => Math.min(hi, Math.max(lo, v))

/** @param {any} raw @returns {CustomTheme} */
function normalize(raw) {
  const mode = raw?.mode === 'light' || raw?.mode === 'dark' ? raw.mode : 'auto'
  const colors = Array.isArray(raw?.colors)
    ? raw.colors
        .filter((c) => c && Number.isFinite(c.h) && Number.isFinite(c.s))
        .slice(0, MAX_COLORS)
        .map((c) => ({ h: ((c.h % 360) + 360) % 360, s: clamp(c.s) }))
    : []
  return {
    mode,
    colors: colors.length ? colors : DEFAULT_CUSTOM.colors.map((c) => ({ ...c })),
    intensity: Number.isFinite(raw?.intensity) ? clamp(raw.intensity) : DEFAULT_CUSTOM.intensity,
    noise: Number.isFinite(raw?.noise) ? clamp(raw.noise) : 0,
    alpha: Number.isFinite(raw?.alpha) ? clamp(raw.alpha, 0.3, 1) : 1,
    surface: raw?.surface === 'matte' || raw?.surface === 'black' ? raw.surface : 'default',
  }
}

function load() {
  try {
    return normalize(JSON.parse(localStorage.getItem(KEY) ?? 'null'))
  } catch {
    return normalize(null)
  }
}

export const customTheme = writable(typeof localStorage === 'undefined' ? normalize(null) : load())

/** Persist a finished edit (the builder previews without saving until then). @param {CustomTheme} t */
export function saveCustomTheme(t) {
  const n = normalize(t)
  customTheme.set(n)
  try { localStorage.setItem(KEY, JSON.stringify(n)) } catch {}
  return n
}

/** Throw away an unsaved preview. */
export function revertCustomTheme() {
  customTheme.set(typeof localStorage === 'undefined' ? normalize(null) : load())
}

/** @param {CustomTheme} [t] */
export function customIsDark(t = get(customTheme)) {
  if (t.mode === 'dark') return true
  if (t.mode === 'light') return false
  try {
    return window.matchMedia?.('(prefers-color-scheme: dark)').matches !== false
  } catch {
    return true
  }
}

/** One oklch() string. @param {number} l @param {number} c @param {number} h @param {number} [a] */
const ok = (l, c, h, a) => `oklch(${l.toFixed(3)} ${c.toFixed(3)} ${h.toFixed(1)}${a === undefined ? '' : ` / ${a}`})`

/** @type {Map<string, string>} */
const _hex = new Map()
/** An oklch colour as #rrggbb, for the few consumers that need hex (the code editor). */
function toHex(/** @type {string} */ css) {
  const hit = _hex.get(css)
  if (hit) return hit
  const out = toHexUncached(css)
  if (_hex.size > 512) _hex.clear()
  _hex.set(css, out)
  return out
}
/** @type {CanvasRenderingContext2D | null} */
let _hexCtx = null
function toHexUncached(/** @type {string} */ css) {
  try {
    if (!_hexCtx) {
      const cv = document.createElement('canvas')
      cv.width = cv.height = 1
      _hexCtx = cv.getContext('2d', { willReadFrequently: true })
    }
    const cx = _hexCtx
    if (!cx) return '#1c1c1c'
    cx.fillStyle = '#000'
    cx.fillStyle = css
    cx.fillRect(0, 0, 1, 1)
    const [r, g, b] = cx.getImageData(0, 0, 1, 1).data
    return '#' + [r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('')
  } catch {
    return '#1c1c1c'
  }
}

/**
 * The token set for a custom theme.
 *
 * Surfaces keep the lightness ladder of the base Studio themes, so contrast
 * holds whatever colour is picked; only their chroma and hue move. Intensity
 * scales how much colour the surfaces carry, from a trace to a clearly tinted
 * canvas; the accent always has enough chroma to read as a colour.
 * @param {CustomTheme} t
 * @returns {Record<string, string>}
 */
export function customTokens(t, dark = customIsDark(t)) {
  const [c1, c2 = c1, c3 = c1] = t.colors
  const i = t.intensity
  // Chroma the surfaces carry: zero for a grey pick, up to ~0.055 (dark) for a
  // saturated pick at full intensity - tinted, never neon.
  const tint = (/** @type {PadColor} */ c, k = 1) => c.s * (dark ? 0.008 + 0.05 * i : 0.006 + 0.035 * i) * k
  const h = c1.h, ha = c2.h, hs = c3.h
  const accentC = Math.max(0.1, Math.min(0.21, 0.08 + c2.s * 0.15))
  const surface = t.surface ?? 'default'
  // Surfaces on the chosen ladder. Black (dark only) pulls every layer down by
  // the background's own lightness, so the canvas is true black and the
  // layers above keep their steps; matte compresses the steps and the colour.
  const base = dark ? 0.135 : 0.965
  /** @param {number} l @param {number} c @param {number} hh */
  const S = (l, c, hh) => {
    if (surface === 'black' && dark) return ok(Math.max(0, l - 0.135), c * 0.5, hh)
    if (surface === 'matte') return ok(base + (l - base) * 0.6 + (dark ? 0.035 : -0.02), c * 0.45, hh)
    return ok(l, c, hh)
  }
  /** Text: a touch softer on matte so it does not glare off the flat ground. @param {number} l @param {number} c @param {number} hh */
  const F = (l, c, hh) => ok(surface === 'matte' ? (dark ? l - 0.05 : l + 0.04) : l, c, hh)

  if (dark) {
    const primary = ok(0.68, accentC, ha)
    return {
      '--background': S(0.135 + 0.025 * i * c1.s, tint(c1), h),
      '--foreground': F(0.95, tint(c1, 0.25), h),
      '--card': S(0.185, tint(c1), h),
      '--card-foreground': F(0.95, tint(c1, 0.25), h),
      '--popover': S(0.18, tint(c1), h),
      '--popover-foreground': F(0.95, tint(c1, 0.25), h),
      '--primary': primary,
      '--primary-foreground': ok(0.16, 0.02, ha),
      '--secondary': S(0.235, tint(c1), h),
      '--secondary-foreground': F(0.9, tint(c1, 0.3), h),
      '--muted': S(0.235, tint(c1), h),
      '--muted-foreground': F(0.73, tint(c1, 0.6), h),
      '--accent': S(0.27, tint(c1, 1.1), h),
      '--accent-foreground': ok(0.97, tint(c1, 0.25), h),
      '--destructive': ok(0.65, 0.2, 25),
      '--border': S(0.29, tint(c1, 0.9), h),
      '--input': S(0.31, tint(c1, 0.9), h),
      '--ring': primary,
      '--sidebar': S(0.165 + 0.02 * i * c3.s, tint(c3, 1.15), hs),
      '--sidebar-foreground': F(0.9, tint(c3, 0.3), hs),
      '--sidebar-accent': S(0.26, tint(c3, 1.1), hs),
      '--sidebar-accent-foreground': ok(0.98, tint(c3, 0.25), hs),
      '--sidebar-border': S(0.27, tint(c3, 0.9), hs),
      '--panel': S(0.135 + 0.025 * i * c1.s, tint(c1), h),
      '--table-grid': S(0.29, tint(c1, 0.9), h),
      '--link': ok(0.77, Math.min(0.14, accentC), ha),
      '--link-hover': ok(0.84, Math.min(0.11, accentC), ha),
      '--editor-surface': toHex(S(0.17, tint(c1), h)),
      '--editor-line-highlight': toHex(S(0.21, tint(c1), h)),
    }
  }
  const primary = ok(0.52, accentC, ha)
  return {
    '--background': S(0.965 - 0.02 * i * c1.s, tint(c1), h),
    '--foreground': F(0.17, tint(c1, 0.6), h),
    '--card': S(0.995, tint(c1, 0.3), h),
    '--card-foreground': F(0.17, tint(c1, 0.6), h),
    '--popover': S(0.995, tint(c1, 0.3), h),
    '--popover-foreground': F(0.17, tint(c1, 0.6), h),
    '--primary': primary,
    '--primary-foreground': ok(0.99, 0, 0),
    '--secondary': S(0.935, tint(c1), h),
    '--secondary-foreground': F(0.23, tint(c1, 0.6), h),
    '--muted': S(0.935, tint(c1), h),
    '--muted-foreground': F(0.46, tint(c1, 0.8), h),
    '--accent': S(0.915, tint(c1, 1.2), h),
    '--accent-foreground': ok(0.17, tint(c1, 0.6), h),
    '--destructive': ok(0.55, 0.22, 25),
    '--border': S(0.86, tint(c1, 1.1), h),
    '--input': S(0.845, tint(c1, 1.1), h),
    '--ring': primary,
    '--sidebar': S(0.945 - 0.025 * i * c3.s, tint(c3, 1.3), hs),
    '--sidebar-foreground': F(0.24, tint(c3, 0.6), hs),
    '--sidebar-accent': S(0.9, tint(c3, 1.3), hs),
    '--sidebar-accent-foreground': ok(0.14, tint(c3, 0.6), hs),
    '--sidebar-border': S(0.865, tint(c3, 1.1), hs),
    '--panel': S(0.965 - 0.02 * i * c1.s, tint(c1), h),
    '--table-grid': S(0.87, tint(c1, 1.1), h),
    '--link': ok(0.5, Math.min(0.17, accentC), ha),
    '--link-hover': ok(0.42, Math.min(0.17, accentC), ha),
    '--editor-surface': toHex(S(0.985, tint(c1, 0.4), h)),
    '--editor-line-highlight': toHex(S(0.95, tint(c1, 0.6), h)),
  }
}

/** The pad colour as a visible swatch (what the handle and the swatch row show). @param {PadColor} c */
export function swatchCss(c) {
  return ok(0.62 + (1 - c.s) * 0.25, 0.02 + c.s * 0.17, c.h)
}

/**
 * The translucent overrides for a Custom theme with alpha below 1, applied
 * only while the window really has its glass material (`data-glass`, set by
 * settings.js on macOS). The body carries one tint in the theme's own colour,
 * at the chosen opacity, with a soft top sheen; the layout surfaces above it
 * go nearly clear so their stack never turns solid (see the Glass themes).
 * @param {CustomTheme} t @param {boolean} dark
 */
function glassCss(t, dark) {
  if (!customIsGlass(t)) return ''
  const tok = customTokens(t, dark)
  const bg = tok['--background'].replace(/\)$/, ` / ${t.alpha.toFixed(3)})`)
  const sheen = dark ? 'rgba(255,255,255,0.07)' : 'rgba(255,255,255,0.45)'
  const ink = dark ? '1 0 0' : '0 0 0'
  return `
html[data-theme="custom"][data-glass] {
  --background: ${tok['--background'].replace(/\)$/, ' / 0.06)')};
  --panel: ${tok['--panel'].replace(/\)$/, ' / 0.03)')};
  --sidebar: ${tok['--sidebar'].replace(/\)$/, ` / ${(t.alpha * 0.35).toFixed(3)})`)};
  --muted: oklch(${ink} / 0.07);
  --secondary: oklch(${ink} / 0.07);
  --accent: oklch(${ink} / 0.09);
  --sidebar-accent: oklch(${ink} / 0.09);
  --border: oklch(${ink} / 0.10);
  --sidebar-border: oklch(${ink} / 0.08);
  --table-grid: oklch(${ink} / 0.08);
  --input: oklch(${ink} / 0.14);
  background: transparent !important;
}
html[data-theme="custom"][data-glass] body {
  background: linear-gradient(180deg, ${sheen}, transparent 38%), ${bg} !important;
}`
}

/**
 * Write the custom theme's stylesheet and the grain level onto the document.
 * Called on every apply (settings.js) and, once a frame, while the builder is
 * open. `persist` caches it for the next launch's first frame - skipped while
 * previewing, where a storage write per pointer move was half the lag.
 * @param {CustomTheme} [t]
 * @param {{ persist?: boolean }} [opts]
 */
export function applyCustomTheme(t = get(customTheme), { persist = true } = {}) {
  if (typeof document === 'undefined') return
  const dark = customIsDark(t)
  const body = Object.entries(customTokens(t, dark)).map(([k, v]) => `  ${k}: ${v};`).join('\n')
  const css = `html[data-theme="custom"] {\n${body}\n}${glassCss(t, dark)}`
  let el = /** @type {HTMLStyleElement | null} */ (document.getElementById('stroke-custom-theme'))
  if (!el) {
    el = document.createElement('style')
    el.id = 'stroke-custom-theme'
    document.head.appendChild(el)
  }
  if (el.textContent !== css) el.textContent = css
  applyNoise(t.noise)
  if (persist) {
    try { localStorage.setItem(CSS_KEY, JSON.stringify({ css, dark, noise: t.noise })) } catch {}
  }
}

/** Grain over the window, any theme. 0 removes the layer. @param {number} amount 0..1 */
export function applyNoise(amount) {
  const root = document.documentElement
  if (amount > 0.001) {
    root.dataset.noise = ''
    // Up to 0.32: past that the grain stops reading as texture and starts
    // reading as dirt on the screen.
    root.style.setProperty('--noise-opacity', String(+(amount * 0.32).toFixed(3)))
  } else {
    delete root.dataset.noise
    root.style.removeProperty('--noise-opacity')
  }
}

// ── Share codes ─────────────────────────────────────────────────────────────
// A Custom theme as one copyable line: `stroke-theme:` + base64url of a compact
// array, so it survives chat apps and URLs (no + / = to mangle). Compact keys
// keep a three-colour theme around 60 characters.
const SHARE_PREFIX = 'stroke-theme:'

/** @param {CustomTheme} t */
export function encodeCustomTheme(t) {
  const n = normalize(t)
  const packed = [1, n.mode[0], n.colors.map((c) => [Math.round(c.h), Math.round(c.s * 100)]), Math.round(n.intensity * 100), Math.round(n.noise * 100), Math.round(n.alpha * 100), n.surface === 'default' ? 0 : n.surface === 'matte' ? 1 : 2]
  const b64 = btoa(JSON.stringify(packed)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
  return SHARE_PREFIX + b64
}

/**
 * A share code back into a theme, or null when it is not one. Accepts the code
 * with or without its prefix and with surrounding whitespace (pasted text).
 * @param {string} code
 * @returns {CustomTheme | null}
 */
export function decodeCustomTheme(code) {
  try {
    let raw = String(code ?? '').trim()
    if (raw.startsWith(SHARE_PREFIX)) raw = raw.slice(SHARE_PREFIX.length)
    raw = raw.replace(/-/g, '+').replace(/_/g, '/')
    while (raw.length % 4) raw += '='
    const p = JSON.parse(atob(raw))
    if (!Array.isArray(p) || p[0] !== 1 || !Array.isArray(p[2])) return null
    const mode = { a: 'auto', l: 'light', d: 'dark' }[/** @type {'a'|'l'|'d'} */ (p[1])] ?? 'auto'
    return normalize({
      mode,
      colors: p[2].map((/** @type {number[]} */ c) => ({ h: Number(c[0]), s: Number(c[1]) / 100 })),
      intensity: Number(p[3]) / 100,
      noise: Number(p[4]) / 100,
      alpha: Number(p[5]) / 100,
      surface: p[6] === 1 ? 'matte' : p[6] === 2 ? 'black' : 'default',
    })
  } catch {
    return null
  }
}
