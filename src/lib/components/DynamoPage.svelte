<script>
  // DynamoDB tables: a table list, a grid of items loaded a page at a time by
  // Scan or Query (on the table or an index), and a side panel to edit, add or
  // delete one item as JSON. A PartiQL panel docks underneath for anything the
  // grid doesn't cover. Items stay in DynamoDB's typed JSON; the editor shows
  // plain JSON and keeps each attribute's type on save (see dynamo-json.js).
  import FieldSelect from './FieldSelect.svelte'
  import { untrack } from 'svelte'
  import { traceInteraction } from '$lib/perf.js'
  import ResizeHandle from './ResizeHandle.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { dynamoRequest } from '$lib/api.js'
  import DataTable from './DataTable.svelte'
  import * as Select from '$lib/components/ui/select/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import Icon from './Icon.svelte'
  import ConfirmDialog from './ConfirmDialog.svelte'
  import { rowsToCsv, rowsToObjects, saveExportFile, buildExportFilename } from '$lib/export.js'
  import Kbd from './Kbd.svelte'
  import { readOnlyMode, guardWrite, READ_ONLY_HINT } from '$lib/stores/read-only.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { getAppScale } from '$lib/app-zoom.js'
  import { cn } from '$lib/utils.js'
  import {
    unmarshallItem, marshallItem, cellText, attrType, columnsFor, keyShapes, keyOf, keyId,
    queryBody, scanBody, saveRequest, newItemTemplate, SORT_OPS, dynamoGrid,
  } from '$lib/dynamo-json.js'
  import X from '@lucide/svelte/icons/x'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import Plus from '@lucide/svelte/icons/plus'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Check from '@lucide/svelte/icons/check'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import TerminalIcon from '@lucide/svelte/icons/terminal'
  import ArrowDownWideNarrow from '@lucide/svelte/icons/arrow-down-wide-narrow'
  import ArrowUpNarrowWide from '@lucide/svelte/icons/arrow-up-narrow-wide'
  import ChevronLeft from '@lucide/svelte/icons/chevron-left'
  import Braces from '@lucide/svelte/icons/braces'
  import Sheet from '@lucide/svelte/icons/sheet'
  import PanelLeft from '@lucide/svelte/icons/panel-left'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'

  /**
   * @typedef {import('$lib/dynamo-json.js').Item} Item
   * @typedef {import('$lib/dynamo-json.js').KeyShape} KeyShape
   */

  /**
   * @type {{ active?: boolean, connection?: import('$lib/stores/connections.js').SavedConnection | null,
   *   table?: string, onopensql?: (sql: string, table: string) => void }}
   */
  let { active = false, connection = null, table: tableProp = '', onopensql = () => {} } = $props()

  const readOnly = $derived($readOnlyMode)

  /** @param {unknown} e */
  const msg = (e) => (e instanceof Error ? e.message : String(e))

  // ── Table (from the app sidebar) ───────────────────────────────────────────
  let table = $state('')
  let openedFor = ''
  $effect(() => {
    const key = `${connection?.id ?? connection?.name ?? ''}:${tableProp}`
    if (!active || key === openedFor) return
    openedFor = key
    // Untracked: opening writes (and reads) a lot of this page's state.
    untrack(() => void openTable(tableProp))
  })

  // ── Table description ─────────────────────────────────────────────────────
  let desc = $state.raw(/** @type {any} */ (null))
  const shapes = $derived(desc ? keyShapes(desc) : [])
  const tableKey = $derived(/** @type {KeyShape | undefined} */ (shapes[0]))
  const keyNames = $derived(tableKey ? [tableKey.hash.name, ...(tableKey.range ? [tableKey.range.name] : [])] : [])

  // ── Items ─────────────────────────────────────────────────────────────────
  let mode = $state(/** @type {'scan' | 'query'} */ ('scan'))
  let indexName = $state('')
  const shape = $derived(shapes.find((s) => s.name === indexName) ?? tableKey)
  let qHash = $state('')
  let qSortOp = $state('=')
  let qSort = $state('')
  let qSort2 = $state('')
  let qDesc = $state(false)

  let items = $state.raw(/** @type {Item[]} */ ([]))
  // Pages: cursors[i] is the ExclusiveStartKey that opens page i.
  /** -1 = All: every page, 1,000 at a time, up to ALL_CAP. */
  const PAGE_SIZES = [20, 50, 100, 250, 500, 1000, -1]
  const ALL_CAP = 50_000
  /** @param {number} n */
  const sizeLabel = (n) => (n === -1 ? 'All' : n.toLocaleString())
  let capped = $state(false)
  let size = $state(50)
  let page = $state(0)
  let cursors = $state.raw(/** @type {(Item | null)[]} */ ([null]))
  const lastKey = $derived(cursors[page + 1] ?? null)
  let loading = $state(false)
  let error = $state('')
  let scanned = $state(0)
  let loadToken = 0

  /** @param {string} name */
  async function openTable(name) {
    table = name
    desc = null
    items = []
    dataVersion++
    page = 0
    cursors = [null]
    selectedId = null
    creating = false
    error = ''
    indexName = ''
    mode = 'scan'
    qHash = qSort = qSort2 = ''
    if (!name) return
    const token = ++loadToken
    loading = true
    const done = traceInteraction(`open dynamodb ${name}`)
    try {
      const d = await dynamoRequest('DescribeTable', { TableName: name })
      if (token !== loadToken) return
      desc = d.Table
      indexName = d.Table?.TableName ?? ''
      await load(true)
      done()
    } catch (e) {
      if (token === loadToken) error = msg(e)
    } finally {
      if (token === loadToken) loading = false
    }
  }

  /** First page (a new query), or the page already showing (refresh). @param {boolean} reset */
  async function load(reset) {
    if (reset) cursors = [null]
    await loadPage(reset ? 0 : page)
  }

  /** @param {number} p */
  async function loadPage(p) {
    if (!table || !shape) return
    const token = ++loadToken
    loading = true
    error = ''
    try {
      const all = size === -1
      /** @param {Item | null} start @param {number} limit */
      const fetchOne = (start, limit) =>
        mode === 'query'
          ? dynamoRequest('Query', queryBody(table, shape, { hash: qHash, sortOp: qSortOp, sort: qSort, sort2: qSort2, descending: qDesc, limit, startKey: start }))
          : dynamoRequest('Scan', scanBody(table, shape, { limit, startKey: start }))
      let out
      if (all) {
        /** @type {Item[]} */
        let acc = []
        let start = null
        let sc = 0
        do {
          const o = await fetchOne(start, 1000)
          if (token !== loadToken) return
          acc = acc.concat(o.Items ?? [])
          sc += o.ScannedCount ?? 0
          start = o.LastEvaluatedKey ?? null
        } while (start && acc.length < ALL_CAP)
        capped = !!start
        out = { Items: acc.slice(0, ALL_CAP), ScannedCount: sc, LastEvaluatedKey: null }
        p = 0
      } else {
        capped = false
        out = await fetchOne(cursors[p] ?? null, size)
      }
      if (token !== loadToken) return
      items = out.Items ?? []
      dataVersion++
      scanned = out.ScannedCount ?? 0
      page = p
      const next = cursors.slice(0, p + 1)
      if (out.LastEvaluatedKey) next[p + 1] = out.LastEvaluatedKey
      cursors = next
      selectedId = null
    } catch (e) {
      if (token === loadToken) error = msg(e)
    } finally {
      if (token === loadToken) loading = false
    }
  }

  function runQuery() {
    if (mode === 'query' && !qHash.trim()) {
      error = `Enter a value for ${shape?.hash.name ?? 'the partition key'}`
      return
    }
    void load(true)
  }

  /** @param {'scan' | 'query'} m */
  function setMode(m) {
    mode = m
    if (m === 'scan') void load(true)
  }

  const columns = $derived(columnsFor(items, keyNames))
  const rowIds = $derived(tableKey ? items.map((it) => keyId(it, tableKey)) : [])

  // ── Editor panel ──────────────────────────────────────────────────────────
  let selectedId = $state(/** @type {string | null} */ (null))
  let creating = $state(false)
  const selectedItem = $derived(selectedId == null ? null : items[rowIds.indexOf(selectedId)] ?? null)
  const panelOpen = $derived(creating || !!selectedItem)

  let typed = $state(false)
  let draft = $state('')
  let draftError = $state('')
  let saving = $state(false)
  let confirmDelete = $state(false)

  /** @param {Record<string, any>} obj */
  const pretty = (obj) => JSON.stringify(obj, null, 2)

  // Reset the editor whenever the item (or new-item mode) changes.
  $effect(() => {
    const it = selectedItem
    const isNew = creating
    confirmDelete = false
    draftError = ''
    if (isNew && tableKey) draft = typed ? pretty(marshallItem(newItemTemplate(tableKey))) : pretty(newItemTemplate(tableKey))
    else if (it) draft = typed ? pretty(it) : pretty(unmarshallItem(it))
  })

  /** Parse the editor into a typed item. @returns {Item} */
  function parseDraft() {
    let obj
    try {
      obj = JSON.parse(draft)
    } catch (e) {
      throw new Error(`Not valid JSON: ${msg(e)}`)
    }
    if (!obj || typeof obj !== 'object' || Array.isArray(obj)) throw new Error('An item is a JSON object')
    return typed ? obj : marshallItem(obj, creating ? undefined : selectedItem ?? undefined)
  }

  /** Switch the editor between plain and typed JSON, keeping edits. */
  function toggleTyped() {
    try {
      const item = parseDraft()
      typed = !typed
      draft = typed ? pretty(item) : pretty(unmarshallItem(item))
      draftError = ''
    } catch (e) {
      draftError = msg(e)
    }
  }

  async function save() {
    if (!tableKey || !guardWrite('save items')) return
    draftError = ''
    try {
      const next = parseDraft()
      const prev = creating ? null : selectedItem
      const req = saveRequest(table, tableKey, next, prev)
      if (req.op === 'TransactWriteItems' && !confirm('The key changed. This creates a new item and deletes the old one. Continue?')) return
      saving = true
      await dynamoRequest(req.op, req.body)
      const id = keyId(next, tableKey)
      const oldId = prev ? keyId(prev, tableKey) : null
      const kept = items.filter((it) => {
        const k = keyId(it, tableKey)
        return k !== id && k !== oldId
      })
      const at = oldId ? rowIds.indexOf(oldId) : -1
      items = at >= 0 ? [...kept.slice(0, at), next, ...kept.slice(at)] : [next, ...kept]
      dataVersion++
      creating = false
      selectedId = id
      toast.success(prev ? 'Item saved' : 'Item added')
    } catch (e) {
      draftError = /ConditionalCheckFailed|TransactionCanceled/.test(msg(e))
        ? 'An item with this key already exists'
        : msg(e)
    } finally {
      saving = false
    }
  }

  // ── Delete several (row menu, header button) ────────────────────────────
  let pendingRemove = $state(/** @type {number[]} */ ([]))
  let removeOpen = $state(false)
  let removing = $state(false)
  /** @param {number[]} idx */
  function askRemove(idx) {
    if (!idx.length || !tableKey || !guardWrite('delete items')) return
    pendingRemove = idx
    removeOpen = true
  }
  async function removeMany() {
    if (!tableKey) return
    const list = pendingRemove.map((i) => items[i]).filter(Boolean)
    removing = true
    /** @type {Set<string>} */
    const gone = new Set()
    /** @type {string[]} */
    const errors = []
    for (const it of list) {
      try { await dynamoRequest('DeleteItem', { TableName: table, Key: keyOf(it, tableKey) }); gone.add(keyId(it, tableKey)) } catch (e) { errors.push(msg(e)) }
    }
    items = items.filter((x) => !gone.has(keyId(x, tableKey)))
    gridSelected = new Set()
    selectedId = null
    dataVersion++
    removing = false
    removeOpen = false
    if (gone.size) toast.success(`Deleted ${gone.size.toLocaleString()} ${gone.size === 1 ? 'item' : 'items'}`)
    if (errors.length) toast.error(`${errors.length} could not be deleted`, { description: errors[0] })
  }

  async function remove() {
    const it = selectedItem
    if (!it || !tableKey || !guardWrite('delete items')) return
    if (!confirmDelete) {
      confirmDelete = true
      return
    }
    saving = true
    try {
      await dynamoRequest('DeleteItem', { TableName: table, Key: keyOf(it, tableKey) })
      const id = keyId(it, tableKey)
      items = items.filter((x) => keyId(x, tableKey) !== id)
      dataVersion++
      selectedId = null
      toast.success('Item deleted')
    } catch (e) {
      draftError = msg(e)
    } finally {
      saving = false
      confirmDelete = false
    }
  }

  function startCreate() {
    if (!guardWrite('add items')) return
    selectedId = null
    creating = true
  }

  function closePanel() {
    creating = false
    selectedId = null
    inspectorRow = null
  }

  /** CodeMirror bindings for the item editor. @type {import('@codemirror/view').KeyBinding[]} */
  const editorKeys = [
    { key: 'Mod-s', preventDefault: true, run: () => { void save(); return true } },
    { key: 'Escape', run: () => { closePanel(); return true } },
  ]

  /** @param {boolean} on */
  function setTyped(on) {
    if (on !== typed) toggleTyped()
  }

  /**
   * Value colour by attribute type, from the app's JSON palette so the grid
   * and the editor agree. Keys stay plain foreground.
   * @param {string} t
   */
  function typeColor(t) {
    if (t === 'N' || t === 'NS') return 'text-[var(--json-number)]'
    if (t === 'BOOL') return 'text-[var(--json-boolean)]'
    if (t === 'NULL') return 'text-[var(--json-null)] italic'
    if (t === 'M' || t === 'L') return 'text-muted-foreground'
    return 'text-foreground/85'
  }

  // ── Layout ────────────────────────────────────────────────────────────────
  const PANEL_KEY = 'stroke:dynamo:panelWidth'
  /** @param {string} k @param {number} d */
  function loadNum(k, d) {
    try {
      const n = Number(localStorage.getItem(k))
      return Number.isFinite(n) && n > 0 ? n : d
    } catch {
      return d
    }
  }
  /** @param {string} k @param {number} v */
  function saveNum(k, v) {
    try { localStorage.setItem(k, String(Math.round(v))) } catch {}
  }
  let panelWidth = $state(Math.min(720, Math.max(300, loadNum(PANEL_KEY, 420))))
  let dragStart = 0
  let dragScale = 1

  const where = $derived(connection?.endpoint || [connection?.profile, connection?.region].filter(Boolean).join(' · ') || 'AWS')
  const indexOptions = $derived(shapes.map((s) => ({ value: s.name, label: s.kind === 'table' ? 'Primary index' : `${s.kind.toUpperCase()} · ${s.name}` })))
  const grid = $derived(dynamoGrid(items, keyNames))
  // Bumped wherever `items` is replaced (not in an effect: `+=` reads it too,
  // and an effect that reads and writes the same state re-runs forever).
  let dataVersion = $state(0)
  let gridSelected = $state(/** @type {Set<number>} */ (new Set()))
  let focusedRow = $state(/** @type {number | null} */ (null))
  let inspectorRow = $state(/** @type {number | null} */ (null))
  // The grid's "Open in inspector" opens the item; while open, follow the focus.
  $effect(() => {
    if (inspectorRow != null && rowIds[inspectorRow] != null) {
      creating = false
      selectedId = rowIds[inspectorRow]
    }
  })
  $effect(() => {
    const f = focusedRow
    // Depends on the focus only; reading selectedId here too would loop.
    untrack(() => {
      if (selectedId != null && !creating && f != null && rowIds[f] != null) selectedId = rowIds[f]
    })
  })

  const VIEW_KEY = 'stroke:dynamo:view'
  let view = $state(/** @type {'table' | 'json'} */ ((() => { try { return localStorage.getItem(VIEW_KEY) === 'json' ? 'json' : 'table' } catch { return 'table' } })()))
  /** @param {'table' | 'json'} v */
  function setView(v) {
    view = v
    try { localStorage.setItem(VIEW_KEY, v) } catch {}
  }
  const pageSelectTrigger = 'h-7 min-w-0 gap-1 border-transparent! bg-transparent! focus-visible:border-ring! data-[state=open]:border-ring! px-2 text-ui-xs font-normal tabular-nums shadow-none hover:bg-accent!'

  /** Selected rows, or every loaded row when nothing is selected. */
  function exportRows() {
    return gridSelected.size ? [...gridSelected].sort((a, b) => a - b).map((i) => grid.rows[i]).filter(Boolean) : grid.rows
  }
  /** @param {'json' | 'jsonl' | 'csv'} fmt */
  async function exportAs(fmt) {
    const rows = exportRows()
    if (!rows.length) return
    const objs = fmt === 'csv' ? null : rowsToObjects(grid.columns, rows)
    const content = fmt === 'csv' ? rowsToCsv(grid.columns, rows) : fmt === 'json' ? JSON.stringify(objs, null, 2) : (objs ?? []).map((o) => JSON.stringify(o)).join('\n')
    try {
      if (await saveExportFile(content, buildExportFilename(table, fmt), fmt)) toast.success(`Exported ${rows.length.toLocaleString()} ${rows.length === 1 ? 'item' : 'items'}`)
    } catch (e) {
      toast.error('Export failed', { description: String(e) })
    }
  }
  async function copyJson() {
    const objs = rowsToObjects(grid.columns, exportRows())
    try {
      await navigator.clipboard.writeText(JSON.stringify(objs.length === 1 ? objs[0] : objs, null, 2))
      toast.success(`Copied ${objs.length.toLocaleString()} as JSON`)
    } catch (e) {
      toast.error('Could not copy', { description: String(e) })
    }
  }

  const iconBtn = 'inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[color,background-color,scale] duration-[120ms] ease-[var(--ease-out)] active:scale-[0.97] hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40'
  /** A quiet text toggle: the on state is foreground, the rest muted. No pill, no frame. */
  const textToggle = 'inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-ui-xs transition-colors hover:text-foreground'
  /** A frameless field inside the command bar. */
  const barInput = 'field-surface h-8 min-w-0 px-3 font-mono text-ui-xs text-foreground shadow-none outline-none placeholder:text-muted-foreground/60'

  const from = $derived(items.length ? (size === -1 ? 1 : page * size + 1) : 0)
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden bg-panel">
  {#if !table}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 text-center">
      <PanelLeft class="size-6 shrink-0 text-muted-foreground/70" />
      <p class="text-ui-sm text-foreground">Pick a table in the sidebar</p>
      <p class="font-mono text-ui-2xs text-muted-foreground">{where}</p>
    </div>
  {:else}
    <!-- Header: what this is, and the few things to do with it. No divider. -->
    <header class="flex h-11 shrink-0 items-center gap-2 border-b border-border/30 pl-4 pr-2">
      <h2
        class="min-w-0 truncate text-ui-sm font-medium text-foreground"
        title={desc ? `${table} · ~${(desc.ItemCount ?? 0).toLocaleString()} items (approximate, refreshed about every six hours)` : table}
      >{table}</h2>
      <span class="flex-1"></span>
      <!-- Pager in the header, as on SQL tables. DynamoDB has no count, so "of many". -->
      <div class="flex shrink-0 items-center gap-1 text-ui-xs text-muted-foreground">
        {#if loading && items.length > 0}<LoaderCircle class="size-3 shrink-0 animate-spin" />{/if}
        {#if desc && items.length > 0}
          <span class="mr-1 tabular-nums" title={mode === 'scan' && scanned > items.length ? `${scanned.toLocaleString()} scanned` : undefined}>
            <span class="text-foreground">{size === -1 ? 1 : from.toLocaleString()}-{(size === -1 ? items.length : page * size + items.length).toLocaleString()}</span>{#if lastKey || capped} of many{/if}
          </span>
        {/if}
        <Select.Root type="single" value={String(size)} onValueChange={(v) => { if (v) { size = Number(v); void load(true) } }} disabled={loading}>
          <Select.Trigger size="sm" class={pageSelectTrigger} title="Items per page" aria-label="Items per page">{sizeLabel(size)}</Select.Trigger>
          <Select.Content align="end" class="min-w-0">
            {#each PAGE_SIZES as n (n)}
              <Select.Item value={String(n)} label={sizeLabel(n)} />
            {/each}
          </Select.Content>
        </Select.Root>
        {#if size !== -1}
          <button type="button" class={iconBtn} onclick={() => void loadPage(page - 1)} disabled={page === 0 || loading} aria-label="Previous page" title="Previous page"><ChevronLeft class="size-3.5 shrink-0" /></button>
          <button type="button" class={iconBtn} onclick={() => void loadPage(page + 1)} disabled={!lastKey || loading} aria-label="Next page" title="Next page"><ChevronRight class="size-3.5 shrink-0" /></button>
        {:else if capped}
          <span class="text-ui-2xs text-warning" title="All stops at {ALL_CAP.toLocaleString()} items so the grid stays fast. Query to narrow it down.">first {ALL_CAP.toLocaleString()}</span>
        {/if}
      </div>
      <span class="mx-1 h-4 w-px shrink-0 bg-border/50" aria-hidden="true"></span>
      <div class="flex items-center" role="group" aria-label="View">
        {#each [{ v: 'table', l: 'Table', I: Sheet }, { v: 'json', l: 'JSON', I: Braces }] as m (m.v)}
          {@const I = m.I}
          <button type="button" aria-pressed={view === m.v} class={cn(textToggle, view === m.v ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/50')} onclick={() => setView(/** @type {'table' | 'json'} */ (m.v))}><I class="size-3.5 shrink-0" />{m.l}</button>
        {/each}
      </div>
      <button type="button" class={iconBtn} onclick={() => void load(false)} disabled={!desc} title="Refresh" aria-label="Refresh"><RefreshCw class={cn('size-3.5', loading && 'animate-spin')} /></button>
      <DropdownMenu.Root>
        <DropdownMenu.Trigger class={iconBtn} title="More actions" aria-label="More actions" disabled={!desc}>
          <Icon name="more-horizontal" class="size-3.5 shrink-0" />
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="min-w-56">
          <DropdownMenu.Item onSelect={() => onopensql(`SELECT * FROM "${table}"`, table)}>
            <TerminalIcon class="size-3.5 shrink-0" />
            <span data-slot="menu-label">Open in PartiQL editor</span>
          </DropdownMenu.Item>
          <DropdownMenu.Item disabled={!items.length} onSelect={() => void copyJson()}>
            <Icon name="copy" class="size-3.5 shrink-0" />
            <span data-slot="menu-label">Copy {gridSelected.size ? `${gridSelected.size} selected` : 'page'} as JSON</span>
          </DropdownMenu.Item>
          <DropdownMenu.Separator />
          <DropdownMenu.Group>
            <DropdownMenu.GroupHeading class="px-1.5 pt-1.5 pb-0.5 text-ui-3xs font-medium text-muted-foreground">
              Export {gridSelected.size ? `${gridSelected.size} selected` : `${items.length.toLocaleString()} loaded`}
            </DropdownMenu.GroupHeading>
            {#each [['json', 'JSON', 'braces'], ['jsonl', 'JSON Lines', 'list-ordered'], ['csv', 'CSV', 'file-text']] as [fmt, label, icon] (fmt)}
              <DropdownMenu.Item disabled={!items.length} onSelect={() => void exportAs(/** @type {'json' | 'jsonl' | 'csv'} */ (fmt))}>
                <Icon name={icon} class="size-3.5 shrink-0" />
                <span data-slot="menu-label">{label}</span>
              </DropdownMenu.Item>
            {/each}
          </DropdownMenu.Group>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
      {#if gridSelected.size > 0}
        <Button size="sm" variant="ghost" class="h-8 rounded-lg text-destructive hover:bg-destructive/10 hover:text-destructive" disabled={readOnly} title={readOnly ? READ_ONLY_HINT : 'Delete the selected'} onclick={() => askRemove([...gridSelected].sort((a, b) => a - b))}>
          <Trash2 /> Delete {gridSelected.size.toLocaleString()}
        </Button>
      {/if}
      <Button size="sm" variant="outline" class="h-8 rounded-lg border-border/60" onclick={startCreate} disabled={readOnly || !tableKey} title={readOnly ? READ_ONLY_HINT : 'Add an item'}><Plus /> Item</Button>
    </header>

    <!-- Command bar: how to read (scan or query), from where, and the key condition. -->
    <form class="flex shrink-0 flex-wrap items-center gap-2 border-b border-border/30 px-3 py-2" onsubmit={(e) => { e.preventDefault(); runQuery() }}>
      <div class="flex h-8 items-center gap-0.5 rounded-lg border border-border/60 p-0.5" role="group" aria-label="Read mode">
        {#each [{ v: 'scan', l: 'Scan' }, { v: 'query', l: 'Query' }] as m (m.v)}
          <button type="button" aria-pressed={mode === m.v} disabled={!desc} class={cn('inline-flex h-full items-center rounded-md px-2.5 text-ui-xs transition-colors', mode === m.v ? 'bg-accent text-foreground' : 'text-muted-foreground hover:text-foreground')} onclick={() => setMode(/** @type {'scan' | 'query'} */ (m.v))}>{m.l}</button>
        {/each}
      </div>
      {#if shapes.length > 1}
        <span class="mx-0.5"></span>
        <FieldSelect size="sm" class="w-40 shrink-0 bg-transparent" bind:value={indexName} onchange={() => { if (mode === 'scan') void load(true) }} aria-label="Table or index" options={indexOptions} />
      {/if}
      {#if mode === 'query' && shape}
        <span class="mx-0.5"></span>
        <span class="shrink-0 font-mono text-ui-xs text-muted-foreground">{shape.hash.name} =</span>
        <input bind:value={qHash} placeholder={shape.hash.type === 'N' ? 'number' : 'value'} aria-label={`${shape.hash.name} value`} spellcheck="false" autocomplete="off" class={cn(barInput, 'w-40')} />
        {#if shape.range}
          <span class="shrink-0 pl-1 font-mono text-ui-xs text-muted-foreground">{shape.range.name}</span>
          <FieldSelect size="sm" class="w-28 shrink-0 bg-transparent" bind:value={qSortOp} aria-label="Sort key condition" options={SORT_OPS} />
          <input bind:value={qSort} placeholder="any" aria-label={`${shape.range.name} value`} spellcheck="false" autocomplete="off" class={cn(barInput, 'w-32')} />
          {#if qSortOp === 'between'}
            <span class="text-ui-xs text-muted-foreground">and</span>
            <input bind:value={qSort2} aria-label={`${shape.range.name} upper value`} spellcheck="false" autocomplete="off" class={cn(barInput, 'w-32')} />
          {/if}
          <button type="button" class={cn(iconBtn, 'size-6', qDesc && 'text-foreground')} onclick={() => (qDesc = !qDesc)} aria-pressed={qDesc} title={qDesc ? 'Newest sort key first' : 'Oldest sort key first'} aria-label="Sort order">
            {#if qDesc}<ArrowDownWideNarrow class="size-3.5 shrink-0" />{:else}<ArrowUpNarrowWide class="size-3.5 shrink-0" />{/if}
          </button>
        {/if}
      {/if}
      <span class="flex-1"></span>
      <button type="submit" disabled={!desc || loading} class="inline-flex h-8 shrink-0 items-center gap-2 rounded-md border border-border/60 px-3 text-ui-xs font-medium text-foreground/85 transition-colors hover:bg-accent disabled:opacity-40" title={mode === 'query' ? 'Run the query (Enter)' : 'Scan again (Enter)'}>
        {#if loading}<LoaderCircle class="size-3.5 shrink-0 animate-spin" />{/if}{mode === 'query' ? 'Run' : 'Scan'}
        <span class="text-ui-2xs text-muted-foreground">↵</span>
      </button>
    </form>

    <div class="flex min-h-0 flex-1 overflow-hidden">
      <!-- Data: the one framed surface on the page, pager inside it. -->
      <section class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden" aria-label={`Items in ${table}`}>
        {#if error}
          <div class="flex flex-1 flex-col items-center justify-center px-6">
            <div class="flex w-full max-w-md flex-col gap-2">
              <p class="text-ui-sm font-medium text-foreground">Couldn't read this table</p>
              <pre class="app-scroll max-h-48 overflow-auto whitespace-pre-wrap break-words font-mono text-ui-2xs leading-relaxed text-muted-foreground">{error}</pre>
              <Button variant="ghost" size="sm" class="mt-2 w-fit rounded-md" onclick={() => void load(false)}>Retry</Button>
            </div>
          </div>
        {:else if !loading && desc && items.length === 0}
          <div class="flex flex-1 flex-col items-center justify-center gap-2 text-center">
            <KeyRound class="size-6 shrink-0 text-muted-foreground/70" />
            <p class="text-ui-sm text-muted-foreground">
              {mode === 'query' ? 'No items match' : lastKey ? 'Nothing on this page; the next page keeps scanning' : 'This table is empty'}
            </p>
          </div>
        {:else if view === 'table'}
          <div class="flex min-h-0 min-w-0 flex-1">
            <DataTable
              columns={grid.columns}
              rows={grid.rows}
              loading={loading && items.length === 0}
              {dataVersion}
              rowNumberOffset={size === -1 ? 0 : page * size}
              bind:selected={gridSelected}
              bind:focusedRow
              bind:inspectorRow
              columnWidthsKey={`${connection?.id ?? ''}\x00dynamo:${table}`}
              onrowopen={(i) => { creating = false; selectedId = rowIds[i] ?? null }}
              onremoverows={readOnly ? null : askRemove}
              readonly
            />
          </div>
        {:else}
          <div class="flex min-h-0 min-w-0 flex-1 flex-col">
            {#await import('./TableJsonView.svelte') then { default: TableJsonView }}
              <TableJsonView columns={grid.columns} rows={grid.rows} tableKey={`${connection?.id ?? ''}:dynamo:${table}`} />
            {/await}
          </div>
        {/if}

      </section>

      {#if panelOpen}
        <ResizeHandle
          axis="x"
          edge="start"
          onresizestart={() => { dragStart = panelWidth; dragScale = getAppScale() }}
          onresize={(d) => (panelWidth = Math.min(720, Math.max(300, dragStart + d / dragScale)))}
          onresizeend={() => saveNum(PANEL_KEY, panelWidth)}
        />
        <!-- Editor: a second surface beside the data, same frame, nothing extra. -->
        <aside class="flex shrink-0 flex-col overflow-hidden border-l border-border/30" style:width="calc({panelWidth}px * var(--app-scale, 1))" aria-label="Item editor">
          <div class="flex h-9 shrink-0 items-center gap-2 pl-3 pr-1">
            {#if creating}
              <span class="min-w-0 flex-1 truncate text-ui-xs font-medium text-foreground">New item</span>
            {:else if selectedItem}
              <span class="flex min-w-0 flex-1 items-baseline gap-2 overflow-hidden">
                {#each keyNames as k (k)}
                  <span class="flex min-w-0 items-baseline gap-1">
                    <span class="shrink-0 text-ui-2xs text-muted-foreground">{k}</span>
                    <span class="truncate font-mono text-ui-xs text-foreground" title={cellText(selectedItem[k], 2000)}>{cellText(selectedItem[k], 80)}</span>
                  </span>
                {/each}
              </span>
            {/if}
            <div class="flex items-center" role="group" aria-label="Editor format">
              {#each [{ v: false, l: 'JSON', t: 'Plain JSON; each attribute keeps its DynamoDB type on save' }, { v: true, l: 'Typed', t: 'DynamoDB typed JSON: {"S": "..."}, {"N": "1"}' }] as m (m.l)}
                <button type="button" aria-pressed={typed === m.v} title={m.t} class={cn(textToggle, 'h-6 px-1.5 text-ui-2xs', typed === m.v ? 'text-foreground' : 'text-muted-foreground')} onclick={() => setTyped(m.v)}>{m.l}</button>
              {/each}
            </div>
            <button type="button" class={iconBtn} onclick={closePanel} aria-label="Close item" title="Close (Esc)"><X class="size-3.5 shrink-0" /></button>
          </div>
          <div class="flex min-h-0 flex-1 flex-col">
            {#await import('./CodeEditor.svelte') then { default: CodeEditor }}
              <CodeEditor bind:value={draft} lang="json" readOnly={readOnly} keys={editorKeys} ariaLabel="Item JSON" />
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
                title={readOnly ? READ_ONLY_HINT : 'Delete this item'}
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
  {/if}
</div>

<ConfirmDialog
  bind:open={removeOpen}
  icon="trash-2"
  variant="destructive"
  title={`Delete ${pendingRemove.length.toLocaleString()} ${pendingRemove.length === 1 ? 'item' : 'items'}?`}
  description={`This removes ${pendingRemove.length === 1 ? 'it' : 'them'} from ${table} right away. It can't be undone.`}
  confirmLabel={removing ? 'Deleting…' : 'Delete'}
  confirmIcon="trash-2"
  disabled={removing}
  confirmOnEnter
  onconfirm={() => void removeMany()}
/>
