/**
 * The one place a keyboard shortcut is written down.
 *
 * Every shortcut is a single combo string in the grammar `createHotkey` already
 * parses - `Mod+Shift+T`, `Alt+1-5`, `F11`. Both platforms come out of that one
 * string: `Mod` prints as ⌘ on macOS and Ctrl everywhere else, `Alt` as ⌥ or
 * Alt. The two spellings used to be written out separately in the help dialog,
 * which is exactly what let them drift from what the app actually binds - so now
 * there is only one of them, and adding a shortcut gets both platforms for free.
 *
 * Keys that print as a symbol (`Enter` → ↵, `Plus` → +) are named here rather
 * than typed as the symbol, so a combo can always be split on '+' without
 * `Mod++` turning into nonsense.
 */

import { detectOs } from './platform.js'

export const IS_MAC = detectOs() === 'macos'

/** Modifier → the keycap this platform prints. */
const GLYPH = IS_MAC
  ? { Mod: '⌘', Meta: '⌘', Alt: '⌥', Ctrl: '⌃', Shift: '⇧' }
  : { Mod: 'Ctrl', Meta: 'Win', Alt: 'Alt', Ctrl: 'Ctrl', Shift: '⇧' }

/** Keys written by name, printed as a glyph. Platform-independent. */
const KEYCAP = {
  Enter: '↵',
  Backspace: '⌫',
  Escape: 'Esc',
  Up: '↑',
  Down: '↓',
  Left: '←',
  Right: '→',
  // The recorder writes the DOM's names, which the hotkey grammar also accepts.
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  Control: IS_MAC ? '⌃' : 'Ctrl',
  Plus: '+',
  Minus: '−',
}

/**
 * The keycaps to render for a combo. Anything that is neither a modifier nor a
 * named key passes through as written, so `F11` and `1-9` need no entry.
 * @param {string} combo
 * @returns {string[]}
 */
export function keycaps(combo) {
  return combo.split('+').map((part) => GLYPH[part] ?? KEYCAP[part] ?? part)
}

/** Keys a tooltip spells out in words off macOS, where the glyphs are not the convention. */
const TITLE_WORD = /** @type {Record<string, string>} */ ({
  Mod: 'Ctrl', Meta: 'Win', Shift: 'Shift', Enter: 'Enter', Backspace: 'Backspace', Escape: 'Esc',
  Up: '↑', Down: '↓', Left: '←', Right: '→', ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→',
})

/**
 * A combo as tooltip text: `⌘⇧P` on macOS, `Ctrl+Shift+P` elsewhere. One
 * spelling per platform for every title in the app - they were written out
 * by hand, half of them as ⌘ glyphs that Windows and Linux showed as they are.
 * @param {string} combo
 */
export function comboTitle(combo) {
  if (IS_MAC) return keycaps(combo).map((k) => TITLE_WORD[k] && k.startsWith('Arrow') ? TITLE_WORD[k] : k).join('')
  return combo.split('+').map((part) => TITLE_WORD[part] ?? part).join('+')
}

/**
 * A combo as flat text, for searching. Includes both the printed keycaps and
 * the written names, so "cmd b", "⌘ b" and "mod b" all find the same row.
 * @param {string} combo
 */
export function comboText(combo) {
  return `${keycaps(combo).join(' ')} ${combo.replace(/\+/g, ' ')}`.toLowerCase()
}

/**
 * A row of the list. `id` marks a row the user can remap (see COMMANDS below):
 * the combo written here is then only its default. A row without an id is
 * owned by a component's own key handling (the SQL editor, the grid) and is
 * listed for reference only.
 *
 * A row whose combo ends in a range (`Mod+1-9`) and has an id is a family: one
 * command per number, `${id}.${n}`, each labelled by `items`.
 *
 * @typedef {{ combo: string, desc: string, id?: string, items?: string[] }} Shortcut
 * @typedef {{ label: string, icon: string, shortcuts: Shortcut[] }} ShortcutGroup
 */

/**
 * Grouped for the help dialog, in the order it shows them. `icon` is a name the
 * dialog maps to a component - this module stays plain data so it can be read
 * from anywhere without dragging Svelte in.
 *
 * This is also where every remappable command's default lives: the keybindings
 * store reads COMMANDS, which is built from these rows.
 * @type {ShortcutGroup[]}
 */
export const SHORTCUT_GROUPS = [
  {
    label: 'Navigation',
    icon: 'navigation',
    shortcuts: [
      { combo: 'Mod+K', desc: 'Command menu', id: 'palette.open' },
      { combo: 'Mod+Shift+P', desc: 'Go to page', id: 'palette.pages' },
      { combo: 'Mod+Alt+1-9', desc: 'Switch to saved connection', id: 'connection.switchTo', items: [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => `Switch to saved connection ${n}`) },
      { combo: 'Mod+P', desc: 'Search tables', id: 'palette.tables' },
      { combo: 'Mod+T', desc: 'Search tables (alternate key)', id: 'palette.tablesAlt' },
      { combo: 'Mod+;', desc: 'Leader key: then a letter or number (a panel lists them)', id: 'leader' },
      { combo: 'Mod+N', desc: 'New tab', id: 'tab.new' },
      { combo: 'Mod+W', desc: 'Close tab', id: 'tab.close' },
      { combo: 'Mod+Shift+W', desc: 'Close all tabs', id: 'tab.closeAll' },
      // Ctrl, not Mod, on purpose: macOS reserves ⌘Tab for the app switcher, so
      // this is Ctrl+Tab on all three platforms - the same chord browsers use.
      { combo: 'Ctrl+Tab', desc: 'Next tab', id: 'tab.next' },
      { combo: 'Ctrl+Shift+Tab', desc: 'Previous tab', id: 'tab.prev' },
      { combo: 'Mod+Shift+T', desc: 'Reopen closed tab', id: 'tab.reopen' },
      { combo: 'Mod+1-9', desc: 'Go to tab (9 = last)', id: 'tab.goto', items: [1, 2, 3, 4, 5, 6, 7, 8].map((n) => `Go to tab ${n}`).concat('Go to last tab') },
      { combo: 'Alt+Shift+T', desc: 'Toggle tab bar', id: 'view.tabBar' },
      { combo: 'Mod+B', desc: 'Toggle sidebar', id: 'view.sidebar' },
      { combo: 'Mod+Shift+1-6', desc: 'Sidebar tabs, in order: Tables, Databases, Queries, and the rest', id: 'sidebar.tab', items: ['Tables', 'Databases', 'Queries', 'Objects', 'Recent', 'Find & replace'].map((t) => `Sidebar: ${t}`) },
      { combo: 'Mod+Alt+Left', desc: 'Previous sidebar section (in a table, from the sidebar only)', id: 'sidebar.prevSection' },
      { combo: 'Mod+Alt+Right', desc: 'Next sidebar section (in a table, from the sidebar only)', id: 'sidebar.nextSection' },
      { combo: 'Mod+Shift+F', desc: 'Focus table filter', id: 'sidebar.filter' },
      { combo: 'Shift+Enter', desc: 'Sidebar: open the table and focus the grid' },
      { combo: 'Mod+D', desc: 'Switch database', id: 'database.switch' },
      { combo: 'Mod+Shift+C', desc: 'Switch connection', id: 'connection.switch' },
      { combo: 'Mod+Alt+D', desc: 'Data view', id: 'view.data' },
      { combo: '/', desc: 'Focus the search box on screen', id: 'search.focus' },
      { combo: 'Mod+/', desc: 'Keyboard shortcuts', id: 'help.shortcuts' },
      { combo: 'F11', desc: 'Toggle fullscreen', id: 'view.fullscreen' },
    ],
  },
  {
    label: 'Views',
    icon: 'monitor',
    shortcuts: [
      { combo: 'Mod+Shift+N', desc: 'New window', id: 'window.new' },
      { combo: 'Mod+Shift+D', desc: 'Disconnect', id: 'connection.disconnect' },
      { combo: 'Mod+Shift+S', desc: 'SQL editor', id: 'view.sql' },
      { combo: 'Mod+Shift+O', desc: 'ORM runner', id: 'view.orm' },
      { combo: 'Mod+J', desc: 'Toggle the terminal panel', id: 'view.terminalDock' },
      { combo: 'Control+`', desc: 'Terminal in a tab (psql, mysql, sqlite3...)', id: 'view.terminal' },
      { combo: 'Control+Shift+`', desc: 'New terminal tab', id: 'view.terminalNew' },
      { combo: 'Mod+Shift+X', desc: 'Extensions', id: 'view.extensions' },
      { combo: 'Mod+Shift+E', desc: 'Toggle AI panel', id: 'ai.panel' },
      { combo: 'Mod+I', desc: 'Toggle AI sidebar', id: 'ai.sidebar' },
      { combo: 'Mod+Shift+L', desc: 'Activity log', id: 'view.logs' },
      { combo: 'Mod+Shift+K', desc: 'Toggle query log', id: 'view.queryLog' },
      { combo: 'Mod+Shift+G', desc: 'Find in database', id: 'search.database' },
      { combo: 'Mod+Shift+R', desc: 'Refresh schemas, tables and rows', id: 'view.refreshAll' },
      { combo: 'Mod+R', desc: 'Refresh current view', id: 'view.refresh' },
      { combo: 'F5', desc: 'Refresh current view (the table, not the app)', id: 'view.refreshKey' },
      { combo: 'Mod+Shift+V', desc: 'Cycle table data view', id: 'table.cycleView' },
      { combo: 'Alt+1-5', desc: 'Table / JSON / Record / Text / Chart view', id: 'table.view', items: ['Table', 'JSON', 'Record', 'Text', 'Chart'].map((v) => `${v} view`) },
    ],
  },
  {
    label: 'SQL Editor',
    icon: 'terminal',
    shortcuts: [
      { combo: 'Mod+Enter', desc: 'Run all statements' },
      { combo: 'Mod+R', desc: 'Run statement at cursor' },
      { combo: IS_MAC ? 'Mod+Alt+[' : 'Mod+Shift+[', desc: 'Fold the statement at the cursor' },
      { combo: IS_MAC ? 'Mod+Alt+]' : 'Mod+Shift+]', desc: 'Unfold it' },
      { combo: 'Ctrl+Alt+[', desc: 'Fold all statements' },
      { combo: 'Ctrl+Alt+]', desc: 'Unfold all' },
      { combo: 'Mod+L', desc: 'Select current statement' },
      { combo: 'Mod+S', desc: 'Save query (into the saved query it came from)' },
      { combo: 'Mod+Shift+S', desc: 'Save as a new query' },
      { combo: 'Mod+/', desc: 'Comment or uncomment lines' },
      { combo: 'Shift+Enter', desc: 'New line (same as Enter; keeps the indentation)' },
      { combo: 'Tab', desc: 'Add the suggested ; at the end of a statement' },
      { combo: 'Mod+.', desc: 'Fix the problem under the caret (add ;, close a bracket or string)' },
      { combo: 'Alt+↑', desc: 'Move line up (Alt+↓: down)' },
      { combo: 'Shift+Alt+↓', desc: 'Copy line down (Shift+Alt+↑: up)' },
      { combo: 'Mod+Shift+K', desc: 'Delete line' },
      { combo: 'Mod+D', desc: 'Select the next match of the selection' },
      { combo: 'Mod+]', desc: 'Indent line (Mod+[: outdent)' },
      { combo: 'Mod+J', desc: 'Toggle output panel' },
      { combo: 'Mod+Shift+B', desc: 'History list in the results pane' },
    ],
  },
  {
    label: 'ORM runner',
    icon: 'code-2',
    shortcuts: [
      { combo: 'Mod+Enter', desc: 'Run query' },
      { combo: 'Mod+S', desc: 'Re-indent the code' },
    ],
  },
  {
    label: 'Data Table',
    icon: 'table-2',
    shortcuts: [
      { combo: 'Mod+F', desc: 'Search rows', id: 'table.search' },
      // Both bindings are listed because both are real: macOS swallows Cmd+H to
      // hide the app, so Mod+Alt+F is the one that always arrives.
      { combo: 'Mod+H', desc: 'Find & replace in column', id: 'table.findReplace' },
      { combo: 'Mod+Alt+F', desc: 'Find & replace in column', id: 'table.findReplaceAlt' },
      { combo: 'Alt+A', desc: 'Open filter menu', id: 'table.filterMenu' },
      { combo: 'Alt+S', desc: 'Open sort menu', id: 'table.sortMenu' },
      { combo: 'Alt+C', desc: 'Open columns menu', id: 'table.columnsMenu' },
      { combo: 'Alt+R', desc: 'Reset table view (clear filters/sort/search)', id: 'table.resetView' },
      { combo: 'Alt+F', desc: 'Filter by the focused cell\'s value' },
      { combo: 'Alt+E', desc: 'Exclude the focused cell\'s value' },
      { combo: 'Alt+X', desc: 'Clear the table search (Escape inside the box does too)', id: 'table.clearSearch' },
      { combo: 'Alt+N', desc: 'Stage a new row (again for another)', id: 'table.addRow' },
      { combo: 'Alt+D', desc: 'Copy the focused row into the insert draft' },
      { combo: 'Mod+Escape', desc: 'Discard every staged row' },
      { combo: 'Mod+E', desc: 'Expand / collapse the focused row' },
      { combo: 'Space', desc: 'Preview the focused cell in the bottom dock (or Shift+click it)' },
      { combo: 'Shift+Space', desc: 'Preview the focused cell and step into the editor' },
      { combo: 'Alt+J', desc: 'Preview the whole row as JSON in the bottom dock' },
      { combo: 'Alt+Space', desc: 'Step into the preview editor (Escape returns to the grid)' },
      { combo: 'Enter', desc: 'Edit cell' },
      { combo: 'F2', desc: 'Edit cell' },
      { combo: 'Escape', desc: 'Cancel edit' },
      { combo: 'Mod+Enter', desc: 'Navigate to FK row (or edit the cell)' },
      { combo: 'Shift+Enter', desc: 'Open the FK target in a new tab' },
      { combo: 'Mod+C', desc: 'Copy cell value' },
      { combo: 'Mod+Backspace', desc: 'Delete selected rows', id: 'table.deleteRows' },
      { combo: 'Mod+A', desc: 'Select all rows' },
      { combo: 'Mod+Shift+A', desc: 'Deselect all rows' },
      { combo: 'Mod+S', desc: 'Apply staged changes', id: 'table.apply' },
      { combo: 'Mod+Alt+S', desc: 'Copy staged changes as SQL', id: 'table.copySql' },
      { combo: 'Alt+Backspace', desc: 'Discard staged changes', id: 'table.discard' },
      { combo: 'Mod+Alt+Backspace', desc: 'Discard staged changes', id: 'table.discardAlt' },
      { combo: 'Mod+Z', desc: 'Undo cell edit' },
      { combo: 'Mod+Shift+Z', desc: 'Redo cell edit' },
      { combo: 'Mod+Up', desc: 'Scroll to top' },
      { combo: 'Mod+Down', desc: 'Scroll to bottom' },
      { combo: 'Mod+Left', desc: 'Previous page' },
      { combo: 'Mod+Right', desc: 'Next page' },
      { combo: 'Mod+Shift+Left', desc: 'First page' },
      { combo: 'Mod+Shift+Right', desc: 'Last page' },
      { combo: 'Mod+Alt+Left', desc: 'Scroll to the first column' },
      { combo: 'Mod+Alt+Right', desc: 'Scroll to the last column' },
      { combo: 'Alt+Shift+1-5', desc: 'Jump to pinned table', id: 'sidebar.pinned', items: [1, 2, 3, 4, 5].map((n) => `Jump to pinned table ${n}`) },
    ],
  },
  {
    label: 'Schema diagram',
    icon: 'network',
    shortcuts: [
      { combo: 'Mod+Shift+Enter', desc: 'Toggle full screen' },
      { combo: 'Mod+F', desc: 'Search the view: tables (Diagram, Hierarchy, Tree), columns (Dictionary), code (Mermaid, DDL)' },
      { combo: 'Escape', desc: 'Exit full screen' },
    ],
  },
  {
    label: 'AI chat',
    icon: 'bot',
    shortcuts: [
      { combo: 'Enter', desc: 'Send message' },
      { combo: 'Shift+Enter', desc: 'New line' },
      { combo: 'Mod+B', desc: 'Toggle conversation list' },
      { combo: 'Mod+Shift+T', desc: 'New conversation' },
    ],
  },
  {
    label: 'Appearance',
    icon: 'palette',
    shortcuts: [
      { combo: 'Mod+B', desc: 'Toggle sidebar', id: 'view.sidebar' },
      { combo: 'Alt+Shift+T', desc: 'Toggle tab bar', id: 'view.tabBar' },
      { combo: 'Mod+Shift+B', desc: 'Toggle status bar (the SQL editor and AI chat use it for their lists)', id: 'view.statusBar' },
      { combo: 'Mod+Alt+T', desc: 'Theme picker', id: 'theme.pick' },
      // Unbound by default: set them in Settings → Keyboard if you want them.
      { combo: '', desc: 'Cycle theme', id: 'theme.next' },
      { combo: '', desc: 'Previous theme', id: 'theme.prev' },
      { combo: 'Mod+Plus', desc: 'Zoom in' },
      { combo: 'Mod+Minus', desc: 'Zoom out' },
      { combo: 'Mod+0', desc: 'Reset zoom' },
    ],
  },
  {
    label: 'General',
    icon: 'settings',
    shortcuts: [
      { combo: 'Mod+?', desc: 'Keyboard shortcuts', id: 'help.shortcutsAlt' },
      { combo: '?', desc: 'Keyboard shortcuts', id: 'help.shortcutsKey' },
      { combo: 'Mod+,', desc: 'Settings', id: 'settings.open' },
      { combo: 'Escape', desc: 'Dismiss / close' },
    ],
  },
]

/**
 * @typedef {{ id: string, label: string, group: string, combo: string }} Command
 */

/** `Mod+1-9` → ['Mod+1', …, 'Mod+9']; anything else → null. */
function expandRange(/** @type {string} */ combo) {
  const m = /^(.*?)(\d)-(\d)$/.exec(combo)
  if (!m) return null
  const out = []
  for (let n = Number(m[2]); n <= Number(m[3]); n++) out.push(`${m[1]}${n}`)
  return out
}

/**
 * Every remappable command, once, with its default combo. A row repeated in two
 * groups (⌘B is under Navigation and Appearance) is one command: the first row
 * names it. Families expand to one command per number.
 * @type {Command[]}
 */
export const COMMANDS = (() => {
  /** @type {Command[]} */
  const out = []
  const seen = new Set()
  for (const g of SHORTCUT_GROUPS) {
    for (const s of g.shortcuts) {
      if (!s.id || seen.has(s.id)) continue
      seen.add(s.id)
      const range = expandRange(s.combo)
      if (range) {
        range.forEach((combo, i) =>
          out.push({ id: `${s.id}.${i + 1}`, label: s.items?.[i] ?? `${s.desc} ${i + 1}`, group: g.label, combo }))
      } else {
        out.push({ id: s.id, label: s.desc, group: g.label, combo: s.combo })
      }
    }
  }
  return out
})()

/** @type {Map<string, Command>} */
export const COMMAND_BY_ID = new Map(COMMANDS.map((c) => [c.id, c]))

/** The ids a family row expands to (`tab.goto` → tab.goto.1 …), or the id itself. */
export function rowCommandIds(/** @type {Shortcut} */ s) {
  if (!s.id) return []
  const range = expandRange(s.combo)
  return range ? range.map((_, i) => `${s.id}.${i + 1}`) : [s.id]
}

/**
 * Rows the user cannot remap, as `{ label, group, combo }`, for the keybindings
 * page to list beside the commands. Editor and grid keys that only mean
 * something inside that component.
 */
export const FIXED_SHORTCUTS = SHORTCUT_GROUPS.flatMap((g) =>
  g.shortcuts.filter((s) => !s.id).map((s) => ({ label: s.desc, group: g.label, combo: s.combo })))
