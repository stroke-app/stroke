// Theme files: a Stroke theme (stroke.click/themes) or a VS Code colour theme,
// read into one shape and turned into the token set every built-in theme in
// app-themes.css defines. Three colours are required (background, foreground,
// primary); every other token is derived from them unless the file sets it.
//
// Stroke theme, version 1:
//   { "version": 1, "name": "Aurora", "author": "...", "appearance": "dark",
//     "colors": { "background": "#0f1117", "foreground": "#e6e6e6",
//                 "primary": "#7aa2f7", "sidebar"?, "border"?, "muted"?, ... } }

/** @typedef {'light' | 'dark'} Appearance */
/**
 * @typedef {{ id: string, name: string, author?: string, appearance: Appearance, colors: Record<string, string> }} StrokeTheme
 */

/** Tokens a file may set directly, beyond the three required. */
export const THEME_COLOR_KEYS = [
  'background', 'foreground', 'primary', 'primaryForeground', 'card', 'popover', 'muted', 'mutedForeground',
  'accent', 'border', 'input', 'ring', 'sidebar', 'sidebarForeground', 'sidebarAccent', 'sidebarBorder',
  'destructive', 'link', 'tableGrid', 'editorSurface', 'editorLineHighlight',
]

const HEX = /^#(?:[0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/i
const FN = /^(?:rgb|rgba|hsl|hsla|oklch|oklab|lab|lch|color)\([^;{}<>]{1,120}\)$/i

/** A colour safe to put in a stylesheet, or null. @param {unknown} v */
export function safeColor(v) {
  const s = typeof v === 'string' ? v.trim() : ''
  return HEX.test(s) || FN.test(s) ? s : null
}

/** @param {string} s */
function slug(s) {
  return String(s).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 48) || 'theme'
}

/** Relative luminance of a hex colour (0 black, 1 white), or null. @param {string} c */
function luminance(c) {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})/i.exec(c)
  if (!m) return null
  const h = m[1].length === 3 ? m[1].split('').map((x) => x + x).join('') : m[1]
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4))
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

/**
 * Read a theme file's JSON (Stroke or VS Code). Throws with a message a
 * person can act on.
 * @param {unknown} raw parsed JSON
 * @returns {StrokeTheme}
 */
export function parseThemeFile(raw) {
  if (!raw || typeof raw !== 'object') throw new Error('A theme file is a JSON object.')
  const o = /** @type {Record<string, any>} */ (raw)
  /** @type {Record<string, string>} */
  const colors = {}

  // VS Code: `colors` keyed by workbench ids, `type` light/dark.
  const vs = o.colors && typeof o.colors === 'object' && ('editor.background' in o.colors || 'sideBar.background' in o.colors)
  if (vs) {
    const c = o.colors
    const pick = (/** @type {string[]} */ ...keys) => keys.map((k) => safeColor(c[k])).find(Boolean) ?? null
    const map = /** @type {Record<string, string | null>} */ ({
      background: pick('editor.background'),
      foreground: pick('editor.foreground', 'foreground'),
      primary: pick('button.background', 'focusBorder', 'textLink.foreground', 'activityBarBadge.background'),
      primaryForeground: pick('button.foreground'),
      sidebar: pick('sideBar.background', 'activityBar.background'),
      sidebarForeground: pick('sideBar.foreground'),
      border: pick('panel.border', 'editorGroup.border', 'sideBar.border', 'contrastBorder'),
      card: pick('editorWidget.background', 'dropdown.background'),
      popover: pick('menu.background', 'editorWidget.background'),
      input: pick('input.background'),
      muted: pick('list.hoverBackground', 'editorWidget.background'),
      mutedForeground: pick('descriptionForeground', 'editorLineNumber.foreground'),
      accent: pick('list.activeSelectionBackground', 'list.hoverBackground'),
      link: pick('textLink.foreground'),
      destructive: pick('errorForeground', 'editorError.foreground'),
      editorSurface: pick('editor.background'),
      editorLineHighlight: pick('editor.lineHighlightBackground'),
    })
    for (const [k, v] of Object.entries(map)) if (v) colors[k] = v
  } else if (o.colors && typeof o.colors === 'object') {
    for (const k of THEME_COLOR_KEYS) {
      const v = safeColor(o.colors[k])
      if (v) colors[k] = v
    }
  }

  for (const k of ['background', 'foreground', 'primary']) {
    if (!colors[k]) throw new Error(`The theme needs a "${k}" colour (hex like #1e1e2e, or rgb()/hsl()/oklch()).`)
  }
  const name = String(o.name ?? o.label ?? 'Imported theme').trim().slice(0, 60) || 'Imported theme'
  const declared = String(o.appearance ?? o.type ?? '').toLowerCase()
  const lum = luminance(colors.background)
  /** @type {Appearance} */
  const appearance = declared.includes('light') ? 'light' : declared.includes('dark') ? 'dark' : lum !== null && lum > 0.45 ? 'light' : 'dark'
  const author = typeof o.author === 'string' ? o.author.slice(0, 60) : typeof o.publisher === 'string' ? o.publisher.slice(0, 60) : undefined
  return { id: slug(name), name, author, appearance, colors }
}

/**
 * The CSS custom properties for a theme: what the file sets, the rest mixed
 * from background, foreground and primary in the theme's own direction.
 * @param {StrokeTheme} t
 * @returns {Record<string, string>}
 */
export function themeTokens(t) {
  const c = t.colors
  const dark = t.appearance === 'dark'
  const bg = c.background
  const fg = c.foreground
  const pr = c.primary
  const mix = (/** @type {string} */ a, /** @type {string} */ b, /** @type {number} */ p) => `color-mix(in oklch, ${a} ${p}%, ${b})`
  // A step "up" from the background: lighter on dark themes, darker on light.
  const step = (/** @type {number} */ p) => mix(fg, bg, p)
  return {
    '--background': bg,
    '--foreground': fg,
    '--card': c.card ?? step(dark ? 5 : 2),
    '--card-foreground': fg,
    '--popover': c.popover ?? step(dark ? 6 : 1),
    '--popover-foreground': fg,
    '--primary': pr,
    '--primary-foreground': c.primaryForeground ?? (dark ? mix('#000', pr, 70) : '#ffffff'),
    '--secondary': c.muted ?? step(dark ? 9 : 6),
    '--secondary-foreground': fg,
    '--muted': c.muted ?? step(dark ? 9 : 6),
    '--muted-foreground': c.mutedForeground ?? mix(fg, bg, dark ? 62 : 58),
    '--accent': c.accent ?? step(dark ? 11 : 7),
    '--accent-foreground': fg,
    '--destructive': c.destructive ?? (dark ? 'oklch(0.65 0.2 25)' : 'oklch(0.55 0.22 25)'),
    '--border': c.border ?? step(dark ? 13 : 11),
    '--input': c.input ?? step(dark ? 15 : 13),
    '--ring': c.ring ?? pr,
    '--sidebar': c.sidebar ?? step(dark ? 3 : 3),
    '--sidebar-foreground': c.sidebarForeground ?? mix(fg, bg, 88),
    '--sidebar-accent': c.sidebarAccent ?? step(dark ? 10 : 7),
    '--sidebar-accent-foreground': fg,
    '--sidebar-border': c.sidebarBorder ?? c.border ?? step(dark ? 13 : 11),
    '--panel': bg,
    '--table-grid': c.tableGrid ?? step(dark ? 10 : 8),
    '--link': c.link ?? pr,
    '--link-hover': mix(c.link ?? pr, fg, 75),
    '--editor-surface': c.editorSurface ?? bg,
    '--editor-line-highlight': c.editorLineHighlight ?? step(dark ? 5 : 4),
  }
}

/** The stylesheet that applies a theme under html[data-theme="imported"]. @param {StrokeTheme} t */
export function themeCss(t) {
  const body = Object.entries(themeTokens(t)).map(([k, v]) => `  ${k}: ${v};`).join('\n')
  return `html[data-theme="imported"] {\n${body}\n  color-scheme: ${t.appearance};\n}`
}

/** A theme as a Stroke theme file (the format stroke.click/themes serves). @param {StrokeTheme} t */
export function toThemeFile(t) {
  return { version: 1, name: t.name, ...(t.author ? { author: t.author } : {}), appearance: t.appearance, colors: t.colors }
}
