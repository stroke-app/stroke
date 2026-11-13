/**
 * `createHotkey` for a remappable command: the combo is whatever the user bound
 * `id` to (see $lib/stores/keybindings.js), read through a getter so a remap
 * re-registers the hotkey at once. Call it where you would call createHotkey -
 * during component init.
 *
 * An unbound command stays registered at its default combo but disabled, so
 * the library never sees an empty combo. Conflicts are checked on the
 * keybindings page, so the library's own duplicate warning is turned off.
 */

import { createHotkey, matchesKeyboardEvent } from '@tanstack/svelte-hotkeys'
import { COMMAND_BY_ID } from '$lib/shortcuts.js'
import { bindingFor, isRecordingShortcut } from '$lib/stores/keybindings.js'

/**
 * @param {string} id a command id from COMMANDS
 * @param {import('@tanstack/svelte-hotkeys').HotkeyCallback} callback
 * @param {import('@tanstack/svelte-hotkeys').CreateHotkeyOptions} [options]
 */
export function createCommandHotkey(id, callback, options = {}) {
  // A missing command must not take the component down with it (it once
  // crashed the whole sidebar): no shortcut, and a note for whoever adds one.
  if (!COMMAND_BY_ID.has(id)) {
    console.warn(`[hotkeys] unknown keyboard command: ${id}`)
    return
  }
  // Unbound-by-default commands still register, so a combo the user assigns
  // later works; until then the placeholder is never enabled.
  const fallback = COMMAND_BY_ID.get(id)?.combo || 'F24'
  createHotkey(
    () => /** @type {any} */ (bindingFor(id) || fallback),
    callback,
    () => ({
      ...options,
      conflictBehavior: 'allow',
      enabled: !!bindingFor(id) && options.enabled !== false,
    }),
  )
}

/**
 * Whether a raw keydown is `id`'s current combo. For the few bindings that
 * have to be a capture-phase window listener rather than a hotkey (⌘B, ⌘P,
 * Ctrl+Tab, F11), so they follow a remap too. An unbound command matches
 * nothing.
 * @param {KeyboardEvent} e @param {string} id
 */
export function matchesCommand(e, id) {
  if (isRecordingShortcut()) return false
  const combo = bindingFor(id)
  return !!combo && matchesKeyboardEvent(e, combo)
}
