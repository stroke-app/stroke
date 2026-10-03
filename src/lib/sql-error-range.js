/**
 * Where to underline a failed statement, from the position the database gave
 * (Postgres: 1-based characters into the text it was sent). Shared by the SQL
 * editor's run marks and the error console, so both mark the same text.
 */

const WORD = /[\w$"]/

/**
 * The span to underline in `text`: the token at `position`, the last token
 * when it failed at the end of input, the first line when there is no position.
 * @param {string} text the statement as it was sent
 * @param {number | null | undefined} position
 * @returns {{ from: number, to: number }} offsets into `text`
 */
export function errorSpan(text, position) {
  const len = text.length
  if (!position || position < 1) {
    const nl = text.indexOf('\n')
    return { from: 0, to: nl === -1 ? len : nl }
  }
  let from = position - 1
  if (from >= len) {
    // "at end of input": point at the last thing that was written.
    let to = len
    while (to > 0 && /[\s;]/.test(text[to - 1])) to--
    from = to
    while (from > 0 && WORD.test(text[from - 1])) from--
    if (from === to) from = Math.max(0, to - 1)
    return { from, to }
  }
  let to = from
  while (to < len && WORD.test(text[to])) to++
  return { from, to: to > from ? to : Math.min(from + 1, len) }
}

/**
 * Line and column (both 1-based) of an offset in `text`.
 * @param {string} text @param {number} offset
 */
export function lineColumn(text, offset) {
  const before = text.slice(0, offset)
  const line = before.split('\n').length
  return { line, column: offset - before.lastIndexOf('\n') }
}

/**
 * The database's message without the wrapping the backend adds
 * (`Query failed: error returned from database: …`).
 * @param {string} error
 */
export function cleanErrorMessage(error) {
  return String(error ?? '')
    .replace(/^Error:\s*/, '')
    .replace(/^(Query|Statement \d+) failed:\s*/i, '')
    .replace(/^error returned from database:\s*/i, '')
}
