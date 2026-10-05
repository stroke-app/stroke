/** @typedef {'light' | 'dark'} ThemeGroup */

/** @typedef {'matte' | 'light' | 'parchment' | 'clay' | 'claude-light' | 'moss-light' | 'orchid-light' | 'ice' | 'github-light' | 'light-high-contrast' | 'dark' | 'obsidian' | 'claude' | 'moss' | 'orchid' | 'graphite' | 'midnight' | 'vitesse' | 'slate' | 'forest' | 'mono' | 'rose-pine' | 'catppuccin' | 'solarized' | 'dark-high-contrast' | 'hotdog'} ThemeId */

/**
 * `hidden` keeps a theme out of the picker until it is found. Only the easter
 * egg uses it; everything else is listed normally.
 * @typedef {{ id: ThemeId, name: string, description: string, isDark: boolean, group: ThemeGroup, hidden?: boolean, preview: { bg: string, fg: string, accent: string } }} ThemeDefinition
 */

export const DEFAULT_THEME_ID = /** @type {const} */ ('dark')

/** @type {Record<ThemeGroup, string>} */
export const THEME_GROUP_LABELS = {
  light: 'Light',
  dark: 'Dark',
}

/** @type {readonly ThemeGroup[]} */
export const THEME_GROUP_ORDER = ['light', 'dark']

/** @type {readonly ThemeDefinition[]} */
export const APP_THEMES = [
  // ── Light ────────────────────────────────────────────────────────────────
  {
    id: 'light',
    name: 'Studio',
    description: 'Clean neutral white',
    isDark: false,
    group: 'light',
    preview: { bg: '#fafafa', fg: '#1a1a1a', accent: '#1a1a1a' },
  },
  {
    id: 'parchment',
    name: 'Parchment',
    description: 'Warm amber cream',
    isDark: false,
    group: 'light',
    preview: { bg: '#faf7f0', fg: '#2a1c10', accent: '#3a50c4' },
  },
  {
    id: 'clay',
    name: 'Clay',
    description: 'Warm paper, terracotta accent',
    isDark: false,
    group: 'light',
    preview: { bg: '#f7f3ec', fg: '#2b2119', accent: '#a85631' },
  },
  {
    id: 'claude-light',
    name: 'Claude',
    description: "Anthropic's cream and coral",
    isDark: false,
    group: 'light',
    preview: { bg: '#f0eee7', fg: '#1b1915', accent: '#af5629' },
  },
  {
    id: 'moss-light',
    name: 'Moss',
    description: 'Pale linen, jade accent',
    isDark: false,
    group: 'light',
    preview: { bg: '#f4f6f1', fg: '#151c13', accent: '#116d45' },
  },
  {
    id: 'orchid-light',
    name: 'Orchid',
    description: 'Blush white, magenta accent',
    isDark: false,
    group: 'light',
    preview: { bg: '#f9f4f8', fg: '#221620', accent: '#953585' },
  },
  {
    id: 'ice',
    name: 'Ice',
    description: 'Cool blue-steel',
    isDark: false,
    group: 'light',
    preview: { bg: '#f2f5fc', fg: '#0e1a30', accent: '#2f5fcc' },
  },
  {
    id: 'github-light',
    name: 'GitHub',
    description: 'Crisp high-contrast light',
    isDark: false,
    group: 'light',
    preview: { bg: '#ffffff', fg: '#1f2328', accent: '#0969da' },
  },
  // ── Dark ─────────────────────────────────────────────────────────────────
  {
    id: 'dark',
    name: 'Studio',
    description: 'Neutral near-black',
    isDark: true,
    group: 'dark',
    preview: { bg: '#1c1c1c', fg: '#f0f0f0', accent: '#f0f0f0' },
  },
  {
    id: 'obsidian',
    name: 'Obsidian',
    description: 'Cool near-black, teal accent',
    isDark: true,
    group: 'dark',
    preview: { bg: '#131419', fg: '#eeeff4', accent: '#67cfd8' },
  },
  {
    id: 'matte',
    name: 'Matte',
    description: 'Flat matte black, no accent hue',
    isDark: true,
    group: 'dark',
    preview: { bg: '#121212', fg: '#d9d9d9', accent: '#dcdcdc' },
  },
  {
    id: 'claude',
    name: 'Claude',
    description: 'Warm charcoal, coral accent',
    isDark: true,
    group: 'dark',
    preview: { bg: '#131210', fg: '#f1f0ec', accent: '#e18b6a' },
  },
  {
    id: 'moss',
    name: 'Moss',
    description: 'Deep moss, jade accent',
    isDark: true,
    group: 'dark',
    preview: { bg: '#090d09', fg: '#e8ede6', accent: '#78c594' },
  },
  {
    id: 'orchid',
    name: 'Orchid',
    description: 'Aubergine black, orchid accent',
    isDark: true,
    group: 'dark',
    preview: { bg: '#0e090e', fg: '#efe9f0', accent: '#e492c9' },
  },
  {
    // Hidden until unlocked - see EASTER_EGG_THEME_ID below.
    id: 'hotdog',
    name: 'Hotdog Stand',
    description: 'You found it. Sorry.',
    isDark: true,
    group: 'dark',
    hidden: true,
    preview: { bg: '#2b0707', fg: '#ffd83d', accent: '#d41710' },
  },
  {
    id: 'graphite',
    name: 'Graphite',
    description: 'Cool graphite, blue accent',
    isDark: true,
    group: 'dark',
    preview: { bg: '#1e1f22', fg: '#dfe1e5', accent: '#3574f0' },
  },
  {
    id: 'midnight',
    name: 'Midnight',
    description: 'Deep ocean navy',
    isDark: true,
    group: 'dark',
    preview: { bg: '#0f1520', fg: '#dde8ff', accent: '#5b9bff' },
  },
  {
    id: 'vitesse',
    name: 'Dusk',
    description: 'Warm amber charcoal',
    isDark: true,
    group: 'dark',
    preview: { bg: '#1d1813', fg: '#eee4d0', accent: '#d4973a' },
  },
  {
    id: 'slate',
    name: 'Iris',
    description: 'Deep violet-purple',
    isDark: true,
    group: 'dark',
    preview: { bg: '#191422', fg: '#e8e0ff', accent: '#a668ff' },
  },
  {
    id: 'forest',
    name: 'Forest',
    description: 'Dark emerald green',
    isDark: true,
    group: 'dark',
    preview: { bg: '#141a14', fg: '#d8edd6', accent: '#4fcf80' },
  },
  {
    id: 'mono',
    name: 'Mono',
    description: 'Pure black, crisp white',
    isDark: true,
    group: 'dark',
    preview: { bg: '#000000', fg: '#fafafa', accent: '#fafafa' },
  },
  {
    id: 'rose-pine',
    name: 'Rosé Pine',
    description: 'Muted rose & pine',
    isDark: true,
    group: 'dark',
    preview: { bg: '#191724', fg: '#e0def4', accent: '#c4a7e7' },
  },
  {
    id: 'catppuccin',
    name: 'Catppuccin',
    description: 'Soft pastel mocha',
    isDark: true,
    group: 'dark',
    preview: { bg: '#1e1e2e', fg: '#cdd6f4', accent: '#cba6f7' },
  },
  {
    id: 'solarized',
    name: 'Solarized',
    description: 'Classic teal & blue',
    isDark: true,
    group: 'dark',
    preview: { bg: '#002b36', fg: '#93a1a1', accent: '#268bd2' },
  },
  {
    id: 'light-high-contrast',
    name: 'High Contrast',
    description: 'Maximum contrast, for low vision',
    isDark: false,
    group: 'light',
    preview: { bg: '#ffffff', fg: '#000000', accent: '#1a3fd0' },
  },
  {
    id: 'dark-high-contrast',
    name: 'High Contrast',
    description: 'Maximum contrast, for low vision',
    isDark: true,
    group: 'dark',
    preview: { bg: '#000000', fg: '#ffffff', accent: '#ffe000' },
  },
]

/** Sync with index.html boot script when adding themes. */
export const THEME_IDS = /** @type {readonly ThemeId[]} */ (APP_THEMES.map((t) => t.id))

/**
 * The themes ⌘M walks. Hidden ones are not in it: an easter egg is something
 * you go and find, not something a cycle hands you on the way past - and
 * `hotdog` in particular is not a theme anyone wants to land on mid-session.
 * Selecting it from the picker still works once it has been found.
 */
export const CYCLE_THEME_IDS = /** @type {readonly ThemeId[]} */ (
  APP_THEMES.filter((t) => !t.hidden).map((t) => t.id)
)

/** @param {unknown} value */
export function normalizeThemeId(value) {
  if (typeof value === 'string' && THEME_IDS.includes(/** @type {ThemeId} */ (value))) {
    return /** @type {ThemeId} */ (value)
  }
  return DEFAULT_THEME_ID
}

/** @param {ThemeId} id */
export function getThemeDefinition(id) {
  return APP_THEMES.find((t) => t.id === id) ?? APP_THEMES.find((t) => t.id === DEFAULT_THEME_ID)
}

/** @param {ThemeId} id */
export function isDarkTheme(id) {
  return getThemeDefinition(id)?.isDark ?? true
}

/** @param {ThemeId} id */
export function nextThemeId(id) {
  const idx = THEME_IDS.indexOf(id)
  return THEME_IDS[(idx + 1) % THEME_IDS.length]
}

/** @param {ThemeId} id */
export function shikiThemeId(id) {
  return isDarkTheme(id) ? 'vitesse-dark' : 'vitesse-light'
}

/** @returns {readonly { id: ThemeGroup, label: string, themes: ThemeDefinition[] }[]} */
export function themesByGroup() {
  return THEME_GROUP_ORDER.map((id) => ({
    id,
    label: THEME_GROUP_LABELS[id],
    // visibleThemes(), not APP_THEMES: the easter egg stays out of the picker
    // until it has been found.
    themes: visibleThemes().filter((t) => t.group === id),
  })).filter((g) => g.themes.length > 0)
}


/**
 * The easter egg, and how it is found: click the version number in the status
 * bar seven times. Kept out of the picker until then so nobody lands on it by
 * scrolling a list, and stored so it stays found once found.
 */
export const EASTER_EGG_THEME_ID = /** @type {const} */ ('hotdog')
export const EASTER_EGG_KEY = 'stroke:egg-found'
export const EASTER_EGG_CLICKS = 7

/** @returns {boolean} */
export function easterEggFound() {
  try {
    return localStorage.getItem(EASTER_EGG_KEY) === '1'
  } catch {
    return false
  }
}

export function markEasterEggFound() {
  try {
    localStorage.setItem(EASTER_EGG_KEY, '1')
  } catch {
    // Private mode or a full quota: the theme still applies for this session.
  }
}

/**
 * Themes to offer in the picker. The egg is filtered out until found, and stays
 * listed afterwards so it can be selected again - and, more to the point, so it
 * can be escaped from.
 * @returns {readonly ThemeDefinition[]}
 */
export function visibleThemes() {
  const found = easterEggFound()
  return APP_THEMES.filter((t) => !t.hidden || found)
}
