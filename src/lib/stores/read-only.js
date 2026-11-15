/**
 * Read-only mode - the session-wide promise that nothing gets written.
 *
 * A prop threaded from StudioShell reached the grid and the record view, which
 * left every other surface (the sidebar's Drop table, the structure editor's
 * ALTER/DROP buttons, the AI tool calls, Backup's restore) free to write on a
 * connection the user had explicitly opened read-only.
 *
 * So the flag lives here instead, and the real gate sits in `api.js` where every
 * mutation funnels through: even a path nobody remembered to disable is refused.
 * Components import `readOnlyMode` to *show* the state - disabled buttons, a
 * reason in the tooltip - but they are not what enforces it.
 */
import { writable, get } from 'svelte/store'
import { toast } from '$lib/components/ui/sonner/toast.svelte.js'

/** @type {import('svelte/store').Writable<boolean>} */
export const readOnlyMode = writable(false)

/** Non-reactive read, for guards inside plain functions. */
export function isReadOnly() {
  return get(readOnlyMode)
}

/** @param {boolean} on */
export function setReadOnly(on) {
  readOnlyMode.set(!!on)
}

/** Thrown by the api layer when a mutation is attempted in read-only mode. */
export class ReadOnlyError extends Error {
  /** @param {string} what the action, phrased for a sentence: "drop this table" */
  constructor(what) {
    super(`This connection is open in read-only mode, so it can't ${what}.`)
    this.name = 'ReadOnlyError'
    /** Marks the error for callers that want to special-case it. */
    this.readOnly = true
  }
}

/**
 * The hard gate. Throws in read-only mode, so a caller that forgot to check
 * still cannot write.
 * @param {string} what
 */
export function assertWritable(what) {
  if (get(readOnlyMode)) throw new ReadOnlyError(what)
}

/**
 * The soft gate, for UI handlers: reports the refusal and returns false so the
 * caller can bail without a try/catch.
 * @param {string} what
 * @returns {boolean} true when the action may proceed
 */
export function guardWrite(what) {
  if (!get(readOnlyMode)) return true
  // One toast, refreshed: a few tries in a row (double-click, then typing)
  // replace it rather than stacking a pile of the same warning.
  toast.warning('Read-only connection', {
    id: 'read-only-mode',
    description: `Turn read-only off in the status bar to ${what}.`,
    duration: 4000,
  })
  return false
}

/** Shared tooltip for a control disabled by read-only mode. */
export const READ_ONLY_HINT = 'Disabled - this connection is open in read-only mode'
