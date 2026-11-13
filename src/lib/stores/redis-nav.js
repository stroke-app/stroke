// Bridge between the Redis page (which owns the data) and the app sidebar
// (which shows databases, views and namespaces). The page publishes its state
// here and registers handlers; the sidebar reads the state and calls them.
import { writable } from 'svelte/store'

/** @typedef {'keys' | 'insights' | 'slowlog' | 'clients' | 'pubsub'} RedisView */

export const redisNav = writable({
  view: /** @type {RedisView} */ ('keys'),
  currentDb: 0,
  /** @type {Record<number, number>} */
  dbKeys: {},
  /** @type {[string, number][]} */
  namespaces: [],
  filter: '',
  consoleOpen: false,
  switching: false,
  /** Staged edits block switching databases. */
  locked: false,
  /** @type {{ version: string, memory: string, clients: string, ops: string } | null} */
  server: null,
})

// Clicks travel as a window event, so they reach the page however the two
// components were loaded (a module-level handler map was lost across hot reloads).
const EVENT = 'stroke:redis-nav'

/** @param {Record<string, (...a: any[]) => void>} h */
export function registerRedisNav(h) {
  /** @param {Event} e */
  const on = (e) => {
    const { name, args } = /** @type {CustomEvent} */ (e).detail ?? {}
    pending = null
    h[name]?.(...(args ?? []))
  }
  window.addEventListener(EVENT, on)
  // Replay a click that opened the page in the first place.
  if (pending && Date.now() - pending.at < 3000) h[pending.name]?.(...pending.args)
  pending = null
  return () => window.removeEventListener(EVENT, on)
}

/** A click made while the Redis page was not open; it runs once the page mounts. */
let pending = /** @type {{ name: string, args: any[], at: number } | null} */ (null)

/** @param {string} name @param {...any} args */
export function redisNavCall(name, ...args) {
  pending = { name, args, at: Date.now() }
  window.dispatchEvent(new CustomEvent(EVENT, { detail: { name, args } }))
}

/** Whether the app's ⌘J terminal dock is open (StudioShell keeps it current). */
export const terminalDockOpen = writable(false)

/** Open or close the ⌘J terminal dock from anywhere. */
export function toggleTerminalDock() {
  window.dispatchEvent(new CustomEvent('stroke:terminal-dock-toggle'))
}
