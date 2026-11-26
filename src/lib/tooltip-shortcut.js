/**
 * Pull the shortcut out of a tooltip so it can be drawn as keycaps.
 *
 * Titles across the app carry their shortcut in parentheses, written by
 * `withShortcut` / `comboTitle`: `Refresh (Ctrl+R)` off macOS, `Refresh (⌘R)`
 * on it, a bare key as `Open (Enter)`. The tooltip used to print that as text,
 * so the key was the least visible part of the bubble. Split off, it renders as
 * caps beside the label, which is how a shortcut gets learned on hover.
 *
 * Only a group that is a whole combo counts. `(Ctrl+scroll)`, `(Ctrl+F, or
 * Shift+Tab from the list)` and `(read only)` stay in the text, and a title with
 * two combos keeps both as text, because one cap row could not say which half
 * of the sentence each belongs to.
 */

const MOD_WORDS = new Map([
  ['ctrl', 'Ctrl'], ['control', 'Ctrl'], ['alt', 'Alt'], ['option', 'Alt'],
  ['shift', '⇧'], ['win', 'Win'], ['cmd', '⌘'], ['meta', '⌘'],
])
const MOD_GLYPHS = new Set(['⌘', '⇧', '⌥', '⌃'])

const NAMED_KEYS = new Map([
  ['enter', '↵'], ['return', '↵'], ['esc', 'Esc'], ['escape', 'Esc'], ['tab', 'Tab'],
  ['space', 'Space'], ['backspace', '⌫'], ['delete', 'Del'], ['del', 'Del'],
  ['home', 'Home'], ['end', 'End'], ['pageup', 'PgUp'], ['pagedown', 'PgDn'],
  ['up', '↑'], ['down', '↓'], ['left', '←'], ['right', '→'],
])
const GLYPH_KEYS = new Set(['↵', '⌫', '↑', '↓', '←', '→', '⇥', '⎋'])

/**
 * The cap for one key token, or null when the token is not a key.
 * @param {string} t
 */
function keyCap(t) {
  if (!t) return null
  if (GLYPH_KEYS.has(t)) return t
  const named = NAMED_KEYS.get(t.toLowerCase())
  if (named) return named
  if (/^f([1-9]|1[0-9]|2[0-4])$/i.test(t)) return t.toUpperCase()
  // One printable character: a letter, digit or mark (`/`, `,`, `` ` ``).
  if ([...t].length === 1 && /\S/.test(t)) return /[a-z]/.test(t) ? t.toUpperCase() : t
  return null
}

/**
 * Caps for one combo written as text, or null when it is not one.
 * Accepts `Ctrl+Shift+P`, `Ctrl+↵`, `Ctrl↵` (a template that glued `${mod}↵`),
 * `⌘⇧P`, `⌘↵` and a single key (`Esc`, `/`, `F11`).
 * @param {string} raw
 * @returns {string[] | null}
 */
export function parseCombo(raw) {
  let s = raw.trim()
  if (!s || s.length > 24) return null
  /** @type {string[]} */
  const caps = []
  // Leading modifiers, as glyphs or as words, with or without a `+` after.
  for (;;) {
    const ch = s[0]
    if (MOD_GLYPHS.has(ch)) { caps.push(ch); s = s.slice(1).replace(/^\+/, ''); continue }
    const word = /^([a-z]+)(\+|(?=[^a-z]))/i.exec(s)
    if (word && MOD_WORDS.has(word[1].toLowerCase()) && s.length > word[1].length) {
      caps.push(/** @type {string} */ (MOD_WORDS.get(word[1].toLowerCase())))
      s = s.slice(word[0].length)
      continue
    }
    break
  }
  const key = keyCap(s)
  if (!key) return null
  caps.push(key)
  return caps
}

/**
 * Split a tooltip into its label and the caps of its one shortcut.
 * @param {string} text
 * @returns {{ label: string, keys: string[] | null }}
 */
export function splitTipShortcut(text) {
  const groups = [...text.matchAll(/\s*\(([^()]{1,24})\)/g)]
  const combos = groups.map((m) => ({ m, caps: parseCombo(m[1]) })).filter((g) => g.caps)
  if (combos.length !== 1) return { label: text, keys: null }
  const { m, caps } = combos[0]
  const at = /** @type {number} */ (m.index)
  const label = (text.slice(0, at) + text.slice(at + m[0].length)).trim()
  if (!label) return { label: text, keys: null }
  return { label, keys: caps }
}
