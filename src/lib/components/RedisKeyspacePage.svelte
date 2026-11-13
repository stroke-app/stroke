<script>
  // Redis keys as an editable table. One row per key (type, TTL, size, length,
  // value preview), loaded a page at a time with SCAN. Edits to values, TTLs
  // and names, new keys and deletes are staged and sent together in one
  // MULTI/EXEC when the user applies them. The selected key's full value opens
  // in a side panel; a redis-cli console docks underneath.
  import FieldSelect from './FieldSelect.svelte'
  import ResizeHandle from './ResizeHandle.svelte'
  import RedisValuePanel from './RedisValuePanel.svelte'
  import { redisNav, registerRedisNav, terminalDockOpen, toggleTerminalDock } from '$lib/stores/redis-nav.js'
  import RedisInsightsView from './RedisInsightsView.svelte'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { executeSql, connectRedis, redisKeyPage, redisKeyRows, redisApply, redisCommand } from '$lib/api.js'
  import { engineFamily } from '$lib/stores/connections.js'
  import { readOnlyMode, guardWrite, READ_ONLY_HINT } from '$lib/stores/read-only.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { getAppScale } from '$lib/app-zoom.js'
  import { cn, plural } from '$lib/utils.js'
  import { Checkbox } from '$lib/components/ui/checkbox/index.js'
  import {
    CREATABLE_TYPES, stage, unstage, stagedFor, toOps, createOps, ttlOps, toPattern,
    humanBytes, ttlLabel, commandLine,
  } from '$lib/redis-staging.js'
  import { tick } from 'svelte'
  import Search from '@lucide/svelte/icons/search'
  import X from '@lucide/svelte/icons/x'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import Plus from '@lucide/svelte/icons/plus'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Undo2 from '@lucide/svelte/icons/undo-2'
  import Check from '@lucide/svelte/icons/check'
  import ChevronUp from '@lucide/svelte/icons/chevron-up'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import TerminalIcon from '@lucide/svelte/icons/terminal'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import Server from '@lucide/svelte/icons/server'
  import Type from '@lucide/svelte/icons/type'
  import Hash from '@lucide/svelte/icons/hash'
  import List from '@lucide/svelte/icons/list'
  import Braces from '@lucide/svelte/icons/braces'
  import ListOrdered from '@lucide/svelte/icons/list-ordered'
  import Radio from '@lucide/svelte/icons/radio'
  import FileJson from '@lucide/svelte/icons/file-json'
  import CircleHelp from '@lucide/svelte/icons/circle-help'

  /**
   * @typedef {import('$lib/api.js').RedisKeyRow} RedisKeyRow
   * @typedef {RedisKeyRow & { at: number, isNew?: boolean }} Row
   * @typedef {import('$lib/redis-staging.js').Staged} Staged
   */

  /** @type {{ active?: boolean, connection?: import('$lib/stores/connections.js').SavedConnection | null }} */
  let { active = false, connection = null } = $props()

  // ── Sidebar: databases, views, namespaces ───────────────────────────────
  /** @typedef {'keys' | 'insights' | 'slowlog' | 'clients' | 'pubsub'} RedisView */
  let view = $state(/** @type {RedisView} */ ('keys'))
  /** Keys per logical db, from INFO's keyspace section. */
  const dbKeys = $derived.by(() => {
    /** @type {Record<number, number>} */
    const m = {}
    for (const [k, v] of Object.entries(info ?? {})) {
      const mm = /^db(\d+)$/.exec(k)
      if (mm) m[Number(mm[1])] = Number(/keys=(\d+)/.exec(v)?.[1] ?? 0)
    }
    return m
  })
  /** Top-level prefixes of the loaded keys (up to the first : / . separator). */
  const namespaces = $derived.by(() => {
    /** @type {Map<string, number>} */
    const m = new Map()
    for (const r of rows) {
      const i = r.key.search(/[:/.]/)
      if (i > 0) m.set(r.key.slice(0, i + 1), (m.get(r.key.slice(0, i + 1)) ?? 0) + 1)
    }
    return [...m].sort((a, b) => b[1] - a[1]).slice(0, 30)
  })
  // Publish what the sidebar shows, and take its clicks.
  $effect(() => {
    redisNav.set({
      view, currentDb, dbKeys, namespaces, filter: filterInput, consoleOpen, switching: switchingDb, locked: staged.length > 0,
      server: info ? {
        version: [info.redis_version ? `Redis ${info.redis_version}` : info.valkey_version ? `Valkey ${info.valkey_version}` : '', info.redis_mode && info.redis_mode !== 'standalone' ? info.redis_mode : ''].filter(Boolean).join(' · '),
        memory: info.used_memory_human ?? '',
        clients: info.connected_clients ?? '',
        ops: info.instantaneous_ops_per_sec ?? '',
      } : null,
    })
  })
  $effect(() => {
    if (!active) return
    return registerRedisNav({
      setView: (/** @type {RedisView} */ v) => (view = v),
      switchDb: (/** @type {number} */ n) => { view = 'keys'; void switchDb(n) },
      filterTo: (/** @type {string} */ p) => filterTo(p),
    })
  })

  /** @param {string} pattern */
  function filterTo(pattern) {
    view = 'keys'
    filterInput = pattern
    onFilterInput()
  }

  const isRedisConn = $derived(engineFamily(connection?.type) === 'redis')
  const readOnly = $derived($readOnlyMode)

  // Type colors are a deliberate exception to the token palette (like syntax
  // highlighting): on the glyph and label only, never as filled chips.
  /** @type {Record<string, { short: string, label: string, icon: any, color: string }>} */
  const TYPE_META = {
    string: { short: 'STR', label: 'string', icon: Type, color: 'text-sky-600 dark:text-sky-400' },
    hash: { short: 'HASH', label: 'hash', icon: Hash, color: 'text-violet-600 dark:text-violet-400' },
    list: { short: 'LIST', label: 'list', icon: List, color: 'text-amber-600 dark:text-amber-400' },
    set: { short: 'SET', label: 'set', icon: Braces, color: 'text-emerald-600 dark:text-emerald-400' },
    zset: { short: 'ZSET', label: 'sorted set', icon: ListOrdered, color: 'text-rose-600 dark:text-rose-400' },
    stream: { short: 'STREAM', label: 'stream', icon: Radio, color: 'text-cyan-600 dark:text-cyan-400' },
    'ReJSON-RL': { short: 'JSON', label: 'JSON', icon: FileJson, color: 'text-orange-600 dark:text-orange-400' },
  }
  /** @param {string} t */
  const metaFor = (t) => TYPE_META[t] ?? { short: t.slice(0, 6).toUpperCase(), label: t, icon: CircleHelp, color: 'text-muted-foreground' }

  const TYPE_FILTERS = [
    { value: '', label: 'All types' },
    ...['string', 'hash', 'list', 'set', 'zset', 'stream'].map((t) => ({ value: t, label: TYPE_META[t].label })),
  ]
  /** @param {string} t */
  const typeLabel = (t) => (t === 'zset' ? 'Sorted set' : t === 'ReJSON-RL' ? 'JSON' : (TYPE_META[t]?.label ?? t).replace(/^./, (c) => c.toUpperCase()))
  const DB_OPTIONS = Array.from({ length: 16 }, (_, n) => ({ value: String(n), label: `db${n}` }))

  // ── Rows ──────────────────────────────────────────────────────────────────
  let rows = $state.raw(/** @type {Row[]} */ ([]))
  let cursor = $state('0')
  let done = $state(false)
  let loading = $state(false)
  let error = $state('')
  let scanned = $state(0)
  let dbSize = $state(/** @type {number | null} */ (null))
  let filterInput = $state('')
  let typeFilter = $state('')
  let currentDb = $state(0)
  let switchingDb = $state(false)
  let loadedConnId = $state(/** @type {string | null} */ (null))
  let loadSeq = 0

  const PAGE_TARGET = 200

  /** @param {boolean} reset */
  async function loadPage(reset) {
    if (!active || !isRedisConn) return
    if (!reset && (loading || done)) return
    const seq = reset ? ++loadSeq : loadSeq
    loading = true
    if (reset) error = ''
    try {
      const page = await redisKeyPage(reset ? '0' : cursor, {
        pattern: toPattern(filterInput),
        type: typeFilter,
        target: PAGE_TARGET,
      })
      if (seq !== loadSeq) return
      const at = Date.now()
      const fresh = page.rows.map((r) => ({ ...r, at }))
      if (reset) {
        rows = fresh
        scanned = page.scanned
      } else {
        // SCAN can repeat a key across pages.
        const have = new Set(rows.map((r) => r.key))
        rows = [...rows, ...fresh.filter((r) => !have.has(r.key))]
        scanned += page.scanned
      }
      cursor = page.cursor
      done = page.cursor === '0'
    } catch (e) {
      if (seq !== loadSeq) return
      error = String(e).replace(/^Error:\s*/, '')
      if (reset) rows = []
      done = true
    } finally {
      if (seq === loadSeq) loading = false
    }
  }

  function reload() {
    cursor = '0'
    done = false
    void loadPage(true)
    void loadServer()
  }

  // ── Server info (footer) ──────────────────────────────────────────────────
  let info = $state(/** @type {Record<string, string> | null} */ (null))

  /** @param {{ rows?: unknown[][] } | null} res */
  const firstValue = (res) => {
    const r = res?.rows?.[0]
    const v = Array.isArray(r) ? r[0] : r
    return v == null ? '' : String(v)
  }

  async function loadServer() {
    if (!active || !isRedisConn) return
    // Straight to the server, not through the SQL console path (which records
    // history and flattens replies): INFO came back empty there for some setups.
    const [size, raw] = await Promise.all([
      redisCommand(['DBSIZE']).then((v) => (v == null ? '' : String(v))).catch(() => ''),
      redisCommand(['INFO']).then((v) => (v == null ? '' : String(v))).catch(() => ''),
    ])
    dbSize = size === '' ? null : Number(size)
    /** @type {Record<string, string>} */
    const map = {}
    for (const line of raw.split(/\r?\n/)) {
      const i = line.indexOf(':')
      if (line && line[0] !== '#' && i > 0) map[line.slice(0, i)] = line.slice(i + 1).trim()
    }
    info = raw ? map : null
  }

  // Load once per connection when the page first becomes active.
  $effect(() => {
    const cid = connection?.id ?? null
    if (!active || !isRedisConn || loadedConnId === cid) return
    loadedConnId = cid
    currentDb = Number(connection?.db) || 0
    staged = []
    checked = new Set()
    selectedKey = null
    reload()
  })

  // Debounced server-side filtering: the filter is a SCAN MATCH, not a filter
  // over what happens to be loaded.
  /** @type {ReturnType<typeof setTimeout> | null} */
  let filterTimer = null
  function onFilterInput() {
    if (filterTimer) clearTimeout(filterTimer)
    filterTimer = setTimeout(() => { cursor = '0'; done = false; void loadPage(true) }, 250)
  }
  $effect(() => () => { if (filterTimer) clearTimeout(filterTimer) })

  /** @param {string} t */
  function setTypeFilter(t) {
    if (t === typeFilter) return
    typeFilter = t
    cursor = '0'
    done = false
    void loadPage(true)
  }

  /** @param {number} n */
  async function switchDb(n) {
    if (n === currentDb || switchingDb || !connection) return
    switchingDb = true
    try {
      await connectRedis({ ...connection, db: n })
      currentDb = n
      selectedKey = null
      checked = new Set()
      reload()
    } catch (e) {
      error = String(e).replace(/^Error:\s*/, '')
    } finally {
      switchingDb = false
    }
  }

  // ── Staged changes ────────────────────────────────────────────────────────
  let staged = $state.raw(/** @type {Staged[]} */ ([]))
  let applying = $state(false)
  let reviewOpen = $state(false)
  let panelReload = $state(0)

  /** List-delete cleanups only matter while a tombstoned element is staged. */
  function dropOrphans(/** @type {Staged[]} */ list) {
    return list.filter((s) => !s.cell.startsWith('post:') || list.some((o) => o.key === s.key && o.deleted && o.cell.startsWith('e:i:')))
  }

  /** @param {Omit<Staged, 'id'>} item */
  function onstage(item) {
    if (!guardWrite('edit keys')) return
    staged = dropOrphans(stage(staged, item))
  }
  /** @param {string} key @param {string} [cell] */
  function onunstage(key, cell) {
    staged = dropOrphans(unstage(staged, key, cell))
  }

  /** Staged edits by key, then cell; rebuilt once per staging change. */
  const stagedByKey = $derived.by(() => {
    /** @type {Map<string, { cells: Map<string, Staged>, entries: boolean }>} */
    const m = new Map()
    for (const s of staged) {
      let e = m.get(s.key)
      if (!e) m.set(s.key, (e = { cells: new Map(), entries: false }))
      e.cells.set(s.cell, s)
      if (s.cell.startsWith('e:')) e.entries = true
    }
    return m
  })

  const pendingCount = $derived(staged.filter((s) => !s.cell.startsWith('post:')).length)
  const failed = $derived(staged.filter((s) => s.error))

  async function apply() {
    if (!staged.length || applying) return
    if (!guardWrite('apply changes')) return
    applying = true
    const sent = staged
    const { ops, owner } = toOps(sent)
    try {
      const outcomes = await redisApply(ops)
      /** @type {Map<number, string>} */
      const errors = new Map()
      outcomes.forEach((o, i) => { if (!o.ok && !errors.has(owner[i])) errors.set(owner[i], o.error ?? 'failed') })
      // Keep what failed staged (with its error); anything staged while the
      // apply was in flight stays too.
      const sentIds = new Set(sent.map((s) => s.id))
      staged = [
        ...sent.filter((s) => errors.has(s.id)).map((s) => ({ ...s, error: errors.get(s.id) })),
        ...staged.filter((s) => !sentIds.has(s.id)),
      ]
      const okCount = sent.filter((s) => !errors.has(s.id) && !s.cell.startsWith('post:')).length
      await refreshKeys(sent.filter((s) => !errors.has(s.id)))
      panelReload++
      if (errors.size) {
        toast.error(`${errors.size} change${errors.size === 1 ? '' : 's'} failed`, {
          description: `${[...errors.values()][0]}${okCount ? ` · ${okCount} applied` : ''}`,
        })
      } else {
        toast.success(`Applied ${okCount} change${okCount === 1 ? '' : 's'}`)
        reviewOpen = false
      }
    } catch (e) {
      toast.error('Nothing was applied', { description: String(e).replace(/^Error:\s*/, '') })
    } finally {
      applying = false
    }
  }

  function discard() {
    staged = []
    reviewOpen = false
  }

  /** Re-read the rows an apply touched; drop deleted keys, add new ones. @param {Staged[]} applied */
  async function refreshKeys(applied) {
    /** @type {Map<string, boolean>} */
    const keys = new Map()
    /** @type {Map<string, string>} */
    const renamed = new Map()
    for (const s of applied) {
      keys.set(s.key, s.binary)
      if (s.cell === 'key') {
        const to = s.ops[0][2]
        keys.set(to, false)
        renamed.set(s.key, to)
      }
    }
    if (!keys.size) return
    let fresh
    try {
      fresh = await redisKeyRows([...keys].map(([key, binary]) => ({ key, binary })))
    } catch {
      return reload()
    }
    const at = Date.now()
    /** @type {Map<string, Row>} */
    const byKey = new Map(fresh.map((r) => [r.key, { ...r, at }]))
    const next = []
    const seen = new Set()
    for (const r of rows) {
      const target = renamed.get(r.key) ?? r.key
      const f = byKey.get(target) ?? (keys.has(r.key) ? null : r)
      if (f && f.type !== 'none' && !seen.has(f.key)) {
        next.push(f)
        seen.add(f.key)
      }
    }
    // Keys that are new to the table (creates) go on top.
    const created = fresh.filter((r) => r.type !== 'none' && !seen.has(r.key)).map((r) => ({ ...r, at }))
    rows = [...created, ...next]
    if (selectedKey && renamed.has(selectedKey)) selectedKey = renamed.get(selectedKey) ?? null
    if (selectedKey && !rows.some((r) => r.key === selectedKey)) selectedKey = null
    checked = new Set([...checked].filter((k) => rows.some((r) => r.key === k)))
    void loadServer()
  }

  // ── Display rows (staged creates first) + sorting ─────────────────────────
  /** @type {{ col: 'key'|'type'|'ttl'|'memory'|'length', dir: 1|-1 } | null} */
  let sort = $state(null)

  /** @param {'key'|'type'|'ttl'|'memory'|'length'} col */
  function toggleSort(col) {
    if (sort?.col !== col) sort = { col, dir: 1 }
    else if (sort.dir === 1) sort = { col, dir: -1 }
    else sort = null
  }

  const creates = $derived(
    staged
      .filter((s) => s.cell === 'create' && s.create)
      .map((s) => /** @type {Row} */ ({
        key: s.key, binary: false, type: s.create?.type ?? 'string', ttl_ms: -1, memory: null,
        length: null, preview: s.create?.preview ?? '', truncated: false, at: Date.now(), isNew: true,
      })),
  )

  const displayRows = $derived.by(() => {
    let list = rows
    if (sort) {
      const { col, dir } = sort
      /** @param {Row} r */
      const val = (r) => col === 'key' ? r.key : col === 'type' ? r.type : col === 'ttl' ? (r.ttl_ms < 0 ? Infinity : r.ttl_ms) : (r[col] ?? -1)
      list = rows.slice().sort((a, b) => {
        const x = val(a)
        const y = val(b)
        return (typeof x === 'string' ? x.localeCompare(String(y), undefined, { numeric: true }) : Number(x) - Number(y)) * dir
      })
    }
    return creates.length ? [...creates, ...list] : list
  })

  // ── Selection ─────────────────────────────────────────────────────────────
  let selectedKey = $state(/** @type {string | null} */ (null))
  let checked = $state(/** @type {Set<string>} */ (new Set()))
  const selectedRow = $derived(displayRows.find((r) => r.key === selectedKey) ?? null)
  const allChecked = $derived(displayRows.length > 0 && displayRows.every((r) => checked.has(r.key)))

  /** @param {string} key */
  function toggleCheck(key) {
    const next = new Set(checked)
    if (next.has(key)) next.delete(key)
    else next.add(key)
    checked = next
  }
  function toggleAll() {
    checked = allChecked ? new Set() : new Set(displayRows.map((r) => r.key))
  }

  function stageDeleteChecked() {
    const targets = checked.size ? displayRows.filter((r) => checked.has(r.key)) : selectedRow ? [selectedRow] : []
    if (!targets.length || !guardWrite('delete keys')) return
    let next = staged
    for (const r of targets) {
      next = stage(next, { key: r.key, binary: r.binary, cell: 'row', label: 'delete key', deleted: true, ops: [['UNLINK', r.key]] })
    }
    staged = dropOrphans(next)
    checked = new Set()
  }

  // ── Inline cell editing ───────────────────────────────────────────────────
  /** @type {{ key: string, col: 'key'|'ttl'|'value' } | null} */
  let editing = $state(null)
  let draft = $state('')
  let editError = $state('')

  /** @param {Row} r @param {'key'|'ttl'|'value'} col */
  function canEdit(r, col) {
    if (readOnly || r.isNew || stagedFor(staged, r.key, 'row')) return false
    if (col === 'value') return r.type === 'string' && !r.truncated
    return true
  }

  /** @param {Row} r @param {'key'|'ttl'|'value'} col */
  function startEdit(r, col) {
    if (!canEdit(r, col)) {
      if (col === 'value') selectedKey = r.key
      return
    }
    const s = stagedFor(staged, r.key, col)
    editError = ''
    draft = s?.display ?? (col === 'key' ? r.key : col === 'ttl' ? (r.ttl_ms > 0 ? ttlLabel(r.ttl_ms, r.at, Date.now()).replace(/\s+/g, '') : '') : r.preview)
    editing = { key: r.key, col }
  }

  /** @param {Row} r */
  function commitEdit(r) {
    if (!editing) return
    const { col } = editing
    const k = r.key
    if (col === 'key') {
      const to = draft.trim()
      if (!to || to === k) onunstage(k, 'key')
      else onstage({ key: k, binary: r.binary, cell: 'key', label: `rename to ${to}`, display: to, ops: [['RENAMENX', k, to]] })
    } else if (col === 'ttl') {
      const parsed = ttlOps(k, draft)
      if (!parsed) {
        editError = 'Use seconds or 30s, 15m, 2h, 1d. Leave blank for no expiry.'
        return
      }
      const unchanged = parsed.seconds == null ? r.ttl_ms === -1 : false
      if (unchanged) onunstage(k, 'ttl')
      else onstage({ key: k, binary: r.binary, cell: 'ttl', label: parsed.seconds == null ? 'remove expiry' : `expire in ${draft.trim()}`, display: parsed.seconds == null ? '' : draft.trim(), ops: parsed.ops })
    } else {
      if (draft === r.preview) onunstage(k, 'value')
      else onstage({ key: k, binary: r.binary, cell: 'value', label: 'set value', display: draft, ops: [['SET', k, draft]] })
    }
    editing = null
    editError = ''
    gridEl?.focus()
  }

  /** @param {KeyboardEvent} e @param {Row} r */
  function onEditKey(e, r) {
    if (e.key === 'Enter') {
      e.preventDefault()
      commitEdit(r)
    } else if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      editing = null
      editError = ''
      gridEl?.focus()
    }
  }

  /** @param {HTMLInputElement} node */
  function focusSelect(node) {
    node.focus()
    node.select()
  }

  // ── New key ───────────────────────────────────────────────────────────────
  let creating = $state(false)
  let nk = $state({ key: '', type: 'string', field: '', score: '', value: '', ttl: '' })
  let nkError = $state('')

  function openCreate() {
    if (!guardWrite('create keys')) return
    creating = true
    nkError = ''
    nk = { key: '', type: 'string', field: '', score: '', value: '', ttl: '' }
  }

  function stageCreate() {
    const key = nk.key.trim()
    if (!key) return
    if (rows.some((r) => r.key === key) || creates.some((r) => r.key === key)) {
      nkError = `"${key}" already exists`
      return
    }
    const ttl = nk.ttl.trim() ? ttlOps(key, nk.ttl) : null
    if (nk.ttl.trim() && !ttl?.seconds) {
      nkError = 'TTL: use seconds or 30s, 15m, 2h, 1d'
      return
    }
    const preview = nk.type === 'hash' ? `{${nk.field || 'field'}: ${nk.value}}` : nk.type === 'string' ? nk.value : `[${nk.value}]`
    onstage({
      key, binary: false, cell: 'create', label: `create ${nk.type}`,
      ops: createOps(key, nk.type, { value: nk.value, field: nk.field, score: nk.score, ttl: ttl?.seconds ?? null }),
      create: { type: nk.type, preview },
    })
    creating = false
  }

  // ── Virtualized grid ──────────────────────────────────────────────────────
  /** @type {HTMLElement | null} */
  let gridEl = $state(null)
  /** @type {HTMLElement | null} */
  let headerEl = $state(null)
  let scrollTop = $state(0)
  let viewportH = $state(0)
  const rowH = $derived(headerEl?.offsetHeight || 28)
  const OVERSCAN = 8
  const first = $derived(Math.max(0, Math.floor(scrollTop / rowH) - OVERSCAN))
  const last = $derived(Math.min(displayRows.length, Math.ceil((scrollTop + viewportH) / rowH) + OVERSCAN))
  const visible = $derived(displayRows.slice(first, last))

  let scrollRaf = 0
  function onScroll() {
    if (scrollRaf) return
    scrollRaf = requestAnimationFrame(() => {
      scrollRaf = 0
      syncScroll()
    })
  }
  $effect(() => () => { if (scrollRaf) cancelAnimationFrame(scrollRaf) })

  function syncScroll() {
    if (!gridEl) return
    scrollTop = gridEl.scrollTop
    if (!done && !loading && gridEl.scrollTop + gridEl.clientHeight > gridEl.scrollHeight - rowH * 20) void loadPage(false)
  }

  // Keep the grid filled: if the first page didn't fill the viewport and the
  // keyspace has more, fetch on.
  $effect(() => {
    if (active && !loading && !done && !error && displayRows.length * rowH < viewportH) void loadPage(false)
  })

  /** @param {number} delta */
  async function moveSelection(delta) {
    const list = displayRows
    if (!list.length) return
    const i = list.findIndex((r) => r.key === selectedKey)
    const next = Math.max(0, Math.min(list.length - 1, i < 0 ? 0 : i + delta))
    selectedKey = list[next].key
    await tick()
    if (!gridEl) return
    const top = next * rowH
    const bottom = top + rowH * 2
    if (top < gridEl.scrollTop + rowH) gridEl.scrollTop = Math.max(0, top - rowH)
    else if (bottom > gridEl.scrollTop + gridEl.clientHeight) gridEl.scrollTop = bottom - gridEl.clientHeight
  }

  /** @param {KeyboardEvent} e */
  function onGridKey(e) {
    if (editing) return
    if (e.key === 'ArrowDown') { e.preventDefault(); void moveSelection(1) }
    else if (e.key === 'ArrowUp') { e.preventDefault(); void moveSelection(-1) }
    else if (e.key === 'Enter' && selectedRow) { e.preventDefault(); startEdit(selectedRow, 'value') }
    else if ((e.key === 'Delete' || e.key === 'Backspace') && (checked.size || selectedRow)) { e.preventDefault(); stageDeleteChecked() }
    else if (e.key === ' ' && selectedRow) { e.preventDefault(); toggleCheck(selectedRow.key) }
  }

  /** @param {KeyboardEvent} e */
  function onPageKey(e) {
    if (!active) return
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
      e.preventDefault()
      void apply()
    } else if ((e.metaKey || e.ctrlKey) && e.key === '`') {
      e.preventDefault()
      toggleTerminalDock()
    }
  }

  // One clock for every TTL countdown on screen.
  let now = $state(Date.now())
  const anyCountdown = $derived(visible.some((r) => r.ttl_ms > 0) || (selectedRow?.ttl_ms ?? -1) > 0)
  $effect(() => {
    if (!active || !anyCountdown) return
    now = Date.now()
    const id = setInterval(() => { if (!document.hidden) now = Date.now() }, 1000)
    return () => clearInterval(id)
  })

  // ── Layout: side panel + console sizes ────────────────────────────────────
  const PANEL_KEY = 'stroke:redis:panelWidth'
  const CONSOLE_KEY = 'stroke:redis:consoleHeight'
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
  let consoleHeight = $state(Math.min(600, Math.max(120, loadNum(CONSOLE_KEY, 220))))
  let consoleOpen = $state(false)
  let searchFocused = $state(false)
  let consoleMounted = $state(false)
  $effect(() => { if (consoleOpen) consoleMounted = true })
  // The real redis-cli, pointed at the database this page is browsing.
  const terminalConnection = $derived(connection ? { ...connection, db: currentDb } : null)
  let dragStart = 0
  let dragScale = 1

  /** @type {{ col: 'key'|'type'|'ttl'|'memory'|'length', label: string, align: string }[]} */
  const SORT_COLS = [
    { col: 'key', label: 'Key', align: '' },
    { col: 'type', label: 'Type', align: '' },
    { col: 'ttl', label: 'TTL', align: 'justify-end' },
    { col: 'memory', label: 'Size', align: 'justify-end' },
    { col: 'length', label: 'Length', align: 'justify-end' },
  ]
  const GRID_COLS = 'grid-cols-[32px_minmax(200px,1.4fr)_84px_88px_76px_76px_minmax(240px,2fr)]'
  const pendingTitle = $derived(readOnly ? READ_ONLY_HINT : 'Apply staged changes in one transaction (⌘S)')
  const loadedLabel = $derived(
    dbSize == null ? `${rows.length.toLocaleString()} ${plural(rows.length, "key")}`
    : filterInput.trim() || typeFilter ? `${rows.length.toLocaleString()} matching${done ? '' : '+'}`
    : `${rows.length.toLocaleString()} of ${dbSize.toLocaleString()} ${plural(dbSize, "key")}`,
  )
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex min-h-0 flex-1 overflow-hidden bg-panel" onkeydown={onPageKey}>

  {#if view !== 'keys'}
    <RedisInsightsView view={view} active={active && view !== 'keys'} onfilter={filterTo} />
  {/if}
  <div class={cn('flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden', view !== 'keys' && 'hidden')}>
  <!-- Toolbar -->
  <div class="flex h-11 shrink-0 items-center gap-1.5 border-b border-border/40 bg-panel px-3">
    <!-- Same field as the SQL table toolbar's search: frameless soft fill,
         w-64 → w-80 while in use, shrinkable down to a floor. -->
    <div class={cn('relative flex h-8 min-w-[7.5rem] shrink items-center transition-[width] duration-200', searchFocused || filterInput ? 'w-80' : 'w-64')}>
      <Search class="pointer-events-none absolute left-2.5 size-3.5 shrink-0 text-muted-foreground" />
      <input
        type="text"
        bind:value={filterInput}
        oninput={onFilterInput}
        placeholder="Filter keys, e.g. user:* or session"
        aria-label="Filter keys"
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        onfocus={() => (searchFocused = true)}
        onblur={() => (searchFocused = false)}
        onkeydown={(e) => { if (e.key === 'Escape' && filterInput) { e.preventDefault(); e.stopPropagation(); filterInput = ''; onFilterInput() } }}
        class="field-surface h-8 w-full min-w-0 pl-8 pr-7 text-ui-sm text-foreground shadow-none outline-none placeholder:text-muted-foreground"
      />
      {#if filterInput}
        <button type="button" class="hit-area absolute right-1.5 inline-flex size-5 items-center justify-center rounded-sm text-muted-foreground transition-[background-color,color] duration-150 ease-out hover:bg-accent hover:text-foreground" aria-label="Clear filter" title="Clear filter (Esc)" onclick={() => { filterInput = ''; onFilterInput() }}>
          <X class="size-3 shrink-0" />
        </button>
      {/if}
    </div>
    <!-- Type: one compact menu instead of seven pills that ran off the edge. -->
    <DropdownMenu.Root>
      <DropdownMenu.Trigger class={cn('inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md px-2.5 text-ui-sm transition-colors', typeFilter ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent hover:text-foreground')} aria-label="Filter by type">
        {#if typeFilter && TYPE_META[typeFilter]}
          {@const G = TYPE_META[typeFilter].icon}
          <G class={cn('size-3.5 shrink-0', TYPE_META[typeFilter].color)} />{typeLabel(typeFilter)}
        {:else}All types{/if}
        <ChevronDown class="size-3.5 shrink-0 opacity-60" />
      </DropdownMenu.Trigger>
      <DropdownMenu.Content align="start" class="min-w-44">
        {#each TYPE_FILTERS as t (t.value)}
          {@const m = t.value ? TYPE_META[t.value] : null}
          <DropdownMenu.Item onSelect={() => setTypeFilter(t.value)}>
            {#if m}{@const G = m.icon}<G class={cn('size-3.5 shrink-0', m.color)} />{:else}<span class="size-3.5 shrink-0"></span>{/if}
            <span data-slot="menu-label">{t.value ? typeLabel(t.value) : 'All types'}</span>
            {#if typeFilter === t.value}<Check class="ml-auto size-3.5 shrink-0" />{/if}
          </DropdownMenu.Item>
        {/each}
      </DropdownMenu.Content>
    </DropdownMenu.Root>
    <span class="hidden min-w-0 truncate pl-1 text-ui-xs tabular-nums text-muted-foreground lg:inline" title={done ? 'Every matching key is loaded' : 'More keys load as you scroll'}>{loadedLabel}</span>

    <span class="flex-1"></span>

    {#if checked.size > 0}
      <Button variant="destructive" size="sm" onclick={stageDeleteChecked} disabled={readOnly} title={readOnly ? READ_ONLY_HINT : 'Stage deletion (Delete)'}>
        <Trash2 /> Delete {checked.size}
      </Button>
    {/if}
    {#if pendingCount > 0}
      <Button variant="ghost" size="sm" onclick={() => (reviewOpen = !reviewOpen)} aria-expanded={reviewOpen}>
        {pendingCount} pending{#if failed.length}<span class="text-destructive"> · {failed.length} failed</span>{/if}
      </Button>
      <Button variant="ghost" size="sm" onclick={discard} disabled={applying}>Discard</Button>
      <Button size="sm" onclick={() => void apply()} disabled={applying || readOnly} title={pendingTitle}>
        {#if applying}<LoaderCircle class="animate-spin" />{:else}<Check />{/if}
        Apply
      </Button>
      <span class="mx-0.5 h-4 w-px bg-border"></span>
    {/if}
    <Button variant="ghost" size="icon-sm" onclick={reload} title="Reload keys" aria-label="Reload keys">
      <RefreshCw class={cn(loading && 'animate-spin')} />
    </Button>
    <Button variant="ghost" size="icon-sm" onclick={toggleTerminalDock} aria-pressed={$terminalDockOpen} title="redis-cli (⌘J)" aria-label="Toggle redis-cli" class={cn($terminalDockOpen && 'bg-accent text-foreground')}>
      <TerminalIcon />
    </Button>
    <Button variant="outline" size="sm" onclick={openCreate} disabled={readOnly} title={readOnly ? READ_ONLY_HINT : 'New key'}>
      <Plus /> Key
    </Button>
  </div>

  <!-- Staged changes, as the commands that will run -->
  {#if reviewOpen && staged.length}
    <div class="app-scroll max-h-48 shrink-0 overflow-auto border-b border-border bg-muted/20 py-1">
      {#each staged as s (s.id)}
        <div class="group/op flex min-h-7 items-start gap-2 px-3 py-0.5">
          <div class="min-w-0 flex-1">
            {#each s.ops as args, i (i)}
              <div class="truncate font-mono text-ui-2xs text-foreground" title={commandLine(args)}>{commandLine(args)}</div>
            {/each}
            {#if s.error}<div class="font-mono text-ui-2xs text-destructive">{s.error}</div>{/if}
          </div>
          {#if !s.cell.startsWith('post:')}
            <button type="button" class="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity hover:bg-accent hover:text-foreground group-hover/op:opacity-100 focus-visible:opacity-100" aria-label="Undo this change" title="Undo" onclick={() => onunstage(s.key, s.cell)}>
              <Undo2 class="size-3.5 shrink-0" />
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    <div class="flex min-w-0 flex-1 flex-col">
      <!-- Grid -->
      <div
        bind:this={gridEl}
        bind:clientHeight={viewportH}
        class="app-scroll relative min-h-0 flex-1 overflow-auto outline-none"
        role="grid"
        aria-label="Redis keys"
        aria-rowcount={displayRows.length}
        tabindex="0"
        onscroll={onScroll}
        onkeydown={onGridKey}
      >
        <div class="min-w-[880px]">
          <!-- Header -->
          <div bind:this={headerEl} role="row" class={cn('sticky top-0 z-20 grid h-7 items-center border-b border-border bg-panel text-ui-2xs font-medium text-muted-foreground', GRID_COLS)}>
            <span class="flex justify-center" role="columnheader">
              <Checkbox checked={allChecked} onCheckedChange={toggleAll} aria-label="Select all loaded keys" />
            </span>
            {#each SORT_COLS as { col, label, align } (col)}
              <button type="button" role="columnheader" class={cn('flex h-full items-center gap-1 px-2 hover:text-foreground', align)} onclick={() => toggleSort(col)} aria-sort={sort?.col === col ? (sort.dir === 1 ? 'ascending' : 'descending') : 'none'}>
                {label}
                {#if sort?.col === col}
                  {#if sort.dir === 1}<ChevronUp class="size-3 shrink-0" />{:else}<ChevronDown class="size-3 shrink-0" />{/if}
                {/if}
              </button>
            {/each}
            <span class="px-2" role="columnheader">Value</span>
          </div>

          {#if creating}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <form
              class="sticky top-7 z-10 flex flex-wrap items-center gap-1.5 border-b border-border bg-panel px-3 py-2 shadow-sm"
              onsubmit={(e) => { e.preventDefault(); stageCreate() }}
              onkeydown={(e) => { if (e.key === 'Escape') { e.preventDefault(); creating = false } }}
            >
              <input use:focusSelect bind:value={nk.key} placeholder="key name" aria-label="Key name" spellcheck="false" autocomplete="off" class="field-surface shadow-none h-7 w-48 min-w-0 px-2 font-mono text-ui-xs text-foreground placeholder:text-muted-foreground" />
              <FieldSelect size="sm" class="w-28" bind:value={nk.type} aria-label="Key type" options={CREATABLE_TYPES.map((t) => ({ value: t, label: TYPE_META[t].label }))} />
              {#if nk.type === 'hash'}
                <input bind:value={nk.field} placeholder="field" aria-label="Field" spellcheck="false" autocomplete="off" class="field-surface shadow-none h-7 w-28 min-w-0 px-2 font-mono text-ui-xs text-foreground placeholder:text-muted-foreground" />
              {:else if nk.type === 'zset'}
                <input bind:value={nk.score} placeholder="score" aria-label="Score" inputmode="decimal" autocomplete="off" class="field-surface shadow-none h-7 w-20 min-w-0 px-2 font-mono text-ui-xs text-foreground placeholder:text-muted-foreground" />
              {/if}
              <input bind:value={nk.value} placeholder={nk.type === 'string' || nk.type === 'hash' ? 'value' : 'first element'} aria-label="Value" spellcheck="false" autocomplete="off" class="field-surface shadow-none h-7 min-w-40 flex-1 px-2 font-mono text-ui-xs text-foreground placeholder:text-muted-foreground" />
              <input bind:value={nk.ttl} placeholder="TTL, e.g. 1h" aria-label="TTL" spellcheck="false" autocomplete="off" class="field-surface shadow-none h-7 w-24 min-w-0 px-2 font-mono text-ui-xs text-foreground placeholder:text-muted-foreground" />
              <Button type="submit" size="sm" disabled={!nk.key.trim()}>Stage</Button>
              <Button type="button" variant="ghost" size="sm" onclick={() => (creating = false)}>Cancel</Button>
              {#if nkError}<p class="w-full text-ui-2xs text-destructive">{nkError}</p>{/if}
            </form>
          {/if}

          <!-- Rows (windowed) -->
          <div class="relative" style:height="{displayRows.length * rowH}px">
            {#each visible as r, i (r.key)}
              {@const m = metaFor(r.type)}
              {@const Glyph = m.icon}
              {@const st = stagedByKey.get(r.key)}
              {@const del = st?.cells.get('row')}
              {@const sKey = st?.cells.get('key')}
              {@const sTtl = st?.cells.get('ttl')}
              {@const sVal = st?.cells.get('value')}
              {@const entryEdits = st?.entries ?? false}
              {@const ttl = r.ttl_ms >= 0 ? ttlLabel(r.ttl_ms, r.at, now) : ''}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                role="row"
                tabindex="-1"
                aria-rowindex={first + i + 1}
                aria-selected={selectedKey === r.key}
                class={cn(
                  'absolute inset-x-0 grid items-center border-b border-border/40 font-mono text-ui-xs',
                  GRID_COLS,
                  del ? 'bg-destructive/5 text-destructive' : r.isNew ? 'bg-success/10' : selectedKey === r.key ? 'bg-accent' : 'hover:bg-accent/40',
                )}
                style:top="{(first + i) * rowH}px"
                style:height="{rowH}px"
                onclick={() => (selectedKey = r.key)}
              >
                <span class="flex justify-center" role="gridcell">
                  <Checkbox checked={checked.has(r.key)} onclick={(e) => e.stopPropagation()} onCheckedChange={() => toggleCheck(r.key)} aria-label={`Select ${r.key}`} />
                </span>

                <!-- Key -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <span role="gridcell" tabindex="-1" class={cn('flex h-full min-w-0 items-center gap-1.5 px-2', sKey && 'bg-warning/10')} ondblclick={() => startEdit(r, 'key')}>
                  {#if editing?.key === r.key && editing.col === 'key'}
                    <input use:focusSelect bind:value={draft} aria-label="Rename key" class="no-field-frame no-focus-ring -mx-2 h-full w-[calc(100%+1rem)] rounded-none border-0 bg-accent/70 px-2 font-mono text-ui-xs text-foreground shadow-none outline-none" onkeydown={(e) => onEditKey(e, r)} onblur={() => commitEdit(r)} />
                  {:else}
                    <span class={cn('truncate', del ? 'line-through' : 'text-foreground')} title={r.key}>{sKey?.display ?? r.key}</span>
                    {#if r.binary}<span class="shrink-0 rounded bg-muted px-1 text-ui-3xs text-muted-foreground" title="Key name contains binary bytes, shown escaped">BIN</span>{/if}
                    {#if r.isNew}<span class="shrink-0 text-ui-3xs font-medium text-success">NEW</span>{/if}
                  {/if}
                </span>

                <!-- Type -->
                <span role="gridcell" class="flex min-w-0 items-center gap-1.5 px-2">
                  <Glyph class={cn('size-3.5 shrink-0', m.color)} />
                  <span class={cn('truncate text-ui-2xs font-medium tracking-wide', m.color)}>{m.short}</span>
                </span>

                <!-- TTL -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <span role="gridcell" tabindex="-1" class={cn('flex h-full items-center justify-end px-2 tabular-nums', sTtl ? 'bg-warning/10 text-foreground' : 'text-muted-foreground')} ondblclick={() => startEdit(r, 'ttl')}>
                  {#if editing?.key === r.key && editing.col === 'ttl'}
                    <input use:focusSelect bind:value={draft} placeholder="none" aria-label="TTL" aria-invalid={!!editError} class="no-field-frame no-focus-ring -mx-2 h-full w-[calc(100%+1rem)] rounded-none border-0 bg-accent/70 px-2 text-right shadow-none outline-none font-mono text-ui-xs text-foreground" onkeydown={(e) => onEditKey(e, r)} onblur={() => commitEdit(r)} />
                  {:else if sTtl}
                    {sTtl.display || 'none'}
                  {:else}
                    {ttl || '—'}
                  {/if}
                </span>

                <span role="gridcell" class="px-2 text-right tabular-nums text-muted-foreground">{humanBytes(r.memory)}</span>
                <span role="gridcell" class="px-2 text-right tabular-nums text-muted-foreground">{r.length?.toLocaleString() ?? ''}</span>

                <!-- Value -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <span role="gridcell" tabindex="-1" class={cn('flex h-full min-w-0 items-center px-2', (sVal || entryEdits) && 'bg-warning/10')} ondblclick={() => startEdit(r, 'value')}>
                  {#if editing?.key === r.key && editing.col === 'value'}
                    <input use:focusSelect bind:value={draft} aria-label="Edit value" class="no-field-frame no-focus-ring -mx-2 h-full w-[calc(100%+1rem)] rounded-none border-0 bg-accent/70 px-2 font-mono text-ui-xs text-foreground shadow-none outline-none" onkeydown={(e) => onEditKey(e, r)} onblur={() => commitEdit(r)} />
                  {:else}
                    <span class={cn('truncate', del ? 'line-through' : 'text-foreground/80')} title={sVal?.display ?? r.preview}>{sVal?.display ?? r.preview}</span>
                  {/if}
                </span>
              </div>
            {/each}
          </div>

          {#if editError}
            <p class="sticky bottom-0 border-t border-border bg-panel px-3 py-1 text-ui-2xs text-destructive">{editError}</p>
          {/if}

          {#if error}
            <div class="px-4 py-10 text-center">
              <p class="text-ui-xs text-destructive">{error}</p>
              <Button variant="ghost" size="sm" class="mt-2" onclick={reload}>Retry</Button>
            </div>
          {:else if !loading && displayRows.length === 0}
            <div class="flex flex-col items-center gap-2 py-16 text-center">
              <KeyRound class="size-8 shrink-0 text-muted-foreground" />
              <p class="text-ui-sm text-muted-foreground">{filterInput.trim() || typeFilter ? 'No keys match' : 'This database is empty'}</p>
            </div>
          {:else if !done || loading}
            <div class="loading-reveal flex h-8 items-center justify-center gap-2 text-ui-2xs text-muted-foreground">
              {#if loading}<LoaderCircle class="size-3.5 shrink-0 animate-spin" /> Scanning…{:else}
                <button type="button" class="hover:text-foreground" onclick={() => void loadPage(false)}>Load more</button>
              {/if}
            </div>
          {/if}
        </div>
      </div>


      {#if consoleMounted}
{#if consoleOpen}
        <ResizeHandle
          axis="y"
          edge="start"
          onresizestart={() => { dragStart = consoleHeight; dragScale = getAppScale() }}
          onresize={(d) => (consoleHeight = Math.min(600, Math.max(120, dragStart + d / dragScale)))}
          onresizeend={() => saveNum(CONSOLE_KEY, consoleHeight)}
        />
        {/if}
        <!-- Stays mounted once opened, so hiding and showing it keeps the
             running redis-cli (and its scrollback) instead of restarting it. -->
        <div class={cn('relative flex shrink-0 flex-col border-t border-border/30', !consoleOpen && 'hidden')} inert={!consoleOpen || undefined} style:height="calc({consoleHeight}px * var(--app-scale, 1))">
          {#await import('./TerminalPage.svelte') then { default: TerminalPage }}
            <TerminalPage active={active && consoleOpen} connection={terminalConnection} dbType="redis" docked onclose={() => (consoleOpen = false)} />
          {/await}
        </div>
      {/if}
    </div>

    {#if selectedRow && !selectedRow.isNew}
      <ResizeHandle
        axis="x"
        edge="start"
        onresizestart={() => { dragStart = panelWidth; dragScale = getAppScale() }}
        onresize={(d) => (panelWidth = Math.min(720, Math.max(300, dragStart + d / dragScale)))}
        onresizeend={() => saveNum(PANEL_KEY, panelWidth)}
      />
      <div class="shrink-0 border-l border-border/30" style:width="calc({panelWidth}px * var(--app-scale, 1))">
        <RedisValuePanel
          row={selectedRow}
          meta={metaFor(selectedRow.type)}
          {staged}
          {now}
          reloadToken={panelReload}
          {readOnly}
          {onstage}
          {onunstage}
          onclose={() => (selectedKey = null)}
        />
      </div>
    {/if}
  </div>
  </div>
</div>
