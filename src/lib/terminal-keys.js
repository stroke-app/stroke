/**
 * How the terminal tab's client edits the line being typed, and so how the
 * edits the page makes for the user (empty the line, jump a word) have to be
 * spelled for it.
 *
 * - `readline`: psql, mysql and sqlite3 link GNU readline or editline on Linux
 *   and macOS; go-sqlcmd (liner) and redis-cli (linenoise) share its Ctrl keys.
 * - `console`: the client has no line editor. On Windows, psql (EDB's build),
 *   mysql, mariadb, sqlite3 and the ODBC sqlcmd read whole lines from the
 *   console, and the console edits the line with cmd.exe's keys: Home, End,
 *   Ctrl+Left/Right, Ctrl+Backspace, Ctrl+Home/End. Any other Ctrl key goes
 *   into the line as text (`^W`, `^E`), and Tab completes nothing. ConPTY
 *   turns the xterm sequences below into those keys.
 *
 * Pure: no DOM, no client.
 */

/** @typedef {'readline' | 'console'} LineEditor */

const CONSOLE_CLIENTS = new Set(['psql', 'mysql', 'mariadb', 'sqlite3'])

/**
 * @param {{ name: string, version?: string | null } | null | undefined} client
 * @param {'windows' | 'macos' | 'linux'} os
 * @returns {LineEditor}
 */
export function lineEditorFor(client, os) {
  if (os !== 'windows' || !client) return 'readline'
  // The ODBC sqlcmd has no --version; go-sqlcmd has one, and edits its line with liner.
  if (CONSOLE_CLIENTS.has(client.name) || (client.name === 'sqlcmd' && !client.version)) return 'console'
  return 'readline'
}

/** @type {Record<LineEditor, string>} */
const CLEAR_LINE = {
  // Ctrl+E, Ctrl+U: to the end, then delete back to the prompt. readline,
  // editline, liner and linenoise all know both.
  readline: '\x05\x15',
  // End, then Ctrl+Home, which deletes back to the prompt. Not Escape, which
  // clears the line too: ConPTY would read it and the next key as one Alt combo.
  console: '\x1b[F\x1b[1;5H',
}

/** Keystrokes that empty the typed line, wherever the cursor is in it. @param {LineEditor} editor */
export function clearLineKeys(editor) {
  return CLEAR_LINE[editor]
}

/**
 * The editing keys people bring from an editor, as the console takes them.
 * null leaves the key to xterm (Home, End and Ctrl+arrows it already sends in
 * a form ConPTY reads); '' drops it.
 * @param {{ key: string, ctrlKey: boolean, altKey: boolean }} e without Shift or Meta
 * @returns {string | null}
 */
export function consoleKeystrokes(e) {
  const mod = e.ctrlKey || e.altKey
  switch (e.key) {
    // ^H is how ConPTY spells Ctrl+Backspace, the console's delete-word.
    case 'Backspace': return mod ? '\x08' : null
    case 'ArrowLeft': return mod ? '\x1b[1;5D' : null
    case 'ArrowRight': return mod ? '\x1b[1;5C' : null
  }
  if (!e.ctrlKey || e.altKey) return null
  switch (e.key.toLowerCase()) {
    case 'e': return '\x1b[F'
    // Ctrl+Home: delete back to the prompt, as Ctrl+U does in readline.
    case 'u': return '\x1b[1;5H'
    // Ctrl+Z and Enter is end of input to the console: the client would quit.
    case 'z': return ''
  }
  return null
}

/** Control keys the console acts on: Enter, Ctrl+C, Ctrl+Backspace, and Escape, which also starts every key sequence. */
const CONSOLE_CONTROLS = new Set(['\r', '\x03', '\x08', '\x1b'])

/**
 * What of xterm's input goes on to a console client: a lone Ctrl key the
 * console would only type into the line (`^R`, `^D`) is dropped.
 * @param {string} data
 */
export function consoleInput(data) {
  return data.length === 1 && data < ' ' && !CONSOLE_CONTROLS.has(data) ? '' : data
}

/** @type {Record<string, string>} */
const CONSOLE_CLEAR = { psql: '\\! cls', sqlite3: '.shell cls' }

/**
 * How a client clears the screen at the Windows console, which has no Ctrl+L:
 * it runs `cls` through its shell escape. null when it has none that works there.
 * @param {string} client
 */
export function consoleClearCommand(client) {
  return CONSOLE_CLEAR[client] ?? null
}
