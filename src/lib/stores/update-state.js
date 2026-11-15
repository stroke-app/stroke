// What the updater is doing, for anything besides its own window that shows
// it (Settings → Updates). UpdateDialog owns the state and registers the
// actions; the rest only reads and asks.

import { writable } from 'svelte/store'

/**
 * @typedef {{
 *   status: 'idle' | 'available' | 'downloading' | 'ready' | 'error' | 'up-to-date',
 *   checking: boolean,
 *   version: string,
 *   progress: number,
 *   applyOnQuit: boolean,
 * }} UpdateSnapshot
 */

/** @type {import('svelte/store').Writable<UpdateSnapshot>} */
export const updateState = writable({ status: 'idle', checking: false, version: '', progress: 0, applyOnQuit: false })

/** Set by UpdateDialog once it is mounted. */
export const updateActions = {
  /** Look for an update now. */
  check: () => {},
  /** Open the update window for what was found. */
  show: () => {},
  /** Install the downloaded build and relaunch. */
  restart: () => {},
}
