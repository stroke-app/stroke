/**
 * App commands a code editor answers itself, on whatever the user bound them to.
 *
 * CodeMirror's own keymaps sit in front of the app's hotkeys (Mod-I selects the
 * parent syntax node, and a chord the editor handles stops there), so the SQL
 * and ORM editors take the app's chords first and run them. They used to list
 * the default keys (`Mod-k`, `Mod-w`, …) in a keymap, which ignored a remap:
 * Ctrl+K kept opening the palette in the editor after it was moved to Alt+K,
 * and Alt+K did nothing there. Matching through `matchesCommand` reads the
 * live binding instead, and an unbound command matches nothing.
 */
import { Prec } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import { matchesCommand } from '$lib/command-hotkey.js'

/**
 * @typedef {[commandId: string, run: (() => void) | undefined]} CommandKey
 * A command id from COMMANDS and its handler; an unwired handler passes the
 * key on to the rest of the app.
 */

/**
 * @param {() => CommandKey[]} getKeys read on every keydown, so handlers passed
 *   as props stay current
 */
export function commandKeys(getKeys) {
  return Prec.highest(
    EditorView.domEventHandlers({
      keydown(e) {
        for (const [id, run] of getKeys()) {
          if (run && matchesCommand(e, id)) {
            e.preventDefault()
            run()
            return true
          }
        }
        return false
      },
    }),
  )
}
