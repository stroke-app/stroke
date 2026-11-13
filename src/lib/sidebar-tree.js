/**
 * The Tree sidebar (beta) as data.
 *
 * One rule runs through all of it: a node is named by its PATH. Every node id
 * is its parent's id plus one segment (`c:conn␟s:public␟g:tables␟t:users`), so
 * expansion, focus, selection and the persisted open set all key off strings
 * that survive a refresh, a filter and a reload. Nothing here keys off a row
 * index or a DOM node - rows come and go as the window scrolls.
 *
 * The tree on screen is one flat array of rows (`flattenTree`), rebuilt from
 * the roots, a children cache and an expanded set whenever one of them changes.
 * The component draws a window of that array; nothing here runs per frame or
 * per row. A 20,000-node flatten is a couple of milliseconds (see the bench in
 * sidebar-tree.test.js).
 */

/** Separator between path segments: never typed into a name, safe in JSON. */
export const SEP = '\u001f'

/**
 * @typedef {'connection' | 'database' | 'schema' | 'folder' | 'group' | 'table' | 'view' | 'matview' | 'object' | 'column' | 'section' | 'key' | 'index'} NodeKind
 *
 * @typedef {{
 *   id: string,
 *   kind: NodeKind,
 *   label: string,
 *   lname: string,
 *   branch: boolean,
 *   count?: number | null,
 *   meta?: string,
 *   data?: any,
 * }} TreeNode
 *   `lname` is the label lowercased once, for the filter and type-ahead.
 *   `branch` says the node can be expanded (its children may not be loaded yet).
 *   `count` is drawn right-aligned (a group's size, a table's rows).
 *   `meta` is a short muted suffix (a column's type).
 *
 * @typedef {{ status: 'loading' | 'ready' | 'error', nodes: TreeNode[], error?: string }} ChildEntry
 *   A cached child listing. `loading` with nodes is a refresh in flight over
 *   the rows already shown.
 *
 * @typedef {{ t: 'node', key: string, node: TreeNode, depth: number, parent: number, pos: number, size: number, open: boolean, loading: boolean }
 *   | { t: 'loading' | 'error' | 'empty', key: string, owner: string, depth: number, parent: number, error?: string }} FlatRow
 *   `parent` is the index of the parent row (-1 at the top), which is what the
 *   keyboard (←), the sticky header and the active guide walk.
 *   `pos`/`size` are aria-posinset / aria-setsize among the visible siblings.
 */

/**
 * The id of a child node.
 * @param {string} parent '' for a root
 * @param {string} kind a short segment tag (`c`, `s`, `g`, `t`, ...)
 * @param {string} name
 */
export function childId(parent, kind, name) {
  const seg = `${kind}:${name}`
  return parent ? parent + SEP + seg : seg
}

/** The parent's id, or '' for a root. @param {string} id */
export function parentIdOf(id) {
  const i = id.lastIndexOf(SEP)
  return i < 0 ? '' : id.slice(0, i)
}

/** Every ancestor of `id`, root first, not including `id`. @param {string} id */
export function ancestorIds(id) {
  /** @type {string[]} */
  const out = []
  let i = id.indexOf(SEP)
  while (i >= 0) {
    out.push(id.slice(0, i))
    i = id.indexOf(SEP, i + 1)
  }
  return out
}

/**
 * Build a node. Kept as a function so every node has the same shape (one
 * hidden class), which matters when there are tens of thousands of them.
 * @param {string} id @param {NodeKind} kind @param {string} label
 * @param {{ branch?: boolean, count?: number | null, meta?: string, data?: any }} [extra]
 * @returns {TreeNode}
 */
export function makeNode(id, kind, label, extra = {}) {
  return {
    id,
    kind,
    label,
    lname: label.toLowerCase(),
    branch: extra.branch ?? false,
    count: extra.count ?? null,
    meta: extra.meta ?? '',
    data: extra.data ?? null,
  }
}

/**
 * What the filter keeps: every loaded node whose label contains the query,
 * and every ancestor of one, so a match is always reachable from its root
 * ("hide non-matches"). Only loaded children are searched - the filter never
 * fires a catalog read.
 * @param {TreeNode[]} roots
 * @param {(id: string) => ChildEntry | undefined} getChildren
 * @param {string} query lowercase, trimmed
 * @returns {{ keep: Set<string>, open: Set<string>, matches: number }}
 *   `open` holds the nodes that must be drawn expanded to show a match.
 */
export function filterTree(roots, getChildren, query) {
  const keep = new Set()
  const open = new Set()
  let matches = 0
  /** @param {TreeNode} n @returns {boolean} */
  const walk = (n) => {
    let hit = n.lname.includes(query)
    if (hit) matches++
    let below = false
    if (n.branch) {
      const entry = getChildren(n.id)
      if (entry) for (const c of entry.nodes) if (walk(c)) below = true
    }
    if (below) open.add(n.id)
    if (hit || below) { keep.add(n.id); return true }
    return false
  }
  for (const r of roots) walk(r)
  return { keep, open, matches }
}

/**
 * The rows on screen, in tree order.
 *
 * An expanded branch whose children are not cached draws one `loading` row and
 * is reported in `missing`, so the caller can start the read; one that failed
 * draws an `error` row (retry lives there). With a filter, only kept nodes are
 * drawn and the ones holding a match are drawn open, whatever the expanded set
 * says - the filter never rewrites what is remembered as open.
 *
 * @param {TreeNode[]} roots
 * @param {(id: string) => ChildEntry | undefined} getChildren
 * @param {(id: string) => boolean} isOpen
 * @param {{ keep: Set<string>, open: Set<string> } | null} [filter]
 * @returns {{ rows: FlatRow[], missing: string[] }}
 */
export function flattenTree(roots, getChildren, isOpen, filter = null) {
  /** @type {FlatRow[]} */
  const rows = []
  /** @type {string[]} */
  const missing = []

  /** @param {TreeNode[]} list @param {number} depth @param {number} parent */
  const emit = (list, depth, parent) => {
    // Siblings that will actually draw, for posinset/setsize.
    let size = list.length
    if (filter) {
      size = 0
      for (const n of list) if (filter.keep.has(n.id)) size++
    }
    let pos = 0
    for (const node of list) {
      if (filter && !filter.keep.has(node.id)) continue
      pos++
      const open = node.branch && (filter ? filter.open.has(node.id) : isOpen(node.id))
      const entry = open ? getChildren(node.id) : undefined
      const index = rows.length
      rows.push({ t: 'node', key: node.id, node, depth, parent, pos, size, open, loading: !!entry && entry.status === 'loading' })
      if (!open) continue
      if (!entry) {
        missing.push(node.id)
        rows.push({ t: 'loading', key: `${node.id}${SEP}~loading`, owner: node.id, depth: depth + 1, parent: index })
        continue
      }
      if (entry.status === 'error' && entry.nodes.length === 0) {
        rows.push({ t: 'error', key: `${node.id}${SEP}~error`, owner: node.id, depth: depth + 1, parent: index, error: entry.error ?? 'Failed to load' })
        continue
      }
      if (entry.nodes.length === 0) {
        if (entry.status === 'loading') rows.push({ t: 'loading', key: `${node.id}${SEP}~loading`, owner: node.id, depth: depth + 1, parent: index })
        else if (!filter) rows.push({ t: 'empty', key: `${node.id}${SEP}~empty`, owner: node.id, depth: depth + 1, parent: index })
        continue
      }
      emit(entry.nodes, depth + 1, index)
    }
  }
  emit(roots, 0, -1)
  return { rows, missing }
}

/**
 * Where a row is, by key; -1 when it is not on screen.
 * @param {FlatRow[]} rows @param {string} key
 */
export function rowIndex(rows, key) {
  if (!key) return -1
  for (let i = 0; i < rows.length; i++) if (rows[i].key === key) return i
  return -1
}

/**
 * @typedef {{ type: 'none' }
 *   | { type: 'move', index: number }
 *   | { type: 'expand', id: string }
 *   | { type: 'collapse', id: string }
 *   | { type: 'activate', index: number }
 *   | { type: 'toggle', id: string }
 *   | { type: 'retry', id: string }} KeyAction
 */

/**
 * What a key does on the active row - the WAI-ARIA tree pattern:
 *   ↑ ↓            previous / next row
 *   Home End       first / last row
 *   PageUp/Down    a page
 *   →              closed branch: open it · open branch: first child
 *   ←              open branch: close it · otherwise: go to the parent
 *   Enter          activate (open a table, toggle a folder, retry an error)
 *   Space          toggle a branch
 * Pure: the component applies the action.
 * @param {FlatRow[]} rows
 * @param {number} index the active row, -1 for none
 * @param {string} key KeyboardEvent.key
 * @param {number} [page] rows per page
 * @returns {KeyAction}
 */
export function treeKeyAction(rows, index, key, page = 10) {
  const n = rows.length
  if (n === 0) return { type: 'none' }
  const clamp = (/** @type {number} */ i) => Math.max(0, Math.min(n - 1, i))
  if (index < 0 || index >= n) {
    if (key === 'End') return { type: 'move', index: n - 1 }
    if (key === 'ArrowDown' || key === 'ArrowUp' || key === 'Home' || key === 'PageDown' || key === 'PageUp') return { type: 'move', index: 0 }
    return { type: 'none' }
  }
  const r = rows[index]
  switch (key) {
    case 'ArrowDown': return index < n - 1 ? { type: 'move', index: index + 1 } : { type: 'none' }
    case 'ArrowUp': return index > 0 ? { type: 'move', index: index - 1 } : { type: 'none' }
    case 'Home': return { type: 'move', index: 0 }
    case 'End': return { type: 'move', index: n - 1 }
    case 'PageDown': return { type: 'move', index: clamp(index + page) }
    case 'PageUp': return { type: 'move', index: clamp(index - page) }
    case 'ArrowRight': {
      if (r.t !== 'node' || !r.node.branch) return { type: 'none' }
      if (!r.open) return { type: 'expand', id: r.node.id }
      const next = rows[index + 1]
      return next && next.parent === index ? { type: 'move', index: index + 1 } : { type: 'none' }
    }
    case 'ArrowLeft': {
      if (r.t === 'node' && r.node.branch && r.open) return { type: 'collapse', id: r.node.id }
      return r.parent >= 0 ? { type: 'move', index: r.parent } : { type: 'none' }
    }
    case 'Enter':
      if (r.t === 'error') return { type: 'retry', id: r.owner }
      return r.t === 'node' ? { type: 'activate', index } : { type: 'none' }
    case ' ':
      return r.t === 'node' && r.node.branch ? { type: 'toggle', id: r.node.id } : { type: 'none' }
  }
  return { type: 'none' }
}

/**
 * The row type-ahead lands on: the next node row, after `from`, whose label
 * starts with what was typed. A longer buffer (still typing one word) may stay
 * on the current row, so "us" then "use" does not hop away from `users`.
 * @param {FlatRow[]} rows @param {number} from @param {string} typed
 */
export function typeAheadIndex(rows, from, typed) {
  const needle = typed.toLowerCase()
  const n = rows.length
  if (!needle || n === 0) return -1
  const start = typed.length > 1 ? Math.max(0, from) : from + 1
  for (let k = 0; k < n; k++) {
    const i = (((start + k) % n) + n) % n
    const r = rows[i]
    if (r.t === 'node' && r.node.lname.startsWith(needle)) return i
  }
  return -1
}

/**
 * The ancestors to pin at the top while scrolling, as row indexes, outermost
 * first: the parents of whatever row sits just under the pinned stack, so the
 * stack always names the branch you are scrolling through.
 * @param {FlatRow[]} rows
 * @param {number} first index of the first row whose top is at or under the viewport's top
 * @param {number} max most rows to pin
 * @returns {number[]}
 */
export function stickyIndices(rows, first, max) {
  /** @type {number[]} */
  const out = []
  if (first <= 0 || max <= 0 || first >= rows.length) return out
  for (;;) {
    if (out.length >= max) break
    // Near the end there may be no row under the stack: the last one stands in.
    const under = Math.min(rows.length - 1, first + out.length)
    // The chain of ancestors of the row under the stack, root first.
    /** @type {number[]} */
    const chain = []
    for (let p = rows[under].parent; p >= 0; p = rows[p].parent) chain.push(p)
    chain.reverse()
    const next = chain[out.length]
    // An ancestor still in view in its own place needs no pin.
    if (next === undefined || next >= first + out.length) break
    out.push(next)
  }
  return out
}

/**
 * Where a pinned row's branch ends: the last row below it that is deeper.
 * @param {FlatRow[]} rows @param {number} i
 */
export function subtreeEnd(rows, i) {
  const d = rows[i]?.depth ?? 0
  let j = i
  while (j + 1 < rows.length && rows[j + 1].depth > d) j++
  return j
}

/**
 * How far each pinned row is pushed up, so the end of its branch slides it out
 * of the stack (the way VS Code and Finder do) instead of it vanishing in one
 * frame. Returns a top offset per pinned row, in px.
 * @param {FlatRow[]} rows @param {number[]} pinned @param {number} scrollTop @param {number} rowH
 */
export function stickyTops(rows, pinned, scrollTop, rowH) {
  return pinned.map((si, k) => {
    const room = (subtreeEnd(rows, si) + 1) * rowH - scrollTop - (k + 1) * rowH
    return k * rowH + Math.min(0, room)
  })
}

/**
 * The run of rows inside the active row's parent, for the stronger guide:
 * rows lo..hi (inclusive) draw their guide at `level` in the active tone.
 * @param {FlatRow[]} rows @param {number} index
 * @returns {{ level: number, lo: number, hi: number } | null}
 */
export function branchRange(rows, index) {
  const r = rows[index]
  if (!r || r.parent < 0) return null
  const p = r.parent
  const level = rows[p].depth
  let hi = p + 1
  while (hi + 1 < rows.length && rows[hi + 1].depth > level) hi++
  return { level, lo: p + 1, hi }
}

// ── A table's sections ─────────────────────────────────────────────────────

/** @param {any} c */
const isPk = (c) => !!(c.isPrimaryKey ?? c.is_primary_key)
/** @param {any} ix */
const isPrimaryIx = (ix) => !!(ix.isPrimary ?? ix.is_primary)
/** @param {any} ix */
const ixType = (ix) => String(ix.indexType ?? ix.index_type ?? '')

/**
 * `schema.table.column` (or `table.column`) as `table(column)`, the way a
 * foreign key reads: the schema is noise when it is the table's own.
 * @param {string} ref @param {string[]} cols
 */
function refLabel(ref, cols) {
  const parts = ref.split('.')
  const table = parts.length >= 2 ? parts[parts.length - 2] : ref
  const known = cols.filter(Boolean)
  return known.length ? `${table}(${known.join(', ')})` : table
}

/**
 * The Keys section: the primary key, then one row per foreign key (a
 * composite key is one row, its columns in order). Where the engine's keys are
 * not one primary key - Cassandra partition / clustering keys, DynamoDB
 * partition / sort keys - the backend sends them as `index_type: 'key'` rows,
 * and those are the keys.
 * @param {string} gid the section's id
 * @param {any[]} cols column structure rows
 * @param {any[]} idx this table's index rows
 * @param {{ schema: string, table: string }} at
 * @returns {TreeNode[]}
 */
export function keyNodes(gid, cols, idx, at) {
  /** @type {TreeNode[]} */
  const out = []
  const declared = idx.filter((ix) => ixType(ix) === 'key')
  if (declared.length) {
    for (const k of declared) {
      out.push(makeNode(childId(gid, 'pk', k.name), 'key', k.name, { meta: `(${k.columns ?? ''})`, data: { ...at, pk: true } }))
    }
  } else {
    const pkCols = cols.filter(isPk).map((c) => c.name)
    if (pkCols.length) {
      const pkIx = idx.find(isPrimaryIx)
      // SQLite names the index behind a PRIMARY KEY sqlite_autoindex_<t>_N: not a name anyone gave it.
      const name = pkIx && !/^sqlite_autoindex_/.test(pkIx.name) ? pkIx.name : 'primary key'
      // Key order where the index says it (a composite key is not in column order).
      const order = pkIx?.columns ? String(pkIx.columns) : pkCols.join(', ')
      out.push(makeNode(childId(gid, 'pk', name), 'key', name, { meta: `(${order})`, data: { ...at, pk: true } }))
    }
  }
  /** @type {Map<string, { cols: string[], refCols: string[], ref: string }>} */
  const fks = new Map()
  for (const c of cols) {
    const ref = c.foreignKey ?? c.foreign_key
    if (!ref) continue
    const name = c.fkConstraintName ?? c.fk_constraint_name ?? `${c.name}_fkey`
    const fk = fks.get(name) ?? { cols: [], refCols: [], ref }
    fk.cols.push(c.name)
    fk.refCols.push(String(ref).split('.').pop() ?? '')
    fks.set(name, fk)
  }
  for (const [name, fk] of fks) {
    const target = refLabel(fk.ref, fk.refCols)
    out.push(makeNode(childId(gid, 'fk', name), 'key', name, { meta: `(${fk.cols.join(', ')}) → ${target}`, data: { ...at, pk: false, ref: fk.ref } }))
  }
  return out
}

/**
 * What a table, view or collection expands into.
 *
 * A table splits into `columns`, `keys` and `indexes` sections, each with its
 * count; a section with nothing in it is left out rather than drawn empty
 * (MongoDB has no columns, ClickHouse no foreign keys, a view no indexes). A
 * view is just its columns, with no section in between.
 *
 * @param {string} id the table node's id
 * @param {{ schema: string, table: string }} at
 * @param {any[]} cols column structure rows
 * @param {any[]} idx this table's index rows (key rows included)
 * @param {boolean} isTable false for views and materialized views
 * @returns {{ nodes: TreeNode[], sections: [string, TreeNode[]][] }}
 *   `nodes` are the table's children; `sections` the children of each section.
 */
export function tableSections(id, at, cols, idx, isTable) {
  const colGroup = isTable ? childId(id, 'g', 'columns') : id
  const columnNodes = cols.map((c) => {
    const fk = c.foreignKey ?? c.foreign_key ?? null
    return makeNode(childId(colGroup, 'col', c.name), 'column', c.name, {
      meta: c.dataType ?? c.data_type ?? '',
      data: { ...at, pk: isPk(c), fk, nullable: c.isNullable ?? c.is_nullable },
    })
  })
  if (!isTable) return { nodes: columnNodes, sections: [] }
  const keyGroup = childId(id, 'g', 'keys')
  const indexGroup = childId(id, 'g', 'indexes')
  const keys = keyNodes(keyGroup, cols, idx, at)
  const indexNodes = idx
    .filter((ix) => ixType(ix) !== 'key')
    .map((ix) => {
      const unique = !!(ix.isUnique ?? ix.is_unique)
      const type = ixType(ix)
      const condition = ix.condition ?? null
      return makeNode(childId(indexGroup, 'ix', ix.name), 'index', ix.name, {
        meta: [unique && !isPrimaryIx(ix) ? 'unique' : '', ix.columns ? `(${ix.columns})` : ''].filter(Boolean).join(' '),
        data: { ...at, primary: isPrimaryIx(ix), unique, type, condition },
      })
    })
  /** @type {[string, TreeNode[]][]} */
  const sections = []
  /** @type {TreeNode[]} */
  const nodes = []
  for (const [gid, label, list] of /** @type {[string, string, TreeNode[]][]} */ ([
    [colGroup, 'columns', columnNodes],
    [keyGroup, 'keys', keys],
    [indexGroup, 'indexes', indexNodes],
  ])) {
    if (!list.length) continue
    sections.push([gid, list])
    nodes.push(makeNode(gid, 'section', label, { branch: true, count: list.length, data: { ...at, section: label } }))
  }
  return { nodes, sections }
}
