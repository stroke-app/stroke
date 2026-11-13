/**
 * The real colours of each app theme, for swatches in the theme pickers.
 *
 * Read from the stylesheet, not copied: the palettes live on
 * `html[data-theme=…]` rules, so a `data-theme` attribute on any other element
 * matches nothing. Instead the custom properties of each theme's rule (on top
 * of the shared `:root` tokens, and `.dark` for dark themes, which is where the
 * status hues change) are copied onto one hidden probe element, and the browser
 * resolves each token there. The page theme is never touched, and the result is
 * cached for the session.
 */
import { APP_THEMES, getThemeDefinition } from '$lib/themes/registry.js'

/** @typedef {import('$lib/themes/registry.js').ThemeId} ThemeId */
/** @typedef {{ key: string, label: string, color: string }} Swatch */

/** Token per swatch, in strip order: surface, text, accent, then the status hues. */
const SWATCH_TOKENS = /** @type {const} */ ([
  ['background', 'Background'],
  ['foreground', 'Text'],
  ['primary', 'Accent'],
  ['destructive', 'Red'],
  ['warning', 'Yellow'],
  ['success', 'Green'],
])

const THEME_SELECTOR = /^html\[data-theme=["']?([\w-]+)["']?\]$/
const ROOT_SELECTOR = /^:root(\s*,\s*:host)?$/

/** @type {Map<ThemeId, Swatch[]> | null} */
let cache = null

/**
 * @param {CSSStyleDeclaration} style
 * @param {Record<string, string>} into
 */
function copyCustomProps(style, into) {
  for (let i = 0; i < style.length; i++) {
    const name = style[i]
    if (name.startsWith('--')) into[name] = style.getPropertyValue(name).trim()
  }
}

/** Custom properties declared on `:root`, `.dark`, and each `html[data-theme]`. */
function collectTokenRules() {
  /** @type {Record<string, string>} */
  const root = {}
  /** @type {Record<string, string>} */
  const dark = {}
  /** @type {Map<string, Record<string, string>>} */
  const themes = new Map()

  /** @param {CSSRuleList} rules */
  const walk = (rules) => {
    for (const rule of Array.from(rules)) {
      if (rule instanceof CSSStyleRule) {
        const sel = rule.selectorText.trim()
        const m = THEME_SELECTOR.exec(sel)
        if (m) {
          const props = themes.get(m[1]) ?? {}
          copyCustomProps(rule.style, props)
          themes.set(m[1], props)
        } else if (ROOT_SELECTOR.test(sel)) {
          copyCustomProps(rule.style, root)
        } else if (sel === '.dark') {
          copyCustomProps(rule.style, dark)
        }
      } else if (rule instanceof CSSMediaRule) {
        if (window.matchMedia(rule.conditionText).matches) walk(rule.cssRules)
      } else if ('cssRules' in rule && rule.cssRules) {
        // @layer, @supports: same tokens, one level down.
        walk(/** @type {CSSRuleList} */ (rule.cssRules))
      }
    }
  }

  for (const sheet of Array.from(document.styleSheets)) {
    try {
      walk(sheet.cssRules)
    } catch {
      // A cross-origin sheet (web fonts) cannot be read and holds no tokens.
    }
  }
  return { root, dark, themes }
}

/** @param {ThemeId} id @returns {Swatch[]} registry fallback when the rule is missing */
function fallbackSwatches(id) {
  const p = getThemeDefinition(id)?.preview
  if (!p) return []
  return [
    { key: 'background', label: 'Background', color: p.bg },
    { key: 'foreground', label: 'Text', color: p.fg },
    { key: 'primary', label: 'Accent', color: p.accent },
  ]
}

function computeAll() {
  /** @type {Map<ThemeId, Swatch[]>} */
  const out = new Map()
  if (typeof document === 'undefined') return out

  const { root, dark, themes } = collectTokenRules()
  const probe = document.createElement('div')
  probe.setAttribute('aria-hidden', 'true')
  probe.style.cssText = 'position:fixed;left:-9999px;top:0;width:0;height:0;visibility:hidden;pointer-events:none;'
  const baseCss = probe.style.cssText
  document.body.appendChild(probe)

  try {
    for (const theme of APP_THEMES) {
      const props = themes.get(theme.id)
      if (!props) {
        out.set(theme.id, fallbackSwatches(theme.id))
        continue
      }
      probe.style.cssText = baseCss
      const merged = { ...root, ...(theme.isDark ? dark : {}), ...props }
      for (const [name, value] of Object.entries(merged)) probe.style.setProperty(name, value)

      /** @type {Swatch[]} */
      const swatches = []
      for (const [token, label] of SWATCH_TOKENS) {
        probe.style.color = `var(--${token})`
        const color = getComputedStyle(probe).color
        if (color) swatches.push({ key: token, label, color })
      }
      out.set(theme.id, swatches)
    }
  } finally {
    probe.remove()
  }
  return out
}

/**
 * The six-colour strip for a theme: background, text, accent, red, yellow, green.
 * @param {ThemeId} id
 * @returns {Swatch[]}
 */
export function themeSwatches(id) {
  if (!cache || cache.size === 0) cache = computeAll()
  return cache.get(id) ?? fallbackSwatches(id)
}
