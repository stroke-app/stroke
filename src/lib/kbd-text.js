/**
 * The keys of a shortcut written as inline code, so the assistant's `Cmd+K`
 * renders as key caps instead of a code chip that reads like an identifier.
 *
 * Only text that is unmistakably a chord converts: at least one modifier and
 * one key (`Ctrl+Shift+P`, `Cmd + Enter`), the macOS glyph form (`⌘⇧P`), or a
 * lone key no SQL word shares (`Esc`, `Enter`, `F5`). `DELETE`, `END` and
 * `HOME` stay code: in a database chat they are far more often SQL or a value.
 */

const MODIFIERS = new Set([
  'cmd', 'command', 'ctrl', 'control', 'alt', 'option', 'opt', 'shift',
  'meta', 'mod', 'win', 'super', 'fn', '⌘', '⌥', '⇧', '⌃',
])

// Keys that follow a modifier. A single printable character is a key too.
const NAMED_KEYS = new Set([
  'enter', 'return', 'esc', 'escape', 'tab', 'space', 'backspace', 'delete',
  'del', 'home', 'end', 'pageup', 'pagedown', 'pgup', 'pgdn', 'insert', 'up',
  'down', 'left', 'right', '↑', '↓', '←', '→', '↵', '⏎', '⌫', '⌦',
])

// Keys that convert on their own, with no modifier.
const LONE_KEYS = new Set(['enter', 'return', 'esc', 'escape', 'tab', 'backspace', '↵', '⏎'])

const FN_KEY = /^f([1-9]|1[0-9]|2[0-4])$/i

/** @param {string} k */
const isKey = (k) => [...k].length === 1 || NAMED_KEYS.has(k.toLowerCase()) || FN_KEY.test(k)

/**
 * @param {string} text the inline code's text, unescaped
 * @returns {string[] | null} the keys in order, or null when it is not a shortcut
 */
export function shortcutKeys(text) {
  const t = text.trim()
  if (!t || t.length > 40) return null
  if (LONE_KEYS.has(t.toLowerCase()) || FN_KEY.test(t)) return [t]
  // ⌘⇧P, ⌘↵: glyph modifiers run together, then one key. Kept as one cap,
  // the way the app prints its own chords on macOS.
  const glyphs = /^([⌘⌥⇧⌃]+)\s*(\S+)$/u.exec(t)
  if (glyphs && isKey(glyphs[2])) return [t]
  // `Cmd++` and `Ctrl+-` end in the key that is also the separator.
  const parts = t.endsWith('++') ? [...t.slice(0, -2).split(/\s*\+\s*/), '+'] : t.split(/\s*\+\s*/)
  if (parts.length < 2 || parts.some((p) => !p)) return null
  const key = parts[parts.length - 1]
  const mods = parts.slice(0, -1)
  if (!mods.every((m) => MODIFIERS.has(m.toLowerCase())) || !isKey(key)) return null
  return parts
}
