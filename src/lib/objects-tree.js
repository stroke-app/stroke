/**
 * The Objects tab as data: every object turned once into the strings a row
 * shows, and the tree flattened into the rows on screen (group headers,
 * extension nodes, items) so the panel can window it.
 *
 * Nothing here runs per frame. `buildGroups` runs when a listing lands;
 * `flattenTree` runs when the filter or an open/closed state changes, and is
 * a walk over arrays it can afford for 10,000 objects.
 */
import { objectKey } from './object-templates.js'

/** Kinds in tree order. Materialized views live in the Views group. */
export const TREE_KINDS = /** @type {const} */ (['view', 'function', 'procedure', 'trigger', 'sequence', 'type', 'event'])

/** @type {Record<string, string>} */
export const GROUP_LABELS = {
  view: 'Views', function: 'Functions', procedure: 'Procedures', trigger: 'Triggers',
  sequence: 'Sequences', type: 'Types', event: 'Events',
}

/**
 * @typedef {{ kind: string, name: string, args?: string, argTypes?: string, table?: string, detail?: string,
 *   subtype?: string, ext?: string, comment?: string | null, rowCount?: number | null }} ObjectLike
 * @typedef {{ key: string, kind: string, name: string, lname: string, args: string, right: string,
 *   ext: string, comment: string, title: string, obj: ObjectLike, row: TreeRow }} TreeItem
 *   `row` is the item's own row, made once: flattening reuses it rather than
 *   allocating thousands of objects per keystroke.
 * @typedef {{ t: 'group', key: string, group: string, label: string, count: number, total: number, open: boolean, depth: 0 }
 *   | { t: 'ext', key: string, group: string, ext: string, label: string, count: number, total: number, open: boolean, depth: 1 }
 *   | { t: 'item', key: string, group: string, parent: string, item: TreeItem, depth: 1 | 2 }} TreeRow
 */

/** The group a kind is filed under. @param {string} kind */
export const groupOf = (kind) => (kind === 'matview' ? 'view' : kind)

/**
 * What an item row shows at its right edge: plain words, no arrows.
 * @param {ObjectLike} o
 */
function rightText(o) {
  switch (o.kind) {
    case 'matview': return 'materialized'
    // A routine's row is its name and arguments; the return type is in the
    // tooltip. On the right it took the width the name needed.
    case 'function': return o.subtype === 'aggregate' || o.subtype === 'window' ? o.subtype : ''
    case 'trigger': return o.table ? `on ${o.table}` : ''
    case 'type': return o.subtype ?? ''
    case 'sequence': return String(o.detail ?? '').split(' · ')[0]
    case 'event': return o.detail ?? ''
    default: return ''
  }
}

/** @type {Record<string, string>} */
const ONE = {
  view: 'view', matview: 'materialized view', function: 'function', procedure: 'procedure',
  trigger: 'trigger', sequence: 'sequence', type: 'type', event: 'event',
}

/** The native tooltip: the whole signature, never truncated. @param {ObjectLike} o */
function titleText(o) {
  const routine = o.kind === 'function' || o.kind === 'procedure'
  const lines = [`${ONE[o.kind] ?? o.kind} ${o.name}${routine ? `(${o.args ?? ''})` : ''}`]
  if (o.kind === 'trigger' && o.table) lines.push(`on ${o.table}`)
  if (o.detail) lines.push(o.detail)
  if (o.ext) lines.push(`from the ${o.ext} extension`)
  if (o.comment) lines.push(o.comment)
  return lines.join('\n')
}

/**
 * Objects by group, each turned into its row strings once.
 * @param {ObjectLike[]} objects
 * @returns {Map<string, TreeItem[]>}
 */
export function buildGroups(objects) {
  /** @type {Map<string, TreeItem[]>} */
  const groups = new Map()
  /** Keys seen so far: a catalog should never repeat one, but a repeat must not take the list down. */
  const seen = new Map()
  for (const o of objects) {
    const group = groupOf(o.kind)
    const routine = o.kind === 'function' || o.kind === 'procedure'
    const base = `o:${objectKey(o)}`
    const n = seen.get(base) ?? 0
    seen.set(base, n + 1)
    /** @type {TreeItem} */
    const item = {
      key: n ? `${base}#${n}` : base,
      kind: o.kind,
      name: o.name,
      lname: o.name.toLowerCase(),
      args: routine ? (o.argTypes || o.args || '') : '',
      right: rightText(o),
      ext: o.ext ?? '',
      comment: o.comment ?? '',
      title: titleText(o),
      obj: o,
      row: /** @type {any} */ (null),
    }
    const list = groups.get(group)
    if (list) list.push(item)
    else groups.set(group, [item])
  }
  for (const [group, list] of groups) {
    // The schema's own first, then by extension; by name within each.
    list.sort((a, b) => (a.ext === b.ext ? 0 : !a.ext ? -1 : !b.ext ? 1 : a.ext < b.ext ? -1 : 1) || (a.lname < b.lname ? -1 : a.lname > b.lname ? 1 : 0))
    for (const item of list) {
      item.row = item.ext
        ? { t: 'item', key: item.key, group, parent: `x:${group}:${item.ext}`, item, depth: 2 }
        : { t: 'item', key: item.key, group, parent: `g:${group}`, item, depth: 1 }
    }
  }
  return groups
}

/**
 * The rows on screen, in tree order. A filter opens every group and extension
 * node holding a match and leaves out the ones holding none; it never changes
 * what is remembered as open.
 * @param {Map<string, TreeItem[]>} groups
 * @param {readonly string[]} kinds group kinds the engine has, in order
 * @param {string} query lowercase, trimmed
 * @param {(key: string) => boolean} isOpen
 * @returns {{ rows: TreeRow[], shown: number, total: number }}
 */
export function flattenTree(groups, kinds, query, isOpen) {
  /** @type {TreeRow[]} */
  const rows = []
  let shown = 0
  let total = 0
  for (const group of kinds) {
    const all = groups.get(group)
    if (!all?.length) continue
    total += all.length
    const items = query ? all.filter((i) => i.lname.includes(query)) : all
    if (!items.length) continue
    shown += items.length
    const key = `g:${group}`
    const open = !!query || isOpen(key)
    rows.push({ t: 'group', key, group, label: GROUP_LABELS[group] ?? group, count: items.length, total: all.length, open, depth: 0 })
    if (!open) continue
    /** Every object an extension owns here, matched or not, for its node's count. */
    const extTotals = new Map()
    for (const x of all) if (x.ext) extTotals.set(x.ext, (extTotals.get(x.ext) ?? 0) + 1)
    // Items are sorted own-first, so the extension runs follow in order.
    let i = 0
    while (i < items.length && !items[i].ext) rows.push(items[i++].row)
    while (i < items.length) {
      const ext = items[i].ext
      let j = i
      while (j < items.length && items[j].ext === ext) j++
      const extKey = `x:${group}:${ext}`
      const extOpen = !!query || isOpen(extKey)
      rows.push({ t: 'ext', key: extKey, group, ext, label: ext, count: j - i, total: extTotals.get(ext) ?? j - i, open: extOpen, depth: 1 })
      if (extOpen) for (let k = i; k < j; k++) rows.push(items[k].row)
      i = j
    }
  }
  return { rows, shown, total }
}

/**
 * Where a row is, by key; -1 when it is not on screen. A scan, not a map: it
 * runs on a keypress, and building a map on every keystroke of the filter
 * cost more than every scan it would save.
 * @param {TreeRow[]} rows @param {string} key
 */
export function rowIndex(rows, key) {
  if (!key) return -1
  for (let i = 0; i < rows.length; i++) if (rows[i].key === key) return i
  return -1
}

/**
 * The row type-ahead lands on: the next one, after `from`, whose label starts
 * with what was typed. A longer buffer (still typing one word) may stay on
 * the current row.
 * @param {TreeRow[]} rows @param {number} from @param {string} typed
 */
export function typeAheadIndex(rows, from, typed) {
  const needle = typed.toLowerCase()
  const n = rows.length
  const start = typed.length > 1 ? from : from + 1
  for (let k = 0; k < n; k++) {
    const i = (((start + k) % n) + n) % n
    const r = rows[i]
    const label = r.t === 'item' ? r.item.lname : r.label.toLowerCase()
    if (label.startsWith(needle)) return i
  }
  return -1
}

/** Open until closed: the two groups most people open the tab for. */
const OPEN_BY_DEFAULT = new Set(['g:view', 'g:function'])

/**
 * Whether a tree node (`g:view`, `x:function:vector`) is open: what was
 * remembered, when the setting keeps it, otherwise the default.
 * @param {string} key
 * @param {Record<string, boolean> | undefined} saved
 * @param {boolean} remember
 */
export function nodeStartsOpen(key, saved, remember) {
  return remember && typeof saved?.[key] === 'boolean' ? saved[key] : OPEN_BY_DEFAULT.has(key)
}
