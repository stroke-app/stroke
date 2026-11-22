// Driver errors reduced to what a person needs to read. Kept apart from ai.js
// (prompts and the AI client) so the startup bundle does not carry all of that
// for one formatter.

/**
 * Reduce a driver error to the sentence a human needs.
 *
 * Engines wrap the useful part in transport noise: D1 returns the whole HTTP
 * envelope (`D1 API error 400 Bad Request: {"messages":[],"result":[],…}`) around
 * a five-word cause. Dumping that raw makes a one-line "no such column: activated"
 * read as a stack trace. The full text is still available on the tool channel and
 * in the query log; this is only what gets shown.
 *
 * Returns the input trimmed when nothing better can be extracted - never empty.
 * @param {string} raw
 * @returns {string}
 */
export function humanizeDbError(raw) {
  const text = String(raw ?? '').replace(/^Error:\s*/i, '').trim()
  if (!text) return 'Query failed'

  // Cloudflare-style envelope: pull the innermost `"message"` out of the JSON tail.
  const jsonStart = text.search(/[{[]/)
  if (jsonStart !== -1) {
    try {
      const parsed = JSON.parse(text.slice(jsonStart))
      const errs = Array.isArray(parsed) ? parsed : parsed?.errors
      const msg = Array.isArray(errs) ? errs.find((e) => e?.message)?.message : parsed?.message
      if (typeof msg === 'string' && msg.trim()) return cleanEngineMessage(msg)
    } catch { /* not JSON, fall through to the regex below */ }
    // Unparseable tail (truncated payload) - lift the first "message" it contains.
    const m = text.slice(jsonStart).match(/"message"\s*:\s*"((?:[^"\\]|\\.)*)"/)
    if (m) return cleanEngineMessage(m[1].replace(/\\"/g, '"'))
  }
  return cleanEngineMessage(text)
}

/** Trim engine bookkeeping that means nothing to the reader. */
function cleanEngineMessage(msg) {
  return String(msg)
    // SQLite appends its own error class and a byte offset into the statement.
    .replace(/\s+at offset \d+/i, '')
    .replace(/:\s*SQLITE_ERROR$/i, '')
    .trim() || 'Query failed'
}
