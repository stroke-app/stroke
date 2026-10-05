<script>
  import { onDestroy, tick, untrack } from 'svelte'
  import dagre from '@dagrejs/dagre'
  import { getSchemaColumnStructure, listIndexes, getTableDdl, saveExportAs } from '$lib/api.js'
  import ErdCanvas from './ErdCanvas.svelte'
  import Loader from '@lucide/svelte/icons/loader'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import Search from '@lucide/svelte/icons/search'
  import LayoutDashboard from '@lucide/svelte/icons/layout-dashboard'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import Link from '@lucide/svelte/icons/link'
  import X from '@lucide/svelte/icons/x'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import Network from '@lucide/svelte/icons/network'
  import Download from '@lucide/svelte/icons/download'
  import Square from '@lucide/svelte/icons/square'
  import Copy from '@lucide/svelte/icons/copy'
  import Check from '@lucide/svelte/icons/check'
  import LayoutGrid from '@lucide/svelte/icons/layout-grid'
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal'
  import ListFilter from '@lucide/svelte/icons/list-filter'
  import Crosshair from '@lucide/svelte/icons/crosshair'
  import EyeOff from '@lucide/svelte/icons/eye-off'
  import GitBranch from '@lucide/svelte/icons/git-branch'
  import ListTree from '@lucide/svelte/icons/list-tree'
  import Workflow from '@lucide/svelte/icons/workflow'
  import Waypoints from '@lucide/svelte/icons/waypoints'
  import MermaidViewer from './MermaidViewer.svelte'
  import RelationTreePage from './RelationTreePage.svelte'
  import ErdHierarchy from './ErdHierarchy.svelte'
  import CodeEditor from './CodeEditor.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import ZoomIn from '@lucide/svelte/icons/zoom-in'
  import ZoomOut from '@lucide/svelte/icons/zoom-out'
  import Maximize2 from '@lucide/svelte/icons/maximize-2'
  import BookOpen from '@lucide/svelte/icons/book-open'
  import FileCode from '@lucide/svelte/icons/file-code'
  import PanelLeftClose from '@lucide/svelte/icons/panel-left-close'
  import PanelLeftOpen from '@lucide/svelte/icons/panel-left-open'
  import WrapText from '@lucide/svelte/icons/wrap-text'
  import Minimize2 from '@lucide/svelte/icons/minimize-2'
  import Plus from '@lucide/svelte/icons/plus'
  import Scan from '@lucide/svelte/icons/scan'
  import Kbd from './Kbd.svelte'
  import { keycaps, IS_MAC } from '$lib/shortcuts.js'
  import { saveDiagram } from '$lib/stores/saved-diagrams.js'
  import { erdToMermaid } from '$lib/erd-mermaid.js'
  import { toast } from "$lib/components/ui/sonner/toast.svelte.js"
  import { svgStringToPngBlob, copyPngToClipboard } from '$lib/svg-png.js'
  import { Popover, PopoverTrigger, PopoverContent } from '$lib/components/ui/popover/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { isCurrentThemeDark } from '$lib/stores/settings.js'
  import { loadErdSettings, saveErdSettings, SPACING_PRESETS, SCOPE_DEFAULTS, DEFAULT_ERD_SETTINGS } from '$lib/stores/erd-settings.js'
  import { routeEdges, routeToSvgPath, corridorPathOrtho, CLEAR, MAX_ROUTED_NODES } from '$lib/erd-routing.js'
  import { separateCards, wrapTallRanks } from '$lib/erd-layout.js'
  import { layoutWithElk } from '$lib/erd-elk.js'
  import { visibleTables, visibleRels, relatedTo, linkedTables, mergeParallelEdges, hubTables } from '$lib/erd-filter.js'
  import { cn } from '$lib/utils.js'

  let {
    schema = 'public',
    schemas = /** @type {string[]} */ ([]),
    /** @type {((schema:string, table:string, opts?:{duplicate?:boolean, viewMode?:string})=>void)|undefined} */
    onopentable = undefined,
    /** When set, the ERD is scoped to this table + the tables directly FK-connected to it. */
    focusTable = '',
    onclearfocus = /** @type {(() => void)|undefined} */ (undefined),
    /** Open the Diagrams tab, after the Mermaid view saved its code there. */
    onopendiagrams = /** @type {(() => void)|undefined} */ (undefined),
    /** True when the host chrome (the table tab bar) owns the Export menu. The
     *  standalone ERD tab has no such menu, so it keeps its own entries. */
    hostExports = false,
    /** True when this diagram is rendered inside the focused table's own tab. */
    insideTableTab = false,
  } = $props()

  /**
   * @typedef {{ name: string, dataType: string, isNullable: boolean,
   *   columnDefault: string|null, foreignKey: string|null,
   *   fkConstraintName: string|null, ordinalPosition: number, isPrimaryKey?: boolean }} Col
   * @typedef {{ name: string, columns: Col[], pkCols: Set<string>,
   *   uniqueCols?: Set<string>, hiddenCount?: number }} TableMeta
   * @typedef {'self' | 'related' | 'all'} Scope
   */

  /** @type {{ id: Scope, label: string, hint: string }[]} */
  const SCOPES = [
    { id: 'self',    label: 'Table',   hint: 'Only this table' },
    { id: 'related', label: 'Related', hint: 'This table and everything FK-linked to it' },
    { id: 'all',     label: 'Schema',  hint: 'Every table in the schema' },
  ]

  // ── Config ────────────────────────────────────────────────────────────────
  const NODE_W = 268
  const ROW_H = 28
  const HDR_H = 42
  const PAD_B = 10
  const WARN_MANY = 60
  /**
   * Above this many tables the whole-schema diagram waits to be asked for.
   * Dagre, the edge router and the cards themselves are each seconds of work at
   * that size, so drawing it unprompted reads as a hang - and nobody looks for
   * anything in a thousand-card diagram anyway. The table filter is the way in.
   */
  const MAX_AUTO_TABLES = 240
  /** Rows rendered in the table filter at once; the search box reaches the rest. */
  const PICKER_ROWS = 200

  const cfg = { NODE_W, ROW_H, HDR_H, PAD_B }

  /** @type {ErdCanvas|null} */
  let erd = $state(null)

  // ── State ─────────────────────────────────────────────────────────────────
  let loading = $state(false)
  let error = $state('')
  let search = $state('')
  let searchEl = $state(/** @type {HTMLInputElement | null} */ (null))
  let rootEl = $state(/** @type {HTMLDivElement | null} */ (null))
  /** Full screen like a maximised terminal pane: ⌘⇧↵ / Ctrl+Shift+Enter. */
  const FULLSCREEN_KEYS = keycaps('Mod+Shift+Enter').join(IS_MAC ? '' : '+')
  let activeSchema = $state(untrack(() => schema))
  let schemaOpen = $state(false)
  let settingsOpen = $state(false)
  let exporting = $state(false)
  let settings = $state(loadErdSettings())
  /** Big schemas open filtered to FK-connected tables - session only, not persisted. */
  let autoConnected = $state(false)
  const connectedOnly = $derived(settings.connectedOnly || autoConnected)
  /** @type {Scope} */
  let scope = $state(untrack(() => (focusTable ? 'related' : 'all')))
  /**
   * What the diagram actually uses: the user's explicit choices win, everything
   * else follows the view's default (a focused table diagram opens wide and
   * unrouted; a whole schema opens denser with routed lines).
   */
  const view = $derived.by(() => {
    const base = focusTable && scope !== 'all' ? SCOPE_DEFAULTS.focused : SCOPE_DEFAULTS.schema
    // A wide schema opens keys-only. Full column lists make every card taller,
    // and card height is what a layout pass and the edge router both pay for.
    const wide = tableMeta.size > 120 && !(focusTable && scope !== 'all')
    return {
      ...settings,
      columnMode: settings.touched.includes('columnMode')
        ? settings.columnMode
        : wide ? 'keys' : DEFAULT_ERD_SETTINGS.columnMode,
      spacing: settings.touched.includes('spacing') ? settings.spacing : base.spacing,
      routing: settings.touched.includes('routing') ? settings.routing : base.routing,
    }
  })
  const gaps = $derived(SPACING_PRESETS[view.spacing] ?? SPACING_PRESETS.comfortable)
  /**
   * Tables the user picked by hand. Empty means "no filter" - the scope and the
   * linked-only toggle decide instead. Session-only, per schema: a filter is
   * something you set while reading one diagram, not a setting to inherit.
   * Always replaced, never mutated, so the reads stay reactive.
   * @type {Set<string>}
   */
  let picked = $state(new Set())
  /** Bumped on every pick change - the layout key can't diff a Set cheaply. */
  let pickVersion = $state(0)
  let pickerOpen = $state(false)
  let pickQuery = $state('')
  /** Set once the user asks for a schema too big to draw unprompted. */
  let drawAll = $state(false)
  /** Table count the diagram is holding back on, 0 when it is drawing. */
  let gatedCount = $state(0)
  /** Every foreign key in the schema, filter-independent. @type {any[]} */
  let allRels = $state(/** @type {any[]} */ ([]))
  /** @type {string|null} */
  let selectedTable = $state(null)
  /** @type {Map<string, TableMeta>} */
  // Raw as well, and for the same reason as `nodes` below: every card's columns
  // and key sets are read straight out of here into the node data the canvas
  // draws from, so a proxy here reaches the per-row hot path no matter what the
  // node array is. Both writers below mutate and then replace the Map, which is
  // what makes reactivity fire either way.
  let tableMeta = $state.raw(new Map())

  // Raw, not deep-reactive. These arrays are the canvas renderer's hot data: it
  // reads n.position, n.data.columns[i].name and friends for every card and every
  // row of every frame. Behind a $state proxy each of those reads is a trap, and
  // every nested column object gets its own proxy allocated the first time it is
  // touched - which is most of what made panning a large ER diagram stutter.
  // Nothing here is ever mutated in place; every write below replaces the whole
  // array, so the raw form loses no reactivity.
  /** @type {any[]} */
  let nodes = $state.raw([])
  /** @type {any[]} */
  let edges = $state.raw([])
  /** Foreign keys actually drawn - a merged line stands for several of them. */
  const fkCount = $derived([...edges, ...hubEdges].reduce((n, e) => n + (e.mergedCount ?? 1), 0))
  /**
   * Edge id → the polyline the layout reserved for it. Present only for layouts
   * that route their own edges (Dagre does); empty otherwise, and the renderer
   * falls back to its own line.
   * @type {Map<string, {x:number,y:number}[]>}
   */
  // Raw for the same reason: the renderer looks a hint up per edge, and a proxied
  // Map puts a trap on every one of those. Replaced wholesale, never mutated.
  let routeHints = $state.raw(new Map())
  /** Relationships into hub tables (see hubTables). Not laid out and not routed:
   *  the canvas names them on the row and draws them for the hovered or selected card. */
  let hubEdges = $state.raw(/** @type {any[]} */ ([]))
  /** @type {Set<string>} */
  let hubs = $state.raw(new Set())

  // ── Views ─────────────────────────────────────────────────────────────────
  // The same tables several ways: the canvas, the hierarchy (foreign-key
  // order, top to bottom), Mermaid source with a live preview, the relation
  // tree, the column dictionary and the DDL. The canvas keeps its layout while
  // another view is up; it remounts where it was.
  /** @type {'canvas' | 'hierarchy' | 'mermaid' | 'tree' | 'dictionary' | 'ddl'} */
  let erdView = $state('canvas')
  /** The page over the whole window below the title bar; Esc or the button ends it. */
  let fullscreen = $state(false)
  /**
   * Above this many tables a Mermaid render is seconds of synchronous work,
   * so the preview waits for a click. The code is still there to copy.
   */
  const MERMAID_AUTO_MAX = 60
  let mermaidForce = $state(false)
  const mermaidGated = $derived(shownTables.length > MERMAID_AUTO_MAX && !mermaidForce)
  /** @type {MermaidViewer | null} */
  let mermaidViewer = $state(null)
  /** The Mermaid code pane. Starts folded: the picture is what the view is
   *  opened for, and the code is one click away to copy or edit. */
  let mermaidCodeOpen = $state(false)
  /** Long relationship lines wrap instead of running off the pane. */
  let mermaidWrap = $state(false)
  /** Width of the code pane once the divider has been dragged; 0 is the default split. */
  let mermaidCodeW = $state(0)
  /** Drag the divider between the code and the preview. @param {PointerEvent} e */
  function startMermaidSplit(e) {
    const host = /** @type {HTMLElement | null} */ (/** @type {HTMLElement} */ (e.currentTarget).parentElement)
    if (!host) return
    const rect = host.getBoundingClientRect()
    const move = (/** @type {PointerEvent} */ ev) => {
      mermaidCodeW = Math.round(Math.max(280, Math.min(rect.width - 240, ev.clientX - rect.left)))
    }
    const up = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    e.preventDefault()
  }
  const VIEWS = /** @type {const} */ ([
    { id: 'canvas', label: 'Diagram', hint: 'Cards and relationship lines, laid out and routed around each other' },
    { id: 'hierarchy', label: 'Hierarchy', hint: 'Top to bottom in foreign-key order, like a roadmap: each table sits under the tables it points at' },
    { id: 'mermaid', label: 'Mermaid', hint: 'The same tables as Mermaid source, with a live preview' },
    { id: 'tree', label: 'Tree', hint: 'One table at a time: what it points at and what points at it' },
    { id: 'dictionary', label: 'Dictionary', hint: 'Every column on the page in one searchable list: type, nulls, keys and what they reference' },
    { id: 'ddl', label: 'DDL', hint: 'CREATE statements for the tables on the page, as the database reports them' },
  ])

  // ── Tree ──────────────────────────────────────────────────────────────────
  // The Tree draws inside this page and its controls live in this page's bar,
  // so the state is held here and bound down.
  let treeView = $state(/** @type {'flow' | 'all' | 'list'} */ ('all'))
  let treeDepth = $state(1)
  /** Folded at first: the picture is what the view is opened for. */
  let treeListOpen = $state(false)
  let treeSearch = $state('')
  let treeSummary = $state('')
  /** @type {RelationTreePage | null} */
  let treePage = $state(null)
  const TREE_VIEWS = [
    { value: 'flow', label: 'Flow', hint: 'A flowchart around the table: arrows from the key to what it points at' },
    { value: 'all', label: 'All', hint: 'Every table on the page, one line per link' },
    { value: 'list', label: 'List', hint: 'Every relationship, table by table, with row counts' },
  ]

  // ── Hierarchy ─────────────────────────────────────────────────────────────
  let hierQuery = $state('')
  /** Lines into hub tables left out, as the Diagram's pills setting does. */
  let hierHideHubs = $state(untrack(() => view.hubLinks !== 'lines'))
  let hierSummary = $state('')
  let hierHubCount = $state(0)
  /** @type {ErdHierarchy | null} */
  let hierPage = $state(null)

  // ── Find (Ctrl/⌘F) ────────────────────────────────────────────────────────
  /** @type {CodeEditor | null} */
  let mermaidEditor = $state(null)
  /** @type {CodeEditor | null} */
  let ddlEditor = $state(null)
  /**
   * Ctrl/⌘F for whichever view is up: the bar's search box, or the code's own
   * find panel in Mermaid (its code pane opened first) and DDL.
   */
  export async function focusSearch() {
    if (erdView === 'mermaid') {
      // A folded pane mounts its editor a frame after it opens.
      if (!mermaidCodeOpen) { mermaidCodeOpen = true; await tick(); await new Promise((r) => requestAnimationFrame(r)) }
      mermaidEditor?.find()
      return
    }
    if (erdView === 'ddl') { ddlEditor?.find(); return }
    searchEl?.focus()
    searchEl?.select()
  }

  // ── Dictionary ────────────────────────────────────────────────────────────
  let dictQuery = $state('')
  /** Rows the dictionary draws. Capped: past this the filter is the tool. */
  const DICT_ROWS_MAX = 1500
  const dictRows = $derived.by(() => {
    const q = dictQuery.trim().toLowerCase()
    /** @type {{ table: string, col: string, type: string, nullable: boolean, pk: boolean, fk: boolean, uk: boolean, ref: string, refTable: string }[]} */
    const rows = []
    for (const t of shownTables) {
      for (const c of t.columns) {
        const parts = c.foreignKey ? c.foreignKey.split('.') : null
        const refTable = parts ? (parts.length >= 3 ? parts[1] : parts[0]) : ''
        const ref = parts ? (parts.length >= 3 ? `${parts[1]}.${parts[2]}` : parts.join('.')) : ''
        const type = String(c.dataType ?? '')
        if (q && !(t.name.toLowerCase().includes(q) || c.name.toLowerCase().includes(q) || type.toLowerCase().includes(q) || ref.toLowerCase().includes(q))) continue
        rows.push({
          table: t.name, col: c.name, type, nullable: c.isNullable !== false,
          pk: t.pkCols?.has(c.name) ?? false, fk: !!c.foreignKey, uk: t.uniqueCols?.has(c.name) ?? false,
          ref, refTable,
        })
      }
    }
    return rows
  })

  // ── DDL ───────────────────────────────────────────────────────────────────
  /** CREATE statements per table, fetched the first time the view opens. */
  let ddlCache = $state(/** @type {Map<string, string>} */ (new Map()))
  let ddlLoading = $state(false)
  async function ensureDdl() {
    const todo = shownTables.map((t) => t.name).filter((n) => !ddlCache.has(n))
    if (!todo.length) return
    ddlLoading = true
    try {
      // A few at a time: one call per table, and a schema can be a few hundred.
      for (let i = 0; i < todo.length; i += 6) {
        const next = new Map(ddlCache)
        await Promise.all(todo.slice(i, i + 6).map(async (name) => {
          try { next.set(name, String((await getTableDdl(activeSchema, name)) ?? '').trim()) }
          catch (e) { next.set(name, `-- ${name}: ${String(e)}`) }
        }))
        ddlCache = next
      }
    } finally {
      ddlLoading = false
    }
  }
  $effect(() => { if (erdView === 'ddl') void ensureDdl() })
  const ddlSource = $derived(shownTables.map((t) => ddlCache.get(t.name) ?? `-- ${t.name}: loading…`).join('\n\n'))
  let copiedAllDdl = $state(false)
  async function copyAllDdl() {
    try {
      await navigator.clipboard.writeText(ddlSource)
      copiedAllDdl = true
      setTimeout(() => (copiedAllDdl = false), 1600)
    } catch { /* clipboard unavailable */ }
  }
  /** Tables on the page, in card order. */
  const shownTables = $derived(nodes.map((n) => tableMeta.get(n.id)).filter((t) => !!t))
  /** Every relationship on the page in the shape the Mermaid generators take.
   *  Hub links are in: the source is an export, and "where is `user`" has to be
   *  answerable from it. The canvas keeps them as pills. */
  const mermaidRels = $derived([...edges, ...hubEdges].map((e) => ({
    source: e.source, target: e.target,
    sourceCol: typeof e.sourceHandle === 'string' && e.sourceHandle.startsWith('src-') ? e.sourceHandle.slice(4) : null,
    many: e.many, optional: e.optional,
  })))
  const mermaidSource = $derived(erdToMermaid(shownTables, mermaidRels, { keysOnly: view.columnMode === 'keys' }))
  /** The code in the editor. Follows the diagram until the user edits it. */
  let mermaidDraft = $state('')
  let mermaidTouched = $state(false)
  let copiedCode = $state(false)
  $effect(() => { if (!mermaidTouched) mermaidDraft = mermaidSource })
  async function copyMermaid() {
    try {
      await navigator.clipboard.writeText(mermaidDraft)
      copiedCode = true
      setTimeout(() => (copiedCode = false), 1600)
    } catch { /* clipboard unavailable */ }
  }
  /** Save the code as a diagram of this connection and go there to edit it. */
  function editInDiagrams() {
    saveDiagram(`Schema diagram · ${activeSchema}`, mermaidDraft, 'Schema diagrams')
    onopendiagrams?.()
  }
  /** A layout is running in the worker; the chip says so. */
  let layingOut = $state(false)
  /** Bumped per layout request so a slow one cannot land over a newer one. */
  let _layoutSeq = 0

  /** @type {Map<string, {x: number, y: number}>} */
  const _posCache = new Map()

  // ── Node height ───────────────────────────────────────────────────────────
  // Must match ErdCanvas' nodeH exactly - the layout reserves the space the
  // canvas then paints, including the "+N more" row in keys-only mode.
  /** @param {TableMeta} t */
  function nodeH(t) {
    return HDR_H + t.columns.length * ROW_H + (t.hiddenCount ? ROW_H : 0) + PAD_B
  }

  /** @param {any} n */
  const hOf = (n) => (n.data ? nodeH(n.data) : HDR_H)

  // ── Layout ────────────────────────────────────────────────────────────────
  /**
   * Focus layout: the opened table sits in the middle, the tables it references
   * (parents) stack to its right and the tables that reference it (children) to
   * its left, each side packed into height-capped sub-columns. Cards never share
   * a band, and every band is separated by a full `colGap`, which is what gives
   * the edge router a clear corridor instead of a path across a table.
   *
   * @param {any[]} ns @param {any[]} es @param {string} focus
   */
  function layoutFocus(ns, es, focus) {
    const focusNode = ns.find(n => n.id === focus)
    if (!focusNode) return null

    /** @type {Set<string>} */
    const parents = new Set()
    /** @type {Set<string>} */
    const children = new Set()
    for (const e of es) {
      if (e.source === focus && e.target !== focus) parents.add(e.target)
      else if (e.target === focus && e.source !== focus) children.add(e.source)
    }
    // A table on both sides is drawn once, on the parent side.
    for (const id of parents) children.delete(id)

    const focusH = hOf(focusNode)
    const focusMid = focusH / 2
    /** @type {Map<string, {x:number,y:number}>} */
    const placed = new Map([[focus, { x: 0, y: 0 }]])

    /** Pack one side into sub-columns growing away from the focus card.
     *  @param {any[]} items @param {1|-1} dir */
    function packSide(items, dir) {
      if (!items.length) return
      const maxH = items.reduce((m, n) => Math.max(m, hOf(n)), HDR_H)
      // Keep a side no taller than a few cards so the whole graph stays legible
      // at fit-zoom; overflow spills into another sub-column instead.
      const targetH = Math.max(focusH, maxH * 4 + gaps.rowGap * 3)
      const rows = Math.max(1, Math.floor(targetH / (maxH + gaps.rowGap)))
      const cols = Math.ceil(items.length / rows)

      // Balance columns by height: tallest card into the shortest column. Slicing
      // in order leaves one full column beside a stub, which is what makes the
      // fan look clumped with holes in it.
      /** @type {any[][]} */
      const colItems = Array.from({ length: cols }, () => [])
      const colH = new Array(cols).fill(0)
      for (const n of [...items].sort((a, b) => hOf(b) - hOf(a))) {
        let k = 0
        for (let j = 1; j < cols; j++) if (colH[j] < colH[k]) k = j
        colItems[k].push(n)
        colH[k] += hOf(n) + gaps.rowGap
      }

      for (let col = 0; col < cols; col++) {
        const slice = colItems[col].sort(byName)
        if (!slice.length) continue
        const total = colH[col] - gaps.rowGap
        const x = dir > 0
          ? NODE_W + gaps.sideGap + col * (NODE_W + gaps.colGap)
          : -gaps.sideGap - (col + 1) * NODE_W - col * gaps.colGap
        let y = focusMid - total / 2
        for (const n of slice) {
          placed.set(n.id, { x, y })
          y += hOf(n) + gaps.rowGap
        }
      }
    }

    const byName = (/** @type {any} */ a, /** @type {any} */ b) => a.id.localeCompare(b.id)
    let right = ns.filter(n => parents.has(n.id)).sort(byName)
    let left = ns.filter(n => children.has(n.id)).sort(byName)
    // Keep the focused table literally in the middle: when every neighbour points
    // the same way, split them across both sides instead of stacking one wing.
    if (!left.length && right.length > 3) {
      const half = Math.ceil(right.length / 2)
      left = right.slice(half); right = right.slice(0, half)
    } else if (!right.length && left.length > 3) {
      const half = Math.ceil(left.length / 2)
      right = left.slice(half); left = left.slice(0, half)
    }
    packSide(right, 1)
    packSide(left, -1)

    // Not linked to the focus table directly (only reachable through a neighbour):
    // park them in a row well below, so they never sit inside a routing corridor.
    const rest = ns.filter(n => n.id !== focus && !placed.has(n.id))
    const restY = focusH + gaps.rowGap * 3
    rest.forEach((n, i) => placed.set(n.id, { x: i * (NODE_W + gaps.colGap), y: restY }))

    let minX = Infinity, minY = Infinity
    for (const p of placed.values()) { minX = Math.min(minX, p.x); minY = Math.min(minY, p.y) }
    return ns.map(n => {
      const p = placed.get(n.id) ?? { x: 0, y: 0 }
      return { ...n, position: { x: Math.round(p.x - minX + 40), y: Math.round(p.y - minY + 40) } }
    })
  }

  /**
   * Connected nodes → Dagre LR.
   * Orphan nodes (no FK edges) → compact grid below connected graph.
   */
  /**
   * The one guarantee every layout has to keep: no card sits on another, and
   * neighbours are far enough apart for the relationship lines between them to
   * have somewhere to run. Dagre reserves that space for the cards it places,
   * but a hand-dragged card, a re-flowed rank or a card that changed size since
   * the layout ran can all break it - so the check happens here, once, on
   * whatever the layout produced.
   * @param {any[]} laid @param {string} [pin] a card that must not move
   */
  function enforceSpacing(laid, pin) {
    if (laid.length < 2) return laid
    const pos = separateCards(
      laid.map((n) => ({ id: n.id, x: n.position.x, y: n.position.y, w: NODE_W, h: hOf(n) })),
      {
        // Wide enough for a routed lane plus its clearance on both sides, and no
        // wider: this runs over grids that are already clear by construction (a
        // folded rank, the orphan block), and a floor above the gaps they were
        // built with would have it pull every one of those rows apart again.
        gapX: Math.max(gaps.colGap, 2 * CLEAR + 12),
        gapY: Math.max(gaps.rowGap, 2 * CLEAR + 8),
        fixed: pin ? new Set([pin]) : undefined,
      },
    )
    return laid.map((n) => ({ ...n, position: pos.get(n.id) ?? n.position }))
  }

  /**
   * Split the graph into connected components, each with its own edges.
   * Union-find rather than a walk: it is one pass over the edges and the result
   * does not depend on which node happens to come first.
   * @param {any[]} ns @param {any[]} es
   * @returns {{ nodes: any[], edges: any[] }[]}
   */
  function componentsOf(ns, es) {
    /** @type {Map<string,string>} */
    const up = new Map(ns.map(n => [n.id, n.id]))
    /** @param {string} a */
    const find = (a) => {
      let r = a
      while (up.get(r) !== r) r = up.get(r) ?? r
      while (up.get(a) !== r) { const nx = up.get(a) ?? r; up.set(a, r); a = nx }
      return r
    }
    for (const e of es) {
      if (!up.has(e.source) || !up.has(e.target)) continue
      const ra = find(e.source), rb = find(e.target)
      if (ra !== rb) up.set(ra, rb)
    }
    /** @type {Map<string, {nodes:any[], edges:any[]}>} */
    const groups = new Map()
    for (const n of ns) {
      const r = find(n.id)
      let g = groups.get(r)
      if (!g) { g = { nodes: [], edges: [] }; groups.set(r, g) }
      g.nodes.push(n)
    }
    for (const e of es) {
      if (!up.has(e.source)) continue
      groups.get(find(e.source))?.edges.push(e)
    }
    return [...groups.values()]
  }

  /**
   * One component through ELK (see erd-elk.js): cards placed and every line
   * routed around them in one pass, normalised to the component's top-left.
   * The lines come back whole, so `hints` carries them port to port.
   * @param {any[]} ns @param {any[]} es
   */
  async function layoutComponentElk(ns, es) {
    const cards = ns.map((n) => {
      /** @type {{ name: string }[]} */
      const cols = n.data?.columns ?? []
      return {
        id: n.id, w: NODE_W, h: hOf(n),
        rowY: (/** @type {string | null} */ col) => {
          const i = col ? cols.findIndex((c) => c.name === col) : -1
          return i >= 0 ? HDR_H + i * ROW_H + ROW_H / 2 : HDR_H / 2
        },
      }
    })
    const links = es
      .filter((e) => e.source !== e.target)
      .map((e) => ({
        id: e.id, source: e.source, target: e.target,
        sourceCol: typeof e.sourceHandle === 'string' && e.sourceHandle.startsWith('src-') ? e.sourceHandle.slice(4) : null,
        targetCol: typeof e.targetHandle === 'string' && e.targetHandle.startsWith('tgt-') ? e.targetHandle.slice(4) : null,
      }))
    const { pos: at, routes } = await layoutWithElk(cards, links, { rankSep: gaps.rankSep, nodeSep: gaps.nodeSep })

    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
    for (const c of cards) {
      const q = at.get(c.id) ?? { x: 0, y: 0 }
      minX = Math.min(minX, q.x); maxX = Math.max(maxX, q.x + c.w)
      minY = Math.min(minY, q.y); maxY = Math.max(maxY, q.y + c.h)
    }
    for (const pts of routes.values()) {
      for (const q of pts) {
        minX = Math.min(minX, q.x); maxX = Math.max(maxX, q.x)
        minY = Math.min(minY, q.y); maxY = Math.max(maxY, q.y)
      }
    }
    /** @type {Map<string, {x:number,y:number}>} */
    const pos = new Map()
    for (const c of cards) {
      const q = at.get(c.id) ?? { x: 0, y: 0 }
      pos.set(c.id, { x: Math.round(q.x - minX), y: Math.round(q.y - minY) })
    }
    /** @type {Map<string, {x:number,y:number}[]>} */
    const hints = new Map()
    for (const [id, pts] of routes) {
      hints.set(id, pts.map((q) => ({ x: Math.round(q.x - minX), y: Math.round(q.y - minY) })))
    }
    return {
      pos, hints, ids: ns.map((n) => n.id).sort(), wrapped: false,
      w: Math.round(maxX - minX), h: Math.round(maxY - minY),
    }
  }

  /**
   * ELK first; Dagre with the folding and the router if ELK is unavailable
   * (its worker could not start, say), so the diagram always draws.
   * @param {any[]} ns @param {any[]} es
   */
  async function layoutComponent(ns, es) {
    try {
      return await layoutComponentElk(ns, es)
    } catch (err) {
      console.warn('[erd] ELK layout failed, falling back to Dagre:', err)
      return layoutComponentDagre(ns, es)
    }
  }

  /**
   * Dagre one component, normalised to its own top-left corner.
   *
   * The bounding box covers the edge corridors as well as the cards, because a
   * corridor that leaves the cards' box would otherwise be packed on top of the
   * next component.
   *
   * A rank taller than the page is folded into sub-columns afterwards (see
   * wrapTallRanks): one rank per column is what turns a hub schema into a 1:10
   * vertical strip. A component that had to fold reports `wrapped`, and gives up
   * its corridors with it - the space they were reserved in has moved, so the
   * caller has to route those lines instead of following them.
   * @param {any[]} ns @param {any[]} es
   */
  function layoutComponentDagre(ns, es) {
    const g = new dagre.graphlib.Graph({ multigraph: true })
    // edgesep is what keeps two foreign keys between the same pair of tables in
    // separate corridors instead of one line drawn twice.
    g.setGraph({
      rankdir: 'LR', ranksep: gaps.rankSep, nodesep: gaps.nodeSep,
      edgesep: Math.max(20, Math.round(gaps.nodeSep / 2)), marginx: 0, marginy: 0,
    })
    g.setDefaultEdgeLabel(() => ({}))
    for (const n of ns) g.setNode(n.id, { width: NODE_W, height: hOf(n) })
    // Named edges: unnamed ones collapse per pair, so two foreign keys between
    // the same tables would share one corridor and one set of markers. Dagre
    // needs a multigraph to accept a name.
    for (const e of es) if (e.source !== e.target) g.setEdge(e.source, e.target, {}, e.id)
    dagre.layout(g)

    // Dagre centres; everything below works in top-left, like the cards do.
    const laid = ns.map((n) => {
      const p = g.node(n.id), h = hOf(n)
      return { id: n.id, x: p.x - NODE_W / 2, y: p.y - h / 2, w: NODE_W, h }
    })
    // rowGap, not nodeSep: a folded diagram gives up the layout's corridors, so
    // its own lines get routed - and a routed line needs a channel wider than
    // twice the clearance it keeps from the cards either side of it.
    const folded = wrapTallRanks(laid, { gapX: gaps.colGap, gapY: gaps.rowGap })
    const at = (/** @type {string} */ id) =>
      folded.pos.get(id) ?? { x: 0, y: 0 }

    /** @type {Map<string, {x:number,y:number}[]>} */
    const raw = new Map()
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
    for (const c of laid) {
      const q = at(c.id)
      minX = Math.min(minX, q.x); maxX = Math.max(maxX, q.x + c.w)
      minY = Math.min(minY, q.y); maxY = Math.max(maxY, q.y + c.h)
    }
    if (!folded.moved) {
      for (const e of es) {
        if (e.source === e.target) continue
        const pts = g.edge({ v: e.source, w: e.target, name: e.id })?.points
        if (!pts || pts.length < 2) continue
        raw.set(e.id, pts)
        for (const q of pts) {
          minX = Math.min(minX, q.x); maxX = Math.max(maxX, q.x)
          minY = Math.min(minY, q.y); maxY = Math.max(maxY, q.y)
        }
      }
    }

    /** @type {Map<string, {x:number,y:number}>} */
    const pos = new Map()
    for (const c of laid) {
      const q = at(c.id)
      pos.set(c.id, { x: Math.round(q.x - minX), y: Math.round(q.y - minY) })
    }
    /** @type {Map<string, {x:number,y:number}[]>} */
    const hints = new Map()
    for (const [id, pts] of raw) {
      hints.set(id, pts.map((/** @type {{x:number,y:number}} */ q) => ({
        x: Math.round(q.x - minX), y: Math.round(q.y - minY),
      })))
    }
    return {
      pos, hints, ids: ns.map(n => n.id).sort(), wrapped: folded.moved,
      w: Math.round(maxX - minX), h: Math.round(maxY - minY),
    }
  }

  /**
   * @param {any[]} ns @param {any[]} es
   * @returns {Promise<{ nodes: any[], hints: Map<string, {x:number,y:number}[]> }>}
   */
  async function layoutNodes(ns, es, hubIds = /** @type {Set<string>} */ (new Set())) {
    /** @type {Map<string, {x:number,y:number}>} */
    const placedConn = new Map()
    /** @type {Map<string, {x:number,y:number}[]>} */
    const hints = new Map()
    if (focusTable && scope === 'related' && ns.length > 1) {
      const laid = layoutFocus(ns, es, focusTable)
      if (laid) return { nodes: enforceSpacing(laid, focusTable), hints }
    }
    const linked = new Set()
    for (const e of es) { linked.add(e.source); linked.add(e.target) }

    const conn = ns.filter(n => linked.has(n.id))
    // A hub with no link of its own left (every line into it is a hub line)
    // sits in a row across the top, where the row pills point. The other
    // unlinked cards keep their grid at the bottom.
    const hubRow = ns.filter(n => !linked.has(n.id) && hubIds.has(n.id))
    const orphans = ns.filter(n => !linked.has(n.id) && !hubIds.has(n.id))
    const laidHubs = hubRow.map((n, i) => ({ ...n, position: { x: 40 + i * (NODE_W + gaps.colGap), y: 40 } }))
    const topY = hubRow.length ? 40 + hubRow.reduce((m, n) => Math.max(m, hOf(n)), 0) + gaps.rowGap * 2 : 40

    let laidConn = []
    let bottomY = topY

    if (conn.length) {
      // One layout per connected component, packed onto shelves.
      //
      // A schema is rarely one graph: it is a few clusters plus a lot of small
      // pairs. Laying them out together puts every cluster in the same rank
      // columns, which is why the diagram came out mostly empty space with the
      // cards small enough to be unreadable at fit-zoom. Graphviz calls this
      // `pack`, and it is the difference between a wall and a page. Smaller
      // graphs also lay out faster than one big one, since Dagre is superlinear.
      const comps = await Promise.all(componentsOf(conn, es).map(cp => layoutComponent(cp.nodes, cp.edges)))
      comps.sort((x, y) => y.h - x.h || y.w - x.w || (x.ids[0] < y.ids[0] ? -1 : 1))
      // All or nothing on corridors. The renderer decides per diagram whether to
      // follow the layout's lines or route its own around the cards, so a single
      // folded component - whose corridors are stale - means every line gets
      // routed. Which is the better half of the trade anyway: a folded diagram is
      // compact enough for the router to be quick.
      const keepHints = !comps.some(cp => cp.wrapped)

      // Shelves of columns, aimed at a landscape page. Tallest component first
      // sets the shelf height, and everything that follows stacks up its column
      // while it fits - a plain row of components would leave the whole strip
      // beside a tall cluster empty, which is most of the page on a schema with
      // one big hub and a tail of small pairs.
      const area = comps.reduce((sum, cp) => sum + (cp.w + gaps.colGap) * (cp.h + gaps.rowGap), 0)
      const targetW = Math.max(comps[0]?.w ?? 0, Math.round(Math.sqrt(area * (16 / 9))))
      const gapX = gaps.colGap * 2
      const gapY = gaps.rowGap * 2
      let shelfY = topY, shelfH = 0
      let colX = 40, colY = topY, colW = 0
      for (const cp of comps) {
        if (colY > shelfY && colY + cp.h > shelfY + shelfH) {
          // Column full: start the next one, or the next shelf if we are at the
          // page's right edge.
          colX += colW + gapX
          colY = shelfY
          colW = 0
          if (colX > 40 && colX + cp.w > targetW) {
            shelfY += shelfH + gapY
            colX = 40
            colY = shelfY
            shelfH = 0
          }
        }
        for (const [id, q] of cp.pos) placedConn.set(id, { x: q.x + colX, y: q.y + colY })
        if (keepHints) {
          for (const [id, pts] of cp.hints) {
            hints.set(id, pts.map(q => ({ x: q.x + colX, y: q.y + colY })))
          }
        }
        colY += cp.h + gapY
        colW = Math.max(colW, cp.w)
        shelfH = Math.max(shelfH, colY - shelfY - gapY)
      }

      laidConn = conn.map(n => {
        const pos = placedConn.get(n.id) ?? { x: 0, y: 0 }
        bottomY = Math.max(bottomY, pos.y + hOf(n))
        return { ...n, position: pos }
      })
    }

    // Orphans in a responsive grid below the connected graph
    const orphanY = laidConn.length ? bottomY + gaps.rowGap * 2 : hubRow.length ? topY : 0
    const maxH = orphans.reduce((m, n) => Math.max(m, hOf(n)), HDR_H)
    const GCOLS = Math.max(3, Math.min(6, Math.ceil(Math.sqrt(orphans.length * 1.8))))
    const laidOrphans = orphans.map((n, i) => ({
      ...n,
      position: {
        x: (i % GCOLS) * (NODE_W + gaps.colGap),
        y: orphanY + Math.floor(i / GCOLS) * (maxH + gaps.rowGap),
      },
    }))

    const combined = [...laidHubs, ...laidConn, ...laidOrphans]
    // Dagre separates the cards it places, and the orphan grid below uses a
    // uniform cell, so both are clear by construction. Nudging them now would
    // move cards off the corridors their edge points describe.
    return { nodes: hints.size ? combined : enforceSpacing(combined), hints }
  }

  // ── Edge list ─────────────────────────────────────────────────────────────
  /** @param {TableMeta[]} all */
  function buildEdgeData(all) {
    /** @type {any[]} */
    const rawEdges = []
    for (const t of all) {
      for (const col of t.columns) {
        if (!col.foreignKey) continue
        const parts = col.foreignKey.split('.')
        const refTable = parts.length >= 3 ? parts[1] : parts[0]
        const refCol = parts.length >= 3 ? parts[2] : parts[1]
        if (!tableMeta.has(refTable)) continue
        // Cardinality straight off the schema: the FK side is "many" unless that
        // column is itself unique or the PK (then it's one-to-one), and a nullable
        // FK means the parent side is optional (zero-or-one).
        const unique = t.pkCols.has(col.name) || t.uniqueCols?.has(col.name) === true
        rawEdges.push({
          id:           `${t.name}__${col.name}__${refTable}`,
          source:       t.name,
          target:       refTable,
          sourceHandle: `src-${col.name}`,
          targetHandle: refCol ? `tgt-${refCol}` : 'tgt',
          type:         'relation',
          many:         !unique,
          optional:     col.isNullable === true,
        })
      }
    }
    return { rawEdges }
  }

  // ── Build graph ───────────────────────────────────────────────────────────
  /** @param {boolean} [forceLayout] */
  async function buildGraph(forceLayout = false) {
    const all = [...tableMeta.values()]
    const { rawEdges } = buildEdgeData(all)
    allRels = rawEdges
    const pick = picked.size ? picked : null
    const visibleIds = visibleTables({
      tables: all.map(t => t.name),
      rels: rawEdges,
      focusTable,
      scope,
      connectedOnly,
      picked: pick,
    })

    // A schema too wide to draw in one go waits for the user to narrow it or to
    // ask for all of it. Bailing here is what keeps the app from spending the
    // next ten seconds laying out a diagram nobody can read.
    if (!pick && !drawAll && visibleIds.size > MAX_AUTO_TABLES) {
      gatedCount = visibleIds.size
      nodes = []
      edges = []
      return
    }
    gatedCount = 0

    const visible = all.filter(t => visibleIds.has(t.name))
    // One line per pair, not one per foreign key column: see mergeParallelEdges.
    // Merging before the layout runs also means Dagre and the router each have
    // fewer edges to work through, which is the difference between a schema
    // finishing its routing budget and running out of it.
    const filteredEdges = mergeParallelEdges(
      visibleRels(rawEdges, visibleIds, { focusTable, scope, picked: pick }),
    )
    // Hub relationships leave the drawing (Settings → Hub links). The layout
    // forms its clusters around the links that remain, which is the structure
    // a line from every card to `tenants` was burying.
    const hubIds = view.hubLinks === 'lines' ? new Set() : hubTables(filteredEdges, visible.length)
    const hubEdgesNow = hubIds.size ? filteredEdges.filter((e) => hubIds.has(e.target) && e.source !== e.target) : []
    const layoutEdges = hubEdgesNow.length ? filteredEdges.filter((e) => !hubIds.has(e.target) || e.source === e.target) : filteredEdges

    const keysOnly = view.columnMode === 'keys'
    const rawNodes = visible.map(t => {
      // Keys-only trims every card to its PK/FK rows: shorter cards mean shorter
      // ranks, which is the cheapest way to make a wide schema readable.
      const columns = keysOnly
        ? t.columns.filter(col => t.pkCols.has(col.name) || !!col.foreignKey)
        : t.columns
      return {
        id:       t.name,
        type:     'tableNode',
        position: _posCache.get(t.name) ?? { x: 0, y: 0 },
        data: {
          ...t,
          columns,
          hiddenCount: t.columns.length - columns.length,
          highlighted: true,
          selected:    t.name === selectedTable,
          isFocus:     !!focusTable && t.name === focusTable,
          onSelect: (/** @type {string} */ name) => { selectedTable = selectedTable === name ? null : name },
          onOpen:   (/** @type {string} */ name) => openTable(name),
        },
      }
    })

    const needsLayout = forceLayout || visible.some(t => !_posCache.has(t.name))
    if (needsLayout) {
      // The layout runs in a worker. The diagram on screen stays as it is until
      // the new one is whole - no cards at the origin, no lines moving as they
      // land - and a result that a later request has overtaken is dropped.
      const seq = ++_layoutSeq
      layingOut = true
      let laid
      try {
        laid = await layoutNodes(rawNodes, layoutEdges, hubIds)
      } finally {
        if (seq === _layoutSeq) layingOut = false
      }
      if (seq !== _layoutSeq) return
      for (const n of laid.nodes) _posCache.set(n.id, n.position)
      nodes = laid.nodes
      routeHints = laid.hints
    } else {
      nodes = rawNodes
    }
    edges = layoutEdges
    hubEdges = hubEdgesNow
    hubs = hubIds
    if (search.trim()) _applySearch(search.trim().toLowerCase())
  }

  // Re-scope + re-layout when the focused table changes (e.g. opened from the sidebar).
  let _prevFocus = untrack(() => focusTable)
  $effect(() => {
    if (focusTable === _prevFocus) return
    _prevFocus = focusTable
    scope = focusTable ? 'related' : 'all'
    _posCache.clear()
    void buildGraph(true).then(() => tick()).then(() => erd?.reveal?.())
  })

  // Scope / card geometry / gutters all change the shape of the graph, so they
  // need a fresh layout rather than a rebuild on cached positions.
  const shapeKey = () =>
    `${scope}|${view.columnMode}|${view.spacing}|${view.hubLinks}|${connectedOnly}|${pickVersion}|${drawAll}`
  let _shapeKey = untrack(shapeKey)
  $effect(() => {
    const key = shapeKey()
    if (key === _shapeKey) return
    _shapeKey = key
    if (tableMeta.size === 0) return
    _posCache.clear()
    void buildGraph(true).then(() => tick()).then(() => erd?.reveal?.())
  })

  // ── Search ────────────────────────────────────────────────────────────────
  /** Highlight rule: the table name or any of its columns matches the query.
   *  @param {string} id @param {string} q */
  function matchesQuery(id, q) {
    if (!q) return true
    const t = tableMeta.get(id)
    return id.toLowerCase().includes(q) || (t?.columns.some(c => c.name.toLowerCase().includes(q)) ?? false)
  }

  /** Enter flies to the first table the query matches. */
  function onSearchKey(/** @type {KeyboardEvent} */ e) {
    if (e.key !== 'Enter') return
    const q = search.trim().toLowerCase()
    const hit = q ? nodes.find(n => matchesQuery(n.id, q)) : null
    if (hit) erd?.focus(hit.id)
  }

  /** @param {string} q */
  function _applySearch(q) {
    nodes = nodes.map(n => ({ ...n, data: { ...n.data, highlighted: matchesQuery(n.id, q) } }))
  }

  function reLayout() {
    _posCache.clear()
    void buildGraph(true).then(() => tick()).then(() => erd?.reveal?.())
  }

  /** Persist a dragged node's new position so rebuilds and exports keep it. */
  function onNodeMoved(/** @type {string} */ id, /** @type {number} */ x, /** @type {number} */ y) {
    // Snap to whole units: the drag divides by the camera zoom, so a card parked
    // at x.37 renders its 1px borders across two device pixels (soft edges) and
    // pushes fractional bounds into every export.
    const p = { x: Math.round(x), y: Math.round(y) }
    _posCache.set(id, p)
    nodes = nodes.map((n) => (n.id === id ? { ...n, position: p } : n))
    // The layout's corridors described where this card used to be.
    if (routeHints.size) {
      const next = new Map(routeHints)
      for (const e of edges) if (e.source === id || e.target === id) next.delete(e.id)
      routeHints = next
    }
  }

  // ── Load ──────────────────────────────────────────────────────────────────
  async function load() {
    loading = true
    error = ''
    tableMeta = new Map()
    nodes = []
    edges = []
    // A filter belongs to the schema it was picked in.
    picked = new Set()
    pickQuery = ''
    drawAll = false
    gatedCount = 0
    _posCache.clear()
    ddlCache = new Map()

    try {
      // One call for the whole schema. This used to fan out one request per
      // table, which meant the diagram couldn't start drawing until N round
      // trips had completed - the dominant cost on any non-trivial schema.
      const schemaCols = await getSchemaColumnStructure(activeSchema)
      autoConnected = schemaCols.length > WARN_MANY

      for (const { table, columns } of schemaCols) {
        const cols = /** @type {Col[]} */ (columns)
        // The catalog's own key flag where the engine gives one; the old
        // name-and-default guess only for rows that predate it. refineKeys()
        // adds the unique keys that make a FK 1:1.
        const pkCols = new Set(
          cols.filter(c =>
            c.isPrimaryKey === true ||
            (c.isPrimaryKey === undefined && (
              c.columnDefault?.includes('nextval') ||
              (c.name === 'id' && !c.isNullable && !c.foreignKey)
            ))
          ).map(c => c.name)
        )
        tableMeta.set(table, /** @type {TableMeta} */ ({ name: table, columns: cols, pkCols }))
      }
      tableMeta = new Map(tableMeta)
      await tick()
      await buildGraph(true)
      _shapeKey = shapeKey()
      void refineKeys()
    } catch (e) {
      error = String(e)
    } finally {
      loading = false
    }
  }

  /** Exact PK columns, plus the single-column unique keys that make a FK 1:1. */
  async function refineKeys() {
    try {
      const idxs = /** @type {{ tableName: string, isPrimary: boolean, isUnique: boolean, columns: string }[]} */ (
        await listIndexes(activeSchema)
      )
      let changed = false
      for (const idx of idxs) {
        const meta = tableMeta.get(idx.tableName)
        if (!meta) continue
        const cols = idx.columns.split(',').map(s => s.trim().replace(/"/g, '')).filter(Boolean)
        if (idx.isPrimary) {
          meta.pkCols = new Set(cols)
          changed = true
        } else if (idx.isUnique && cols.length === 1) {
          meta.uniqueCols = new Set([...(meta.uniqueCols ?? []), cols[0]])
          changed = true
        }
      }
      if (changed) { tableMeta = new Map(tableMeta); void buildGraph() }
    } catch { /* non-critical */ }
  }

  // ── Reactivity ────────────────────────────────────────────────────────────
  $effect(() => { void activeSchema; void load() })


  $effect(() => {
    const q = search.trim().toLowerCase()
    const sel = selectedTable
    untrack(() => {
      if (!nodes.length) return
      nodes = nodes.map(n => ({
        ...n,
        data: { ...n.data, selected: n.id === sel, highlighted: matchesQuery(n.id, q) },
      }))
    })
  })

  const focusNodeId = $derived.by(() => {
    const q = search.trim().toLowerCase()
    if (!q) return null
    const names = [...tableMeta.keys()]
    const exact = names.find(n => n.toLowerCase() === q)
    if (exact) return exact
    const hits = names.filter(n => n.toLowerCase().includes(q))
    return hits.length === 1 ? hits[0] : null
  })

  $effect(() => {
    const id = focusNodeId
    if (id) untrack(() => erd?.focus(id))
  })

  /** The scoped table, only once it actually exists in the loaded schema. */
  const focusId = $derived(focusTable && tableMeta.has(focusTable) ? focusTable : null)

  /** Table whose DDL was just copied - drives the tick on the inspector button. */
  let copiedDdl = $state('')
  /** @type {ReturnType<typeof setTimeout>|null} */
  let _copiedTimer = null
  onDestroy(() => { if (_copiedTimer) clearTimeout(_copiedTimer) })

  /** Copy a table's CREATE statement straight from the inspector. @param {string} name */
  async function copyDdl(name) {
    try {
      const ddl = await getTableDdl(activeSchema, name)
      await navigator.clipboard.writeText(ddl)
      copiedDdl = name
      if (_copiedTimer) clearTimeout(_copiedTimer)
      _copiedTimer = setTimeout(() => { copiedDdl = '' }, 1800)
    } catch (e) {
      toast.error('Could not copy DDL', { description: String(e) })
    }
  }

  /**
   * Open a table from the diagram. Opening the table this diagram belongs to would
   * just re-activate the tab we are already in - which is still showing the
   * diagram, so nothing appears to happen. Ask for a duplicate tab in table view
   * instead, which is what "Open table" means from here.
   * @param {string} name
   */
  function openTable(name) {
    const duplicate = insideTableTab && !!focusTable && name === focusTable
    onopentable?.(activeSchema, name, duplicate ? { duplicate: true, viewMode: 'table' } : undefined)
  }

  /**
   * Apply a table filter. An empty set means no filter; a set naming every table
   * is the same thing, so it collapses to empty rather than pinning the filter on
   * with nothing filtered.
   * @param {Iterable<string>} names
   */
  function setPicked(names) {
    const next = new Set(names)
    // A pick that matches what the page shows unfiltered is no pick at all.
    const resolved = sameSet(next, baseOnPage) ? new Set() : next
    if (sameSet(resolved, picked)) return
    picked = resolved
    pickVersion += 1
    // An explicit choice is the point the gate was waiting for.
    if (picked.size) { drawAll = false; gatedCount = 0 }
  }

  /** The boxes show what is on the page, so a tick puts a table on it and an
   *  untick takes it off. Narrowing 900 tables to three is the search and
   *  "Show only", or a row's Focus - not 897 unticks. @param {string} name */
  function togglePicked(name) {
    if (name === focusTable) return
    const next = new Set(onPage)
    if (next.has(name)) next.delete(name)
    else next.add(name)
    if (!next.size) return
    setPicked(next)
  }

  /** Show only this table and whatever it is FK-linked to. @param {string} name */
  function isolate(name) {
    setPicked(relatedTo([name], allRels))
    pickerOpen = false
  }

  /** Drop one table from the diagram. @param {string} name */
  function hideTable(name) {
    const next = new Set(onPage)
    next.delete(name)
    if (!next.size) return
    setPicked(next)
    if (selectedTable === name) selectedTable = null
  }

  /** Table rows for the filter, with the numbers that help pick: columns + FKs. */
  const pickerRows = $derived.by(() => {
    const q = pickQuery.trim().toLowerCase()
    const rows = []
    for (const t of tableMeta.values()) {
      if (q && !t.name.toLowerCase().includes(q)) continue
      rows.push({
        name: t.name,
        cols: t.columns.length,
        fks: t.columns.filter(c => c.foreignKey).length,
      })
    }
    return rows.sort((a, b) => a.name.localeCompare(b.name))
  })

  /** @param {Set<string>} a @param {Set<string>} b */
  const sameSet = (a, b) => a.size === b.size && [...a].every((n) => b.has(n))
  /** What the scope and "only linked" put on the page with no pick. */
  const baseOnPage = $derived(visibleTables({
    tables: [...tableMeta.keys()], rels: allRels, focusTable, scope, connectedOnly, picked: null,
  }))
  /** What is on the page now - what the filter's boxes show. */
  const onPage = $derived(picked.size
    ? visibleTables({ tables: [...tableMeta.keys()], rels: allRels, focusTable, scope, connectedOnly, picked })
    : baseOnPage)
  const linkedSet = $derived(linkedTables(allRels))
  /** The page plus every table one foreign key away from it. */
  const grown = $derived(relatedTo(onPage, allRels))
  /** The Tree reads the whole schema; every other view draws the filtered page. */
  const filterApplies = $derived(erdView !== 'tree')

  /** Every table the filter's search matches, and nothing else. */
  function showOnlyMatches() {
    if (!pickerRows.length) return
    setPicked(pickerRows.map((r) => r.name))
    pickQuery = ''
  }

  /** @param {Partial<import('$lib/stores/erd-settings.js').ErdSettings>} patch */
  function updateSettings(patch) {
    settings = saveErdSettings(patch)
    if ('connectedOnly' in patch) autoConnected = false
  }

  const selMeta = $derived(selectedTable ? (tableMeta.get(selectedTable) ?? null) : null)
  const selFks = $derived(selMeta?.columns.filter(c => c.foreignKey) ?? [])
  const refBy = $derived(
    selectedTable
      ? [...tableMeta.values()].filter(t =>
          t.name !== selectedTable &&
          t.columns.some(c => {
            const p = c.foreignKey?.split('.')
            return p && p.length >= 3 && p[1] === selectedTable
          })
        )
      : []
  )

  // ── Export / Download ─────────────────────────────────────────────────────
  // The export used to render on a fixed dark sheet with hardcoded ink, so a
  // diagram exported from a light theme came back as someone else's dark
  // diagram. It samples the running theme instead - same tokens the canvas
  // resolves, so the file matches what's on screen.
  //
  // Ink that carries meaning (PK amber, FK blue, focus teal) stays authored here
  // in OKLCH, in a light and a dark variant: those hues aren't in the token set,
  // and picking by polarity keeps them legible on either sheet.
  const INK = {
    dark:  { edge: 'oklch(0.55 0.03 255)', pk: 'oklch(0.80 0.12 82)', fk: 'oklch(0.70 0.11 252)', focus: 'oklch(0.80 0.11 192)' },
    light: { edge: 'oklch(0.62 0.04 255)', pk: 'oklch(0.60 0.13 72)', fk: 'oklch(0.52 0.14 255)', focus: 'oklch(0.55 0.10 196)' },
  }

  // The on-screen legend and the column key/link glyphs use the same ink as the
  // canvas, so they have to follow theme polarity rather than sit on the dark set.
  const ink = $derived($isCurrentThemeDark ? INK.dark : INK.light)

  /**
   * Snapshot the theme the export should be drawn in. Tokens are read as
   * computed values so the SVG carries concrete colours rather than `var(--…)`
   * references, which resolve to nothing once the file leaves the app.
   */
  function exportPalette() {
    const css = getComputedStyle(document.documentElement)
    const tok = (/** @type {string} */ n, /** @type {string} */ fallback) =>
      css.getPropertyValue(n).trim() || fallback
    const dark = document.documentElement.classList.contains('dark')
    const ink = dark ? INK.dark : INK.light
    return {
      ...ink,
      sheet:  tok('--background', dark ? '#0d0d10' : '#ffffff'),
      card:   tok('--card', dark ? '#141418' : '#ffffff'),
      header: tok('--muted', dark ? '#1d1d26' : '#f4f4f5'),
      border: tok('--border', dark ? '#252535' : '#e4e4e7'),
      text:   tok('--foreground', dark ? '#e2e2ea' : '#18181b'),
      muted:  tok('--muted-foreground', dark ? '#6b6b80' : '#71717a'),
      faint:  tok('--muted-foreground', dark ? '#333345' : '#a1a1aa'),
      accent: tok('--primary', 'oklch(0.62 0.19 275)'),
      dot:    dark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.06)',
      shadow: dark ? 'rgba(0,0,0,0.45)' : 'rgba(0,0,0,0.10)',
      rule:   dark ? 'rgba(255,255,255,0.05)' : 'rgba(0,0,0,0.05)',
    }
  }

  /**
   * Crow's foot marker as SVG path data, oriented along the line.
   * @param {{x:number,y:number}} at @param {{x:number,y:number}} toward
   * @param {'fork'|'bar'} kind @param {boolean} ring
   */
  function svgMarker(at, toward, kind, ring) {
    const dx = toward.x - at.x, dy = toward.y - at.y
    const m = Math.hypot(dx, dy) || 1
    const ux = dx / m, uy = dy / m
    const px = -uy, py = ux
    const L = 11, W = 5.5
    if (kind === 'fork') {
      const bx = at.x + ux * L, by = at.y + uy * L
      return `M${bx} ${by}L${at.x + px * W} ${at.y + py * W}` +
             `M${bx} ${by}L${at.x - px * W} ${at.y - py * W}` +
             `M${bx} ${by}L${at.x} ${at.y}`
    }
    const bx = at.x + ux * (L * 0.55), by = at.y + uy * (L * 0.55)
    let d = `M${bx + px * W} ${by + py * W}L${bx - px * W} ${by - py * W}`
    if (ring) {
      const cx = at.x + ux * (L * 1.5), cy = at.y + uy * (L * 1.5)
      d += `M${cx - 3} ${cy}a3 3 0 1 0 6 0a3 3 0 1 0 -6 0`
    }
    return d
  }

  /** @param {string} s */
  function xesc(s) {
    return String(s ?? '')
      .replace(/&/g, '&amp;').replace(/</g, '&lt;')
      .replace(/>/g, '&gt;').replace(/"/g, '&quot;')
  }

  /** @returns {{ xml: string, width: number, height: number } | null} */
  function generateSvg() {
    const vis = nodes.filter(n => n.data?.highlighted !== false)
    if (!vis.length) return null

    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity
    for (const n of vis) {
      const h = nodeH(n.data)
      x0 = Math.min(x0, n.position.x)
      y0 = Math.min(y0, n.position.y)
      x1 = Math.max(x1, n.position.x + NODE_W)
      y1 = Math.max(y1, n.position.y + h)
    }
    const P = 56
    // Dragged nodes carry sub-pixel positions (the drag divides by the camera
    // zoom), so floor/ceil the bounds: whole numbers keep the raster grid-aligned
    // and never clip a card by a fraction of a pixel.
    x0 = Math.floor(x0); y0 = Math.floor(y0)
    x1 = Math.ceil(x1); y1 = Math.ceil(y1)
    const W = x1 - x0 + P * 2
    const H = y1 - y0 + P * 2
    const dx = -x0 + P
    const dy = -y0 + P
    const FONT = 'ui-monospace,Cascadia Code,Menlo,Consolas,monospace'
    const visIds = new Set(vis.map(n => n.id))
    const c = exportPalette()

    const o = []
    o.push(`<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">`)
    o.push(`<rect width="${W}" height="${H}" fill="${c.sheet}"/>`)
    o.push(`<defs><pattern id="dp" width="22" height="22" patternUnits="userSpaceOnUse">`)
    o.push(`<circle cx="1.2" cy="1.2" r="0.8" fill="${c.dot}"/></pattern></defs>`)
    o.push(`<rect width="${W}" height="${H}" fill="url(#dp)"/>`)

    // Edges first (behind nodes), routed by the same engine the canvas uses so an
    // exported diagram matches what's on screen - no lines across tables.
    const boxes = vis.map(n => ({
      id: n.id, x: n.position.x + dx, y: n.position.y + dy, w: NODE_W, h: nodeH(n.data),
    }))
    /** @type {import('$lib/erd-routing.js').Link[]} */
    const links = []
    for (const e of edges) {
      if (!visIds.has(e.source) || !visIds.has(e.target)) continue
      const sn = vis.find(n => n.id === e.source)
      const tn = vis.find(n => n.id === e.target)
      if (!sn || !tn) continue
      const scols = sn.data?.columns ?? []
      const tcols = tn.data?.columns ?? []
      const ci = scols.findIndex(c => `src-${c.name}` === e.sourceHandle)
      const ti = tcols.findIndex(c => `tgt-${c.name}` === e.targetHandle)
      const rowY = (/** @type {number} */ y, /** @type {number} */ i) =>
        y + HDR_H + (i >= 0 ? i * ROW_H + ROW_H / 2 : -HDR_H / 2)
      // Same rule as the canvas: a side port where the cards are separated
      // horizontally, the near horizontal edge where they overlap.
      const sx0 = sn.position.x + dx, tx0 = tn.position.x + dx
      const sy0 = sn.position.y + dy, ty0 = tn.position.y + dy
      const toRight = tx0 - (sx0 + NODE_W)
      const toLeft = sx0 - (tx0 + NODE_W)
      if (toRight >= 0 || toLeft >= 0) {
        const leftToRight = toRight >= toLeft
        links.push({
          id: e.id, source: e.source, target: e.target,
          sx: sx0 + (leftToRight ? NODE_W : 0), sy: rowY(sy0, ci),
          tx: tx0 + (leftToRight ? 0 : NODE_W), ty: rowY(ty0, ti),
          sdx: leftToRight ? 1 : -1, sdy: 0,
          tdx: leftToRight ? -1 : 1, tdy: 0,
        })
      } else {
        const down = ty0 >= sy0
        links.push({
          id: e.id, source: e.source, target: e.target,
          sx: sx0 + NODE_W / 2, sy: down ? sy0 + nodeH(sn.data) : sy0,
          tx: tx0 + NODE_W / 2, ty: down ? ty0 : ty0 + nodeH(tn.data),
          sdx: 0, sdy: down ? 1 : -1,
          tdx: 0, tdy: down ? -1 : 1,
        })
      }
    }
    // Same lines the canvas draws: the layout's own corridors where it routed
    // them, an A* route where it didn't and the graph is small enough to afford
    // one. An export that disagrees with the screen is worse than a plain one.
    const hinted = routeHints.size > 0
    const routed = hinted || links.length > MAX_ROUTED_NODES
      ? new Map()
      : routeEdges(boxes, links)
    // One path element for every line, so a crossing never paints twice as bright.
    const lineD = []
    const markD = []
    /** @type {{x:number,y:number,n:number}[]} */
    const countD = []
    for (const l of links) {
      const hint = routeHints.get(l.id)
      const shifted = hint?.map(p => ({ x: p.x + dx, y: p.y + dy }))
      const pts = routed.get(l.id) ?? corridorPathOrtho(l, shifted)
      lineD.push(routeToSvgPath(pts))
      const a0 = pts[0], a1 = pts[1] ?? a0
      const z0 = pts[pts.length - 1], z1 = pts[pts.length - 2] ?? z0
      const edge = edges.find(e => e.id === l.id)
      markD.push(svgMarker(a0, a1, edge?.many === false ? 'bar' : 'fork', false))
      markD.push(svgMarker(z0, z1, 'bar', edge?.optional === true))
      // Same chip the canvas draws, so a merged pair reads the same on paper.
      const merged = edge?.mergedCount ?? 1
      if (merged > 1) {
        const m = Math.hypot(a1.x - a0.x, a1.y - a0.y) || 1
        const cx = a0.x + ((a1.x - a0.x) / m) * 30
        const cy = a0.y + ((a1.y - a0.y) / m) * 30 - (Math.abs(a1.y - a0.y) < 0.5 ? 10 : 0)
        countD.push({ x: cx, y: cy, n: merged })
      }
    }
    o.push(`<path d="${lineD.join(' ')}" fill="none" stroke="${c.edge}" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>`)
    o.push(`<path d="${markD.join(' ')}" fill="none" stroke="${c.edge}" stroke-width="1.2" stroke-linecap="round"/>`)
    for (const chip of countD) {
      const label = `\u00d7${chip.n}`
      const w = 9 + label.length * 5
      o.push(`<rect x="${chip.x - w / 2}" y="${chip.y - 7}" width="${w}" height="14" rx="4" fill="${c.card}" stroke="${c.border}" stroke-width="1"/>`)
      o.push(`<text x="${chip.x}" y="${chip.y + 3.5}" font-size="9" font-weight="600" font-family="${FONT}" fill="${c.muted}" text-anchor="middle">${label}</text>`)
    }

    for (const n of vis) {
      const cols = n.data?.columns ?? []
      const pkCols = n.data?.pkCols ?? new Set()
      const h = nodeH(n.data)
      const nx = n.position.x + dx
      const ny = n.position.y + dy
      const isSel = n.data?.selected
      const isFocus = n.data?.isFocus

      o.push(`<rect x="${nx+3}" y="${ny+4}" width="${NODE_W}" height="${h}" rx="8" fill="${c.shadow}"/>`)
      // focus halo - marks the table the diagram was scoped to
      if (isFocus) {
        o.push(`<rect x="${nx-6}" y="${ny-6}" width="${NODE_W+12}" height="${h+12}" rx="13" fill="${c.focus}" fill-opacity="0.10" stroke="${c.focus}" stroke-opacity="0.4" stroke-width="1.5"/>`)
      }
      o.push(`<rect x="${nx}" y="${ny}" width="${NODE_W}" height="${h}" rx="8" fill="${c.card}" stroke="${isFocus || isSel ? c.accent : c.border}" stroke-width="${isFocus ? 2 : isSel ? 1.5 : 1}"/>`)
      o.push(`<clipPath id="hc${n.id.replace(/\W/g,'_')}"><rect x="${nx}" y="${ny}" width="${NODE_W}" height="${HDR_H + 8}" rx="8"/></clipPath>`)
      o.push(`<rect x="${nx}" y="${ny}" width="${NODE_W}" height="${HDR_H + 8}" fill="${c.header}" clip-path="url(#hc${n.id.replace(/\W/g,'_')})"/>`)
      o.push(`<line x1="${nx}" y1="${ny+HDR_H}" x2="${nx+NODE_W}" y2="${ny+HDR_H}" stroke="${isFocus ? c.accent : c.border}" stroke-opacity="${isFocus ? 0.55 : 1}" stroke-width="1"/>`)
      o.push(`<text x="${nx+12}" y="${ny+HDR_H/2+4}" font-size="11" font-weight="600" font-family="${FONT}" fill="${c.text}">${xesc(n.data.name)}</text>`)
      if (isFocus) {
        o.push(`<rect x="${nx+NODE_W-62}" y="${ny+HDR_H/2-7}" width="50" height="14" rx="4" fill="${c.accent}"/>`)
        o.push(`<text x="${nx+NODE_W-37}" y="${ny+HDR_H/2+4}" font-size="8" font-weight="700" font-family="${FONT}" fill="${c.card}" text-anchor="middle">current</text>`)
      }

      for (let i = 0; i < cols.length; i++) {
        const col = cols[i]
        const isPk = pkCols.has(col.name)
        const isFk = !!col.foreignKey
        const cy = ny + HDR_H + i * ROW_H
        if (i > 0) o.push(`<line x1="${nx}" y1="${cy}" x2="${nx+NODE_W}" y2="${cy}" stroke="${c.rule}" stroke-width="0.5"/>`)
        const nc = isPk ? c.pk : isFk ? c.fk : c.muted
        const ts = view.showTypes ? String(col.dataType ?? '').slice(0, 11) : ''
        const tyY = cy + ROW_H / 2 + 4
        o.push(`<text x="${nx+12}" y="${tyY}" font-size="10" font-family="${FONT}" fill="${nc}">${xesc(col.name)}</text>`)
        if (isPk || isFk) {
          const b = isPk ? 'pk' : 'fk'
          const tc = isPk ? c.pk : c.fk
          o.push(`<rect x="${nx+NODE_W-42}" y="${cy+5}" width="18" height="13" rx="2" fill="${tc}" fill-opacity="0.15"/>`)
          o.push(`<text x="${nx+NODE_W-33}" y="${cy+15}" font-size="7.5" font-weight="700" font-family="${FONT}" fill="${tc}" text-anchor="middle">${b}</text>`)
          if (ts) o.push(`<text x="${nx+NODE_W-50}" y="${tyY}" font-size="9" font-family="${FONT}" fill="${c.faint}" fill-opacity="0.7" text-anchor="end">${xesc(ts)}</text>`)
        } else if (ts) {
          o.push(`<text x="${nx+NODE_W-12}" y="${tyY}" font-size="9" font-family="${FONT}" fill="${c.faint}" fill-opacity="0.7" text-anchor="end">${xesc(ts)}</text>`)
        }
      }

      if (n.data?.hiddenCount) {
        const cy = ny + HDR_H + cols.length * ROW_H
        o.push(`<line x1="${nx}" y1="${cy}" x2="${nx+NODE_W}" y2="${cy}" stroke="${c.rule}" stroke-width="0.5"/>`)
        o.push(`<text x="${nx+12}" y="${cy+ROW_H/2+4}" font-size="9" font-family="${FONT}" fill="${c.faint}">+${n.data.hiddenCount} more</text>`)
      }
    }
    o.push('</svg>')
    return { xml: o.join(''), width: W, height: H }
  }

  /**
   * Save an export through the native dialog and report where it landed.
   * @param {Blob | string} payload
   * @param {string} name
   * @param {{ name: string, extensions: string[] }} filter
   * @param {string} label what was exported, for the toast title
   */
  async function saveExport(payload, name, filter, label) {
    const path = await saveExportAs(payload, name, filter)
    if (!path) return  // dialog cancelled
    toast.success(`Exported ${label}`, { description: `Saved to ${path}` })
  }

  async function exportMermaid() {
    const visIds = new Set(nodes.map(n => n.id))
    const lines = ['# Schema diagram', '', '```mermaid', 'erDiagram']
    for (const e of edges) {
      if (!visIds.has(e.source) || !visIds.has(e.target)) continue
      const sm = tableMeta.get(e.source)
      const fkCol = sm?.columns.find(c => `src-${c.name}` === e.sourceHandle)
      lines.push(`    ${e.source} }|--|| ${e.target} : "${fkCol?.name ?? ''}"`)
    }
    for (const n of nodes) {
      const t = tableMeta.get(n.id)
      if (!t) continue
      lines.push(`    ${t.name} {`)
      for (const col of t.columns) {
        const typ = (col.dataType || 'text').replace(/[^a-zA-Z0-9_]/g, '_').replace(/_+/g, '_').replace(/^_|_$/, '')
        const flags = [t.pkCols.has(col.name) && 'PK', !!col.foreignKey && 'FK'].filter(Boolean)
        lines.push(`        ${typ || 'text'} ${col.name}${flags.length ? ' ' + flags.join(',') : ''}`)
      }
      lines.push('    }')
    }
    lines.push('```')
    try {
      await saveExport(
        lines.join('\n'), `erd-${activeSchema}.md`,
        { name: 'Markdown', extensions: ['md'] }, 'Mermaid markdown',
      )
    } catch (e) {
      toast.error('Export failed', { description: String(e) })
    }
  }

  async function exportSVG() {
    try {
      const svg = generateSvg()?.xml
      if (!svg) return
      await saveExport(
        svg, `erd-${activeSchema}.svg`,
        { name: 'SVG image', extensions: ['svg'] }, 'diagram as SVG',
      )
    } catch (e) {
      toast.error('Export failed', { description: String(e) })
    }
  }

  // Public API: the diagram's export actions are driven from the tab bar's
  // Export menu (StudioShell), so the canvas toolbar stays free of chrome.
  export function exportDiagram(/** @type {'png'|'copy-png'|'svg'|'mermaid'} */ kind) {
    if (kind === 'png') return void renderPng('download')
    if (kind === 'copy-png') return void renderPng('clipboard')
    if (kind === 'svg') return void exportSVG()
    return void exportMermaid()
  }
  /** @param {'download'|'clipboard'} sink */
  async function renderPng(sink) {
    exporting = true
    try {
      const svg = generateSvg()
      if (!svg) return
      // Dimensions come from the generator itself. They used to be scraped back
      // out of the markup with a regex that only matched whole numbers, so one
      // dragged (sub-pixel) node silently fell back to a 1200x800 box - which
      // cropped the diagram to its top-left corner and blew that fragment up to
      // fill the raster.
      const { xml, width: W, height: H } = svg
      // 3x keeps 8px badge text legible when the diagram is viewed at 100%.
      // MAX_PX is the safety rail: WebKit refuses to allocate a canvas past
      // roughly 16k on a side, and a wide schema hits that well before 3x.
      const MAX_PX = 12000
      const scale = Math.max(1, Math.min(3, MAX_PX / Math.max(W, H)))
      // The sheet rect inside the SVG already covers this; the canvas fill is
      // the backstop so the PNG is never transparent where a viewer would
      // composite it against its own (possibly white) page.
      const blob = await svgStringToPngBlob(xml, { width: W, height: H, scale, background: exportPalette().sheet })
      if (sink === 'download') {
        await saveExport(
          blob, `erd-${activeSchema}.png`,
          { name: 'PNG image', extensions: ['png'] }, 'diagram as PNG',
        )
        return
      }
      await copyPngToClipboard(blob)
      toast.success('Diagram copied as PNG')
    } catch (e) {
      toast.error(sink === 'clipboard' ? 'Copy failed' : 'Export failed', { description: String(e) })
    } finally {
      exporting = false
    }
  }

</script>

<svelte:window onkeydown={(e) => {
  if (e.key === 'Escape' && fullscreen) { fullscreen = false; return }
  const mod = (e.ctrlKey || e.metaKey) && !e.altKey
  // Only the page on screen answers: a hidden tab's page has no width.
  if (mod && e.shiftKey && e.key === 'Enter' && rootEl?.offsetWidth) {
    e.preventDefault(); fullscreen = !fullscreen; return
  }
  // Ctrl/⌘F normally arrives through the shell (focusSearch); this catches
  // it when nothing upstream took it, and only for the page on screen.
  if (e.defaultPrevented || !rootEl?.offsetWidth) return
  if (mod && !e.shiftKey && (e.key === 'f' || e.key === 'F')) {
    if (e.target instanceof Element && e.target.closest('.cm-editor')) return
    e.preventDefault(); void focusSearch()
  }
}} />

{#snippet searchBox(
  /** @type {string} */ value,
  /** @type {(v: string) => void} */ set,
  /** @type {{ label: string, placeholder: string, wide?: boolean, onkeydown?: (e: KeyboardEvent) => void, oninput?: () => void }} */ o,
)}
  <!-- Wide enough for its placeholder in a monospace UI font, and the right
       padding only makes room for the clear button when there is something
       to clear, so an empty box never cuts its own hint short. -->
  <div class={cn('relative flex min-w-0 shrink items-center', o.wide ? 'w-72' : 'w-52')}>
    <Search class="pointer-events-none absolute left-2.5 size-3.5 text-muted-foreground" />
    <input
      type="text"
      bind:this={searchEl}
      {value}
      oninput={(e) => { set(e.currentTarget.value); o.oninput?.() }}
      onkeydown={(e) => {
        if (e.key === 'Escape' && value) { set(''); e.stopPropagation(); return }
        o.onkeydown?.(e)
      }}
      placeholder={o.placeholder}
      aria-label={o.label}
      title="{o.label} ({keycaps('Mod+F').join(IS_MAC ? '' : '+')})"
      spellcheck="false"
      autocomplete="off"
      class={cn('field-surface h-7 w-full min-w-0 bg-input/30 pl-8 text-ui-sm text-foreground outline-none placeholder:text-muted-foreground', value ? 'pr-7' : 'pr-2.5')}
    />
    {#if value}
      <button
        type="button"
        onclick={() => { set(''); searchEl?.focus() }}
        aria-label="Clear"
        title="Clear (Esc)"
        class="absolute right-1 inline-flex size-5 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      ><X class="size-3" /></button>
    {/if}
  </div>
{/snippet}

{#snippet viewIcon(/** @type {string} */ id, /** @type {string} */ cls)}
  {#if id === 'canvas'}<LayoutDashboard class={cls} />
  {:else if id === 'mermaid'}<GitBranch class={cls} />
  {:else if id === 'hierarchy'}<Workflow class={cls} />
  {:else if id === 'tree'}<ListTree class={cls} />
  {:else if id === 'dictionary'}<BookOpen class={cls} />
  {:else}<FileCode class={cls} />{/if}
{/snippet}

{#snippet segmented(/** @type {{value:string,label:string,hint?:string}[]} */ items, /** @type {string} */ current, /** @type {(v:string)=>void} */ pick)}
  <div class="field-surface inline-flex h-6 shrink-0 items-center bg-muted/25 p-0.5">
    {#each items as it (it.value)}
      <button
        type="button"
        title={it.hint}
        aria-pressed={current === it.value}
        onclick={() => pick(it.value)}
        class={cn(
          'inline-flex h-5 min-w-7 items-center justify-center rounded-[5px] px-1.5 text-ui-2xs whitespace-nowrap transition-colors',
          current === it.value
            ? 'bg-background text-foreground ring-1 ring-inset ring-border/70'
            : 'text-muted-foreground hover:text-foreground',
        )}
      >{it.label}</button>
    {/each}
  </div>
{/snippet}

{#snippet settingRow(/** @type {string} */ label, /** @type {string} */ hint, /** @type {import('svelte').Snippet} */ control)}
  <div class="flex h-8 items-center justify-between gap-3 px-2" title={hint}>
    <span class="truncate text-ui-xs text-foreground">{label}</span>
    {@render control()}
  </div>
{/snippet}

{#snippet switchRow(/** @type {string} */ label, /** @type {string} */ hint, /** @type {boolean} */ checked, /** @type {() => void} */ ontoggle)}
  <!-- The whole row is the switch: a 28px toggle is a small thing to aim at. -->
  <button
    type="button" role="switch" aria-checked={checked} title={hint}
    onclick={ontoggle}
    class="flex h-8 w-full items-center justify-between gap-3 rounded-sm px-2 text-left hover:bg-accent"
  >
    <span class="truncate text-ui-xs text-foreground">{label}</span>
    <span class={cn('inline-flex h-4 w-7 shrink-0 items-center rounded-full px-0.5 transition-colors duration-150', checked ? 'bg-primary' : 'bg-input')}>
      <span class={cn('block size-3 rounded-full transition-transform duration-150', checked ? 'translate-x-3 bg-background' : 'translate-x-0 bg-foreground/80')}></span>
    </span>
  </button>
{/snippet}

<div
  bind:this={rootEl}
  class={cn('flex min-h-0 flex-1 flex-col overflow-hidden', fullscreen && 'fixed inset-x-0 bottom-0 z-40 bg-background')}
  style={fullscreen ? 'top: var(--app-titlebar-h, 38px)' : ''}
>
  <!-- ── Toolbar ──────────────────────────────────────────────────────────── -->
  <div class="studio-chrome flex h-9 shrink-0 items-center gap-2 border-b border-border bg-panel px-3" data-studio-chrome>
    <Network class="size-3.5 shrink-0 text-muted-foreground" aria-label="Schema diagram" />

    {#if schemas.length > 1}
      <Popover bind:open={schemaOpen}>
        <PopoverTrigger
        class= "field-surface ml-1 flex h-7 shrink-0 items-center gap-1.5 bg-input/30 px-2.5 text-ui-sm font-medium text-foreground transition-colors hover:bg-accent focus:outline-none data-[state=open]:bg-accent"
        >
          {activeSchema}
          <ChevronDown class="size-3 shrink-0 text-muted-foreground" />
        </PopoverTrigger>
        <PopoverContent class="max-h-72 w-44 overflow-y-auto p-1" align="start">
          {#each schemas as s (s)}
            <button
              type="button"
              class="flex w-full rounded-md px-2 py-1.5 text-ui-sm transition-colors hover:bg-accent hover:text-foreground {s === activeSchema ? 'font-medium text-foreground' : 'text-muted-foreground'}"
              onclick={() => { activeSchema = s; schemaOpen = false }}
            >{s}</button>
          {/each}
        </PopoverContent>
      </Popover>
    {/if}

    <!-- The view's own search, one box for every view: Ctrl/⌘F lands here
         (the shell routes it, see focusSearch). Mermaid and DDL search their
         code, so they get a button that opens the editor's find instead. -->
    {#if erdView === 'canvas'}
      {@render searchBox(search, (v) => (search = v), { label: 'Search tables', placeholder: 'Search tables', onkeydown: onSearchKey })}
    {:else if erdView === 'dictionary'}
      {@render searchBox(dictQuery, (v) => (dictQuery = v), { label: 'Search the dictionary', placeholder: 'Search columns and types', wide: true })}
    {:else if erdView === 'hierarchy'}
      <!-- Enter steps through the matches, centring each. -->
      {@render searchBox(hierQuery, (v) => (hierQuery = v), {
        label: 'Find a table',
        placeholder: 'Find a table',
        onkeydown: (e) => { if (e.key === 'Enter') { e.preventDefault(); hierPage?.next() } },
      })}
    {:else if erdView === 'tree'}
      <button
        type="button"
        title={treeListOpen ? 'Hide the table list' : 'Show the table list'}
        aria-label={treeListOpen ? 'Hide the table list' : 'Show the table list'}
        aria-pressed={treeListOpen}
        onclick={() => (treeListOpen = !treeListOpen)}
        class={cn('inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground', treeListOpen && 'bg-accent text-foreground')}
      >{#if treeListOpen}<PanelLeftClose class="size-3.5" />{:else}<PanelLeftOpen class="size-3.5" />{/if}</button>
      <!-- Typing opens the list it filters. -->
      {@render searchBox(treeSearch, (v) => (treeSearch = v), { label: 'Filter tables', placeholder: 'Filter tables', oninput: () => (treeListOpen = true) })}
    {:else}
      <button
        type="button"
        title="Find in the {erdView === 'ddl' ? 'DDL' : 'Mermaid code'} ({keycaps('Mod+F').join(IS_MAC ? '' : '+')})"
        aria-label="Find in the code"
        onclick={() => void focusSearch()}
        class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      ><Search class="size-3.5" /></button>
    {/if}

    {#if filterApplies}
    <!-- Table filter: which tables are on the page. The boxes say what is on it
         now, whatever put it there - the scope, "only linked", or a pick. -->
    <Popover bind:open={pickerOpen}>
      <PopoverTrigger
        title="Choose the tables on the page"
        class={cn(
          'field-surface inline-flex h-7 shrink-0 items-center gap-1.5 px-2 text-ui-xs transition-colors focus:outline-none',
          onPage.size < tableMeta.size
            ? 'bg-accent text-foreground'
            : 'bg-input/30 text-muted-foreground hover:bg-accent hover:text-foreground data-[state=open]:bg-accent',
        )}
      >
        <ListFilter class="size-3.5 shrink-0" />
        {#if onPage.size < tableMeta.size}
          <span class="font-mono text-ui-2xs tabular-nums">{onPage.size}/{tableMeta.size}</span>
        {/if}
      </PopoverTrigger>
      <PopoverContent class="w-80 p-0" align="start">
        <div class="flex flex-col gap-2 border-b border-border/50 p-2">
          <div class="relative flex items-center">
            <Search class="pointer-events-none absolute left-2.5 size-3.5 text-muted-foreground" />
            <!-- svelte-ignore a11y_autofocus -->
            <input
              type="text"
              autofocus
              bind:value={pickQuery}
              placeholder="Find a table…"
              aria-label="Find a table"
              onkeydown={(e) => {
                if (e.key === 'Enter' && pickQuery.trim()) { showOnlyMatches(); e.preventDefault() }
              }}
              class= "field-surface h-8 w-full bg-input/30 pl-8 pr-2 text-ui-sm outline-none placeholder:text-muted-foreground"
            />
          </div>
          <div class="flex items-center gap-1">
            {#each [
              { id: 'all', label: 'All', title: 'Every table in the schema', active: onPage.size === tableMeta.size, disabled: false, run: () => setPicked(tableMeta.keys()) },
              { id: 'linked', label: 'Linked', title: 'Only the tables with a foreign key, in or out', active: onPage.size < tableMeta.size && sameSet(onPage, linkedSet), disabled: !linkedSet.size, run: () => setPicked(linkedSet) },
              { id: 'grow', label: 'Neighbours', title: 'Add every table one foreign key away from the ones on the page', active: false, disabled: grown.size === onPage.size, run: () => setPicked(grown) },
            ] as p (p.id)}
              <button
                type="button"
                title={p.title}
                aria-pressed={p.id === 'grow' ? undefined : p.active}
                disabled={p.disabled}
                onclick={p.run}
                class={cn(
                  'inline-flex h-6 shrink-0 items-center gap-1 rounded-md px-2 text-ui-2xs disabled:pointer-events-none disabled:opacity-40',
                  p.active ? 'bg-accent font-medium text-foreground' : 'text-muted-foreground hover:bg-accent hover:text-foreground',
                )}
              >
                {#if p.id === 'all'}<LayoutGrid class="size-3 shrink-0" />
                {:else if p.id === 'linked'}<Link class="size-3 shrink-0" />
                {:else}<Plus class="size-3 shrink-0" />{/if}
                {p.label}
              </button>
            {/each}
          </div>
        </div>

        <div class="max-h-72 overflow-y-auto p-1">
          {#each pickerRows.slice(0, PICKER_ROWS) as row (row.name)}
            {@const on = onPage.has(row.name)}
            {@const locked = row.name === focusTable}
            <div class="group/pick relative flex h-7 items-center rounded-sm hover:bg-accent">
              <button
                type="button"
                role="checkbox"
                aria-checked={on}
                disabled={locked}
                title={locked ? 'The table this diagram was opened for is always on it' : undefined}
                onclick={() => togglePicked(row.name)}
                class="flex h-full min-w-0 flex-1 items-center gap-2 px-2 text-left disabled:cursor-default"
              >
                <span class={cn(
                  'flex size-4 shrink-0 items-center justify-center rounded-[4px] border',
                  on ? 'border-primary bg-primary text-primary-foreground' : 'border-input dark:bg-input/30',
                  locked && 'opacity-60',
                )}>
                  {#if on}<Check class="size-3" />{/if}
                </span>
                <span class={cn('min-w-0 flex-1 truncate font-mono text-ui-xs', on ? 'text-foreground' : 'text-muted-foreground')}>{row.name}</span>
                {#if locked}
                  <span class="shrink-0 pr-1 text-ui-3xs text-muted-foreground">this table</span>
                {:else if row.fks}
                  <span class="shrink-0 pr-1 font-mono text-ui-3xs tabular-nums text-muted-foreground group-hover/pick:opacity-0 group-focus-within/pick:opacity-0" title="{row.fks} foreign keys">{row.fks} fk</span>
                {/if}
              </button>
              {#if !locked}
                <!-- Sits where the fk count is, shown under the pointer or the
                     keyboard: a column of twenty "Focus"es reads as a word. -->
                <button
                  type="button"
                  title="Show only {row.name} and the tables it links to"
                  onclick={() => isolate(row.name)}
                  class="absolute right-1 inline-flex h-5 items-center rounded-md px-1.5 text-ui-3xs font-medium text-muted-foreground opacity-0 hover:bg-background hover:text-foreground focus-visible:opacity-100 group-hover/pick:opacity-100"
                >Focus</button>
              {/if}
            </div>
          {/each}
          {#if pickerRows.length > PICKER_ROWS}
            <p class="px-2 py-1.5 text-ui-3xs text-muted-foreground">
              {pickerRows.length - PICKER_ROWS} more - narrow the search to reach them.
            </p>
          {:else if !pickerRows.length}
            <p class="px-2 py-1.5 text-ui-2xs text-muted-foreground">No table matches “{pickQuery}”.</p>
          {/if}
        </div>

        <div class="flex h-9 items-center gap-2 border-t border-border/50 px-2">
          {#if pickQuery.trim() && pickerRows.length}
            <button
              type="button"
              onclick={showOnlyMatches}
              class="flex h-7 min-w-0 flex-1 items-center gap-2 rounded-md px-2 text-left text-ui-xs text-foreground hover:bg-accent"
            >
              <span class="min-w-0 flex-1 truncate">Show only {pickerRows.length === 1 ? 'this table' : `these ${pickerRows.length}`}</span>
              <Kbd combo="Enter" size="sm" />
            </button>
          {:else}
            <span class="min-w-0 flex-1 truncate px-1 text-ui-2xs tabular-nums text-muted-foreground">
              {onPage.size} of {tableMeta.size} tables on the page
            </span>
            {#if picked.size}
              <button
                type="button"
                title="Back to what the scope and settings show"
                onclick={() => setPicked([])}
                class="inline-flex h-6 shrink-0 items-center rounded-md px-2 text-ui-2xs text-muted-foreground hover:bg-accent hover:text-foreground"
              >Reset</button>
            {/if}
          {/if}
        </div>
      </PopoverContent>
    </Popover>
    {#if picked.size}
      <button
        type="button"
        title="Clear the table filter"
        aria-label="Clear the table filter"
        onclick={() => setPicked([])}
        class="hit-area -ml-1 inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      ><X class="size-3" /></button>
    {/if}
    {/if}

    {#if focusTable && filterApplies}
      <!-- Scope: what the diagram covers, relative to the table it was opened for.
           Icons only - the table's name is already in the tab and on its card. -->
      <div class="ml-0.5 flex shrink-0 items-center gap-1">
        <div class= "field-surface inline-flex h-7 shrink-0 items-center bg-muted/25 p-0.5">
          {#each SCOPES as s (s.id)}
            <button
              type="button"
              title={s.hint}
              aria-label={s.label}
              aria-pressed={scope === s.id}
              onclick={() => (scope = s.id)}
              class={cn(
                'inline-flex size-6 items-center justify-center rounded-[6px] transition-[background-color,color,transform] duration-150 ease-out active:scale-[0.97]',
                scope === s.id
                  ? 'bg-background text-foreground ring-1 ring-inset ring-border/70'
                  : 'text-muted-foreground hover:text-foreground',
              )}
            >
              {#if s.id === 'self'}<Square class="size-3.5" />
              {:else if s.id === 'related'}<Network class="size-3.5" />
              {:else}<LayoutGrid class="size-3.5" />{/if}
            </button>
          {/each}
        </div>
        {#if onclearfocus}
          <button
            type="button"
            class="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[background-color,color,transform] duration-150 ease-out hover:bg-accent hover:text-foreground active:scale-[0.97]"
            title="Clear focus on {focusTable}"
            onclick={onclearfocus}
          ><X class="size-3" /></button>
        {/if}
      </div>
    {/if}

    <div class="ml-auto flex shrink-0 items-center gap-1.5">
      <!-- The view's own numbers and actions. -->
      {#if tableMeta.size > 0 && !loading}
        {#if erdView === 'canvas'}
          <span class="whitespace-nowrap pr-1 font-mono text-ui-2xs tabular-nums text-muted-foreground">
            {nodes.length}/{tableMeta.size} tables · {fkCount} fk{layingOut ? ' · laying out…' : ''}
          </span>
        {:else if erdView === 'dictionary'}
          <span class="whitespace-nowrap pr-1 font-mono text-ui-2xs tabular-nums text-muted-foreground">
            {dictRows.length} column{dictRows.length === 1 ? '' : 's'} · {shownTables.length} tables
          </span>
        {:else if erdView === 'ddl'}
          <span class="inline-flex items-center gap-1.5 whitespace-nowrap pr-1 font-mono text-ui-2xs tabular-nums text-muted-foreground">
            {#if ddlLoading}<Loader class="size-3 shrink-0 animate-spin" />{/if}
            {shownTables.length} tables{ddlLoading ? ' · reading…' : ''}
          </span>
          <button
            type="button"
            title="Copy every statement"
            aria-label="Copy every statement"
            onclick={copyAllDdl}
            class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          >{#if copiedAllDdl}<Check class="size-3.5 text-success" />{:else}<Copy class="size-3.5" />{/if}</button>
          <Button
            variant="outline"
            size="sm"
            title="Save as a .sql file"
            onclick={() => void saveExport(ddlSource, `${activeSchema}-schema.sql`, { name: 'SQL', extensions: ['sql'] }, 'DDL')}
          ><Download class="size-3.5" />Export .sql</Button>
        {:else if erdView === 'hierarchy'}
          {#if hierSummary}
            <span class="min-w-0 truncate whitespace-nowrap pr-1 font-mono text-ui-2xs tabular-nums text-muted-foreground">{hierSummary}</span>
          {/if}
          {#if hierHubCount > 0}
            <!-- One switch: on draws the lines into hub tables (tenants, users);
                 off leaves them out and counts them on the hub's card. -->
            <button
              type="button"
              aria-pressed={!hierHideHubs}
              onclick={() => (hierHideHubs = !hierHideHubs)}
              title={hierHideHubs
                ? 'Draw the lines into hub tables, the ones most of the schema points at'
                : 'Leave out the lines into hub tables; their cards count them instead'}
              class={cn(
                'inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground',
                !hierHideHubs && 'bg-accent text-foreground',
              )}
            ><Waypoints class="size-3.5 shrink-0" />Hub lines</button>
          {/if}
          <div class="flex shrink-0 items-center">
            <button type="button" title="Zoom out" aria-label="Zoom out" onclick={() => hierPage?.zoom('out')} class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"><ZoomOut class="size-3.5" /></button>
            <button type="button" title="Zoom in" aria-label="Zoom in" onclick={() => hierPage?.zoom('in')} class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"><ZoomIn class="size-3.5" /></button>
            <button type="button" title="Fit" aria-label="Fit" onclick={() => hierPage?.zoom('fit')} class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"><Scan class="size-3.5" /></button>
          </div>
        {:else if erdView === 'tree'}
          {#if treeSummary}
            <span class="min-w-0 truncate whitespace-nowrap pr-1 font-mono text-ui-2xs tabular-nums text-muted-foreground">{treeSummary}</span>
          {/if}
          {#if treeView === 'flow'}
            {@render segmented(
              [{ value: '1', label: '1 hop', hint: 'The table and its neighbours' }, { value: '2', label: '2 hops', hint: 'Their neighbours too' }],
              String(treeDepth),
              (v) => (treeDepth = Number(v)),
            )}
          {/if}
          {#if treeView !== 'list'}
            <div class="flex shrink-0 items-center">
              <button type="button" title="Zoom out" aria-label="Zoom out" onclick={() => treePage?.zoom('out')} class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"><ZoomOut class="size-3.5" /></button>
              <button type="button" title="Zoom in" aria-label="Zoom in" onclick={() => treePage?.zoom('in')} class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"><ZoomIn class="size-3.5" /></button>
              <button type="button" title="Fit" aria-label="Fit" onclick={() => treePage?.zoom('fit')} class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"><Scan class="size-3.5" /></button>
            </div>
          {/if}
          {@render segmented(TREE_VIEWS, treeView, (v) => (treeView = /** @type {'flow'|'all'|'list'} */ (v)))}
        {/if}
      {/if}

      <button
        type="button"
        title={fullscreen ? `Exit full screen (${FULLSCREEN_KEYS} or Esc)` : `Full screen (${FULLSCREEN_KEYS})`}
        aria-pressed={fullscreen}
        onclick={() => (fullscreen = !fullscreen)}
        class={cn('inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground', fullscreen && 'bg-accent text-foreground')}
      >{#if fullscreen}<Minimize2 class="size-3.5" />{:else}<Maximize2 class="size-3.5" />{/if}</button>

      <!-- Which view of the map: one line each, the description on hover. -->
      <DropdownMenu.Root>
        <DropdownMenu.Trigger
          title="Switch view"
          class="field-surface inline-flex h-7 shrink-0 items-center gap-1.5 bg-input/30 pl-2 pr-1.5 text-ui-sm text-foreground transition-colors hover:bg-accent focus:outline-none data-[state=open]:bg-accent"
        >
          {@render viewIcon(erdView, 'size-3.5 shrink-0 text-muted-foreground')}
          {VIEWS.find((v) => v.id === erdView)?.label}
          <ChevronDown class="size-3 shrink-0 text-muted-foreground" />
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="min-w-40">
          <DropdownMenu.RadioGroup value={erdView} onValueChange={(v) => (erdView = /** @type {typeof erdView} */ (v))}>
            {#each VIEWS as v (v.id)}
              <DropdownMenu.RadioItem value={v.id} title={v.hint}>
                {@render viewIcon(v.id, 'size-3.5 shrink-0 text-muted-foreground')}
                <span data-slot="menu-label">{v.label}</span>
              </DropdownMenu.RadioItem>
            {/each}
          </DropdownMenu.RadioGroup>
        </DropdownMenu.Content>
      </DropdownMenu.Root>

      <div class="h-4 w-px shrink-0 bg-border/60"></div>

      {#if erdView === 'canvas'}
      <!-- Diagram settings: cards, layout and lines, so the Diagram's alone. -->
      <Popover bind:open={settingsOpen}>
        <PopoverTrigger
          class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[background-color,color,transform] duration-150 ease-out hover:bg-accent hover:text-foreground active:scale-[0.97] focus:outline-none data-[state=open]:bg-accent data-[state=open]:text-foreground"
          title="Diagram settings"
        >
          <SlidersHorizontal class="size-3.5" />
        </PopoverTrigger>
        <PopoverContent class="w-64 p-1" align="end">
          {#snippet columnsCtl()}
            {@render segmented(
              [{ value: 'all', label: 'All' }, { value: 'keys', label: 'Keys', hint: 'Only the key columns: primary and foreign' }],
              view.columnMode,
              (v) => updateSettings({ columnMode: /** @type {'all'|'keys'} */ (v) }),
            )}
          {/snippet}
          {#snippet spacingCtl()}
            {@render segmented(
              [{ value: 'compact', label: 'S' }, { value: 'comfortable', label: 'M' }, { value: 'spacious', label: 'L' }],
              view.spacing,
              (v) => updateSettings({ spacing: /** @type {'compact'|'comfortable'|'spacious'} */ (v) }),
            )}
          {/snippet}
          {#snippet routingCtl()}
            {@render segmented(
              [
                { value: 'smart', label: 'Routed', hint: 'Follow the corridors the layout kept clear of the cards' },
                { value: 'direct', label: 'Direct', hint: 'Straight lines - fastest on a huge diagram' },
              ],
              view.routing,
              (v) => updateSettings({ routing: /** @type {'smart'|'direct'} */ (v) }),
            )}
          {/snippet}
          {#snippet hubCtl()}
            {@render segmented(
              [
                { value: 'badges', label: 'Pills', hint: 'A table most of the schema points at (tenants, users) is named on each row instead of drawn from every card; its lines show for the card under the pointer or the selected one' },
                { value: 'lines', label: 'Lines', hint: 'Draw every relationship, hubs included' },
              ],
              view.hubLinks,
              (v) => updateSettings({ hubLinks: /** @type {'badges'|'lines'} */ (v) }),
            )}
          {/snippet}

          {@render settingRow('Columns', 'Every column on the card, or only the keys', columnsCtl)}
          {@render switchRow('Data types', 'Show each column’s type on the card', view.showTypes, () => updateSettings({ showTypes: !view.showTypes }))}

          <div class="-mx-1 my-1 h-px bg-border/60"></div>
          {@render settingRow('Spacing', 'Room between the cards', spacingCtl)}
          {@render settingRow('Routing', 'How relationship lines find their way between cards', routingCtl)}
          {#if view.routing === 'smart' && !routeHints.size && nodes.length > MAX_ROUTED_NODES}
            <p class="px-2 pb-1 text-ui-2xs leading-snug text-muted-foreground">
              Lines are direct: routing stops at {MAX_ROUTED_NODES} tables and this page has {nodes.length}.
            </p>
          {/if}
          {@render settingRow('Hub links', 'How links into tables most of the schema points at are drawn', hubCtl)}
          {@render switchRow('Only linked tables', 'Hide tables without any foreign key', connectedOnly, () => updateSettings({ connectedOnly: !connectedOnly }))}
          {@render switchRow('Grid dots', 'Dotted background on the canvas', view.grid, () => updateSettings({ grid: !view.grid }))}

          {#if !hostExports}
            <div class="-mx-1 my-1 h-px bg-border/60"></div>
            <button
              type="button"
              disabled={exporting || tableMeta.size === 0}
              class="flex h-8 w-full items-center gap-2 rounded-sm px-2 text-left text-ui-xs text-foreground hover:bg-accent disabled:pointer-events-none disabled:opacity-40"
              onclick={() => { exportDiagram('copy-png'); settingsOpen = false }}
            >
              <Copy class="size-3.5 shrink-0 text-muted-foreground" />Copy as PNG
            </button>
            <div class="flex h-8 items-center gap-2 px-2">
              {#if exporting}<Loader class="size-3.5 shrink-0 animate-spin text-muted-foreground" />{:else}<Download class="size-3.5 shrink-0 text-muted-foreground" />{/if}
              <span class="text-ui-xs text-foreground">Download</span>
              <div class="ml-auto flex items-center gap-0.5">
                {#each [
                  { kind: /** @type {const} */ ('png'), label: 'PNG' },
                  { kind: /** @type {const} */ ('svg'), label: 'SVG' },
                  { kind: /** @type {const} */ ('mermaid'), label: 'Mermaid' },
                ] as item (item.kind)}
                  <button
                    type="button"
                    title="Download the diagram as {item.label}"
                    disabled={exporting || tableMeta.size === 0}
                    class="inline-flex h-6 items-center rounded-md px-1.5 text-ui-2xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
                    onclick={() => { exportDiagram(item.kind); settingsOpen = false }}
                  >{item.label}</button>
                {/each}
              </div>
            </div>
          {/if}
        </PopoverContent>
      </Popover>

      <button
        type="button"
        class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[background-color,color,transform] duration-150 ease-out hover:bg-accent hover:text-foreground active:scale-[0.97] disabled:pointer-events-none disabled:opacity-40"
        onclick={reLayout}
        title="Re-run automatic layout"
      >
        <LayoutDashboard class="size-3.5" />
      </button>
      {/if}


      <button
        type="button"
        disabled={loading}
        class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[background-color,color,transform] duration-150 ease-out hover:bg-accent hover:text-foreground active:scale-[0.97] disabled:pointer-events-none disabled:opacity-40"
        onclick={() => void load()}
        title="Reload schema"
      >
        <RefreshCw class="size-3.5 {loading ? 'animate-spin' : ''}" />
      </button>
    </div>
  </div>

  <!-- ── Canvas + docked inspector ─────────────────────────────────────────── -->
  <div class="flex min-h-0 flex-1">
  <div class="relative min-h-0 min-w-0 flex-1">
    {#if loading && tableMeta.size === 0}
      <div class="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-background">
        <Loader class="size-6 animate-spin text-muted-foreground" />
        <p class="font-mono text-ui-xs text-muted-foreground">Loading schema structure…</p>
      </div>

    {:else if error}
      <div class="absolute inset-0 flex items-center justify-center bg-background">
        <p class="max-w-sm text-center font-mono text-ui-xs text-destructive">{error}</p>
      </div>

    {:else}
      {#if erdView === 'mermaid'}
        <!-- Mermaid source on the left, rendered on the right. The source
             follows the diagram (scope, filter, keys-only) until it is edited;
             Regenerate hands it back. -->
        <div class="absolute inset-0 flex min-h-0">
          {#if !mermaidCodeOpen}
            <div class="flex w-9 shrink-0 flex-col items-center border-r border-border/60 bg-panel pt-1.5">
              <button
                type="button"
                title="Show the Mermaid code"
                onclick={() => (mermaidCodeOpen = true)}
                class="flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              ><PanelLeftOpen class="size-3.5" /></button>
            </div>
          {:else}
          <div class="flex w-[42%] min-w-[280px] shrink-0 flex-col border-r border-border/60 bg-panel" style={mermaidCodeW ? `width:${mermaidCodeW}px` : ''}>
            <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border/40 px-2.5">
              <button
                type="button"
                title="Hide the code, keep the preview"
                onclick={() => (mermaidCodeOpen = false)}
                class="flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              ><PanelLeftClose class="size-3.5" /></button>
              <span class="shrink-0 text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground">Mermaid</span>
              <span class="min-w-0 truncate font-mono text-ui-3xs tabular-nums text-muted-foreground" title="{shownTables.length} tables, {mermaidRels.length} relationships">
                {shownTables.length} tables · {mermaidRels.length} links
              </span>
              <span class="ml-auto"></span>
              {#if mermaidTouched}
                <button
                  type="button"
                  title="Throw the edits away and generate the code from the diagram again"
                  onclick={() => { mermaidTouched = false; mermaidDraft = mermaidSource }}
                  class="inline-flex h-6 shrink-0 items-center gap-1 whitespace-nowrap rounded-md px-1.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                ><RefreshCw class="size-3" />Regenerate</button>
              {/if}
              <button
                type="button"
                title="Wrap long lines"
                aria-pressed={mermaidWrap}
                onclick={() => (mermaidWrap = !mermaidWrap)}
                class={cn('inline-flex size-6 shrink-0 items-center justify-center rounded-md transition-colors hover:bg-accent hover:text-foreground', mermaidWrap ? 'bg-accent text-foreground' : 'text-muted-foreground')}
              ><WrapText class="size-3.5" /></button>
              <button
                type="button"
                title="Copy the Mermaid code"
                onclick={copyMermaid}
                class="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              >{#if copiedCode}<Check class="size-3.5 text-success" />{:else}<Copy class="size-3.5" />{/if}</button>
              {#if onopendiagrams}
                <button
                  type="button"
                  title="Save this as a diagram of the connection and open it in the Diagrams page"
                  onclick={editInDiagrams}
                  class="field-surface inline-flex h-6 shrink-0 items-center gap-1 whitespace-nowrap bg-input/20 px-2 text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                ><GitBranch class="size-3" />Edit in Diagrams</button>
              {/if}
            </div>
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <CodeEditor bind:this={mermaidEditor} bind:value={mermaidDraft} wrap={mermaidWrap} ariaLabel="Mermaid code" onchange={() => (mermaidTouched = true)} />
            </div>
          </div>
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize the code pane"
            onpointerdown={startMermaidSplit}
            class="-ml-0.5 w-1 shrink-0 cursor-col-resize transition-colors hover:bg-border active:bg-ring/60"
          ></div>
          {/if}
          <div class="flex min-w-0 flex-1 flex-col">
            <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border/40 bg-panel px-2.5">
              <span class="text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground">Preview</span>
              <span class="ml-auto"></span>
              <button type="button" title="Zoom out" onclick={() => mermaidViewer?.dispatch('diagram:zoomout')} class="flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"><ZoomOut class="size-3.5" /></button>
              <button type="button" title="Zoom in" onclick={() => mermaidViewer?.dispatch('diagram:zoomin')} class="flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"><ZoomIn class="size-3.5" /></button>
              <button type="button" title="Fit" onclick={() => mermaidViewer?.dispatch('diagram:reset')} class="flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"><Maximize2 class="size-3.5" /></button>
            </div>
            <div class="min-h-0 flex-1 overflow-hidden bg-background">
              {#if mermaidGated}
                <div class="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
                  <p class="text-ui-sm text-foreground">{shownTables.length} tables is a few seconds of rendering.</p>
                  <p class="max-w-sm text-ui-xs text-muted-foreground">The code on the left is ready to copy. Narrow the diagram with the table filter, or render it anyway.</p>
                  <Button size="sm" onclick={() => (mermaidForce = true)}>Render {shownTables.length} tables</Button>
                </div>
              {:else}
                <MermaidViewer bind:this={mermaidViewer} code={mermaidDraft} spacing={{ nodeSpacing: 32, layerSpacing: 72 }} class="h-full w-full" />
              {/if}
            </div>
          </div>
        </div>
      {:else if erdView === 'hierarchy'}
        <!-- The tables on the page in foreign-key order, top to bottom. -->
        <div class="absolute inset-0">
          <ErdHierarchy
            bind:this={hierPage}
            tables={shownTables}
            rels={mermaidRels}
            schema={activeSchema}
            hideHubs={hierHideHubs}
            query={hierQuery}
            bind:summary={hierSummary}
            bind:hubCount={hierHubCount}
            onopen={(name) => openTable(name)}
          />
        </div>
      {:else if erdView === 'tree'}
        <!-- One table at a time, from the tables this page already loaded. -->
        <div class="absolute inset-0 flex min-h-0">
          <RelationTreePage
            bind:this={treePage}
            embedded
            bind:view={treeView}
            bind:depth={treeDepth}
            bind:listOpen={treeListOpen}
            bind:listSearch={treeSearch}
            bind:summary={treeSummary}
            initialMeta={tableMeta}
            schema={activeSchema}
            {schemas}
            onopentable={(_s, t) => openTable(t)}
          />
        </div>
      {:else if erdView === 'dictionary'}
        <!-- Every column of every table on the page, one row each. The filter
             matches table, column, type and reference, so "tenant_id" finds the
             key wherever it lives and "jsonb" finds every document column. -->
        <div class="absolute inset-0 flex min-h-0 flex-col">
          <div class="min-h-0 flex-1 overflow-auto">
            <table class="w-full border-separate border-spacing-0 font-mono text-ui-xs">
              <thead class="sticky top-0 z-10 bg-panel">
                <tr>
                  {#each ['Table', 'Column', 'Type', 'Null', 'Key', 'References'] as h (h)}
                    <th class="border-b border-border/60 px-3 py-1.5 text-left text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground">{h}</th>
                  {/each}
                </tr>
              </thead>
              <tbody>
                {#each dictRows.slice(0, DICT_ROWS_MAX) as r, i (r.table + '.' + r.col)}
                  {@const sameTable = i > 0 && dictRows[i - 1].table === r.table}
                  <tr class="group/row hover:bg-accent/40">
                    <td class={cn('whitespace-nowrap border-b border-border/15 px-3 py-1', sameTable ? 'text-transparent' : 'text-foreground/85')}>
                      <button type="button" class="hover:underline hover:text-foreground" onclick={() => openTable(r.table)} title="Open {r.table}">{r.table}</button>
                    </td>
                    <td class={cn('whitespace-nowrap border-b border-border/15 px-3 py-1', r.pk ? 'font-semibold' : '')} style={r.pk ? `color:${ink.pk}` : r.fk ? `color:${ink.fk}` : ''}>{r.col}</td>
                    <td class="whitespace-nowrap border-b border-border/15 px-3 py-1 text-muted-foreground">{r.type}</td>
                    <td class="whitespace-nowrap border-b border-border/15 px-3 py-1 text-ui-3xs text-muted-foreground">{r.nullable ? 'null' : 'not null'}</td>
                    <td class="whitespace-nowrap border-b border-border/15 px-3 py-1">
                      {#if r.pk}<span class="rounded-[3px] px-1 text-ui-3xs font-semibold" style="color:{ink.pk};background:color-mix(in oklch, {ink.pk} 14%, transparent)">pk</span>{/if}
                      {#if r.fk}<span class="rounded-[3px] px-1 text-ui-3xs font-semibold" style="color:{ink.fk};background:color-mix(in oklch, {ink.fk} 14%, transparent)">fk</span>{/if}
                      {#if r.uk && !r.pk}<span class="rounded-[3px] bg-muted/60 px-1 text-ui-3xs font-semibold text-muted-foreground">uk</span>{/if}
                    </td>
                    <td class="whitespace-nowrap border-b border-border/15 px-3 py-1 text-muted-foreground">
                      {#if r.ref}
                        <button type="button" class="hover:underline hover:text-foreground" onclick={() => openTable(r.refTable)} title="Open {r.refTable}">→ {r.ref}</button>
                      {/if}
                    </td>
                  </tr>
                {:else}
                  <tr><td colspan="6" class="px-3 py-6 text-center text-muted-foreground">No column matches “{dictQuery}”.</td></tr>
                {/each}
              </tbody>
            </table>
            {#if dictRows.length > DICT_ROWS_MAX}
              <p class="px-3 py-2 text-ui-2xs text-muted-foreground">{dictRows.length - DICT_ROWS_MAX} more - narrow the filter to reach them.</p>
            {/if}
          </div>
        </div>
      {:else if erdView === 'ddl'}
        <!-- The CREATE statements, as the engine reports them, one document.
             Read-only here; the SQL console is where statements are run. -->
        <div class="absolute inset-0 flex min-h-0 flex-col">
          <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
            <CodeEditor bind:this={ddlEditor} value={ddlSource} readOnly lang="sql" ariaLabel="DDL" />
          </div>
        </div>
      {:else}
      <ErdCanvas
        bind:this={erd}
        {nodes}
        {edges}
        {cfg}
        selectedId={selectedTable}
        {focusId}
        routing={view.routing}
        hints={routeHints}
        {hubEdges}
        {hubs}
        showTypes={view.showTypes}
        grid={view.grid}
        onselect={(id) => (selectedTable = id)}
        onopen={(name) => openTable(name)}
        onnodemoved={onNodeMoved}
      />

      {#if gatedCount > 0}
        <!-- Too wide to draw unasked. Laying out and routing this many cards is
             seconds of work, and the result is a wall nobody reads - so the
             diagram waits for a filter, or for an explicit go-ahead. -->
        <div class="absolute inset-0 flex items-center justify-center bg-background p-6">
          <div class="max-w-sm rounded-xl border border-border/60 bg-panel p-5 text-center elevate-2-rim">
            <Network class="mx-auto size-5 text-muted-foreground" />
            <p class="mt-2.5 text-ui-sm font-medium text-foreground">
              {gatedCount} tables in <span class="font-mono">{activeSchema}</span>
            </p>
            <p class="mt-1.5 text-ui-xs leading-relaxed text-muted-foreground">
              A diagram this wide takes seconds to lay out and is hard to read once it lands.
              Pick the tables worth seeing, or draw the whole schema anyway.
            </p>
            <div class="mt-4 flex items-center justify-center gap-2">
              <button
                type="button"
                onclick={() => (pickerOpen = true)}
                class="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-3 text-ui-xs font-medium text-primary-foreground transition-transform duration-150 ease-out active:scale-[0.98]"
              >
                <ListFilter class="size-3.5 shrink-0" />
                Pick tables
              </button>
              <button
                type="button"
                onclick={() => (drawAll = true)}
                class= "field-surface inline-flex h-8 items-center bg-input/20 px-3 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              >Draw all {gatedCount}</button>
            </div>
          </div>
        </div>
      {/if}

      {#if tableMeta.size === 0 && !loading}
        <div class="pointer-events-none absolute inset-x-0 top-4 flex justify-center">
          <div class="rounded-lg border border-border/50 bg-panel px-4 py-3 font-mono text-ui-xs text-muted-foreground">
            No tables found in <span class="text-foreground/70">{activeSchema}</span>
          </div>
        </div>
      {/if}

      <!-- ── Legend ───────────────────────────────────────────────────────── -->
      {#if nodes.length > 0}
        <div class="pointer-events-none absolute inset-x-0 bottom-3 flex justify-center">
          <div class="flex h-6 items-center gap-2.5 rounded-full border border-border/40 bg-panel/75 px-2.5 text-ui-3xs text-muted-foreground backdrop-blur-sm">
            <span class="flex items-center gap-1"><span class="size-1.5 rounded-full" style="background:{ink.pk}"></span>PK</span>
            <span class="flex items-center gap-1"><span class="size-1.5 rounded-full" style="background:{ink.fk}"></span>FK</span>
            <span class="h-2.5 w-px bg-border/60"></span>
            <!-- Crow's foot: fork on the FK side, bar on the referenced side. -->
            <span class="flex items-center gap-1" title="One-to-many: the fork sits on the foreign key side">
              <svg width="20" height="7" viewBox="0 0 20 7" aria-hidden="true" class="shrink-0 opacity-80">
                <path d="M1 1v5M2 3.5h12M14 3.5l5-2.5M14 3.5l5 2.5M14 3.5h5" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" />
              </svg>
              1:N
            </span>
            <span class="flex items-center gap-1" title="One-to-one: the foreign key column is unique">
              <svg width="16" height="7" viewBox="0 0 16 7" aria-hidden="true" class="shrink-0 opacity-80">
                <path d="M1 1v5M2 3.5h11M14 1v5" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" />
              </svg>
              1:1
            </span>
            {#if focusId}
              <span class="h-2.5 w-px bg-border/60"></span>
              <!-- Matches the rim on the card. Teal, not the theme blue: blue is
                   already the foreign-key colour and the two collided. -->
              <span class="flex items-center gap-1">
                <span class="size-2 rounded-[3px] border" style="border-color:{ink.focus};background:color-mix(in oklab, {ink.focus} 24%, transparent)"></span>
                Current
              </span>
            {/if}
          </div>
        </div>
      {/if}

      {/if}
    {/if}
  </div>

  <!-- ── Inspector: docked, not floating over the diagram ────────────────── -->
  {#if selMeta}
    <aside class="flex w-72 shrink-0 flex-col overflow-hidden border-l border-border bg-panel">
      <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border/60 px-3">
        <span class="min-w-0 flex-1 truncate font-mono text-ui-xs font-semibold text-foreground">{selMeta.name}</span>
        <button
          type="button"
          class="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[background-color,color,transform] duration-150 ease-out hover:bg-accent hover:text-foreground active:scale-[0.97]"
          title="Copy CREATE statement"
          onclick={() => void copyDdl(selMeta.name)}
        >
          {#if copiedDdl === selMeta.name}
            <Check class="size-3.5 text-success" />
          {:else}
            <Copy class="size-3.5" />
          {/if}
        </button>
        <button
          type="button"
          class="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[background-color,color,transform] duration-150 ease-out hover:bg-accent hover:text-foreground active:scale-[0.97]"
          title="Close inspector"
          onclick={() => (selectedTable = null)}
        ><X class="size-3.5" /></button>
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto">
        <p class="px-3 pt-2.5 pb-1 text-ui-3xs font-semibold uppercase tracking-[0.08em] text-muted-foreground">
          Columns <span class="font-normal tabular-nums text-muted-foreground">{selMeta.columns.length}</span>
        </p>
        {#each selMeta.columns as col (col.name)}
          {@const isPk = selMeta.pkCols.has(col.name)}
          {@const isFk = !!col.foreignKey}
          <div class="flex items-center gap-2 px-3 py-1">
            {#if isPk}<KeyRound class="size-3 shrink-0" style="color:{ink.pk}" />
            {:else if isFk}<Link class="size-3 shrink-0" style="color:{ink.fk}" />
            {:else}<span class="size-3 shrink-0"></span>{/if}
            <span class="min-w-0 flex-1 truncate font-mono text-ui-2xs {isPk ? 'font-medium text-foreground' : 'text-foreground/75'}">{col.name}</span>
            <span class="shrink-0 font-mono text-ui-3xs text-muted-foreground">{col.dataType}</span>
          </div>
        {/each}

        {#if selFks.length > 0}
          <p class="mt-2 border-t border-border/40 px-3 pt-2.5 pb-1 text-ui-3xs font-semibold uppercase tracking-[0.08em] text-muted-foreground">References</p>
          {#each selFks as fk (fk.name)}
            {@const ref = fk.foreignKey?.split('.') ?? []}
            <button
              type="button"
              class="flex w-full items-baseline gap-1.5 px-3 py-1 text-left transition-colors hover:bg-accent/60"
              onclick={() => { selectedTable = ref[1] ?? null }}
            >
              <span class="min-w-0 truncate font-mono text-ui-2xs text-muted-foreground">{fk.name}</span>
              <span class="shrink-0 text-ui-3xs text-muted-foreground">→</span>
              <span class="min-w-0 truncate font-mono text-ui-2xs text-foreground/80">{ref[1]}.{ref[2]}</span>
            </button>
          {/each}
        {/if}

        {#if refBy.length > 0}
          <p class="mt-2 border-t border-border/40 px-3 pt-2.5 pb-1 text-ui-3xs font-semibold uppercase tracking-[0.08em] text-muted-foreground">Referenced by</p>
          {#each refBy as t (t.name)}
            <button
              type="button"
              class="flex w-full items-center gap-1.5 px-3 py-1 text-left transition-colors hover:bg-accent/60"
              onclick={() => { selectedTable = t.name }}
            >
              <span class="shrink-0 text-ui-3xs text-muted-foreground">←</span>
              <span class="min-w-0 truncate font-mono text-ui-2xs text-muted-foreground">{t.name}</span>
            </button>
          {/each}
        {/if}
      </div>

      <div class="flex shrink-0 items-center gap-1.5 border-t border-border/60 p-2">
        <button
          type="button"
          class= "field-surface inline-flex h-8 min-w-0 flex-1 items-center justify-center gap-1.5 bg-input/20 text-ui-xs font-medium text-foreground transition-[background-color,transform] duration-150 ease-out hover:bg-accent active:scale-[0.99]"
          onclick={() => openTable(selMeta.name)}
        >Open table</button>
        <button
          type="button"
          title="Show only {selMeta.name} and what it links to"
          class="inline-flex size-8 shrink-0 items-center justify-center rounded-md border border-border/60 bg-input/20 text-muted-foreground transition-[background-color,color,transform] duration-150 ease-out hover:bg-accent hover:text-foreground active:scale-[0.97]"
          onclick={() => isolate(selMeta.name)}
        ><Crosshair class="size-3.5" /></button>
        <button
          type="button"
          disabled={selMeta.name === focusTable}
          title={selMeta.name === focusTable
            ? 'This is the table the diagram belongs to'
            : `Take ${selMeta.name} off the diagram`}
          class="inline-flex size-8 shrink-0 items-center justify-center rounded-md border border-border/60 bg-input/20 text-muted-foreground transition-[background-color,color,transform] duration-150 ease-out hover:bg-accent hover:text-foreground active:scale-[0.97] disabled:pointer-events-none disabled:opacity-40"
          onclick={() => hideTable(selMeta.name)}
        ><EyeOff class="size-3.5" /></button>
      </div>
    </aside>
  {/if}
  </div>
</div>
