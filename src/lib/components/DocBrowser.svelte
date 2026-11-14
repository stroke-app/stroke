<script>
  // One page for document and wide-column databases (MongoDB, CouchDB,
  // Cassandra, Firestore). The collection comes from the app sidebar. Layout
  // follows the SQL table page: a dense toolbar, a query bar in the engine's
  // own language (Compass-style, with sort/projection where the engine has
  // them), the app's own data grid or JSON view, and a footer with the page
  // range and navigation. Opening a row (double-click, Space, or the grid's
  // "Open in inspector" menu item) edits that document as JSON in a side panel.
  import ResizeHandle from './ResizeHandle.svelte'
  import { untrack } from 'svelte'
  import { traceInteraction } from '$lib/perf.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import Icon from './Icon.svelte'
  import ConfirmDialog from './ConfirmDialog.svelte'
  import { rowsToCsv, saveExportFile, buildExportFilename } from '$lib/export.js'
  import Kbd from './Kbd.svelte'
  import DataTable from './DataTable.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { readOnlyMode, guardWrite, READ_ONLY_HINT } from '$lib/stores/read-only.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { getAppScale } from '$lib/app-zoom.js'
  import { cn } from '$lib/utils.js'
  import { docGrid, docField, PAGE_SIZES, PAGE_SIZE_ALL, ALL_CHUNK, ALL_CAP } from '$lib/doc-adapters.js'
  import X from '@lucide/svelte/icons/x'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import Plus from '@lucide/svelte/icons/plus'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Check from '@lucide/svelte/icons/check'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import Filter from '@lucide/svelte/icons/filter'
  import TerminalIcon from '@lucide/svelte/icons/terminal'
  import ChevronLeft from '@lucide/svelte/icons/chevron-left'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal'
  import Braces from '@lucide/svelte/icons/braces'
  import Sheet from '@lucide/svelte/icons/sheet'
  import PanelLeft from '@lucide/svelte/icons/panel-left'
  import ListTree from '@lucide/svelte/icons/list-tree'
  import Layers from '@lucide/svelte/icons/layers'
  import Workflow from '@lucide/svelte/icons/workflow'
  import Gauge from '@lucide/svelte/icons/gauge'
  import FileUp from '@lucide/svelte/icons/file-up'
  import MongoExplainPanel from './MongoExplainPanel.svelte'
  import MongoSchemaView from './MongoSchemaView.svelte'
  import MongoIndexesView from './MongoIndexesView.svelte'
  import MongoAggregateView from './MongoAggregateView.svelte'
  import DocImportDialog from './DocImportDialog.svelte'

  /**
   * @type {{
   *   active?: boolean,
   *   connection?: import('$lib/stores/connections.js').SavedConnection | null,
   *   adapter: import('$lib/doc-adapters.js').DocAdapter,
   *   collection?: string,
   *   where?: string,
   *   onopensql?: (text: string, collection: string) => void,
   * }}
   */
  let { active = false, connection = null, adapter, collection = '', where = '', onopensql = () => {} } = $props()

  const readOnly = $derived($readOnlyMode)
  const noun = $derived(adapter.noun)
  const options = $derived(adapter.options ?? [])

  /** @param {unknown} e */
  const msg = (e) => (e instanceof Error ? e.message : String(e))

  // ── Collection ─────────────────────────────────────────────────────────────
  let coll = $state('')
  let info = $state.raw(/** @type {{ summary: string[], keys: string[], meta?: any } | null} */ (null))
  const keys = $derived(info?.keys ?? [])
  let error = $state('')
  let token = 0

  // Follow the sidebar's selection; reopen when the connection changes.
  let openedFor = ''
  $effect(() => {
    const key = `${connection?.id ?? connection?.name ?? ''}:${adapter.engine}:${collection}`
    if (!active || key === openedFor) return
    openedFor = key
    // Untracked: opening writes (and reads) a lot of this page's state.
    untrack(() => void openColl(collection))
  })

  /** @param {string} name */
  async function openColl(name) {
    const t = ++token
    coll = name
    info = null
    docs = []
    total = null
    page = 0
    cursors = [null]
    closePanel()
    explainQuery = null
    lastFields = []
    error = ''
    filter = appliedFilter = ''
    sort = projection = appliedSort = appliedProjection = ''
    if (!name) return
    loading = true
    const done = traceInteraction(`open ${adapter.engine} ${name}`)
    try {
      const d = await adapter.describe(name)
      if (t !== token) return
      info = d
      await load(0)
      done()
    } catch (e) {
      if (t === token) error = msg(e)
    } finally {
      if (t === token) loading = false
    }
  }

  // ── Query bar ──────────────────────────────────────────────────────────────
  let filter = $state('')
  let sort = $state('')
  let projection = $state('')
  let appliedFilter = $state('')
  let appliedSort = $state('')
  let appliedProjection = $state('')
  let optionsOpen = $state(false)
  /** Sort / project fields opened by hand; filled ones always show. */
  let openOptions = $state(/** @type {Set<string>} */ (new Set()))
  /** @param {string} k */
  const optionShown = (k) => openOptions.has(k) || (k === 'sort' ? !!sort.trim() : !!projection.trim())
  /** @param {string} k */
  function showOption(k) { openOptions = new Set([...openOptions, k]) }
  /** @param {string} k */
  function hideOption(k) {
    const n = new Set(openOptions); n.delete(k); openOptions = n
    if (k === 'sort') sort = ''; else projection = ''
  }

  /** The selected documents, or every loaded one when nothing is selected. */
  function exportDocs() {
    return selected.size ? [...selected].sort((a, b) => a - b).map((i) => docs[i]).filter(Boolean) : docs
  }
  /** @param {'json' | 'jsonl' | 'csv'} fmt */
  async function exportAs(fmt) {
    const list = exportDocs()
    if (!list.length) return
    let content
    if (fmt === 'json') content = JSON.stringify(list, null, 2)
    else if (fmt === 'jsonl') content = list.map((d) => JSON.stringify(d)).join('\n')
    else {
      const idx = new Set(list.map((d) => docs.indexOf(d)))
      content = rowsToCsv(grid.columns, grid.rows.filter((_, i) => idx.has(i)))
    }
    try {
      if (await saveExportFile(content, buildExportFilename(coll, fmt), fmt)) toast.success(`Exported ${list.length.toLocaleString()} ${list.length === 1 ? noun.item.toLowerCase() : noun.items}`)
    } catch (e) {
      toast.error('Export failed', { description: msg(e) })
    }
  }
  async function copyJson() {
    const list = exportDocs()
    try {
      await navigator.clipboard.writeText(JSON.stringify(list.length === 1 ? list[0] : list, null, 2))
      toast.success(`Copied ${list.length.toLocaleString()} as JSON`)
    } catch (e) {
      toast.error('Could not copy', { description: msg(e) })
    }
  }
  /** @type {HTMLInputElement | null} */
  let filterEl = $state(null)
  let filterFocused = $state(false)
  /** Field names from the last page that had documents (or the schema
   *  sample), so an empty result still offers something to filter on. */
  let lastFields = $state(/** @type {string[]} */ ([]))
  $effect(() => {
    if (docs.length) lastFields = grid.columns.map((c) => c.name)
  })
  const fieldNames = $derived((docs.length ? grid.columns.map((c) => c.name) : lastFields).slice(0, 40))

  /** Insert `field: ` at the caret, wrapping in braces when the filter is empty. @param {string} f */
  function insertField(f) {
    const key = /^[A-Za-z_$][\w$]*$/.test(f) ? f : JSON.stringify(f)
    const el = filterEl
    const cur = filter
    if (!cur.trim()) {
      filter = `{${key}: }`
      requestAnimationFrame(() => { el?.focus(); el?.setSelectionRange(filter.length - 1, filter.length - 1) })
      return
    }
    const at = el?.selectionStart ?? cur.length
    const before = cur.slice(0, at)
    const sep = /[{,\s]$/.test(before) ? '' : ', '
    const ins = `${sep}${key}: `
    filter = before + ins + cur.slice(at)
    const pos = at + ins.length
    requestAnimationFrame(() => { el?.focus(); el?.setSelectionRange(pos, pos) })
  }
  const dirty = $derived(filter.trim() !== appliedFilter || sort.trim() !== appliedSort || projection.trim() !== appliedProjection)
  const queried = $derived(!!(appliedFilter || appliedSort || appliedProjection))

  function find() {
    appliedFilter = filter.trim()
    appliedSort = sort.trim()
    appliedProjection = projection.trim()
    void restart()
  }
  function reset() {
    filter = sort = projection = ''
    find()
  }

  // ── Pages ──────────────────────────────────────────────────────────────────
  const SIZE_KEY = 'stroke:docs:pageSize'
  const savedSize = loadNum(SIZE_KEY, 20)
  let size = $state(PAGE_SIZES.includes(savedSize) ? savedSize : 20)
  let docs = $state.raw(/** @type {Record<string, any>[]} */ ([]))
  let page = $state(0)
  /** cursors[i] opens page i; cursors[page + 1] exists once a next page is known. */
  let cursors = $state.raw(/** @type {any[]} */ ([null]))
  let total = $state(/** @type {number | null} */ (null))
  let loading = $state(false)
  let dataVersion = $state(0)

  const isAll = $derived(size === PAGE_SIZE_ALL)
  /** True when All stopped at ALL_CAP with more left. */
  let capped = $state(false)
  const from = $derived(docs.length ? (isAll ? 1 : page * size + 1) : 0)
  const to = $derived(isAll ? docs.length : page * size + docs.length)
  /** @param {number} n */
  const sizeLabel = (n) => (n === PAGE_SIZE_ALL ? 'All rows' : `${n.toLocaleString()} rows`)

  /** Every page in ALL_CHUNK pieces, until the cursor runs out or ALL_CAP. */
  async function loadAll() {
    /** @type {Record<string, any>[]} */
    let acc = []
    let cursor = null
    do {
      const res = await adapter.load(coll, { cursor, filter: appliedFilter, size: ALL_CHUNK, sort: appliedSort, projection: appliedProjection, meta: info.meta })
      acc = acc.concat(res.docs)
      cursor = res.cursor
    } while (cursor != null && acc.length < ALL_CAP)
    capped = cursor != null
    return { docs: acc.slice(0, ALL_CAP), cursor: null }
  }
  const hasNext = $derived(cursors[page + 1] != null)
  const pages = $derived(total != null ? Math.max(1, Math.ceil(total / size)) : null)

  async function restart() {
    page = 0
    cursors = [null]
    total = null
    await load(0)
  }

  /** @param {number} p */
  async function load(p) {
    if (!coll || !info) return
    const t = token
    loading = true
    error = ''
    try {
      const [res, count] = await Promise.all([
        isAll ? loadAll() : adapter.load(coll, { cursor: cursors[p] ?? null, filter: appliedFilter, size, sort: appliedSort, projection: appliedProjection, meta: info.meta }),
        p === 0 && adapter.count ? adapter.count(coll, appliedFilter, info.meta).catch(() => null) : Promise.resolve(total),
      ])
      if (t !== token) return
      if (!isAll) capped = false
      docs = res.docs
      page = p
      const next = cursors.slice(0, p + 1)
      if (res.cursor != null) next[p + 1] = res.cursor
      cursors = next
      total = count ?? null
      dataVersion += 1
      closePanel()
    } catch (e) {
      if (t === token) error = msg(e)
    } finally {
      if (t === token) loading = false
    }
  }

  /** @param {string} v */
  function setSize(v) {
    size = Number(v)
    saveNum(SIZE_KEY, size)
    void restart()
  }

  const grid = $derived(docGrid(docs, keys))
  const ids = $derived(docs.map((d) => adapter.idOf(d, info?.meta)))

  // ── View ───────────────────────────────────────────────────────────────────
  const VIEW_KEY = 'stroke:docs:view'
  /** @typedef {'table' | 'json' | 'schema' | 'indexes' | 'aggregate'} View */
  const views = $derived([
    { v: 'table', l: 'Table', I: Sheet },
    { v: 'json', l: 'JSON', I: Braces },
    ...(adapter.schema ? [{ v: 'schema', l: 'Schema', I: ListTree }] : []),
    ...(adapter.indexes ? [{ v: 'indexes', l: 'Indexes', I: Layers }] : []),
    ...(adapter.aggregate ? [{ v: 'aggregate', l: 'Aggregate', I: Workflow }] : []),
  ])
  let savedView = $state(/** @type {View} */ (/** @type {any} */ (loadStr(VIEW_KEY, 'table'))))
  /** The saved view when this engine has it, else the table. */
  const view = $derived(/** @type {View} */ (views.some((m) => m.v === savedView) ? savedView : 'table'))
  const dataView = $derived(view === 'table' || view === 'json')
  /** @param {View} v */
  function setView(v) {
    savedView = v
    try { localStorage.setItem(VIEW_KEY, v) } catch {}
  }

  // ── Explain ───────────────────────────────────────────────────────────────
  /** The query the plan panel explains; null when it is closed. */
  let explainQuery = $state(/** @type {{ filter: string, sort: string, projection: string } | null} */ (null))
  function openExplain() {
    closePanel()
    explainQuery = { filter: filter.trim(), sort: sort.trim(), projection: projection.trim() }
  }

  /** Re-read the header summary (index count) after an index change. */
  async function refreshInfo() {
    const c = coll
    try {
      const d = await adapter.describe(c)
      if (c === coll) info = d
    } catch {}
  }

  // ── Import ────────────────────────────────────────────────────────────────
  let importOpen = $state(false)

  // ── Inline field edits (MongoDB) ──────────────────────────────────────────
  // Every engine edits in the grid: one-field updates where the engine has them
  // (MongoDB), otherwise the document is saved whole with the cell changed.
  const inlineEdit = $derived(!readOnly && !!info)
  /** Extended JSON wrapper per grid type, so an optimistic value displays right. */
  const EXT_WRAP = /** @type {Record<string, string>} */ ({ objectId: '$oid', date: '$date', long: '$numberLong', decimal: '$numberDecimal', uuid: '$uuid' })

  /** @param {number | null} r @param {number} c @returns {string | null} */
  function cellEditBlock(r, c) {
    const col = grid.columns[c]
    if (!col) return 'Not editable'
    if (col.name === '_id') return '_id is changed in the document editor'
    // Keys and engine bookkeeping (CouchDB _rev, Firestore's name, Cassandra's
    // primary key columns) are not edited in place.
    if (col.name.startsWith('_') || keys.includes(col.name)) return `${col.name} is a key; change it in the document editor`
    if (col.data_type === 'object' || col.data_type === 'array') return 'Nested values are edited in the document editor'
    if (r != null) {
      const t = docField(docs[r]?.[col.name]).type
      if (t === 'object' || t === 'array') return 'Nested values are edited in the document editor'
    }
    return null
  }

  /**
   * Text typed in a cell -> the value to `$set`. Typed columns validate; mixed
   * ones read JSON (numbers, true, "quoted") and fall back to the text.
   * @param {string} raw @param {any} col
   * @returns {{ ok: true, value: unknown } | { ok: false, message: string }}
   */
  function parseDocCell(raw, col) {
    const t = col?.data_type ?? ''
    const s = raw.trim()
    if (t === 'string') return { ok: true, value: s === 'null' ? null : raw }
    if (s === '' || s === 'null') return { ok: true, value: null }
    if (t === 'int' || t === 'long') {
      if (!/^-?\d+$/.test(s)) return { ok: false, message: `"${s}" isn't a whole number` }
      const n = Number(s)
      if (Number.isSafeInteger(n)) return { ok: true, value: n }
      return t === 'long' ? { ok: true, value: s } : { ok: false, message: 'Too large for an int' }
    }
    if (t === 'double' || t === 'decimal') {
      if (!/^-?\d+(\.\d+)?([eE][+-]?\d+)?$/.test(s)) return { ok: false, message: `"${s}" isn't a number` }
      return { ok: true, value: t === 'decimal' ? s : Number(s) }
    }
    if (t === 'bool') {
      if (/^(true|false)$/i.test(s)) return { ok: true, value: s.toLowerCase() === 'true' }
      return { ok: false, message: 'Use true or false' }
    }
    if (t === 'objectId') return /^[0-9a-f]{24}$/i.test(s) ? { ok: true, value: s.toLowerCase() } : { ok: false, message: 'An ObjectId is 24 hex characters' }
    if (t === 'date') return Number.isNaN(Date.parse(s)) ? { ok: false, message: 'Use a date like 2026-01-31T12:00:00Z' } : { ok: true, value: s }
    try {
      return { ok: true, value: JSON.parse(s) }
    } catch {
      return { ok: true, value: raw }
    }
  }

  /** Write one cell: optimistic, then the stored document, or roll back. @param {{ rowIdx: number, colIdx: number, value: unknown, unset?: boolean }} d */
  async function commitCell({ rowIdx, colIdx, value, unset = false }) {
    const col = grid.columns[colIdx]
    const doc = docs[rowIdx]
    if (!adapter.updateField || !col || !doc || !guardWrite(`change ${noun.items}`)) return
    const blocked = cellEditBlock(rowIdx, colIdx)
    if (blocked) {
      toast.message(blocked)
      return
    }
    const id = adapter.idOf(doc, info?.meta)
    const cellType = docField(doc[col.name]).type
    const typeHint = cellType && cellType !== 'mixed' ? cellType : col.data_type in EXT_WRAP ? col.data_type : ''
    /** @param {Record<string, any>} next */
    const put = (next) => {
      const at = docs.findIndex((d) => adapter.idOf(d, info?.meta) === id)
      if (at < 0) return
      docs = [...docs.slice(0, at), next, ...docs.slice(at + 1)]
      dataVersion += 1
    }
    const optimistic = { ...doc }
    if (unset) delete optimistic[col.name]
    else optimistic[col.name] = value != null && EXT_WRAP[typeHint] && typeof value !== 'object' ? { [EXT_WRAP[typeHint]]: String(value) } : value
    put(optimistic)
    try {
      put(await adapter.updateField(coll, doc, col.name, value ?? null, { typeHint, unset }))
      if (unset) toast.success(`Removed ${col.name}`)
    } catch (e) {
      put(doc)
      toast.error(`Could not save ${col.name}`, { description: msg(e) })
    }
  }

  /**
   * Apply one staged cell edit (the grid stages them; Apply sends each here).
   * Throws on failure so the grid keeps that edit staged and shows the error.
   * @param {{ rowIdx: number, colIdx: number, value: unknown }} d
   */
  async function saveCell({ rowIdx, colIdx, value }) {
    const col = grid.columns[colIdx]
    const doc = docs[rowIdx]
    if (!col || !doc) throw new Error('Nothing to save')
    const blocked = cellEditBlock(rowIdx, colIdx)
    if (blocked) throw new Error(blocked)
    const id = adapter.idOf(doc, info?.meta)
    const cellType = docField(doc[col.name]).type
    const typeHint = cellType && cellType !== 'mixed' ? cellType : col.data_type in EXT_WRAP ? col.data_type : ''
    const next = adapter.updateField
      ? await adapter.updateField(coll, doc, col.name, value ?? null, { typeHint, unset: false })
      : await adapter.save(coll, { ...doc, [col.name]: value ?? null }, doc, info?.meta)
    const at = docs.findIndex((x) => adapter.idOf(x, info?.meta) === id)
    if (at >= 0) {
      docs = [...docs.slice(0, at), next, ...docs.slice(at + 1)]
      dataVersion += 1
    }
  }
  // Staged edits: the grid owns the queue; these come back from it.
  let pendingEditCount = $state(0)
  let applyEdits = $state(/** @type {() => void | Promise<void>} */ (() => {}))
  let resetEdits = $state(/** @type {() => void} */ (() => {}))
  let applying = $state(false)
  async function applyStaged() {
    if (!guardWrite(`change ${noun.items}`)) return
    applying = true
    try { await applyEdits() } finally { applying = false }
  }

  // ── Grid selection → editor panel ─────────────────────────────────────────
  let selected = $state(/** @type {Set<number>} */ (new Set()))
  let focusedRow = $state(/** @type {number | null} */ (null))
  /** Set by the grid when a row is opened (double-click, Space, menu). */
  let inspectorRow = $state(/** @type {number | null} */ (null))
  let creating = $state(false)
  let editingId = $state(/** @type {string | null} */ (null))
  // Open the row the grid asked for; while open, follow the focused row.
  $effect(() => {
    if (inspectorRow != null && ids[inspectorRow] != null) {
      creating = false
      editingId = ids[inspectorRow]
      untrack(() => (explainQuery = null))
    }
  })
  $effect(() => {
    const f = focusedRow
    // Depends on the focus only; reading editingId here too would loop.
    untrack(() => {
      if (editingId != null && !creating && f != null && ids[f] != null) editingId = ids[f]
    })
  })
  const editing = $derived(editingId == null ? null : docs[ids.indexOf(editingId)] ?? null)
  const panelOpen = $derived(creating || !!editing)

  let draft = $state('')
  let draftError = $state('')
  let saving = $state(false)
  let confirmDelete = $state(false)

  $effect(() => {
    const d = editing
    const isNew = creating
    confirmDelete = false
    draftError = ''
    if (isNew) draft = JSON.stringify(adapter.template(coll, info?.meta), null, 2)
    else if (d) draft = JSON.stringify(d, null, 2)
  })

  async function save() {
    if (!guardWrite(`save ${noun.items}`)) return
    draftError = ''
    /** @type {Record<string, any>} */
    let next
    try {
      next = JSON.parse(draft)
    } catch (e) {
      draftError = `Not valid JSON: ${msg(e)}`
      return
    }
    if (!next || typeof next !== 'object' || Array.isArray(next)) {
      draftError = `A ${noun.item.toLowerCase()} is a JSON object`
      return
    }
    const prev = creating ? null : editing
    if (prev && adapter.idOf(next, info?.meta) !== adapter.idOf(prev, info?.meta) && !confirm(adapter.keyChangeWarning)) return
    saving = true
    try {
      const stored = await adapter.save(coll, next, prev, info?.meta)
      const id = adapter.idOf(stored, info?.meta)
      const oldId = prev ? adapter.idOf(prev, info?.meta) : null
      const at = oldId != null ? ids.indexOf(oldId) : -1
      const kept = docs.filter((d) => {
        const k = adapter.idOf(d, info?.meta)
        return k !== id && k !== oldId
      })
      docs = at >= 0 ? [...kept.slice(0, at), stored, ...kept.slice(at)] : [stored, ...kept]
      if (!prev && total != null) total += 1
      dataVersion += 1
      creating = false
      editingId = id
      toast.success(prev ? `${noun.item} saved` : `${noun.item} added`)
    } catch (e) {
      draftError = msg(e)
    } finally {
      saving = false
    }
  }

  async function remove() {
    const d = editing
    if (!d || !guardWrite(`delete ${noun.items}`)) return
    if (!confirmDelete) {
      confirmDelete = true
      return
    }
    saving = true
    try {
      await adapter.remove(coll, d, info?.meta)
      const id = adapter.idOf(d, info?.meta)
      docs = docs.filter((x) => adapter.idOf(x, info?.meta) !== id)
      if (total != null) total = Math.max(0, total - 1)
      dataVersion += 1
      closePanel()
      toast.success(`${noun.item} deleted`)
    } catch (e) {
      draftError = msg(e)
    } finally {
      saving = false
      confirmDelete = false
    }
  }

  // ── Delete several (row menu, header button) ────────────────────────────
  let pendingRemove = $state(/** @type {number[]} */ ([]))
  let removeOpen = $state(false)
  let removing = $state(false)
  /** @param {number[]} idx */
  function askRemove(idx) {
    if (!idx.length || !guardWrite(`delete ${noun.items}`)) return
    pendingRemove = idx
    removeOpen = true
  }
  async function removeMany() {
    const list = pendingRemove.map((i) => docs[i]).filter(Boolean)
    removing = true
    /** @type {any[]} */
    const done = []
    /** @type {string[]} */
    const errors = []
    for (const d of list) {
      try { await adapter.remove(coll, d, info?.meta); done.push(adapter.idOf(d, info?.meta)) } catch (e) { errors.push(msg(e)) }
    }
    const gone = new Set(done.map((x) => JSON.stringify(x)))
    docs = docs.filter((x) => !gone.has(JSON.stringify(adapter.idOf(x, info?.meta))))
    if (total != null) total = Math.max(0, total - done.length)
    selected = new Set()
    dataVersion += 1
    closePanel()
    removing = false
    removeOpen = false
    const ok = done.length
    if (ok) toast.success(`Deleted ${ok.toLocaleString()} ${ok === 1 ? noun.item.toLowerCase() : noun.items}`)
    if (errors.length) toast.error(`${errors.length} could not be deleted`, { description: errors[0] })
  }

  function startCreate() {
    if (!guardWrite(`add ${noun.items}`)) return
    explainQuery = null
    editingId = null
    inspectorRow = null
    creating = true
  }
  function closePanel() {
    creating = false
    editingId = null
    inspectorRow = null
  }

  /** @type {import('@codemirror/view').KeyBinding[]} */
  const editorKeys = [
    { key: 'Mod-s', preventDefault: true, run: () => { void save(); return true } },
    { key: 'Escape', run: () => { closePanel(); return true } },
  ]

  // ── Layout ─────────────────────────────────────────────────────────────────
  const PANEL_KEY = 'stroke:docs:panelWidth'
  /** @param {string} k @param {number} d */
  function loadNum(k, d) {
    try {
      const n = Number(localStorage.getItem(k))
      return Number.isFinite(n) && n > 0 ? n : d
    } catch {
      return d
    }
  }
  /** @param {string} k @param {string} d */
  function loadStr(k, d) {
    try {
      return localStorage.getItem(k) ?? d
    } catch {
      return d
    }
  }
  /** @param {string} k @param {number} v */
  function saveNum(k, v) {
    try { localStorage.setItem(k, String(Math.round(v))) } catch {}
  }
  let panelWidth = $state(Math.min(720, Math.max(320, loadNum(PANEL_KEY, 440))))
  let dragStart = 0
  let dragScale = 1

  const iconBtn = 'inline-flex size-8 shrink-0 items-center justify-center rounded-lg text-muted-foreground transition-[color,background-color,scale] duration-[120ms] ease-[var(--ease-out)] active:scale-[0.97] hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40'
  /** A quiet text toggle: the on state is foreground, the rest muted. No pill, no frame. */
  const pageSelectTrigger = 'h-8 min-w-0 gap-1 rounded-lg! border-transparent! bg-transparent! focus-visible:border-ring! data-[state=open]:border-ring! px-2 text-ui-xs font-normal tabular-nums shadow-none hover:bg-accent!'
  const textToggle = 'inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-ui-xs transition-colors hover:text-foreground'
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden bg-panel">
  {#if !coll}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 text-center">
      <PanelLeft class="size-6 shrink-0 text-muted-foreground/70" />
      <p class="text-ui-sm text-foreground">Pick a {noun.collection.toLowerCase()} in the sidebar</p>
      {#if where}<p class="font-mono text-ui-2xs text-muted-foreground">{where}</p>{/if}
    </div>
  {:else}
    <!-- Header: what this is, and the few things to do with it. No divider. -->
    <header class="flex h-11 shrink-0 items-center gap-2 overflow-hidden whitespace-nowrap border-b border-border/30 pl-4 pr-2">
      <h2 class="min-w-0 truncate text-ui-sm font-medium text-foreground" title={info?.summary.length ? `${coll} · ${info.summary.join(" · ")}` : coll}>{coll}</h2>
      <span class="flex-1"></span>
      <!-- Pager in the header, as on SQL tables: range, rows per page, arrows. -->
      <div class={cn('flex shrink-0 items-center gap-1 text-ui-xs text-muted-foreground', !dataView && 'hidden')}>
        {#if loading && docs.length > 0}<LoaderCircle class="size-3 shrink-0 animate-spin" />{/if}
        {#if info && docs.length > 0}
          <span class="mr-1 hidden tabular-nums xl:inline">
            <span class="text-foreground">{from.toLocaleString()}-{to.toLocaleString()}</span>
            {#if total != null} of {total.toLocaleString()}{:else if hasNext || capped} of many{/if}
          </span>
        {/if}
        <Select.Root type="single" value={String(size)} onValueChange={(v) => { if (v) setSize(v) }} disabled={loading}>
          <Select.Trigger size="sm" class={pageSelectTrigger} title="Rows per page" aria-label="Rows per page">{sizeLabel(size)}</Select.Trigger>
          <Select.Content align="end" class="min-w-0">
            {#each PAGE_SIZES as n (n)}
              <Select.Item value={String(n)} label={sizeLabel(n)} />
            {/each}
          </Select.Content>
        </Select.Root>
        {#if !isAll}
          <button type="button" class={iconBtn} onclick={() => void load(page - 1)} disabled={page === 0 || loading} aria-label="Previous page" title="Previous page"><ChevronLeft class="size-3.5 shrink-0" /></button>
          <button type="button" class={iconBtn} onclick={() => void load(page + 1)} disabled={!hasNext || loading} aria-label="Next page" title="Next page"><ChevronRight class="size-3.5 shrink-0" /></button>
        {:else if capped}
          <span class="text-ui-2xs text-warning" title="All stops at {ALL_CAP.toLocaleString()} so the grid stays fast. Filter to narrow it down.">first {ALL_CAP.toLocaleString()}</span>
        {/if}
      </div>
      <span class={cn('mx-1 h-4 w-px shrink-0 bg-border/50', !dataView && 'hidden')} aria-hidden="true"></span>
      <!-- Views: with more than two, only the current one keeps its label. -->
      <!-- One segmented control, text only: five icons in a row read as a puzzle. -->
      <div class="flex h-8 items-center gap-0.5 rounded-lg bg-muted/50 p-0.5" role="group" aria-label="View">
        {#each views as m (m.v)}
          {@const on = view === m.v}
          <button
            type="button"
            aria-pressed={on}
            class={cn('inline-flex h-full items-center rounded-md px-2.5 text-ui-xs transition-colors', on ? 'bg-background text-foreground shadow-sm ring-1 ring-border/50' : 'text-muted-foreground hover:text-foreground')}
            onclick={() => setView(/** @type {View} */ (m.v))}
          >{m.v === 'table' ? noun.items.replace(/^./, (c) => c.toUpperCase()) : m.l}</button>
        {/each}
      </div>
      <button type="button" class={iconBtn} onclick={() => void load(page)} disabled={!info} title="Refresh" aria-label="Refresh">
        <RefreshCw class={cn('size-3.5', loading && 'animate-spin')} />
      </button>
      <!-- ⋯ More: the editor, copy and export. Acts on the selection when there is one. -->
      <DropdownMenu.Root>
        <DropdownMenu.Trigger class={iconBtn} title="More actions" aria-label="More actions" disabled={!info}>
          <Icon name="more-horizontal" class="size-3.5 shrink-0" />
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="min-w-56">
          <DropdownMenu.Item onSelect={() => onopensql(adapter.querySeed(coll), coll)}>
            <TerminalIcon class="size-3.5 shrink-0" />
            <span data-slot="menu-label">Open in {adapter.consoleLabel} editor</span>
          </DropdownMenu.Item>
          {#if adapter.import}
            <DropdownMenu.Item disabled={readOnly} onSelect={() => { if (guardWrite(`import ${noun.items}`)) importOpen = true }}>
              <FileUp class="size-3.5 shrink-0" />
              <span data-slot="menu-label">Import…</span>
            </DropdownMenu.Item>
          {/if}
          <DropdownMenu.Item disabled={!docs.length} onSelect={() => void copyJson()}>
            <Icon name="copy" class="size-3.5 shrink-0" />
            <span data-slot="menu-label">Copy {selected.size ? `${selected.size} selected` : 'page'} as JSON</span>
          </DropdownMenu.Item>
          <DropdownMenu.Separator />
          <DropdownMenu.Group>
            <DropdownMenu.GroupHeading class="px-1.5 pt-1.5 pb-0.5 text-ui-3xs font-medium text-muted-foreground">
              Export {selected.size ? `${selected.size} selected` : `${docs.length.toLocaleString()} loaded`}
            </DropdownMenu.GroupHeading>
            {#each [['json', 'JSON', 'braces'], ['jsonl', 'JSON Lines', 'list-ordered'], ['csv', 'CSV', 'file-text']] as [fmt, label, icon] (fmt)}
              <DropdownMenu.Item disabled={!docs.length} onSelect={() => void exportAs(/** @type {'json' | 'jsonl' | 'csv'} */ (fmt))}>
                <Icon name={icon} class="size-3.5 shrink-0" />
                <span data-slot="menu-label">{label}</span>
              </DropdownMenu.Item>
            {/each}
          </DropdownMenu.Group>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
      {#if selected.size > 0}
        <Button size="sm" variant="ghost" class="h-8 rounded-lg text-destructive hover:bg-destructive/10 hover:text-destructive" disabled={readOnly} title={readOnly ? READ_ONLY_HINT : 'Delete the selected'} onclick={() => askRemove([...selected].sort((a, b) => a - b))}>
          <Trash2 /> Delete {selected.size.toLocaleString()}
        </Button>
      {/if}
      <Button size="sm" variant="outline" class="h-8 rounded-lg border-border/60" onclick={startCreate} disabled={readOnly || !info} title={readOnly ? READ_ONLY_HINT : `Add a ${noun.item.toLowerCase()}`}>
        <Plus /><span class="hidden lg:inline">{noun.item}</span>
      </Button>
    </header>

    <!-- Command bar: one field holds the query and its options; Enter runs it. -->
    <!-- Query: the filter is one soft field that owns the row. Sort and project
         are add-ons: a quiet chip until used, a field once opened or filled.
         Run and Explain sit together at the end. Focusing the filter offers
         the collection's field names to insert. -->
    <form class={cn('flex shrink-0 flex-col gap-1.5 border-b border-border/30 px-3 py-2', (view === 'indexes' || view === 'aggregate') && 'hidden')} onsubmit={(e) => { e.preventDefault(); find() }}>
      <div class="flex items-center gap-2">
        <div class="field-surface flex h-8 min-w-0 flex-1 items-center gap-2 pl-3 pr-1 shadow-none transition-[border-color] focus-within:border-ring">
          <Filter class="size-3.5 shrink-0 text-muted-foreground" />
          <input
            bind:this={filterEl}
            type="text"
            bind:value={filter}
            placeholder={adapter.filterPlaceholder}
            title={adapter.filterHint}
            aria-label={adapter.filterHint}
            spellcheck="false"
            autocapitalize="off"
            autocomplete="off"
            disabled={!info}
            onfocus={() => (filterFocused = true)}
            onblur={() => setTimeout(() => (filterFocused = false), 120)}
            class="no-field-frame no-focus-ring h-full min-w-0 flex-1 border-0 bg-transparent font-mono text-ui-xs text-foreground shadow-none outline-none placeholder:text-muted-foreground/50"
          />
          {#if queried}
            <button type="button" class={cn(iconBtn, 'size-6')} onclick={reset} title="Clear the query" aria-label="Clear the query"><X class="size-3.5 shrink-0" /></button>
          {/if}
          {#each options as o (o.key)}
            {#if !optionShown(o.key)}
              <button type="button" class="inline-flex h-6 shrink-0 items-center gap-1 rounded-md px-1.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" onclick={() => showOption(o.key)}>
                <Plus class="size-3 shrink-0" />{o.label}
              </button>
            {/if}
          {/each}
          <span class="mx-0.5 h-4 w-px shrink-0 bg-border/60" aria-hidden="true"></span>
          <button type="submit" disabled={!info || loading} class={cn('inline-flex h-6 shrink-0 items-center gap-1.5 rounded-md px-2 text-ui-2xs font-medium transition-[color,background-color,scale] duration-150 ease-out active:scale-[0.96] disabled:opacity-40', dirty ? 'bg-foreground text-background hover:bg-foreground/90' : 'text-foreground/80 hover:bg-accent hover:text-foreground')} title="Run the query (Enter)">
            {#if loading}<LoaderCircle class="size-3 shrink-0 animate-spin" />{/if}Run
            <span class={cn('text-ui-3xs', dirty ? 'text-background/60' : 'text-muted-foreground')} aria-hidden="true">↵</span>
          </button>
        </div>
        {#if adapter.explain}
          <button type="button" disabled={!info} onclick={openExplain} class={cn('inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg px-2.5 text-ui-xs transition-colors disabled:opacity-40', explainQuery ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/60 hover:text-foreground')} title="Show how MongoDB runs this query">
            <Gauge class="size-3.5 shrink-0" /> Explain
          </button>
        {/if}
      </div>
      {#if options.some((o) => optionShown(o.key))}
        <div class="flex flex-wrap items-center gap-2">
          {#each options as o (o.key)}
            {#if optionShown(o.key)}
              <label class="field-surface flex h-8 min-w-56 flex-1 items-center gap-2 pl-3 pr-1 shadow-none transition-[border-color] focus-within:border-ring">
                <span class="w-12 shrink-0 text-ui-2xs text-muted-foreground">{o.label}</span>
                {#if o.key === 'sort'}
                  <input bind:value={sort} placeholder={o.placeholder} spellcheck="false" autocomplete="off" aria-label={o.label} class="no-field-frame no-focus-ring h-full min-w-0 flex-1 border-0 bg-transparent font-mono text-ui-xs text-foreground shadow-none outline-none placeholder:text-muted-foreground/40" />
                {:else}
                  <input bind:value={projection} placeholder={o.placeholder} spellcheck="false" autocomplete="off" aria-label={o.label} class="no-field-frame no-focus-ring h-full min-w-0 flex-1 border-0 bg-transparent font-mono text-ui-xs text-foreground shadow-none outline-none placeholder:text-muted-foreground/40" />
                {/if}
                <button type="button" class={cn(iconBtn, 'size-6')} onclick={() => hideOption(o.key)} aria-label={`Remove ${o.label.toLowerCase()}`} title={`Remove ${o.label.toLowerCase()}`}><X class="size-3 shrink-0" /></button>
              </label>
            {/if}
          {/each}
        </div>
      {/if}
      {#if filterFocused && fieldNames.length}
        <!-- Field names: click one to drop `name: ` into the filter at the caret. -->
        <div class="flex items-center gap-1 overflow-x-auto pl-1 [scrollbar-width:none]">
          {#each fieldNames as f (f)}
            <button
              type="button"
              class="h-6 shrink-0 rounded-md px-2 font-mono text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              onmousedown={(e) => e.preventDefault()}
              onclick={() => insertField(f)}
            >{f}</button>
          {/each}
        </div>
      {/if}
    </form>

    <div class="flex min-h-0 flex-1 overflow-hidden">
      <!-- Data: the one framed surface on the page, pager inside it. -->
      <section class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden" aria-label={`${noun.items} in ${coll}`}>
        {#if pendingEditCount > 0}
          <!-- Staged edits float over the grid, out of the header's way: nothing
               is written until Apply, as on SQL tables. -->
          <div class="absolute bottom-4 left-1/2 z-30 flex -translate-x-1/2 items-center gap-1 whitespace-nowrap rounded-full border border-border/60 bg-popover py-1 pl-4 pr-1 elevate-2-rim" role="status">
            <span class="size-1.5 shrink-0 rounded-full bg-warning"></span>
            <span class="ml-1.5 mr-2 text-ui-xs tabular-nums text-foreground">{pendingEditCount.toLocaleString()} unsaved {pendingEditCount === 1 ? 'change' : 'changes'}</span>
            <Button size="sm" variant="ghost" class="h-7 rounded-full" disabled={applying} onclick={() => resetEdits()}>Discard</Button>
            <Button size="sm" class="h-7 rounded-full" disabled={applying || readOnly} title={readOnly ? READ_ONLY_HINT : 'Write the staged changes'} onclick={() => void applyStaged()}>
              {#if applying}<LoaderCircle class="animate-spin" />{/if}Apply
            </Button>
          </div>
        {/if}
        {#if !dataView}
          {#if !info}
            <div class="flex flex-1 items-center justify-center">{#if error}<p class="text-ui-xs text-destructive">{error}</p>{:else}<LoaderCircle class="size-4 shrink-0 animate-spin text-muted-foreground" />{/if}</div>
          {:else if view === 'schema'}
            <MongoSchemaView {adapter} {coll} filter={appliedFilter} onfields={(f) => { if (!docs.length) lastFields = f }} />
          {:else if view === 'indexes'}
            <MongoIndexesView {adapter} {coll} fields={lastFields.length ? lastFields : fieldNames} {readOnly} onchanged={() => void refreshInfo()} />
          {:else}
            <MongoAggregateView {adapter} {coll} connId={connection?.id ?? ''} {onopensql} />
          {/if}
        {:else if error}
          {@const head = error.split(':')[0]}
          {@const detail = error.slice(head.length + 1).trim()}
          <div class="flex flex-1 flex-col items-center justify-center px-6">
            <div class="flex w-full max-w-md flex-col gap-2">
              <p class="text-ui-sm font-medium text-foreground">{detail ? head : 'Something went wrong'}</p>
              <pre class="app-scroll max-h-48 overflow-auto whitespace-pre-wrap break-words font-mono text-ui-2xs leading-relaxed text-muted-foreground">{detail || error}</pre>
              <div class="mt-2 flex items-center gap-2">
                {#if queried}<Button variant="secondary" size="sm" onclick={reset}>Clear query</Button>{/if}
                <Button variant="ghost" size="sm" onclick={() => void load(page)}>Retry</Button>
              </div>
            </div>
          </div>
        {:else if !loading && info && docs.length === 0}
          <div class="flex flex-1 flex-col items-center justify-center gap-2 text-center">
            <KeyRound class="size-6 shrink-0 text-muted-foreground/70" />
            <p class="text-ui-sm text-muted-foreground">{queried ? `No ${noun.items} match` : `No ${noun.items} yet`}</p>
          </div>
        {:else if view === 'table'}
          <div class="flex min-h-0 min-w-0 flex-1">
            <DataTable
              columns={grid.columns}
              rows={grid.rows}
              loading={loading && docs.length === 0}
              {dataVersion}
              rowNumberOffset={isAll ? 0 : page * size}
              bind:selected
              bind:focusedRow
              bind:inspectorRow
              filtered={queried}
              onclearfilters={queried ? reset : null}
              columnWidthsKey={`${connection?.id ?? ''}\x00doc:${coll}`}
              onrowopen={(i) => { creating = false; explainQuery = null; editingId = ids[i] ?? null }}
              onremoverows={readOnly ? null : askRemove}
              readonly={!inlineEdit}
              primaryKey={inlineEdit ? (keys.length ? keys : ['_id']) : []}
              onsave={inlineEdit ? saveCell : undefined}
              bind:pendingEditCount
              bind:applyEdits
              bind:resetEdits
              cellEditBlock={inlineEdit ? cellEditBlock : null}
              cellParser={inlineEdit ? parseDocCell : null}
            />
          </div>
        {:else}
          <div class="flex min-h-0 min-w-0 flex-1 flex-col">
            {#await import('./TableJsonView.svelte') then { default: TableJsonView }}
              <TableJsonView columns={grid.columns} rows={docs.map((d) => grid.columns.map((c) => d[c.name]))} tableKey={`${connection?.id ?? ''}:${coll}`} />
            {/await}
          </div>
        {/if}

      </section>

      {#if explainQuery && dataView && adapter.explain}
        <ResizeHandle
          axis="x"
          edge="start"
          onresizestart={() => { dragStart = panelWidth; dragScale = getAppScale() }}
          onresize={(d) => (panelWidth = Math.min(720, Math.max(320, dragStart + d / dragScale)))}
          onresizeend={() => saveNum(PANEL_KEY, panelWidth)}
        />
        <aside class="flex shrink-0 flex-col overflow-hidden border-l border-border/30" style:width="calc({panelWidth}px * var(--app-scale, 1))" aria-label="Query plan">
          <MongoExplainPanel {adapter} {coll} filter={explainQuery.filter} sort={explainQuery.sort} projection={explainQuery.projection} onclose={() => (explainQuery = null)} />
        </aside>
      {:else if panelOpen && dataView}
        <ResizeHandle
          axis="x"
          edge="start"
          onresizestart={() => { dragStart = panelWidth; dragScale = getAppScale() }}
          onresize={(d) => (panelWidth = Math.min(720, Math.max(320, dragStart + d / dragScale)))}
          onresizeend={() => saveNum(PANEL_KEY, panelWidth)}
        />
        <!-- Editor: a second surface beside the data, same frame, nothing extra. -->
        <aside class="flex shrink-0 flex-col overflow-hidden border-l border-border/30" style:width="calc({panelWidth}px * var(--app-scale, 1))" aria-label={`${noun.item} editor`}>
          <div class="flex h-9 shrink-0 items-center gap-2 pl-3 pr-1">
            {#if creating}
              <span class="min-w-0 flex-1 truncate text-ui-xs font-medium text-foreground">New {noun.item.toLowerCase()}</span>
            {:else if editing}
              <span class="flex min-w-0 flex-1 items-baseline gap-2 overflow-hidden">
                {#each keys as k (k)}
                  {@const f = docField(editing[k])}
                  <span class="flex min-w-0 items-baseline gap-1">
                    <span class="shrink-0 text-ui-2xs text-muted-foreground">{k}</span>
                    <span class="truncate font-mono text-ui-xs text-foreground" title={typeof f.value === 'object' ? JSON.stringify(f.value) : String(f.value ?? '')}>{typeof f.value === 'object' ? JSON.stringify(f.value) : String(f.value ?? 'null')}</span>
                  </span>
                {/each}
              </span>
            {/if}
            <button type="button" class={iconBtn} onclick={closePanel} aria-label={`Close ${noun.item.toLowerCase()}`} title="Close (Esc)"><X class="size-3.5 shrink-0" /></button>
          </div>
          <div class="flex min-h-0 flex-1 flex-col">
            {#await import('./CodeEditor.svelte') then { default: CodeEditor }}
              <CodeEditor bind:value={draft} lang="json" {readOnly} keys={editorKeys} ariaLabel={`${noun.item} JSON`} />
            {/await}
          </div>
          {#if draftError}<p class="shrink-0 bg-destructive/5 px-3 py-1.5 text-ui-2xs text-destructive">{draftError}</p>{/if}
          <div class="flex h-12 shrink-0 items-center gap-1.5 border-t border-border/30 px-3">
            {#if !creating}
              <Button
                variant={confirmDelete ? 'destructive' : 'ghost'}
                size="sm"
                class={cn(!confirmDelete && 'text-muted-foreground hover:text-destructive')}
                onclick={() => void remove()}
                onblur={() => (confirmDelete = false)}
                disabled={saving || readOnly}
                title={readOnly ? READ_ONLY_HINT : `Delete this ${noun.item.toLowerCase()}`}
              >
                <Trash2 /> {confirmDelete ? 'Click again to delete' : 'Delete'}
              </Button>
            {/if}
            <span class="flex-1"></span>
            <Button variant="ghost" size="sm" onclick={closePanel}>Cancel</Button>
            <Button size="sm" onclick={() => void save()} disabled={saving || readOnly} title={readOnly ? READ_ONLY_HINT : 'Save (⌘S)'}>
              {#if saving}<LoaderCircle class="animate-spin" />{:else}<Check />{/if}
              {creating ? 'Add' : 'Save'}
            </Button>
          </div>
        </aside>
      {/if}
    </div>
    {#if adapter.import}
      <DocImportDialog bind:open={importOpen} {adapter} {coll} {readOnly} onimported={() => void restart()} />
    {/if}
  {/if}
</div>

<ConfirmDialog
  bind:open={removeOpen}
  icon="trash-2"
  variant="destructive"
  title={`Delete ${pendingRemove.length.toLocaleString()} ${pendingRemove.length === 1 ? noun.item.toLowerCase() : noun.items}?`}
  description={`This removes ${pendingRemove.length === 1 ? 'it' : 'them'} from ${coll} right away. It can't be undone.`}
  confirmLabel={removing ? 'Deleting…' : 'Delete'}
  confirmIcon="trash-2"
  disabled={removing}
  confirmOnEnter
  onconfirm={() => void removeMany()}
/>
