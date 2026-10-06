/**
 * Code editor colour themes: the presets, their labels and validation. Pure
 * data, so the settings store can hold and check the choice without importing
 * CodeMirror; `cm-editor-theme.js` turns a palette into editor extensions.
 *
 * 'app' is the editor as it has always looked: transparent over its surface,
 * coloured from the app theme's tokens (and the JSON colours setting). Every
 * other entry is a fixed palette taken from its canonical source, so a light
 * editor can sit in a dark app and the other way round.
 */

/**
 * @typedef {'app' | 'one-dark' | 'github-light' | 'github-dark' | 'dracula' | 'monokai'
 *   | 'nord' | 'solarized-light' | 'solarized-dark' | 'tokyo-night' | 'catppuccin-mocha'
 *   | 'catppuccin-latte' | 'rose-pine' | 'gruvbox-dark'} EditorThemeId
 */

/**
 * @typedef {object} EditorPalette
 * @property {string} bg editor background
 * @property {string} fg default text
 * @property {string} gutter line numbers
 * @property {string} gutterActive the caret line's number
 * @property {string} line current line band
 * @property {string} selection drawn selection (focused; unfocused is a lighter mix)
 * @property {string} cursor caret
 * @property {string} panel tooltips, completion list, find panel fields
 * @property {string} border tooltip and gutter edges
 * @property {string} muted secondary text: completion details, placeholders, fold markers
 * @property {string} keyword
 * @property {string} string
 * @property {string} number
 * @property {string} comment
 * @property {string} fn function calls and SQL builtins
 * @property {string} type type and class names
 * @property {string} operator
 * @property {string} variable plain names (SQL identifiers, JS variables)
 * @property {string} property object keys, JSON keys, attributes
 * @property {string} punctuation
 * @property {string} constant true, false, null, atoms
 * @property {string} tag markup tag names
 */

/** @typedef {{ label: string, dark: boolean, palette: EditorPalette | null }} EditorThemeDef */

/** @type {Record<EditorThemeId, EditorThemeDef>} */
export const EDITOR_THEMES = {
  app: { label: 'Match app theme', dark: false, palette: null },
  // @codemirror/theme-one-dark's values. Plain names stay ivory rather than
  // its coral, which turned every SQL column name red.
  'one-dark': {
    label: 'One Dark',
    dark: true,
    palette: {
      bg: '#282c34', fg: '#abb2bf', gutter: '#7d8799', gutterActive: '#abb2bf',
      line: '#2c313a', selection: '#3e4451', cursor: '#528bff',
      panel: '#21252b', border: '#181a1f', muted: '#9da5b4',
      keyword: '#c678dd', string: '#98c379', number: '#d19a66', comment: '#7d8799',
      fn: '#61afef', type: '#e5c07b', operator: '#56b6c2', variable: '#abb2bf',
      property: '#e06c75', punctuation: '#abb2bf', constant: '#d19a66', tag: '#e06c75',
    },
  },
  // Primer's light default (github-vscode-theme).
  'github-light': {
    label: 'GitHub Light',
    dark: false,
    palette: {
      bg: '#ffffff', fg: '#1f2328', gutter: '#8c959f', gutterActive: '#1f2328',
      line: '#f6f8fa', selection: '#0969da33', cursor: '#0969da',
      panel: '#ffffff', border: '#d0d7de', muted: '#59636e',
      keyword: '#cf222e', string: '#0a3069', number: '#0550ae', comment: '#6e7781',
      fn: '#8250df', type: '#953800', operator: '#cf222e', variable: '#1f2328',
      property: '#0550ae', punctuation: '#1f2328', constant: '#0550ae', tag: '#116329',
    },
  },
  // Primer's dark default.
  'github-dark': {
    label: 'GitHub Dark',
    dark: true,
    palette: {
      bg: '#0d1117', fg: '#e6edf3', gutter: '#6e7681', gutterActive: '#e6edf3',
      line: '#6e76811a', selection: '#264f78', cursor: '#2f81f7',
      panel: '#161b22', border: '#30363d', muted: '#8d96a0',
      keyword: '#ff7b72', string: '#a5d6ff', number: '#79c0ff', comment: '#8b949e',
      fn: '#d2a8ff', type: '#ffa657', operator: '#ff7b72', variable: '#e6edf3',
      property: '#79c0ff', punctuation: '#e6edf3', constant: '#79c0ff', tag: '#7ee787',
    },
  },
  // draculatheme.com/spec.
  dracula: {
    label: 'Dracula',
    dark: true,
    palette: {
      bg: '#282a36', fg: '#f8f8f2', gutter: '#6272a4', gutterActive: '#f8f8f2',
      line: '#44475a59', selection: '#44475a', cursor: '#f8f8f2',
      panel: '#21222c', border: '#191a21', muted: '#a4aecf',
      keyword: '#ff79c6', string: '#f1fa8c', number: '#bd93f9', comment: '#6272a4',
      fn: '#50fa7b', type: '#8be9fd', operator: '#ff79c6', variable: '#f8f8f2',
      property: '#8be9fd', punctuation: '#f8f8f2', constant: '#bd93f9', tag: '#ff79c6',
    },
  },
  // Sublime Text's classic Monokai.
  monokai: {
    label: 'Monokai',
    dark: true,
    palette: {
      bg: '#272822', fg: '#f8f8f2', gutter: '#90908a', gutterActive: '#c2c2bf',
      line: '#3e3d32', selection: '#49483e', cursor: '#f8f8f0',
      panel: '#1e1f1c', border: '#414339', muted: '#b4b09c',
      keyword: '#f92672', string: '#e6db74', number: '#ae81ff', comment: '#75715e',
      fn: '#a6e22e', type: '#66d9ef', operator: '#f92672', variable: '#f8f8f2',
      property: '#a6e22e', punctuation: '#f8f8f2', constant: '#ae81ff', tag: '#f92672',
    },
  },
  // nordtheme.com, as its VS Code port colours code.
  nord: {
    label: 'Nord',
    dark: true,
    palette: {
      bg: '#2e3440', fg: '#d8dee9', gutter: '#4c566a', gutterActive: '#d8dee9',
      line: '#3b4252', selection: '#434c5ecc', cursor: '#d8dee9',
      panel: '#3b4252', border: '#434c5e', muted: '#aeb6c4',
      keyword: '#81a1c1', string: '#a3be8c', number: '#b48ead', comment: '#616e88',
      fn: '#88c0d0', type: '#8fbcbb', operator: '#81a1c1', variable: '#d8dee9',
      property: '#8fbcbb', punctuation: '#eceff4', constant: '#81a1c1', tag: '#81a1c1',
    },
  },
  // ethanschoonover.com/solarized: base3 ground, base00 body, base1 comments.
  // Tooltips sit on the ground, not base2: body text on base2 is 3.6:1.
  'solarized-light': {
    label: 'Solarized Light',
    dark: false,
    palette: {
      bg: '#fdf6e3', fg: '#657b83', gutter: '#93a1a1', gutterActive: '#586e75',
      line: '#eee8d5', selection: '#93a1a14d', cursor: '#657b83',
      panel: '#fdf6e3', border: '#ddd6c1', muted: '#586e75',
      keyword: '#859900', string: '#2aa198', number: '#d33682', comment: '#93a1a1',
      fn: '#268bd2', type: '#b58900', operator: '#859900', variable: '#657b83',
      property: '#268bd2', punctuation: '#657b83', constant: '#b58900', tag: '#268bd2',
    },
  },
  // Solarized dark: base03 ground, base0 body, base01 comments. Tooltips on
  // the ground for the same reason as the light variant.
  'solarized-dark': {
    label: 'Solarized Dark',
    dark: true,
    palette: {
      bg: '#002b36', fg: '#839496', gutter: '#586e75', gutterActive: '#93a1a1',
      line: '#073642', selection: '#274642', cursor: '#93a1a1',
      panel: '#002b36', border: '#0f4d5c', muted: '#93a1a1',
      keyword: '#859900', string: '#2aa198', number: '#d33682', comment: '#586e75',
      fn: '#268bd2', type: '#b58900', operator: '#859900', variable: '#839496',
      property: '#268bd2', punctuation: '#839496', constant: '#b58900', tag: '#268bd2',
    },
  },
  // folke/tokyonight, the Night style.
  'tokyo-night': {
    label: 'Tokyo Night',
    dark: true,
    palette: {
      bg: '#1a1b26', fg: '#a9b1d6', gutter: '#3b4261', gutterActive: '#737aa2',
      line: '#292e42', selection: '#283457', cursor: '#c0caf5',
      panel: '#16161e', border: '#292e42', muted: '#9aa5ce',
      keyword: '#bb9af7', string: '#9ece6a', number: '#ff9e64', comment: '#565f89',
      fn: '#7aa2f7', type: '#2ac3de', operator: '#89ddff', variable: '#c0caf5',
      property: '#73daca', punctuation: '#a9b1d6', constant: '#ff9e64', tag: '#f7768e',
    },
  },
  // catppuccin.com/palette, Mocha, following its style guide.
  'catppuccin-mocha': {
    label: 'Catppuccin Mocha',
    dark: true,
    palette: {
      bg: '#1e1e2e', fg: '#cdd6f4', gutter: '#7f849c', gutterActive: '#b4befe',
      line: '#2a2b3c', selection: '#9399b240', cursor: '#f5e0dc',
      panel: '#181825', border: '#313244', muted: '#a6adc8',
      keyword: '#cba6f7', string: '#a6e3a1', number: '#fab387', comment: '#9399b2',
      fn: '#89b4fa', type: '#f9e2af', operator: '#89dceb', variable: '#cdd6f4',
      property: '#b4befe', punctuation: '#9399b2', constant: '#fab387', tag: '#89b4fa',
    },
  },
  // Catppuccin Latte.
  'catppuccin-latte': {
    label: 'Catppuccin Latte',
    dark: false,
    palette: {
      bg: '#eff1f5', fg: '#4c4f69', gutter: '#8c8fa1', gutterActive: '#7287fd',
      line: '#e6e9ef', selection: '#7c7f9340', cursor: '#dc8a78',
      panel: '#e6e9ef', border: '#ccd0da', muted: '#5c5f77',
      keyword: '#8839ef', string: '#40a02b', number: '#fe640b', comment: '#7c7f93',
      fn: '#1e66f5', type: '#df8e1d', operator: '#04a5e5', variable: '#4c4f69',
      property: '#7287fd', punctuation: '#7c7f93', constant: '#fe640b', tag: '#1e66f5',
    },
  },
  // rosepinetheme.com, main variant.
  'rose-pine': {
    label: 'Rosé Pine',
    dark: true,
    palette: {
      bg: '#191724', fg: '#e0def4', gutter: '#6e6a86', gutterActive: '#e0def4',
      line: '#21202e', selection: '#403d52', cursor: '#e0def4',
      panel: '#1f1d2e', border: '#26233a', muted: '#908caa',
      keyword: '#31748f', string: '#f6c177', number: '#f6c177', comment: '#6e6a86',
      fn: '#ebbcba', type: '#9ccfd8', operator: '#908caa', variable: '#e0def4',
      property: '#c4a7e7', punctuation: '#908caa', constant: '#ebbcba', tag: '#9ccfd8',
    },
  },
  // morhetz/gruvbox, dark medium contrast.
  'gruvbox-dark': {
    label: 'Gruvbox Dark',
    dark: true,
    palette: {
      bg: '#282828', fg: '#ebdbb2', gutter: '#7c6f64', gutterActive: '#fabd2f',
      line: '#3c3836', selection: '#504945', cursor: '#ebdbb2',
      panel: '#32302f', border: '#504945', muted: '#a89984',
      keyword: '#fb4934', string: '#b8bb26', number: '#d3869b', comment: '#928374',
      fn: '#b8bb26', type: '#fabd2f', operator: '#ebdbb2', variable: '#ebdbb2',
      property: '#83a598', punctuation: '#a89984', constant: '#d3869b', tag: '#8ec07c',
    },
  },
}

/** @type {EditorThemeId} */
export const DEFAULT_EDITOR_THEME = 'app'
export const EDITOR_THEME_IDS = /** @type {EditorThemeId[]} */ (Object.keys(EDITOR_THEMES))

/** @returns {EditorThemeId} */
export function normalizeEditorTheme(/** @type {unknown} */ id) {
  return typeof id === 'string' && Object.hasOwn(EDITOR_THEMES, id) ? /** @type {EditorThemeId} */ (id) : DEFAULT_EDITOR_THEME
}

/**
 * Colours for a settings preview card. 'app' answers with the app's own
 * tokens, the same ones the editor reads in that mode.
 * @param {EditorThemeId} id
 * @returns {Pick<EditorPalette, 'bg' | 'fg' | 'gutter' | 'keyword' | 'string' | 'comment' | 'operator' | 'punctuation' | 'variable'>}
 */
export function editorPreviewColors(id) {
  const p = EDITOR_THEMES[id]?.palette
  if (p) return p
  return {
    bg: 'var(--background)',
    fg: 'var(--foreground)',
    gutter: 'color-mix(in oklch, var(--muted-foreground) 50%, transparent)',
    keyword: 'var(--json-boolean)',
    string: 'var(--json-string)',
    comment: 'var(--json-null)',
    operator: 'var(--muted-foreground)',
    punctuation: 'var(--muted-foreground)',
    variable: 'var(--foreground)',
  }
}
