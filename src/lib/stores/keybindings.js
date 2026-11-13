/**
 * The user's keyboard-shortcut overrides.
 *
 * Defaults live in `$lib/shortcuts.js` (COMMANDS, built from SHORTCUT_GROUPS);
 * this store only keeps what the user changed, as `{ [commandId]: combo }`. An
 * empty combo means "unbound". The combo a command answers to is
 * `override ?? default` - `bindingFor(id)` - and every hotkey site, menu label
 * and the native menu read it through that, so a remap applies at once.
 *
 * Everything that decides something (conflicts, what a keydown records, the
 * native accelerator) is a pure function of its arguments so it can be tested
 * without a DOM.
 */

import { writable, get, fromStore } from 'svelte/store'
import { COMMANDS, COMMAND_BY_ID, FIXED_SHORTCUTS, SHORTCUT_GROUPS, rowCommandIds, IS_MAC, comboTitle } from '$lib/shortcuts.js'
import { notifyLocalChange } from '$lib/sync/changes.js'

const STORAGE_KEY = 'stroke:keybindings'

/** @typedef {Record<string, string>} Overrides */

/**
 * Keep only string overrides for commands that exist. A stale id (a command
 * that was removed) would otherwise sit in storage forever and could report a
 * conflict with nothing the user can see.
 * @param {unknown} raw
 * @returns {Overrides}
 */
export function sanitizeOverrides(raw) {
  /** @type {Overrides} */
  const out = {}
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return out
  for (const [id, combo] of Object.entries(raw)) {
    const cmd = COMMAND_BY_ID.get(id)
    if (!cmd || typeof combo !== 'string') continue
    const c = combo.trim()
    if (c === cmd.combo) continue
    out[id] = c
  }
  return out
}

/** @returns {Overrides} */
function load() {
  try {
    if (typeof localStorage === 'undefined') return {}
    return sanitizeOverrides(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}'))
  } catch {
    return {}
  }
}

/** @param {Overrides} value */
function persist(value) {
  try {
    if (typeof localStorage === 'undefined') return
    if (Object.keys(value).length === 0) localStorage.removeItem(STORAGE_KEY)
    else localStorage.setItem(STORAGE_KEY, JSON.stringify(value))
  } catch {
    /* storage full or blocked: the remap still holds for this session */
  }
}

/** Overrides only - see `bindingFor` for the combo a command answers to. */
export const keybindings = writable(load())

/** @param {(prev: Overrides) => Overrides} fn */
function update(fn) {
  const next = fn(get(keybindings))
  keybindings.set(next)
  persist(next)
  notifyLocalChange('settings')
}

/**
 * Bind `id` to `combo`; '' unbinds it. Binding a command back to its default
 * drops the override, so "differs from default" is just "has an override".
 * @param {string} id @param {string} combo
 */
export function setBinding(id, combo) {
  const cmd = COMMAND_BY_ID.get(id)
  if (!cmd) return
  update((prev) => {
    const next = { ...prev }
    if (combo === cmd.combo) delete next[id]
    else next[id] = combo
    return next
  })
}

/** @param {string} id */
export function unbind(id) {
  setBinding(id, '')
}

/** @param {string} id */
export function resetBinding(id) {
  update((prev) => {
    const next = { ...prev }
    delete next[id]
    return next
  })
}

export function resetAllBindings() {
  update(() => ({}))
}

/**
 * Take a full set of overrides from sync. Not a local change.
 * @param {unknown} raw
 */
export function replaceKeybindings(raw) {
  const next = sanitizeOverrides(raw)
  keybindings.set(next)
  persist(next)
}

/**
 * Bind `id` to `combo`, unbinding whichever command held it first.
 * @param {string} id @param {string} combo @param {string | null} displacedId
 */
export function assignReplacing(id, combo, displacedId) {
  const cmd = COMMAND_BY_ID.get(id)
  if (!cmd) return
  update((prev) => {
    const next = { ...prev }
    if (displacedId && displacedId !== id && COMMAND_BY_ID.has(displacedId)) next[displacedId] = ''
    if (combo === cmd.combo) delete next[id]
    else next[id] = combo
    return next
  })
}

// ── Reading ─────────────────────────────────────────────────────────────────

/**
 * @param {Overrides} overrides @param {string} id
 * @returns {string} the combo, or '' when unbound or unknown
 */
export function effectiveCombo(overrides, id) {
  if (Object.prototype.hasOwnProperty.call(overrides, id)) return overrides[id]
  return COMMAND_BY_ID.get(id)?.combo ?? ''
}

/** @param {Overrides} overrides @param {string} id */
export function isOverridden(overrides, id) {
  return Object.prototype.hasOwnProperty.call(overrides, id)
}

/** Reactive view of the overrides: reads inside an effect or template subscribe. */
const live = fromStore(keybindings)

/**
 * The combo `id` answers to right now ('' when unbound). Reactive when read in
 * a `$derived`, `$effect`, template, or a `createHotkey` getter.
 * @param {string} id
 */
export function bindingFor(id) {
  return effectiveCombo(live.current, id)
}

/**
 * A tooltip: `Close (⌘W)`, or just `Close` when the command is unbound.
 * Reactive like `bindingFor`.
 * @param {string} label @param {string} id
 */
export function withShortcut(label, id) {
  const combo = bindingFor(id)
  return combo ? `${label} (${comboTitle(combo)})` : label
}

/**
 * True while the keybindings page is recording a combo. The few bindings that
 * are capture-phase window listeners (⌘B, ⌘P, Ctrl+Tab, F11) run before the
 * recorder can stop the key, so `matchesCommand` reads this and stands down.
 */
let recording = false
/** @param {boolean} on */
export function setRecordingShortcut(on) { recording = on }
export function isRecordingShortcut() { return recording }

// ── Combos ──────────────────────────────────────────────────────────────────

const MOD_ALIAS = /** @type {Record<string, string>} */ ({
  ctrl: 'Control', control: 'Control',
  alt: 'Alt', option: 'Alt', opt: 'Alt',
  shift: 'Shift',
  meta: 'Meta', cmd: 'Meta', command: 'Meta', super: 'Meta', win: 'Meta',
})
const MOD_ORDER = ['Control', 'Alt', 'Shift', 'Meta']
const KEY_ALIAS = /** @type {Record<string, string>} */ ({
  up: 'ArrowUp', down: 'ArrowDown', left: 'ArrowLeft', right: 'ArrowRight',
  arrowup: 'ArrowUp', arrowdown: 'ArrowDown', arrowleft: 'ArrowLeft', arrowright: 'ArrowRight',
  esc: 'Escape', escape: 'Escape', return: 'Enter', enter: 'Enter',
  space: 'Space', ' ': 'Space', spacebar: 'Space',
  plus: '+', minus: '-', del: 'Delete', delete: 'Delete', backspace: 'Backspace', tab: 'Tab',
})

/**
 * One spelling per physical chord, for comparing two combos: `Mod` resolved for
 * the platform, aliases folded (`Ctrl`/`Control`, `Up`/`ArrowUp`), modifiers in
 * a fixed order, the key upper-cased. '' for an empty combo.
 * @param {string} combo @param {boolean} [isMac]
 */
export function canonicalCombo(combo, isMac = IS_MAC) {
  if (!combo) return ''
  const parts = combo.split('+')
  // A trailing '+' key (`Mod++`) splits into an empty last part.
  let key = parts.pop() ?? ''
  if (key === '' && parts.length && parts[parts.length - 1] === '') { parts.pop(); key = '+' }
  const mods = new Set()
  for (const p of parts) {
    const lower = p.trim().toLowerCase()
    if (lower === 'mod') mods.add(isMac ? 'Meta' : 'Control')
    else if (MOD_ALIAS[lower]) mods.add(MOD_ALIAS[lower])
  }
  const k = KEY_ALIAS[key.toLowerCase()] ?? (key.length === 1 ? key.toUpperCase() : key.replace(/^f(\d+)$/i, 'F$1'))
  return [...MOD_ORDER.filter((m) => mods.has(m)), k].join('+')
}

/**
 * @typedef {{ id: string | null, label: string, group: string, fixed: boolean }} Conflict
 */

/**
 * Who else answers to `combo`: a remappable command (which the user may unbind
 * to make room) or, failing that, a built-in key of the editor or grid (which
 * they can only be warned about). Ignores `exceptId` itself.
 * @param {Overrides} overrides @param {string} combo @param {string | null} [exceptId]
 * @param {boolean} [isMac]
 * @returns {Conflict | null}
 */
export function findConflict(overrides, combo, exceptId = null, isMac = IS_MAC) {
  const want = canonicalCombo(combo, isMac)
  if (!want) return null
  for (const cmd of COMMANDS) {
    if (cmd.id === exceptId) continue
    if (canonicalCombo(effectiveCombo(overrides, cmd.id), isMac) === want) {
      return { id: cmd.id, label: cmd.label, group: cmd.group, fixed: false }
    }
  }
  for (const s of FIXED_SHORTCUTS) {
    if (canonicalCombo(s.combo, isMac) === want) return { id: null, label: s.label, group: s.group, fixed: true }
  }
  return null
}

// ── Recording ───────────────────────────────────────────────────────────────

/** Keys that are only ever held with another key. Pressed alone they record nothing. */
const MODIFIER_KEYS = new Set(['Shift', 'Control', 'Alt', 'AltGraph', 'Meta', 'OS', 'Super', 'Hyper', 'Fn', 'FnLock', 'CapsLock', 'NumLock', 'ScrollLock', 'Symbol', 'SymbolLock'])

/** Physical punctuation keys, read off `code` so Shift or ⌥ can't change them. */
const PUNCTUATION_CODE = /** @type {Record<string, string>} */ ({
  Backquote: '`', Backslash: '\\', BracketLeft: '[', BracketRight: ']', Comma: ',',
  Equal: '=', Minus: '-', Period: '.', Semicolon: ';', Slash: '/',
})

/**
 * The key part of a keydown, or null when it is a modifier on its own or a key
 * we cannot name. Letters and digits come from `code`, so ⌥A records `Alt+A`
 * rather than `Alt+å` and ⇧1 records `Shift+1` rather than `Shift+!`.
 * @param {{ key: string, code?: string }} e
 */
function keyName(e) {
  const key = e.key ?? ''
  if (!key || MODIFIER_KEYS.has(key) || key === 'Unidentified' || key === 'Process') return null
  const code = e.code ?? ''
  if (/^Key[A-Z]$/.test(code)) return code.slice(3)
  if (/^Digit\d$/.test(code)) return code.slice(5)
  if (PUNCTUATION_CODE[code]) return PUNCTUATION_CODE[code]
  if (key === 'Dead') return null
  if (key === ' ') return 'Space'
  if (key === 'Esc') return 'Escape'
  if (key === '+') return 'Plus'
  if (/^f\d{1,2}$/i.test(key)) return key.toUpperCase()
  if (key.length === 1) return key.toUpperCase()
  return key
}

/**
 * @typedef {{ type: 'cancel' } | { type: 'clear' } | { type: 'combo', combo: string }} RecordResult
 */

/**
 * What a keydown means to the recorder. `null` means "keep listening": a bare
 * modifier (Shift, ⌘, ...) never records - the combo is taken only once a
 * non-modifier key arrives with whatever modifiers are held.
 *
 * Escape alone cancels. Backspace or Delete alone unbinds. With a modifier held
 * they are ordinary keys (⌘⌫ is a real shortcut).
 *
 * The combo is written in the hotkey grammar: `Mod` for ⌘ on macOS and Ctrl
 * elsewhere, so a binding made on one platform means the same on the other.
 * @param {{ key: string, code?: string, metaKey?: boolean, ctrlKey?: boolean, altKey?: boolean, shiftKey?: boolean }} e
 * @param {boolean} [isMac]
 * @returns {RecordResult | null}
 */
export function recordKeydown(e, isMac = IS_MAC) {
  const key = keyName(e)
  if (!key) return null
  const meta = !!e.metaKey, ctrl = !!e.ctrlKey, alt = !!e.altKey, shift = !!e.shiftKey
  const anyMod = meta || ctrl || alt || shift
  if (!anyMod && key === 'Escape') return { type: 'cancel' }
  if (!anyMod && (key === 'Backspace' || key === 'Delete')) return { type: 'clear' }
  const parts = []
  const mod = isMac ? meta : ctrl
  if (mod) parts.push('Mod')
  if (isMac ? ctrl : false) parts.push('Ctrl')
  if (alt) parts.push('Alt')
  if (shift) parts.push('Shift')
  if (isMac ? false : meta) parts.push('Meta')
  parts.push(key)
  return { type: 'combo', combo: parts.join('+') }
}

// ── Lists ───────────────────────────────────────────────────────────────────

/**
 * SHORTCUT_GROUPS with each remappable row showing what it is bound to now. A
 * family (`Mod+1-9`) stays one row while untouched and splits into its members
 * once any of them is changed. Unbound rows drop out.
 * @param {Overrides} overrides
 * @returns {import('$lib/shortcuts.js').ShortcutGroup[]}
 */
export function effectiveShortcutGroups(overrides) {
  return SHORTCUT_GROUPS.map((g) => ({
    ...g,
    shortcuts: g.shortcuts.flatMap((s) => {
      const ids = rowCommandIds(s)
      if (ids.length === 0) return [s]
      if (!ids.some((id) => isOverridden(overrides, id))) return [s]
      return ids
        .map((id) => ({ combo: effectiveCombo(overrides, id), desc: ids.length > 1 ? COMMAND_BY_ID.get(id)?.label ?? s.desc : s.desc, id }))
        .filter((r) => r.combo)
    }),
  }))
}

// ── Native menu ─────────────────────────────────────────────────────────────

/** Native (macOS) menu item → the command whose key it shows. */
export const NATIVE_MENU_COMMANDS = /** @type {Record<string, string>} */ ({
  newTab: 'tab.new',
  newWindow: 'window.new',
  disconnect: 'connection.disconnect',
  closeTab: 'tab.close',
  closeAllTabs: 'tab.closeAll',
  search: 'table.search',
  findReplace: 'table.findReplaceAlt',
  applyEdits: 'table.apply',
  copyEditsSql: 'table.copySql',
  toggleSidebar: 'view.sidebar',
  toggleChat: 'ai.sidebar',
  fullscreen: 'view.fullscreen',
  commandPalette: 'palette.open',
  logs: 'view.logs',
  extensions: 'view.extensions',
  shortcuts: 'help.shortcuts',
  openSettings: 'settings.open',
})

const ACCEL_KEY = /** @type {Record<string, string>} */ ({
  ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right', Plus: '=', Minus: '-',
})

/**
 * A combo as a Tauri menu accelerator (`Mod+Shift+T` → `CmdOrCtrl+Shift+T`),
 * or null when unbound or not expressible (a shifted symbol like `?`).
 * @param {string} combo
 */
export function toAccelerator(combo) {
  if (!combo) return null
  const parts = combo.split('+')
  const key = parts.pop() ?? ''
  if (!key) return null
  const mods = parts.map((p) => {
    const l = p.toLowerCase()
    if (l === 'mod') return 'CmdOrCtrl'
    if (l === 'ctrl' || l === 'control') return 'Ctrl'
    if (l === 'meta' || l === 'cmd' || l === 'command') return 'Super'
    if (l === 'alt' || l === 'option') return 'Alt'
    if (l === 'shift') return 'Shift'
    return null
  })
  if (mods.includes(null)) return null
  const k = ACCEL_KEY[key] ?? key
  if (k.length === 1 && !/[A-Za-z0-9`\-=[\]\\;',./]/.test(k)) return null
  return [...mods, k.length === 1 ? k.toUpperCase() : k].join('+')
}

/**
 * Every native menu item's accelerator under `overrides`.
 * @param {Overrides} overrides
 * @returns {Record<string, string | null>}
 */
export function nativeMenuAccelerators(overrides) {
  /** @type {Record<string, string | null>} */
  const out = {}
  for (const [action, id] of Object.entries(NATIVE_MENU_COMMANDS)) out[action] = toAccelerator(effectiveCombo(overrides, id))
  return out
}
