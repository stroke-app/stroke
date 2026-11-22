/**
 * Lay out JSON for reading, at any size.
 *
 * `JSON.stringify(v, null, 2)` and compact JSON both fail on the values people
 * actually store. A jsonb holding a file as an array of byte integers is one
 * 1.2-million-character line compact (an editor stops painting a row that wide)
 * and three million one-number lines indented. Here objects get one key per
 * line, as with the indented form, while a run of scalars in an array is packed
 * into lines of about `width` columns. Every line stays short, whatever the
 * value's size, and the line count stays proportional to its size over the
 * width rather than to its element count.
 *
 * Linear in the size of the value: each container is tried inline at most once,
 * and that attempt stops as soon as it passes the width.
 *
 * Whitespace only. Parsing the result gives back the same value.
 */

const DEFAULT_WIDTH = 100

/** @param {unknown} v */
function isScalar(v) {
  return v === null || typeof v !== 'object'
}

/**
 * Compact JSON for `v`, or null once it would pass `limit` characters.
 * @param {unknown} v @param {number} limit
 * @returns {string | null}
 */
function inlineWithin(v, limit) {
  if (isScalar(v)) {
    const s = JSON.stringify(v) ?? 'null'
    return s.length <= limit ? s : null
  }
  const parts = []
  let used = 2
  if (Array.isArray(v)) {
    for (const item of v) {
      const s = inlineWithin(item, limit - used)
      if (s === null) return null
      used += s.length + (parts.length ? 2 : 0)
      if (used > limit) return null
      parts.push(s)
    }
    return `[${parts.join(', ')}]`
  }
  for (const [k, item] of Object.entries(/** @type {Record<string, unknown>} */ (v))) {
    if (item === undefined) continue
    const key = JSON.stringify(k)
    const s = inlineWithin(item, limit - used - key.length - 2)
    if (s === null) return null
    used += key.length + 2 + s.length + (parts.length ? 2 : 0)
    if (used > limit) return null
    parts.push(`${key}: ${s}`)
  }
  return `{${parts.join(', ')}}`
}

/**
 * @param {unknown} value a parsed JSON value
 * @param {{ width?: number, indent?: number }} [opts]
 * @returns {string}
 */
export function layoutJson(value, opts = {}) {
  const width = Math.max(20, opts.width ?? DEFAULT_WIDTH)
  const unit = ' '.repeat(opts.indent ?? 2)
  /** @type {string[]} */
  const out = []

  /**
   * Append `v` (its first line continues the current one).
   * @param {unknown} v @param {string} pad indentation of the current line @param {number} col column already used on it
   */
  function emit(v, pad, col) {
    if (isScalar(v)) { out.push(JSON.stringify(v) ?? 'null'); return }
    const inline = inlineWithin(v, width - col)
    if (inline !== null) { out.push(inline); return }
    const inner = pad + unit
    if (Array.isArray(v)) {
      // Scalars packed: the runs that make huge values huge (byte arrays,
      // vectors, id lists) become width-sized lines instead of one per item.
      if (v.every(isScalar)) {
        out.push('[\n', inner)
        let lineLen = inner.length
        for (let i = 0; i < v.length; i++) {
          const s = (JSON.stringify(v[i]) ?? 'null') + (i < v.length - 1 ? ',' : '')
          if (lineLen > inner.length && lineLen + 1 + s.length > width) {
            out.push('\n', inner)
            lineLen = inner.length
          } else if (lineLen > inner.length) {
            out.push(' ')
            lineLen++
          }
          out.push(s)
          lineLen += s.length
        }
        out.push('\n', pad, ']')
        return
      }
      out.push('[')
      for (let i = 0; i < v.length; i++) {
        out.push('\n', inner)
        emit(v[i], inner, inner.length)
        if (i < v.length - 1) out.push(',')
      }
      out.push('\n', pad, ']')
      return
    }
    const entries = Object.entries(/** @type {Record<string, unknown>} */ (v)).filter(([, x]) => x !== undefined)
    out.push('{')
    entries.forEach(([k, item], i) => {
      const key = JSON.stringify(k)
      out.push('\n', inner, key, ': ')
      emit(item, inner, inner.length + key.length + 2)
      if (i < entries.length - 1) out.push(',')
    })
    out.push('\n', pad, '}')
  }

  emit(value, '', 0)
  return out.join('')
}

/**
 * `text` laid out when it is a JSON object or array, else null. For a string
 * column that holds JSON, and for a value that arrived as text.
 * @param {string} text @param {{ width?: number, maxChars?: number }} [opts]
 */
export function layoutJsonText(text, opts = {}) {
  const t = text.trim()
  if (t.length > (opts.maxChars ?? 64 * 1024 * 1024)) return null
  if (!((t.startsWith('{') && t.endsWith('}')) || (t.startsWith('[') && t.endsWith(']')))) return null
  try {
    return layoutJson(JSON.parse(t), opts)
  } catch {
    return null
  }
}
