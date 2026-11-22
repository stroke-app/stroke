// Which code fences run as queries on the active connection. Set by the shell
// when the connection changes; read by the AI reply parser (ai.js). Apart from
// ai.js so setting it does not pull the AI client into the startup bundle.
import { runnableFences } from '$lib/ai-query-languages.js'

/** Extra fences that hold runnable queries on the active connection (```javascript for MongoDB, ...). */
let _queryFences = /** @type {Set<string> | null} */ (null)

/**
 * Called when the connection changes: a NoSQL engine's own query fences get
 * a Run button like SQL blocks do.
 * @param {string | null | undefined} engine
 */
export function setQueryEngine(engine) {
  _queryFences = runnableFences(engine)
}

export function getQueryFences() {
  return _queryFences
}
