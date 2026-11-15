// Line diff for the schema diff page: Myers' O((N+M)·D) algorithm, the one git
// uses, after trimming the common head and tail. Two DDL files that differ in
// a few columns cost a few hundred steps, where the LCS table it replaces cost
// N×M cells (4 million for a 2,000-line pair) and an O(n²) unshift on top.

/**
 * @typedef {{ type: 'same' | 'add' | 'remove', content: string, lineA?: number, lineB?: number }} DiffLine
 * @typedef {{ text: string, changed: boolean }} WordPart
 */

/** A trailing comma comes and goes as a column stops or starts being last. */
const norm = (/** @type {string} */ s) => s.trimEnd().replace(/,$/, '')

/** Above this many differing lines a diff stops being worth reading line by line. */
const MAX_EDITS = 4000

/**
 * @param {string[]} a before
 * @param {string[]} b after
 * @returns {DiffLine[]}
 */
export function diffLines(a, b) {
  const na = a.map(norm)
  const nb = b.map(norm)
  let start = 0
  while (start < na.length && start < nb.length && na[start] === nb[start]) start++
  let endA = na.length
  let endB = nb.length
  while (endA > start && endB > start && na[endA - 1] === nb[endB - 1]) { endA--; endB-- }

  /** @type {DiffLine[]} */
  const out = []
  for (let i = 0; i < start; i++) out.push({ type: 'same', content: a[i], lineA: i + 1, lineB: i + 1 })

  const middle = myers(na, nb, start, endA, start, endB)
  if (middle) {
    for (const op of middle) {
      if (op.t === 'same') out.push({ type: 'same', content: a[op.i], lineA: op.i + 1, lineB: op.j + 1 })
      else if (op.t === 'remove') out.push({ type: 'remove', content: a[op.i], lineA: op.i + 1 })
      else out.push({ type: 'add', content: b[op.j], lineB: op.j + 1 })
    }
  } else {
    // Too different to align usefully: everything out, everything in.
    for (let i = start; i < endA; i++) out.push({ type: 'remove', content: a[i], lineA: i + 1 })
    for (let j = start; j < endB; j++) out.push({ type: 'add', content: b[j], lineB: j + 1 })
  }

  const shift = endB - endA
  for (let i = endA; i < a.length; i++) out.push({ type: 'same', content: a[i], lineA: i + 1, lineB: i + shift + 1 })
  return out
}

/**
 * Myers' greedy diff over a[a0..a1) and b[b0..b1). Returns the edit script in
 * order, removals before additions at each change, or null past MAX_EDITS.
 * @param {string[]} a @param {string[]} b
 * @param {number} a0 @param {number} a1 @param {number} b0 @param {number} b1
 * @returns {{ t: 'same' | 'add' | 'remove', i: number, j: number }[] | null}
 */
function myers(a, b, a0, a1, b0, b1) {
  const n = a1 - a0
  const m = b1 - b0
  if (n === 0 && m === 0) return []
  const max = Math.min(n + m, MAX_EDITS)
  const off = max + 1
  let v = new Int32Array(2 * max + 3)
  /** @type {Int32Array[]} */
  const trace = []
  let found = -1
  for (let d = 0; d <= max; d++) {
    trace.push(v.slice())
    for (let k = -d; k <= d; k += 2) {
      let x = k === -d || (k !== d && v[off + k - 1] < v[off + k + 1]) ? v[off + k + 1] : v[off + k - 1] + 1
      let y = x - k
      while (x < n && y < m && a[a0 + x] === b[b0 + y]) { x++; y++ }
      v[off + k] = x
      if (x >= n && y >= m) { found = d; break }
    }
    if (found >= 0) break
  }
  if (found < 0) return null

  // Walk the trace back from (n, m), then reverse once (no unshift).
  /** @type {{ t: 'same' | 'add' | 'remove', i: number, j: number }[]} */
  const ops = []
  let x = n
  let y = m
  for (let d = found; d > 0; d--) {
    const pv = trace[d]
    const k = x - y
    const prevK = k === -d || (k !== d && pv[off + k - 1] < pv[off + k + 1]) ? k + 1 : k - 1
    const prevX = pv[off + prevK]
    const prevY = prevX - prevK
    while (x > prevX && y > prevY) { x--; y--; ops.push({ t: 'same', i: a0 + x, j: b0 + y }) }
    if (x === prevX) { y--; ops.push({ t: 'add', i: a0 + x, j: b0 + y }) }
    else { x--; ops.push({ t: 'remove', i: a0 + x, j: b0 + y }) }
  }
  while (x > 0 && y > 0) { x--; y--; ops.push({ t: 'same', i: a0 + x, j: b0 + y }) }
  ops.reverse()
  return ops
}

/**
 * Word-level highlight for a changed line pair: the shared head and tail
 * stay plain, the middle is marked, so `price numeric(10,2)` becoming
 * `price numeric(12,2)` lights up `10` and `12`, not the whole line.
 * @param {string} before @param {string} after
 * @returns {{ before: WordPart[], after: WordPart[] }}
 */
export function wordDiff(before, after) {
  const tok = (/** @type {string} */ s) => s.match(/\w+|\s+|[^\w\s]/g) ?? []
  const ta = tok(before)
  const tb = tok(after)
  let p = 0
  while (p < ta.length && p < tb.length && ta[p] === tb[p]) p++
  let ea = ta.length
  let eb = tb.length
  while (ea > p && eb > p && ta[ea - 1] === tb[eb - 1]) { ea--; eb-- }
  /** @param {string[]} t @param {number} e */
  const parts = (t, e) => {
    /** @type {WordPart[]} */
    const out = []
    const head = t.slice(0, p).join('')
    const mid = t.slice(p, e).join('')
    const tail = t.slice(e).join('')
    if (head) out.push({ text: head, changed: false })
    if (mid) out.push({ text: mid, changed: true })
    if (tail) out.push({ text: tail, changed: false })
    return out
  }
  return { before: parts(ta, ea), after: parts(tb, eb) }
}

/**
 * Pair each run of removals with the additions right after it, line by line,
 * so a changed line can show what changed inside it. Returns, per line index,
 * the parts to draw (or nothing for an unpaired line).
 * @param {DiffLine[]} lines
 * @returns {Map<number, WordPart[]>}
 */
export function pairWordDiffs(lines) {
  /** @type {Map<number, WordPart[]>} */
  const out = new Map()
  let i = 0
  while (i < lines.length) {
    if (lines[i].type !== 'remove') { i++; continue }
    const r0 = i
    while (i < lines.length && lines[i].type === 'remove') i++
    const a0 = i
    while (i < lines.length && lines[i].type === 'add') i++
    const pairs = Math.min(a0 - r0, i - a0)
    for (let k = 0; k < pairs; k++) {
      const w = wordDiff(lines[r0 + k].content, lines[a0 + k].content)
      out.set(r0 + k, w.before)
      out.set(a0 + k, w.after)
    }
  }
  return out
}
