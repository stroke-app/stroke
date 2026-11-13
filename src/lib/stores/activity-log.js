/**
 * @typedef {'connect' | 'disconnect' | 'table_open' | 'row_fetch' | 'row_save' | 'row_delete' | 'row_insert' | 'sql_exec' | 'export' | 'schema_change' | 'filter' | 'error' | 'mcp'} ActivityType
 */

/**
 * @typedef {{
 *   id: string
 *   type: ActivityType
 *   title: string
 *   detail?: string
 *   schema?: string
 *   table?: string
 *   rowCount?: number
 *   success: boolean
 *   durationMs?: number
 *   error?: string
 *   timestamp: number
 * }} ActivityEntry
 */

const MAX_ENTRIES = 100

/** @type {ActivityEntry[]} */
let _log = []
/** @type {((entries: ActivityEntry[]) => void)[]} */
let _listeners = []
let _rafId = 0

/**
 * Notify all subscribers with a fresh shallow copy so Svelte's equality
 * check always sees a new reference and triggers re-renders. The copy is
 * scheduled via rAF so rapid bursts coalesce into one update per frame.
 */
function scheduleNotify() {
  if (_rafId) return
  _rafId = requestAnimationFrame(() => {
    _rafId = 0
    const snapshot = _log.slice() // shallow copy - new ref every frame
    for (const fn of _listeners) fn(snapshot)
  })
}

/**
 * @param {Omit<ActivityEntry, 'id' | 'timestamp'>} entry
 * @returns {ActivityEntry}
 */
export function recordActivity(entry) {
  const full = /** @type {ActivityEntry} */ ({
    id: crypto.randomUUID(),
    timestamp: Date.now(),
    ...entry,
  })
  _log.unshift(full)
  if (_log.length > MAX_ENTRIES) _log.length = MAX_ENTRIES
  scheduleNotify()
  return full
}

export function clearActivityLog() {
  _log = []
  scheduleNotify()
}

/**
 * @param {(entries: ActivityEntry[]) => void} fn
 * @returns {() => void} unsubscribe
 */
export function subscribeActivityLog(fn) {
  _listeners = [..._listeners, fn]
  fn(_log.slice())
  return () => { _listeners = _listeners.filter((l) => l !== fn) }
}


let _mcpListening = false
/**
 * Record every MCP tool call (the server's `mcp-activity` event) in the log:
 * who called, which tool, the SQL, how long, and whether it ran, failed or was
 * refused by the access gate. Call once at startup.
 */
export async function listenMcpActivity() {
  if (_mcpListening) return
  _mcpListening = true
  try {
    const { listen } = await import('@tauri-apps/api/event')
    await listen('mcp-activity', (e) => {
      const a = /** @type {{ client: string, tool: string, detail: string, ok: boolean, refused: boolean, error: string | null, ms: number }} */ (e.payload)
      recordActivity({
        type: 'mcp',
        title: `${a.client} · ${a.tool}${a.refused ? ' (refused)' : ''}`,
        detail: a.detail || undefined,
        success: a.ok,
        durationMs: a.ms,
        error: a.error ?? undefined,
      })
    })
  } catch {
    /* outside Tauri */
  }
}
