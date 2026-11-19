/**
 * Markdown-aware typing for plain text boxes (the AI composer, notebook text
 * cells): lists continue on Enter, an empty item ends the list, Tab and
 * Shift+Tab indent list lines, and Enter inside an open ``` fence is a newline.
 *
 * Pure functions over (text, selection) so the box only applies the result:
 * each returns the new text and caret, or null when the key is not theirs.
 */

/** `  - [ ] ` / `* ` / `+ ` / `12. ` / `3) ` at the start of a line. */
const LIST_RE = /^(\s*)([-*+]|\d{1,9}[.)])(\s+)(\[[ xX]\]\s+)?/

/** @typedef {{ text: string, start: number, end: number }} Edit */

/** @param {string} text @param {number} pos */
function lineAt(text, pos) {
  const start = text.lastIndexOf('\n', pos - 1) + 1
  const nl = text.indexOf('\n', pos)
  return { start, end: nl < 0 ? text.length : nl }
}

/**
 * Is the caret inside a ``` fence that has not been closed yet?
 * @param {string} text @param {number} pos
 */
export function inOpenFence(text, pos) {
  const before = text.slice(0, pos)
  let open = false
  for (const line of before.split('\n')) if (/^\s*(```|~~~)/.test(line)) open = !open
  return open
}

/**
 * Enter. Returns the edit, or null to let the caller do its default (send).
 * @param {string} text @param {number} start @param {number} end
 * @returns {Edit | null}
 */
export function markdownEnter(text, start, end) {
  if (inOpenFence(text, start)) return insert(text, start, end, '\n')
  const ln = lineAt(text, start)
  const line = text.slice(ln.start, ln.end)
  const m = LIST_RE.exec(line)
  // Only at the end of the item: Enter in the middle of a line is a send.
  if (!m || start !== end || start < ln.start + m[0].length || text.slice(start, ln.end).trim()) return null
  const body = line.slice(m[0].length)
  if (!body.trim()) {
    // An empty item ends the list: the marker goes, the line stays.
    const next = text.slice(0, ln.start) + text.slice(ln.end)
    return { text: next, start: ln.start, end: ln.start }
  }
  const [, indent, marker, gap, box] = m
  const num = /^(\d+)([.)])$/.exec(marker)
  const nextMarker = num ? `${Number(num[1]) + 1}${num[2]}` : marker
  return insert(text, start, end, `\n${indent}${nextMarker}${gap}${box ? '[ ] ' : ''}`)
}

/**
 * Tab / Shift+Tab on list lines: indent or outdent every selected list line by
 * two spaces. Null when no selected line is a list item (Tab keeps its focus move).
 * @param {string} text @param {number} start @param {number} end @param {boolean} outdent
 * @returns {Edit | null}
 */
export function markdownIndent(text, start, end, outdent) {
  const first = lineAt(text, start).start
  const last = lineAt(text, end).end
  const lines = text.slice(first, last).split('\n')
  if (!lines.some((l) => LIST_RE.test(l))) return null
  let delta0 = 0
  let total = 0
  const out = lines.map((l, i) => {
    if (!LIST_RE.test(l)) return l
    let d = 0
    let next = l
    if (outdent) {
      const cut = /^ {1,2}|^\t/.exec(l)?.[0].length ?? 0
      next = l.slice(cut)
      d = -cut
    } else {
      next = `  ${l}`
      d = 2
    }
    if (i === 0) delta0 = d
    total += d
    return next
  })
  const next = text.slice(0, first) + out.join('\n') + text.slice(last)
  return { text: next, start: Math.max(first, start + delta0), end: Math.max(first, end + total) }
}

/** @param {string} text @param {number} start @param {number} end @param {string} s @returns {Edit} */
function insert(text, start, end, s) {
  return { text: text.slice(0, start) + s + text.slice(end), start: start + s.length, end: start + s.length }
}
