// Duplicate spotter - values that repeat in a mostly-unique column share a
// tint, so duplicate emails, slugs or external ids stand out on the page.
//
// The table counts values per column once per data change (ctx.stats.counts,
// over the same sample the other stats use). A column is only checked when it
// is mostly distinct: in `status` or `country` repeats are the point, not a
// problem. Each repeated value gets its own hue, so pairs can be told apart.

/** Same value → same hue. @param {string} s */
function hue(s) {
  let h = 2166136261
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619) }
  return Math.abs(h) % 360
}

/** Columns whose repeats are expected. */
const SKIP_NAME = /(^|_)(status|state|type|kind|role|country|city|currency|lang|language|locale|gender|category|tag|level|priority|plan|tier|source|channel|platform|os|browser|device|created_at|updated_at|deleted_at|date|day|month|year|is_\w+|has_\w+)$/i

export const duplicates = {
  id: 'duplicates',
  name: 'Duplicate Spotter',
  description: 'Tint repeated values in mostly-unique columns (emails, slugs, external ids), one colour per value.',
  kind: 'formatter',
  defaultConfig: {},
  needsStats: true,

  /** @param {string} type @param {string} name */
  appliesTo(type, name) {
    if (/bool/i.test(type ?? '')) return false
    return !name || !SKIP_NAME.test(name)
  },

  /**
   * @param {unknown} value @param {string} _type @param {Record<string, unknown>} _config @param {string} _name
   * @param {{ stats?: { counts?: Map<string, number>, distinct?: number, total?: number, nulls?: number } } | undefined} ctx
   */
  format(value, _type, _config, _name, ctx) {
    const st = ctx?.stats
    if (!st?.counts || !st.total) return null
    const filled = st.total - (st.nulls ?? 0)
    // Mostly unique, and enough rows that a repeat means something.
    if (filled < 4 || (st.distinct ?? 0) / filled < 0.6) return null
    const key = typeof value === 'object' ? JSON.stringify(value) : String(value)
    if (key === '') return null
    const n = st.counts.get(key) ?? 0
    if (n < 2) return null
    return { bgTint: `hsla(${hue(key)}, 75%, 55%, 0.16)`, title: `Appears ${n} times on this page` }
  },
}
