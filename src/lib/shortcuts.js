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

/**
 * A combo as flat text, for searching. Includes both the printed keycaps and
 * the written names, so "cmd b", "⌘ b" and "mod b" all find the same row.
 * @param {string} combo
 */
export function comboText(combo) {
  return `${keycaps(combo).join(' ')} ${combo.replace(/\+/g, ' ')}`.toLowerCase()
}

/**
 * @typedef {{ combo: string, desc: string }} Shortcut
 * @typedef {{ label: string, icon: string, shortcuts: Shortcut[] }} ShortcutGroup
 */

/**
 * Grouped for the help dialog, in the order it shows them. `icon` is a name the
 * dialog maps to a component - this module stays plain data so it can be read
 * from anywhere without dragging Svelte in.
 * @type {ShortcutGroup[]}
 */
export const SHORTCUT_GROUPS = [
  {
    label: 'Navigation',
    icon: 'navigation',
    shortcuts: [
      { combo: 'Mod+K', desc: 'Command menu' },
      { combo: 'Mod+Shift+P', desc: 'Go to page' },
      { combo: 'Mod+Alt+1-9', desc: 'Switch to saved connection' },
      { combo: 'Mod+P', desc: 'Search tables' },
      { combo: 'Mod+T', desc: 'Search tables' },
      { combo: 'Mod+N', desc: 'New tab' },
      { combo: 'Mod+W', desc: 'Close tab' },
      { combo: 'Mod+Shift+W', desc: 'Close all tabs' },
      // Ctrl, not Mod, on purpose: macOS reserves ⌘Tab for the app switcher, so
      // this is Ctrl+Tab on all three platforms - the same chord browsers use.
      { combo: 'Ctrl+Tab', desc: 'Next tab' },
      { combo: 'Ctrl+Shift+Tab', desc: 'Previous tab' },
      { combo: 'Mod+Shift+T', desc: 'Reopen closed tab' },
      { combo: 'Mod+1-9', desc: 'Go to tab (9 = last)' },
      { combo: 'Alt+Shift+T', desc: 'Toggle tab bar' },
      { combo: 'Mod+B', desc: 'Toggle sidebar' },
      { combo: 'Mod+Shift+1-5', desc: 'Sidebar: Tables / Databases / Views / Recent / Pins' },
      { combo: 'Mod+Alt+Left/Right', desc: 'Cycle sidebar sections' },
      { combo: 'Mod+Shift+F', desc: 'Focus table filter' },
      { combo: 'Shift+Enter', desc: 'Sidebar: open the table and focus the grid' },
      { combo: 'Mod+D', desc: 'Switch database' },
      { combo: 'Mod+Shift+C', desc: 'Switch connection' },
      { combo: 'Mod+Alt+D', desc: 'Data view' },
      { combo: '/', desc: 'Focus the search box on screen' },
      { combo: 'Mod+/', desc: 'Keyboard shortcuts' },
      { combo: 'F11', desc: 'Toggle fullscreen' },
    ],
  },
  {
    label: 'Views',
    icon: 'monitor',
    shortcuts: [
      { combo: 'Mod+Shift+N', desc: 'New window' },
      { combo: 'Mod+Shift+D', desc: 'Disconnect' },
      { combo: 'Mod+Shift+S', desc: 'SQL editor' },
      { combo: 'Mod+Shift+O', desc: 'ORM Runner' },
      { combo: 'Mod+Shift+X', desc: 'Extensions' },
      { combo: 'Mod+Shift+E', desc: 'Toggle AI panel' },
      { combo: 'Mod+I', desc: 'Toggle AI sidebar' },
      { combo: 'Mod+Shift+L', desc: 'Activity log' },
      { combo: 'Mod+R', desc: 'Refresh current view' },
      { combo: 'F5', desc: 'Refresh current view (the table, not the app)' },
      { combo: 'Mod+Shift+V', desc: 'Cycle table data view' },
      { combo: 'Alt+1-5', desc: 'Table / JSON / Record / Text / Chart view' },
    ],
  },
  {
    label: 'SQL Editor',
    icon: 'terminal',
    shortcuts: [
      { combo: 'Mod+Enter', desc: 'Run all statements' },
      { combo: 'Mod+R', desc: 'Run statement at cursor' },
      { combo: 'Mod+L', desc: 'Select current statement' },
      { combo: 'Mod+S', desc: 'Save query' },
      { combo: 'Mod+J', desc: 'Toggle output panel' },
      { combo: 'Mod+Shift+B', desc: 'Query history' },
    ],
  },
  {
    label: 'ORM Runner',
    icon: 'code-2',
    shortcuts: [
      { combo: 'Mod+Enter', desc: 'Run query' },
      { combo: 'Mod+S', desc: 'Format code' },
    ],
  },
  {
    label: 'Data Table',
    icon: 'table-2',
    shortcuts: [
      { combo: 'Mod+F', desc: 'Search rows' },
      // Both bindings are listed because both are real: macOS swallows Cmd+H to
      // hide the app, so Mod+Alt+F is the one that always arrives.
      { combo: 'Mod+H', desc: 'Find & replace in column' },
      { combo: 'Mod+Alt+F', desc: 'Find & replace in column' },
      { combo: 'Alt+A', desc: 'Open filter menu' },
      { combo: 'Alt+S', desc: 'Open sort menu' },
      { combo: 'Alt+C', desc: 'Open columns menu' },
      { combo: 'Alt+R', desc: 'Reset table view (clear filters/sort/search)' },
      { combo: 'Alt+F', desc: 'Filter by the focused cell\'s value' },
      { combo: 'Alt+E', desc: 'Exclude the focused cell\'s value' },
      { combo: 'Alt+X', desc: 'Clear the table search (Escape inside the box does too)' },
      { combo: 'Alt+N', desc: 'Stage a new row (again for another)' },
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
      { combo: 'Mod+Backspace', desc: 'Delete selected rows' },
      { combo: 'Mod+A', desc: 'Select all rows' },
      { combo: 'Mod+Shift+A', desc: 'Deselect all rows' },
      { combo: 'Mod+S', desc: 'Apply staged changes' },
      { combo: 'Mod+Alt+S', desc: 'Copy staged changes as SQL' },
      { combo: 'Alt+Backspace', desc: 'Discard staged changes' },
      { combo: 'Mod+Z', desc: 'Undo cell edit' },
      { combo: 'Mod+Shift+Z', desc: 'Redo cell edit' },
      { combo: 'Mod+Up', desc: 'Scroll to top' },
      { combo: 'Mod+Down', desc: 'Scroll to bottom' },
      { combo: 'Mod+Left', desc: 'Previous page' },
      { combo: 'Mod+Right', desc: 'Next page' },
      { combo: 'Mod+Shift+Left', desc: 'First page' },
      { combo: 'Mod+Shift+Right', desc: 'Last page' },
      { combo: 'Alt+Shift+1-5', desc: 'Jump to pinned table' },
    ],
  },
  {
    label: 'Data model',
    icon: 'network',
    shortcuts: [
      { combo: 'Mod+Shift+Enter', desc: 'Toggle full screen' },
      { combo: 'Mod+F', desc: 'Search tables (Diagram) or columns (Dictionary)' },
      { combo: 'Escape', desc: 'Exit full screen' },
    ],
  },
  {
    label: 'AI Chat',
    icon: 'bot',
    shortcuts: [
      { combo: 'Enter', desc: 'Send message' },
      { combo: 'Shift+Enter', desc: 'New line' },
      { combo: 'Mod+Shift+B', desc: 'Toggle conversation list' },
      { combo: 'Mod+Shift+T', desc: 'New conversation' },
    ],
  },
  {
    label: 'Appearance',
    icon: 'palette',
    shortcuts: [
      { combo: 'Mod+B', desc: 'Toggle sidebar' },
      { combo: 'Mod+Shift+T', desc: 'Toggle tab bar' },
      { combo: 'Mod+Shift+B', desc: 'Toggle status bar' },
      { combo: 'Mod+M', desc: 'Cycle theme' },
      { combo: 'Mod+Shift+M', desc: 'Previous theme' },
      { combo: 'Mod+Plus', desc: 'Zoom in' },
      { combo: 'Mod+Minus', desc: 'Zoom out' },
      { combo: 'Mod+0', desc: 'Reset zoom' },
    ],
  },
  {
    label: 'General',
    icon: 'settings',
    shortcuts: [
      { combo: 'Mod+?', desc: 'Keyboard shortcuts' },
      { combo: '?', desc: 'Keyboard shortcuts' },
      { combo: 'Mod+,', desc: 'Settings' },
      { combo: 'Escape', desc: 'Dismiss / close' },
    ],
  },
]
