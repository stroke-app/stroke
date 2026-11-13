<script>
  import { onMount } from 'svelte'
  import { cn } from '$lib/utils.js'
  import EChartPanel from './EChartPanel.svelte'
  import SearchableMenu from './SearchableMenu.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { readOnlyMode, READ_ONLY_HINT } from '$lib/stores/read-only.js'
  import { appThemeId } from '$lib/stores/settings.js'
  import { resolveCssColor } from '$lib/chart-utils.js'
  import {
    instanceVersion, instanceActivity, instanceState, instanceConfig, instanceReplication,
    instanceSetConfig,
  } from '$lib/api.js'
  import Database from '@lucide/svelte/icons/database'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import Gauge from '@lucide/svelte/icons/gauge'
  import Activity from '@lucide/svelte/icons/activity'
  import Settings from '@lucide/svelte/icons/settings-2'
  import GitBranch from '@lucide/svelte/icons/git-branch'
  import Search from '@lucide/svelte/icons/search'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import Check from '@lucide/svelte/icons/check'
  import X from '@lucide/svelte/icons/x'
  import Lock from '@lucide/svelte/icons/lock'
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw'
  import PowerOff from '@lucide/svelte/icons/power-off'
  import Layers from '@lucide/svelte/icons/layers'
  import ListFilter from '@lucide/svelte/icons/list-filter'
  import Circle from '@lucide/svelte/icons/circle'
  import CircleDot from '@lucide/svelte/icons/circle-dot'
  import CircleCheck from '@lucide/svelte/icons/circle-check'
  import CircleAlert from '@lucide/svelte/icons/circle-alert'
  import CircleX from '@lucide/svelte/icons/circle-x'

  let { active = false, connectionName = '', dbType = '' } = $props()

  const SUBTABS = [
    { id: 'activity', label: 'Activity', icon: Gauge },
    { id: 'state', label: 'State', icon: Activity },
    { id: 'config', label: 'Config', icon: Settings },
    { id: 'replication', label: 'Replication', icon: GitBranch },
  ]
  let subtab = $state('activity')
  let autoRefresh = $state(false)
  let refreshing = $state(false)
  let lastUpdated = $state(/** @type {number | null} */ (null))
  // $state.raw for every server payload: each one is replaced wholesale by its
  // refresh fn and only ever read, never mutated in place. Deep $state would
  // proxy every row object and every field - `config` alone is 350+ rows on
  // Postgres and 600+ on MySQL, and `stateData` carries the session/lock lists -
  // so the proxies cost more to build than the render they feed.
  let version = $state.raw(/** @type {any} */ (null))
  let activity = $state.raw(/** @type {any} */ (null))
  let stateData = $state.raw(/** @type {any} */ (null))
  let config = $state.raw(/** @type {any[]} */ ([]))
  let replication = $state.raw(/** @type {any} */ (null))
  let configSearch = $state('')
  let error = $state('')
  /** @type {{ t: number, sessions: any, counters: any }[]} */
  let samples = $state.raw([])
  const MAX_SAMPLES = 40
  const REFRESH_MS = 5000

  const supported = $derived(dbType === 'postgres' || dbType === 'mysql')
  const isPg = $derived((version?.engine ?? dbType) === 'postgres')

  onMount(() => { if (active && supported) void refreshAll() })

  // First-open fetch when the tab becomes active.
  let _everActive = false
  $effect(() => {
    if (active && supported && !_everActive) { _everActive = true; void refreshAll() }
  })

  // Auto-refresh loop while enabled + visible.
  $effect(() => {
    if (!autoRefresh || !active || !supported) return
    const id = setInterval(() => {
      if (typeof document !== 'undefined' && document.hidden) return
      void refreshCurrent()
    }, REFRESH_MS)
    return () => clearInterval(id)
  })

  // Re-kick once when the window becomes visible again.
  $effect(() => {
    if (!autoRefresh || !active || !supported) return
    if (typeof document === 'undefined') return
    const onVisible = () => { if (!document.hidden) void refreshCurrent() }
    document.addEventListener('visibilitychange', onVisible)
    return () => document.removeEventListener('visibilitychange', onVisible)
  })

  // 1s heartbeat so the "updated Ns ago" label stays honest without a re-fetch.
  let tick = $state(0)
  $effect(() => {
    if (!active) return
    const id = setInterval(() => {
      // Skip while backgrounded - the label resyncs on the next visible tick.
      if (typeof document !== 'undefined' && document.hidden) return
      tick++
    }, 1000)
    return () => clearInterval(id)
  })
  const agoLabel = $derived.by(() => {
    void tick
    if (!lastUpdated) return ''
    const s = Math.max(0, Math.round((Date.now() - lastUpdated) / 1000))
    if (s < 5) return 'just now'
    if (s < 60) return `${s}s ago`
    const m = Math.floor(s / 60)
    return m < 60 ? `${m}m ago` : `${Math.floor(m / 60)}h ago`
  })

  async function loadVersion() { try { version = await instanceVersion() } catch {} }
  async function refreshActivity() {
    try {
      const a = await instanceActivity(); activity = a
      samples = [...samples, { t: Date.now(), sessions: a.sessions, counters: a.counters }].slice(-MAX_SAMPLES)
    } catch (e) { error = String(e) }
  }
  async function refreshState() { try { stateData = await instanceState() } catch (e) { error = String(e) } }
  async function refreshConfig() { try { config = await instanceConfig() } catch (e) { error = String(e) } }
  async function refreshReplication() { try { replication = await instanceReplication() } catch (e) { error = String(e) } }

  async function refreshAll() {
    if (refreshing) return
    refreshing = true
    error = ''
    try {
      // All five, in parallel, on open. Deferring config until its tab is opened
      // was tried and reverted: the sub-tab bar shows a "restart pending" count
      // derived from the full settings list, and that badge exists precisely to
      // send you to a tab you would not otherwise open. The fetch is cheap now
      // that the payload is $state.raw and the list renders behind a budget.
      await Promise.all([loadVersion(), refreshActivity(), refreshState(), refreshConfig(), refreshReplication()])
      lastUpdated = Date.now()
      // Per-second rates (TPS, tuples, block I/O) are deltas between two samples,
      // so a single fetch leaves every rate stuck at 0.00 until the next tick.
      // Take a quick second activity sample on first open so the rate cards and
      // timelines populate immediately.
      if (active && samples.length < 2) {
        await new Promise((r) => setTimeout(r, 1000))
        if (active) { await refreshActivity(); lastUpdated = Date.now() }
      }
    } finally { refreshing = false }
  }
  async function refreshCurrent() {
    await refreshActivity() // always - keeps the charts flowing regardless of the visible tab
    if (subtab === 'state') await refreshState()
    else if (subtab === 'config') await refreshConfig()
    else if (subtab === 'replication') await refreshReplication()
    lastUpdated = Date.now()
  }
  /** @param {() => Promise<void>} fn */
  async function withSpinner(fn) {
    if (refreshing) return
    refreshing = true
    try { await fn(); lastUpdated = Date.now() } finally { refreshing = false }
  }

  /** Roving-tabindex arrow navigation across the sub-tabs. @param {KeyboardEvent} e */
  function onTabKeydown(e) {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return
    e.preventDefault()
    const i = SUBTABS.findIndex((t) => t.id === subtab)
    const next = SUBTABS[(i + (e.key === 'ArrowRight' ? 1 : -1) + SUBTABS.length) % SUBTABS.length]
    subtab = next.id
    const list = /** @type {HTMLElement} */ (e.currentTarget).closest('[role="tablist"]')
    requestAnimationFrame(() => /** @type {HTMLElement | null} */ (list?.querySelector(`[data-tab="${next.id}"]`))?.focus())
  }

  // ── Per-second rates from the cumulative counters ──────────────────────────
  const rateHistory = $derived.by(() => {
    const out = []
    for (let i = 1; i < samples.length; i++) {
      const a = samples[i - 1], b = samples[i]
      const dt = Math.max(0.001, (b.t - a.t) / 1000)
      const d = (/** @type {string} */ k) => Math.max(0, ((b.counters[k] ?? 0) - (a.counters[k] ?? 0)) / dt)
      const reads = d('blksRead'), hits = d('blksHit')
      out.push({
        t: b.t, commits: d('commits'), rollbacks: d('rollbacks'), tps: d('commits') + d('rollbacks'),
        tupUpdated: d('tupUpdated'), tupDeleted: d('tupDeleted'),
        reads, hits, hitRatio: (hits + reads) > 0 ? (hits / (hits + reads)) * 100 : 100,
      })
    }
    return out
  })
  const cur = $derived(rateHistory[rateHistory.length - 1] ?? null)
  const hasSeries = $derived(rateHistory.length > 1)
  /** Short 24h clock - 12h "03:42:11 PM" ticks collide on a 40-sample axis. */
  const clock = (/** @type {number} */ t) => new Date(t).toLocaleTimeString('en-GB', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' })
  const timeLabels = $derived(rateHistory.map((r) => clock(r.t)))

  const num = (/** @type {number} */ n, /** @type {number} */ dp = 2) => (n ?? 0).toLocaleString('en-US', { minimumFractionDigits: dp, maximumFractionDigits: dp })
  const big = (/** @type {number} */ n) => (n ?? 0).toLocaleString('en-US')
  /** Cumulative counters run to the billions - 1.2B reads far more easily. */
  const compact = (/** @type {number} */ n) => (n ?? 0).toLocaleString('en-US', { notation: 'compact', maximumFractionDigits: 1 })

  // ── Theme-resolved chart palette ───────────────────────────────────────────
  // The tokens are authored in oklch; echarts needs a concrete colour it can both
  // paint and parse. Resolve once per theme so charts follow the app instead of
  // the hardcoded hexes this page used to ship.
  const FALLBACK = { accent: '#6366f1', ok: '#22c55e', warn: '#f59e0b', bad: '#ef4444', axis: 'rgba(148,163,184,0.75)', grid: 'rgba(148,163,184,0.16)', panel: '#0f1115', border: 'rgba(148,163,184,0.28)', fg: '#e5e7eb' }
  let palette = $state({ ...FALLBACK, font: 'inherit' })
  $effect(() => {
    void $appThemeId // re-resolve whenever the user switches theme
    if (!active) return
    const c = (/** @type {string} */ v, /** @type {string} */ fb) => resolveCssColor(v) || fb
    palette = {
      accent: c('--primary', FALLBACK.accent),
      ok: c('--success', FALLBACK.ok),
      warn: c('--warning', FALLBACK.warn),
      bad: c('--destructive', FALLBACK.bad),
      axis: c('--muted-foreground', FALLBACK.axis),
      grid: c('--border', FALLBACK.grid),
      panel: c('--popover', FALLBACK.panel),
      border: c('--border', FALLBACK.border),
      fg: c('--foreground', FALLBACK.fg),
      font: typeof document !== 'undefined' ? getComputedStyle(document.body).fontFamily : 'inherit',
    }
  })

  // ── Chart options ──────────────────────────────────────────────────────────
  // Legends are rendered as HTML in each card header instead of by echarts: it
  // keeps chart labels on the app's type scale and hands the plot back its space.
  const axisStyle = $derived({
    axisLine: { show: false },
    axisTick: { show: false },
    axisLabel: { color: palette.axis, fontSize: 10, fontFamily: palette.font, hideOverlap: true, margin: 8 },
    splitLine: { lineStyle: { color: palette.grid, width: 1 } },
  })
  const baseChart = $derived({
    animationDuration: 240,
    animationDurationUpdate: 240,
    animationEasing: /** @type {const} */ ('cubicOut'),
    legend: { show: false },
    grid: { left: 6, right: 6, top: 10, bottom: 2, containLabel: true },
    tooltip: {
      trigger: /** @type {const} */ ('axis'),
      backgroundColor: palette.panel,
      borderColor: palette.border,
      borderWidth: 1,
      padding: [7, 10],
      textStyle: { color: palette.fg, fontSize: 11, fontFamily: palette.font },
      axisPointer: { type: /** @type {const} */ ('line'), lineStyle: { color: palette.axis, width: 1, opacity: 0.35 } },
      extraCssText: 'border-radius:10px;box-shadow:0 10px 30px rgba(0,0,0,0.22);',
    },
  })
  const line = (/** @type {string} */ name, /** @type {number[]} */ data, /** @type {string} */ color, /** @type {any} */ extra = {}) => ({
    name, type: 'line', smooth: 0.35, showSymbol: false, symbolSize: 5, data,
    lineStyle: { color, width: 1.75 }, itemStyle: { color }, ...extra,
  })
  const bar = (/** @type {string} */ name, /** @type {number[]} */ data, /** @type {string} */ color) => ({
    name, type: 'bar', data, itemStyle: { color, opacity: 0.72, borderRadius: [2, 2, 0, 0] }, barMaxWidth: 14,
  })

  const sessionsChart = $derived({
    ...baseChart,
    xAxis: { type: 'category', boundaryGap: false, data: samples.map((s) => clock(s.t)), ...axisStyle },
    yAxis: { type: 'value', minInterval: 1, ...axisStyle },
    series: [
      line('Total', samples.map((s) => s.sessions.total), palette.accent, {
        areaStyle: { color: palette.accent, opacity: 0.1 },
      }),
      line('Active', samples.map((s) => s.sessions.active), palette.ok),
      line('Idle', samples.map((s) => s.sessions.idle), palette.warn),
    ],
  })
  const txChart = $derived({
    ...baseChart,
    xAxis: { type: 'category', data: timeLabels, ...axisStyle },
    yAxis: { type: 'value', ...axisStyle },
    series: [
      bar('Commits/s', rateHistory.map((r) => +r.commits.toFixed(2)), palette.ok),
      bar('Rollbacks/s', rateHistory.map((r) => +r.rollbacks.toFixed(2)), palette.bad),
      line('TPS', rateHistory.map((r) => +r.tps.toFixed(2)), palette.accent),
    ],
  })
  const ioChart = $derived({
    ...baseChart,
    xAxis: { type: 'category', data: timeLabels, ...axisStyle },
    yAxis: [
      { type: 'value', ...axisStyle },
      { type: 'value', min: 0, max: 100, axisLabel: { ...axisStyle.axisLabel, formatter: '{value}%' }, axisLine: { show: false }, axisTick: { show: false }, splitLine: { show: false } },
    ],
    series: [
      bar('Disk reads/s', rateHistory.map((r) => +r.reads.toFixed(1)), palette.bad),
      bar('Cache hits/s', rateHistory.map((r) => +r.hits.toFixed(1)), palette.ok),
      line('Hit ratio', rateHistory.map((r) => +r.hitRatio.toFixed(1)), palette.accent, { yAxisIndex: 1 }),
    ],
  })

  // ── Health thresholds ──────────────────────────────────────────────────────
  const TONE = /** @type {Record<string, string>} */ ({
    ok: 'text-success', warn: 'text-warning', bad: 'text-destructive',
    muted: 'text-muted-foreground', plain: 'text-foreground',
  })
  const toneClass = (/** @type {string} */ t) => TONE[t] ?? TONE.muted
  /** Connection pool pressure: past 75% you start planning, past 90% you act. */
  const connTone = $derived(!activity ? 'muted' : activity.sessions.usagePct >= 90 ? 'bad' : activity.sessions.usagePct >= 75 ? 'warn' : 'ok')
  /** A healthy buffer cache serves ~99% of reads from memory. */
  const cacheTone = $derived.by(() => {
    const pct = (activity?.bufferHitRatio ?? 0) * 100
    if (!activity) return 'muted'
    return pct >= 99 ? 'ok' : pct >= 90 ? 'warn' : 'bad'
  })
  /** Rollbacks as a share of transactions - a rising ratio means failing writes. */
  const rollbackPct = $derived(cur && cur.tps > 0 ? (cur.rollbacks / cur.tps) * 100 : 0)

  // ── Config browsing + editing ──────────────────────────────────────────────
  // pg_settings categories read like "Write-Ahead Log / Archiving" - the part
  // before the slash is the group people actually think in (WAL, Memory, …).
  /** @param {any} c */
  const groupOf = (c) => (c.category || '').split('/')[0].trim()

  /** A value the server is not running with yet, or one the user has moved off the default. */
  /** @param {any} c */
  const isModified = (c) => !!c.source && c.source !== 'default' && c.source !== 'client'

  let configGroup = $state('all')
  let configStatus = $state(/** @type {'all' | 'modified' | 'restart'} */ ('all'))

  const configGroups = $derived.by(() => {
    /** @type {Map<string, number>} */
    const counts = new Map()
    for (const c of config) {
      const g = groupOf(c)
      if (g) counts.set(g, (counts.get(g) ?? 0) + 1)
    }
    return [...counts.entries()].sort((a, b) => a[0].localeCompare(b[0])).map(([id, n]) => ({ id, n }))
  })

  const modifiedCount = $derived(config.filter(isModified).length)
  const pendingRestartCount = $derived(config.filter((c) => c.pendingRestart).length)
  /** One mutually-exclusive status filter. "Restart pending" only exists when it does. */
  const statusOptions = $derived([
    { id: 'all', label: 'All', n: config.length },
    ...(isPg ? [{ id: 'modified', label: 'Modified', n: modifiedCount }] : []),
    ...(pendingRestartCount ? [{ id: 'restart', label: 'Restart pending', n: pendingRestartCount }] : []),
  ])
  // The "restart pending" option disappears once the server is restarted; don't
  // strand the list on a filter that no longer exists.
  $effect(() => { if (!statusOptions.some((o) => o.id === configStatus)) configStatus = 'all' })

  const configFiltered = $derived.by(() => {
    const q = configSearch.trim().toLowerCase()
    return config.filter((c) => {
      if (configGroup !== 'all' && groupOf(c) !== configGroup) return false
      if (configStatus === 'modified' && !isModified(c)) return false
      if (configStatus === 'restart' && !c.pendingRestart) return false
      if (!q) return true
      return (c.name + ' ' + c.category + ' ' + c.description).toLowerCase().includes(q)
    })
  })
  /** Bordered toolbar button beside the search field (Category / Filters / Refresh). */
  const cfgBtn = 'field-surface inline-flex h-9 shrink-0 items-center gap-2 px-3 text-ui-sm text-foreground/85 shadow-none outline-none transition-colors hover:bg-accent hover:text-foreground focus-visible:border-ring data-[state=open]:border-ring disabled:pointer-events-none disabled:opacity-50'
  const configFilterCount = $derived((configGroup !== 'all' ? 1 : 0) + (configStatus !== 'all' ? 1 : 0))
  const configFiltering = $derived(configGroup !== 'all' || configStatus !== 'all' || !!configSearch.trim())

  /** Browsing the unfiltered list means 350+ rows; break it into labelled
   *  category sections so it can be scanned. Filtered results stay flat - the
   *  query is already the organising principle. */
  const configSections = $derived.by(() => {
    if (configGroup !== 'all' || configSearch.trim()) return [{ group: '', rows: configFiltered }]
    /** @type {Map<string, any[]>} */
    const map = new Map()
    for (const r of configFiltered) {
      const g = groupOf(r) || 'Other'
      const bucket = map.get(g)
      if (bucket) bucket.push(r)
      else map.set(g, [r])
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0])).map(([group, rows]) => ({ group, rows }))
  })

  // ── Config render budget ───────────────────────────────────────────────────
  // The unfiltered list is 350+ rows on Postgres and 600+ on MySQL, and each row
  // is a ~20-node subtree. Building all of them the instant the Config tab opens
  // is a visible hitch for a list where about fifteen rows fit on screen.
  // `content-visibility` already skips their layout and paint; this skips their
  // construction too.
  //
  // Nothing ends up hidden: the budget refills itself during idle time until the
  // whole list is in the DOM, so ctrl-F and assistive tech still see every row a
  // moment after the tab opens. The scroll sentinel below is the safety net for
  // a user who scrolls faster than idle fills.
  const CONFIG_PAGE = 60
  let configRenderLimit = $state(CONFIG_PAGE)

  // Fill the rest of the list in idle slots, one page per slot. This is the
  // difference between "the first paint is cheap" and "the list is incomplete" -
  // only the former is wanted.
  $effect(() => {
    if (subtab !== 'config' || !active) return
    if (configRenderLimit >= configFiltered.length) return
    const ric = window.requestIdleCallback ?? ((/** @type {any} */ fn) => setTimeout(fn, 60))
    const cancel = window.cancelIdleCallback ?? clearTimeout
    const handle = ric(() => { configRenderLimit += CONFIG_PAGE }, { timeout: 500 })
    return () => cancel(handle)
  })

  // A new filter is a new list: start it at one page instead of inheriting the
  // budget the previous scroll had grown to.
  $effect(() => {
    void configGroup
    void configStatus
    void configSearch
    configRenderLimit = CONFIG_PAGE
  })

  /** `configSections`, truncated to the current budget. */
  const configSectionsShown = $derived.by(() => {
    let budget = configRenderLimit
    const out = []
    for (const sec of configSections) {
      if (budget <= 0) break
      out.push(sec.rows.length <= budget ? sec : { ...sec, rows: sec.rows.slice(0, budget) })
      budget -= sec.rows.length
    }
    return out
  })
  const configHasMore = $derived(configFiltered.length > configRenderLimit)

  /**
   * Sentinel that extends the config render budget as it approaches the viewport.
   * @type {import('svelte/action').Action<HTMLElement>}
   */
  function revealMoreConfig(node) {
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries[0].isIntersecting) return
        configRenderLimit += CONFIG_PAGE
        // Re-arm. If the sentinel is still in view after the list grew, there is
        // no intersection *change* for the observer to report, and the list
        // would stall short of the end. Re-observing redelivers immediately.
        io.unobserve(node)
        io.observe(node)
      },
      { rootMargin: '300px' },
    )
    io.observe(node)
    return { destroy: () => io.disconnect() }
  }

  function clearConfigFilters() {
    configGroup = 'all'; configStatus = 'all'; configSearch = ''
  }

  // ── Editing one setting ────────────────────────────────────────────────────
  let openSetting = $state('')
  let draft = $state('')
  let savingSetting = $state('')

  /** @param {any} row */
  function toggleSetting(row) {
    if (openSetting === row.name) { openSetting = ''; return }
    openSetting = row.name
    draft = row.value ?? ''
  }

  /** Display value: pg reports an empty string for "unset", which reads as a bug. */
  /** @param {any} row */
  const shownValue = (row) => (row.value === '' || row.value == null ? '—' : row.value)

  /** When the setting takes effect, in words. pg's `context` is jargon. */
  const CONTEXT_HELP = /** @type {Record<string, string>} */ ({
    internal: 'Fixed when the server was built',
    postmaster: 'Applies after a server restart',
    sighup: 'Applies on the next config reload',
    'superuser-backend': 'Set by a superuser at connection time',
    backend: 'Set at connection time',
    superuser: 'A superuser can change it live',
    user: 'Any session can change it live',
  })

  /**
   * Write the setting, or reset it to the server default.
   * @param {any} row
   * @param {boolean} [toDefault]
   */
  /**
   * True once the server has told us it will not take `ALTER SYSTEM` at all.
   *
   * Managed Postgres (RDS, Cloud SQL, Azure, Neon…) blocks the statement
   * outright: the parameters live in the provider's own parameter group, not in
   * `postgresql.auto.conf`. Learned from the first rejection rather than guessed
   * from the hostname, which would be wrong for a self-hosted server behind a
   * proxy and for every provider not on the list.
   */
  let configWritesBlocked = $state(false)

  /** @param {unknown} e */
  function isAlterSystemBlocked(e) {
    return /alter system .*not supported|permission denied .*alter system|must be superuser to execute alter system/i.test(String(e))
  }

  async function applySetting(row, toDefault = false) {
    if (savingSetting) return
    savingSetting = row.name
    try {
      const res = await instanceSetConfig(row.name, toDefault ? null : draft)
      if (res.requiresRestart) toast.warning('Restart required', { description: res.message, duration: 7000 })
      else toast.success(res.message)
      openSetting = ''
      configWritesBlocked = false
      await refreshConfig()
    } catch (e) {
      if (isAlterSystemBlocked(e)) {
        configWritesBlocked = true
        toast.error('This server does not allow ALTER SYSTEM', {
          description:
            'Managed Postgres keeps its settings in the provider\'s parameter group - RDS parameter groups, Cloud SQL flags, Azure server parameters. Change it there and the value shows up here.',
          duration: 10000,
        })
      } else {
        toast.error('Could not change setting', { description: String(e), duration: 8000 })
      }
    } finally {
      savingSetting = ''
    }
  }

  // ── Raw result tables (State / Replication) ────────────────────────────────
  /** @param {any[]} rows */
  function keysOf(rows) { return rows?.length ? Object.keys(rows[0]) : [] }
  /** @param {any} v */
  function cell(v) { return v === null || v === undefined ? '–' : Array.isArray(v) ? (v.length ? v.join(', ') : '–') : String(v) }

  /**
   * Columns whose every present value is a number. Those get right-aligned
   * tabular figures, so PIDs and counts line up on the decimal down the column
   * instead of ragging against a left edge like prose.
   * @param {any[]} rows @param {string[]} cols
   */
  function numericCols(rows, cols) {
    /** @type {Set<string>} */
    const out = new Set()
    for (const k of cols) {
      let seen = false
      let allNumeric = true
      for (const row of rows) {
        const v = row[k]
        if (v === null || v === undefined) continue
        seen = true
        if (typeof v !== 'number') { allNumeric = false; break }
      }
      if (seen && allNumeric) out.add(k)
    }
    return out
  }

  const ACRONYMS = new Set(['id', 'ids', 'pid', 'pids', 'db', 'ip', 'addr', 'xid', 'lsn', 'wal', 'gid', 'sql', 'os', 'tcp', 'io'])
  /** `blocking_pids` → `Blocking PIDs`. Raw catalog column names are not labels. */
  const humanize = (/** @type {string} */ k) => String(k).replace(/[_-]+/g, ' ').trim().split(' ')
    .map((w) => (ACRONYMS.has(w.toLowerCase()) ? w.toUpperCase() : w.charAt(0).toUpperCase() + w.slice(1)))
    .join(' ')

  /** Session state is the one column worth reading at a glance. */
  const stateTone = (/** @type {any} */ v) => {
    const s = String(v ?? '').toLowerCase()
    if (s === 'active' || s === 'query') return 'bg-success/12 text-success'
    if (s.includes('idle in transaction')) return 'bg-warning/12 text-warning'
    if (s === 'idle' || s === 'sleep') return 'bg-muted/50 text-muted-foreground'
    return 'bg-muted/40 text-foreground/70'
  }
  /** A session waiting on someone else's lock is the thing you opened this tab for. */
  const isBlocked = (/** @type {any} */ row) => Array.isArray(row?.blocking_pids) && row.blocking_pids.length > 0

  const blockedCount = $derived((stateData?.sessions ?? []).filter(isBlocked).length)
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden bg-panel">
  <!-- ── Header ──────────────────────────────────────────────────────────── -->
  <div class="flex shrink-0 items-center gap-4 px-6 pt-5 pb-3">
    <div class="flex min-w-0 flex-1 items-center gap-3">
      <div class="min-w-0">
        <h1 class="truncate text-ui-lg font-semibold leading-tight tracking-tight text-foreground">Instance insights</h1>
        <p class="mt-1 flex min-w-0 items-center gap-1.5 text-ui-xs text-muted-foreground">
          <span class="truncate">{connectionName || '—'}</span>
          {#if version}
            <span class="text-muted-foreground">·</span>
            <span class="shrink-0 truncate">{isPg ? 'PostgreSQL' : 'MySQL'} {version.version}</span>
          {/if}
        </p>
      </div>
    </div>

    {#if supported}
      <div class="flex shrink-0 items-center gap-2">
        {#if lastUpdated}
          <span class="hidden text-ui-2xs tabular-nums text-muted-foreground sm:inline">Updated {agoLabel}</span>
        {/if}
        <button
          type="button"
          role="switch"
          aria-checked={autoRefresh}
          title={autoRefresh ? `Refreshing every ${REFRESH_MS / 1000}s` : 'Poll the server automatically'}
          class={cn(
            'inline-flex h-7 items-center gap-1.5 rounded-md px-3 text-ui-xs transition-colors',
            'focus-visible:ring-2 focus-visible:ring-ring/18 focus-visible:outline-none',
            autoRefresh ? 'bg-success/10 text-success' : 'text-muted-foreground hover:bg-accent hover:text-foreground',
          )}
          onclick={() => (autoRefresh = !autoRefresh)}
        >
          <span class="relative flex size-1.5 shrink-0">
            {#if autoRefresh}
              <span class="live-ping absolute inline-flex size-full rounded-full bg-success opacity-70"></span>
            {/if}
            <span class={cn('relative inline-flex size-1.5 rounded-full', autoRefresh ? 'bg-success' : 'bg-muted-foreground/50')}></span>
          </span>
          Live
          {#if autoRefresh}<span class="tabular-nums opacity-70">{REFRESH_MS / 1000}s</span>{/if}
        </button>
        <button
          type="button"
          class="inline-flex h-7 items-center gap-1.5 rounded-md bg-muted/60 px-3 text-ui-xs text-foreground/85 transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/18 focus-visible:outline-none disabled:pointer-events-none disabled:opacity-50"
          disabled={refreshing}
          onclick={() => void refreshAll()}
        >
          <RefreshCw class={cn('size-3.5 shrink-0', refreshing && 'animate-spin')} />
          Refresh
        </button>
      </div>
    {/if}
  </div>

  {#if !supported}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 p-8 text-center">
      <Database class="size-5 shrink-0 text-muted-foreground" />
      <p class="text-ui-sm text-muted-foreground">Instance Insights needs a server-based engine</p>
      <p class="max-w-sm text-ui-xs text-muted-foreground">
        Live sessions, transaction rates and server configuration come from PostgreSQL and MySQL catalogs.
        Embedded engines don't expose them.
      </p>
    </div>
  {:else}
    <!-- ── Sub-tabs ──────────────────────────────────────────────────────── -->
    <div class="shrink-0 px-6 pb-1">
      <div
        role="tablist"
        aria-label="Instance sections"
        class="inline-flex items-center gap-1"
      >
        {#each SUBTABS as t (t.id)}
          <button
            type="button"
            role="tab"
            data-tab={t.id}
            aria-selected={subtab === t.id}
            tabindex={subtab === t.id ? 0 : -1}
            class={cn(
              'inline-flex h-7 items-center gap-1.5 rounded-md px-3 text-ui-xs font-medium transition-colors',
              'focus-visible:ring-2 focus-visible:ring-ring/18 focus-visible:outline-none',
              subtab === t.id ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground',
            )}
            onclick={() => (subtab = t.id)}
            onkeydown={onTabKeydown}
          >
            <t.icon class="size-3.5 shrink-0" />
            {t.label}
            {#if t.id === 'state' && blockedCount}
              <span class="ml-0.5 rounded bg-destructive/15 px-1 text-ui-3xs tabular-nums text-destructive">{blockedCount}</span>
            {:else if t.id === 'config' && pendingRestartCount}
              <span class="ml-0.5 rounded bg-warning/15 px-1 text-ui-3xs tabular-nums text-warning">{pendingRestartCount}</span>
            {/if}
          </button>
        {/each}
      </div>
    </div>

    <div class={cn('app-scroll min-h-0 flex-1 overflow-y-auto px-6 pb-6', subtab !== 'config' && 'pt-4')}>
      {#if error}
        <div class={cn('mb-4 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-ui-xs text-destructive', subtab === 'config' && 'mt-5')}>{error}</div>
      {/if}

      <!-- ══ ACTIVITY ══════════════════════════════════════════════════════ -->
      {#if subtab === 'activity'}
        {@render listHeader(Gauge, 'Overview', 4)}
        <div class="flex flex-col gap-px">
          {@render stat({
            label: 'Connections',
            help: 'Client sessions open on the server right now, against max_connections.',
            value: activity ? big(activity.sessions.total) : '—',
            unit: activity?.sessions.max ? `/ ${big(activity.sessions.max)}` : '',
            hint: activity ? `${num(activity.sessions.usagePct, 1)}% used` : '',
            tone: connTone,
            series: samples.map((s) => s.sessions.total),
            rows: [
              { k: 'Active', v: activity ? big(activity.sessions.active) : '—' },
              { k: 'Idle', v: activity ? big(activity.sessions.idle) : '—' },
            ],
            meter: activity?.sessions.max ? Math.min(100, activity.sessions.usagePct) : null,
          })}
          {@render stat({
            label: 'Transactions',
            help: 'Commits plus rollbacks per second, measured between the last two samples.',
            value: cur ? num(cur.tps) : '—',
            unit: '/s',
            hint: cur && cur.tps > 0 ? `${num(rollbackPct, 1)}% rolled back` : '',
            tone: rollbackPct >= 10 ? 'bad' : rollbackPct >= 2 ? 'warn' : 'ok',
            series: rateHistory.map((r) => r.tps),
            rows: [
              { k: 'Commits/s', v: cur ? num(cur.commits) : '—' },
              { k: 'Rollbacks/s', v: cur ? num(cur.rollbacks) : '—' },
              { k: 'Commits total', v: activity ? compact(activity.counters.commits) : '—' },
              { k: 'Rollbacks total', v: activity ? compact(activity.counters.rollbacks) : '—' },
            ],
          })}
          {@render stat({
            label: 'Row writes',
            help: 'Rows updated and deleted per second across every database on this instance.',
            value: cur ? num(cur.tupUpdated + cur.tupDeleted) : '—',
            unit: '/s',
            hint: '',
            tone: 'plain',
            series: rateHistory.map((r) => r.tupUpdated + r.tupDeleted),
            rows: [
              { k: 'Updates/s', v: cur ? num(cur.tupUpdated) : '—' },
              { k: 'Deletes/s', v: cur ? num(cur.tupDeleted) : '—' },
              { k: 'Fetched total', v: activity ? compact(activity.counters.tupFetched) : '—' },
              { k: 'Returned total', v: activity ? compact(activity.counters.tupReturned) : '—' },
            ],
          })}
          {@render stat({
            label: 'Cache hit ratio',
            help: 'Share of block reads served from memory. Below ~99% means the server is going to disk.',
            value: activity ? num(activity.bufferHitRatio * 100, 1) : '—',
            unit: '%',
            hint: cacheTone === 'ok' ? 'healthy' : cacheTone === 'warn' ? 'watch' : activity ? 'reading from disk' : '',
            tone: cacheTone,
            series: rateHistory.map((r) => r.hitRatio),
            rows: [
              { k: 'Cache hits/s', v: cur ? num(cur.hits, 1) : '—' },
              { k: 'Disk reads/s', v: cur ? num(cur.reads, 1) : '—' },
              { k: 'Hits total', v: activity ? compact(activity.counters.blksHit) : '—' },
              { k: 'Reads total', v: activity ? compact(activity.counters.blksRead) : '—' },
            ],
            meter: activity ? activity.bufferHitRatio * 100 : null,
          })}
        </div>

        {#if !autoRefresh}
          <p class="mt-2 flex items-center gap-1.5 px-3 text-ui-2xs text-muted-foreground">
            Rates are sampled per refresh - turn on <span class="font-medium text-foreground/80">Live</span> to watch them move.
          </p>
        {/if}

        <div class="mt-5">{@render listHeader(Activity, 'Trends', 3)}</div>
        <div class="grid grid-cols-1 gap-3 xl:grid-cols-2">
          {@render chartCard({
            title: 'Sessions',
            caption: 'How many clients are connected, and how many are doing work.',
            legend: [{ label: 'Total', color: palette.accent }, { label: 'Active', color: palette.ok }, { label: 'Idle', color: palette.warn }],
            option: sessionsChart,
            ready: samples.length > 1,
          })}
          {@render chartCard({
            title: 'Transactions',
            caption: 'Throughput per second. Rollback bars rising is the signal to look at.',
            legend: [{ label: 'Commits/s', color: palette.ok }, { label: 'Rollbacks/s', color: palette.bad }, { label: 'TPS', color: palette.accent }],
            option: txChart,
            ready: hasSeries,
          })}
        </div>
        <div class="mt-3">
          {@render chartCard({
            title: 'Block I/O',
            caption: 'Cache hits versus disk reads, with the resulting hit ratio on the right axis.',
            legend: [{ label: 'Cache hits/s', color: palette.ok }, { label: 'Disk reads/s', color: palette.bad }, { label: 'Hit ratio', color: palette.accent }],
            option: ioChart,
            ready: hasSeries,
          })}
        </div>

      <!-- ══ STATE ═════════════════════════════════════════════════════════ -->
      {:else if subtab === 'state'}
        {#if blockedCount}
          <div class="mb-3 flex items-center gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-ui-xs text-destructive">
            <Lock class="size-3.5 shrink-0" />
            {blockedCount} {blockedCount === 1 ? 'session is' : 'sessions are'} waiting on a lock held by another session.
          </div>
        {/if}
        {@render dataSection({
          title: 'Sessions',
          caption: 'Every client connection the server currently holds.',
          rows: stateData?.sessions ?? [],
          empty: 'No sessions reported.',
          onRefresh: () => void withSpinner(refreshState),
        })}
        <div class="mt-5 grid grid-cols-1 gap-5 lg:grid-cols-2">
          <div>
            {@render dataSection({
              title: 'Locks',
              caption: 'Locks currently held or awaited, capped at 200 rows.',
              rows: stateData?.locks ?? [],
              empty: 'Nothing is locked right now.',
            })}
          </div>
          <div>
            {@render dataSection({
              title: 'Prepared transactions',
              caption: 'Two-phase commits left open - these hold locks until resolved.',
              rows: stateData?.preparedTransactions ?? [],
              empty: 'No prepared transactions.',
            })}
          </div>
        </div>

      <!-- ══ CONFIG ════════════════════════════════════════════════════════ -->
      {:else if subtab === 'config'}
        <!-- One row, like a list app's header: a wide search, then bordered
             Category / Status menus and a square Refresh. Opaque and sticky; no
             negative vertical margin (sticky resolves against the margin box, so
             rows would show through a gap above it). -->
        <div class="sticky top-0 z-20 -mx-6 mb-2 bg-panel px-6 pb-3 pt-5">
          <div class="flex items-center gap-2">
            <div class="relative min-w-0 flex-1">
              <Search class="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
              <input
                bind:value={configSearch}
                placeholder="Search {big(config.length)} settings, names or descriptions"
                aria-label="Search settings"
                class="field-surface h-9 w-full pl-9 pr-9 text-ui-sm text-foreground shadow-none outline-none placeholder:text-muted-foreground"
                onkeydown={(e) => { if (e.key === 'Escape' && configSearch) { e.preventDefault(); e.stopPropagation(); configSearch = '' } }}
              />
              {#if configSearch}
                <button
                  type="button"
                  aria-label="Clear search"
                  class="absolute right-2 top-1/2 inline-flex size-5 -translate-y-1/2 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                  onclick={() => (configSearch = '')}
                >
                  <X class="size-3 shrink-0" />
                </button>
              {/if}
            </div>

            <SearchableMenu
              items={[{ value: 'all', label: 'All categories' }, ...configGroups.map((g) => ({ value: g.id, label: g.id }))]}
              placeholder="Filter categories…"
              empty="No matching category"
              contentClass="min-w-[260px]"
              onselect={(it) => (configGroup = it.value)}
            >
              {#snippet trigger(props)}
                <button
                  {...props}
                  type="button"
                  title={configGroup === 'all' ? 'Filter by category' : configGroup}
                  class={cn(cfgBtn, 'max-w-[14rem]', configGroup !== 'all' && 'text-foreground')}
                >
                  <Layers class="size-4 shrink-0 text-muted-foreground" />
                  <span class="min-w-0 truncate">{configGroup === 'all' ? 'Category' : configGroup}</span>
                  <ChevronDown class="size-3.5 shrink-0 text-muted-foreground" />
                </button>
              {/snippet}
              {#snippet item(it)}
                <span class="min-w-0 flex-1 truncate">{it.label}</span>
                <span class="shrink-0 text-ui-2xs tabular-nums text-muted-foreground">
                  {it.value === 'all' ? config.length : (configGroups.find((g) => g.id === it.value)?.n ?? 0)}
                </span>
                {#if it.value === configGroup}<Check class="size-3.5 shrink-0 text-primary" />{/if}
              {/snippet}
            </SearchableMenu>

            <SearchableMenu
              items={statusOptions.map((o) => ({ value: o.id, label: o.label }))}
              placeholder="Status…"
              contentClass="min-w-[220px]"
              onselect={(it) => (configStatus = /** @type {any} */ (it.value))}
            >
              {#snippet trigger(props)}
                <button {...props} type="button" class={cn(cfgBtn, configStatus !== 'all' && 'text-foreground')}>
                  <ListFilter class="size-4 shrink-0 text-muted-foreground" />
                  Filters
                  {#if configFilterCount}
                    <span class="inline-flex h-5 min-w-5 items-center justify-center rounded-sm bg-muted px-1 text-ui-2xs tabular-nums text-foreground">{configFilterCount}</span>
                  {/if}
                </button>
              {/snippet}
              {#snippet item(it)}
                {@const o = statusOptions.find((x) => x.id === it.value)}
                {#if it.value === 'restart'}<PowerOff class="size-3.5 shrink-0 text-warning" />{:else if it.value === 'modified'}<span class="grid size-3.5 shrink-0 place-items-center"><span class="size-1.5 rounded-full bg-primary"></span></span>{:else}<span class="size-3.5 shrink-0"></span>{/if}
                <span class="min-w-0 flex-1 truncate">{it.label}</span>
                <span class="shrink-0 text-ui-2xs tabular-nums text-muted-foreground">{o?.n ?? ''}</span>
                {#if it.value === configStatus}<Check class="size-3.5 shrink-0 text-primary" />{/if}
              {/snippet}
            </SearchableMenu>

            <button
              type="button"
              class={cn(cfgBtn, 'w-9 justify-center px-0')}
              disabled={refreshing}
              title="Reload settings"
              aria-label="Reload settings"
              onclick={() => void withSpinner(refreshConfig)}
            >
              <RefreshCw class={cn('size-4 shrink-0', refreshing && 'animate-spin')} />
            </button>
          </div>
          {#if configFiltering}
            <div class="mt-2 flex items-center gap-2 px-1 text-ui-2xs tabular-nums text-muted-foreground">
              <span>{big(configFiltered.length)} of {big(config.length)} settings</span>
              <button type="button" class="rounded-sm px-1 text-primary hover:underline" onclick={clearConfigFilters}>Clear filters</button>
            </div>
          {/if}
        </div>

        {#if !configFiltered.length}
          <div class="py-16 text-center">
            <p class="text-ui-sm text-muted-foreground">No settings match these filters</p>
            {#if configFiltering}
              <button type="button" class="mt-2 text-ui-xs text-primary hover:underline" onclick={clearConfigFilters}>Clear filters</button>
            {/if}
          </div>
        {:else}
          <div class="flex flex-col gap-4">
            {#each configSectionsShown as sec (sec.group || '_flat')}
              <section>
                {#if sec.group}
                  <!-- Icon, name, count, then a hairline to the edge. -->
                  <div class="mb-1 flex items-center gap-2 px-3 py-1.5">
                    <Settings class="size-4 shrink-0 text-muted-foreground" />
                    <h3 class="text-ui-sm text-muted-foreground">{sec.group}</h3>
                    <span class="text-ui-sm tabular-nums text-muted-foreground/70">{sec.rows.length}</span>
                    <span class="ml-2 h-px flex-1 bg-border/50" aria-hidden="true"></span>
                  </div>
                {/if}
                <!-- One row per setting; content-visibility keeps the ~350-row list
                     smooth by skipping layout/paint for rows that are off-screen. -->
                <!-- Hairlines, not a card. Each section was a bordered, tinted
                     panel holding rows that are themselves separated by borders,
                     inside a page that is already a panel - three nested frames
                     for one list. The heading above and the space between
                     sections carry the grouping; a rule per row is all the
                     structure the rows need. -->
                <div class="flex flex-col gap-px">
                  {#each sec.rows as row (row.name)}
                    {@const open = openSetting === row.name}
                    {@const editable = row.editable !== false && !$readOnlyMode && !configWritesBlocked}
                    {@const modified = isModified(row)}
                    <!-- A write that needs a restart leaves `source` at `default`
                         until the server comes back, so `isModified` reads false
                         and the only way back out - ALTER SYSTEM RESET, which is
                         what this button sends - was disabled exactly when it was
                         needed. A queued change you cannot cancel is a trap. -->
                    {@const canReset = modified || !!row.pendingRestart}
                    {@const resetLabel = !modified && row.pendingRestart ? 'Discard' : 'Reset'}
                    <!-- pg setting names are [a-z0-9_], so they make valid ids as-is. -->
                    {@const panelId = `cfg-panel-${row.name}`}
                    {@const fieldId = `cfg-value-${row.name}`}
                    {@const labelId = `cfg-label-${row.name}`}
                    <div class={cn('cfg-row group overflow-hidden rounded-lg transition-colors', open ? 'bg-muted/30' : 'hover:bg-muted/25')}>
                      <!-- Two lines like a list item: status glyph, name and badges
                           on top, description below; value and its source on the
                           right, stacked the same way. Ring is inset so the rounded
                           row does not clip it. -->
                      <button
                        type="button"
                        aria-expanded={open}
                        aria-controls={panelId}
                        class="flex w-full items-start gap-3 rounded-lg px-3 py-2.5 text-left outline-none focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
                        onclick={() => toggleSetting(row)}
                      >
                        <span class="mt-0.5 grid size-4 shrink-0 place-items-center" aria-hidden="true">
                          {#if row.pendingRestart}
                            <PowerOff class="size-4 text-warning" />
                          {:else if row.editable === false}
                            <Lock class="size-3.5 text-muted-foreground/60" />
                          {:else if modified}
                            <CircleDot class="size-4 text-primary" />
                          {:else}
                            <Circle class="size-4 text-muted-foreground/40" />
                          {/if}
                        </span>
                        <div class="min-w-0 flex-1">
                          <div class="flex min-w-0 items-center gap-2">
                            <span class="truncate font-mono text-ui-sm text-foreground">{row.name}</span>
                            {#if modified}
                              <span class="shrink-0 text-ui-2xs text-primary" title="Not the built-in default - set by {row.source}">Modified</span>
                            {/if}
                            {#if row.pendingRestart}
                              <span class="shrink-0 text-ui-2xs text-warning">Restart pending</span>
                            {/if}
                          </div>
                          {#if row.description}
                            <p class="mt-0.5 truncate text-ui-xs text-muted-foreground">{row.description}</p>
                          {/if}
                        </div>

                        <span class="flex max-w-[16rem] shrink-0 flex-col items-end gap-0.5 text-right">
                          <span class="flex max-w-full items-baseline gap-1">
                            <span class={cn('truncate font-mono text-ui-sm tabular-nums', modified ? 'text-primary' : 'text-foreground')}>{shownValue(row)}</span>
                            {#if row.unit}<span class="font-mono text-ui-2xs text-muted-foreground">{row.unit}</span>{/if}
                          </span>
                          <span class="truncate text-ui-xs text-muted-foreground">{row.source || row.vartype || ''}</span>
                        </span>
                        <ChevronRight class={cn('mt-0.5 size-4 shrink-0 text-muted-foreground opacity-0 transition-[transform,opacity] duration-150 group-hover:opacity-100', open && 'rotate-90 opacity-100')} />
                      </button>

                      {#if open}
                        <div id={panelId} class="pb-3 pl-10 pr-3 pt-1">
                          <div class="flex max-w-3xl flex-wrap items-end gap-2">
                            <div class="min-w-[13rem] flex-1">
                              <div class="mb-1 flex items-baseline justify-between gap-2">
                                <label
                                  id={labelId}
                                  for={row.vartype === 'bool' ? undefined : fieldId}
                                  class="text-ui-2xs font-medium text-muted-foreground"
                                >New value</label>
                                <span class="truncate font-mono text-ui-3xs text-muted-foreground">
                                  current: {shownValue(row)}{row.unit ? ` ${row.unit}` : ''}
                                </span>
                              </div>
                              {#if row.vartype === 'bool'}
                              <div role="group" aria-labelledby={labelId} class="inline-flex h-8 items-center gap-1">
                                  {#each ['on', 'off'] as v (v)}
                                    <button
                                      type="button"
                                      disabled={!editable}
                                      aria-pressed={draft === v}
                                      onclick={() => (draft = v)}
                                      class={cn('h-full rounded-md px-4 font-mono text-ui-xs transition-colors disabled:opacity-40', draft === v ? 'bg-foreground text-background' : 'bg-background/60 text-muted-foreground hover:text-foreground')}
                                    >{v}</button>
                                  {/each}
                                </div>
                              {:else if row.vartype === 'enum' && row.enumVals?.length}
                                <select
                                  id={fieldId}
                                  bind:value={draft}
                                  disabled={!editable}
                                  class="h-8 w-full rounded-lg border-0 bg-background/70 px-2.5 font-mono text-ui-xs text-foreground shadow-none outline-none focus:ring-0 disabled:opacity-40"
                                >
                                  {#each row.enumVals as v (v)}<option value={v}>{v}</option>{/each}
                                </select>
                              {:else}
                                <!-- Deliberately `type="text"`, never `type="number"`.
                                     Svelte coerces `bind:value` on a number input to a
                                     JS number, and `instance_set_config` takes an
                                     Option<String> - so every integer and real setting
                                     failed at the IPC boundary with "invalid type:
                                     floating point `0.2`, expected a string", which is
                                     most of the 364. `inputmode` still brings up the
                                     numeric keypad, and the spinner was wrong here
                                     anyway: half of pg's "integer" settings are written
                                     with a unit (`8MB`, `1min`), which a number input
                                     will not accept at all. -->
                                <input
                                  id={fieldId}
                                  bind:value={draft}
                                  disabled={!editable}
                                  type="text"
                                  inputmode={row.vartype === 'integer' ? 'numeric' : row.vartype === 'real' ? 'decimal' : 'text'}
                                  placeholder={row.bootVal || 'value'}
                                  onkeydown={(e) => {
                                    if (e.key === 'Enter' && editable && draft !== row.value) { e.preventDefault(); void applySetting(row) }
                                    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); openSetting = '' }
                                  }}
                                  class="h-8 w-full rounded-lg border-0 bg-background/70 px-3 font-mono text-ui-xs tabular-nums text-foreground shadow-none outline-none transition-colors placeholder:text-muted-foreground focus:bg-background focus:ring-0 disabled:opacity-40"
                                />
                              {/if}
                            </div>

                            <div class="flex items-center gap-1.5">
                              <Button
                                class="rounded-md"
                                disabled={!editable || savingSetting === row.name || draft === row.value}
                                title={$readOnlyMode ? READ_ONLY_HINT : undefined}
                                onclick={() => void applySetting(row)}
                              >
                                {savingSetting === row.name ? 'Applying…' : 'Apply'}
                              </Button>
                              <Button
                                variant="ghost"
                                class="rounded-md text-muted-foreground hover:text-foreground"
                                disabled={!editable || savingSetting === row.name || !canReset}
                                title={!modified && row.pendingRestart
                                  ? 'Drop the queued change - the server keeps running the value it has now'
                                  : modified
                                    ? `Reset to the server default${row.bootVal ? ` (${row.bootVal})` : ''}`
                                    : 'Already at the server default'}
                                onclick={() => void applySetting(row, true)}
                              >
                                <RotateCcw /> {resetLabel}
                              </Button>
                            </div>
                          </div>

                          <!-- Everything the server knows about this setting, as a
                               readable grid rather than one run-on mono line. -->
                          <!-- Same column as the field above it: on a wide window
                               these six pairs were strung across 1,900px, so the
                               value sat half a screen from its own label. -->
                          <dl class="mt-3 grid max-w-3xl grid-cols-1 gap-x-6 gap-y-1.5 sm:grid-cols-2">
                            {#each [['Category', row.category], ['Type', row.vartype], ['Unit', row.unit], ['Default', row.bootVal], ['Range', (row.minVal || row.maxVal) ? `${row.minVal || '−∞'} … ${row.maxVal || '∞'}` : ''], ['Source', row.source]] as [k, v] (k)}
                              {#if v}
                                <div class="flex min-w-0 items-baseline gap-2">
                                  <dt class="w-[4.5rem] shrink-0 text-ui-2xs text-muted-foreground">{k}</dt>
                                  <dd class="min-w-0 flex-1 truncate font-mono text-ui-2xs text-foreground/85" title={String(v)}>{v}</dd>
                                </div>
                              {/if}
                            {/each}
                          </dl>

                          <!-- Notes read as notes: one line of text at the size of
                               the metadata above it. They were full-width tinted
                               bands, which gave a sentence of context the weight of
                               an alert and stacked a fourth surface into the row. -->
                          {#if configWritesBlocked}
                            <p class="mt-3 flex max-w-3xl items-start gap-1.5 text-ui-2xs text-muted-foreground">
                              <Lock class="mt-px size-3 shrink-0" />
                              <span>
                                This server refuses <span class="font-mono">ALTER SYSTEM</span>, which is how managed
                                Postgres is set up - its settings live in the provider's parameter group (RDS parameter
                                groups, Cloud SQL flags, Azure server parameters). Values changed there show up here.
                              </span>
                            </p>
                          {:else if row.editable === false}
                            <p class="mt-3 max-w-3xl text-ui-2xs text-muted-foreground">
                              Compiled into the server - it can only change by rebuilding or re-initialising the cluster.
                            </p>
                          {:else if $readOnlyMode}
                            <p class="mt-3 max-w-3xl text-ui-2xs text-muted-foreground">{READ_ONLY_HINT}</p>
                          {:else if row.requiresRestart}
                            <p class="mt-3 flex max-w-3xl items-start gap-1.5 text-ui-2xs text-warning">
                              <PowerOff class="mt-px size-3 shrink-0" />
                              <span>Applying writes the value now, but the server has to restart before it takes effect.</span>
                            </p>
                          {:else if isPg}
                            <p class="mt-3 max-w-3xl text-ui-2xs text-muted-foreground">
                              Written with <span class="font-mono">ALTER SYSTEM</span> and reloaded - persists across restarts. Needs a superuser role.{row.context && CONTEXT_HELP[row.context] ? ` ${CONTEXT_HELP[row.context]}.` : ''}
                            </p>
                          {/if}
                        </div>
                      {/if}
                    </div>
                  {/each}
                </div>
              </section>
            {/each}
            {#if configHasMore}
              <!-- Extends the render budget before it reaches the viewport, so
                   the rest of the list is already built by the time you get there. -->
              <div use:revealMoreConfig class="h-6 shrink-0" aria-hidden="true"></div>
            {/if}
          </div>
        {/if}

      <!-- ══ REPLICATION ═══════════════════════════════════════════════════ -->
      {:else if subtab === 'replication'}
        {@render dataSection({
          title: 'Replicas',
          caption: 'Standbys currently streaming from this instance.',
          rows: replication?.stats ?? [],
          empty: 'No replicas are streaming from this instance.',
          onRefresh: () => void withSpinner(refreshReplication),
        })}
        <div class="mt-5">
          {@render dataSection({
            title: 'Replication slots',
            caption: 'Slots reserve WAL for a consumer - an inactive slot grows disk usage until it is dropped.',
            rows: replication?.slots ?? [],
            empty: 'No replication slots exist.',
          })}
        </div>
      {/if}
    </div>
  {/if}
</div>

<!-- ── Snippets ────────────────────────────────────────────────────────────── -->

{#snippet sparkline(/** @type {number[]} */ values, /** @type {string} */ toneClass)}
  <div class="h-7 w-full">
    {#if values.length > 1}
      {@const max = Math.max(...values)}
      {@const min = Math.min(...values)}
      {@const span = max - min || 1}
      {@const pts = values.map((v, i) => `${(i / (values.length - 1)) * 100},${25 - ((v - min) / span) * 22}`).join(' ')}
      <svg viewBox="0 0 100 28" preserveAspectRatio="none" class={cn('size-full', toneClass)} aria-hidden="true">
        <polyline
          points={pts}
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
          vector-effect="non-scaling-stroke"
          opacity="0.85"
        />
      </svg>
    {:else}
      <!-- Placeholder keeps every card the same height while samples accumulate. -->
      <svg viewBox="0 0 100 28" preserveAspectRatio="none" class="size-full text-muted-foreground" aria-hidden="true">
        <line x1="0" y1="14" x2="100" y2="14" stroke="currentColor" stroke-width="1" stroke-dasharray="3 4" vector-effect="non-scaling-stroke" />
      </svg>
    {/if}
  </div>
{/snippet}

{#snippet listHeader(/** @type {any} */ Glyph, /** @type {string} */ title, /** @type {number | string} */ n)}
  <!-- Icon, name, count, then a hairline to the edge - the Config list's header. -->
  <div class="mb-1 flex items-center gap-2 px-3 py-1.5">
    <Glyph class="size-4 shrink-0 text-muted-foreground" />
    <h3 class="text-ui-sm text-muted-foreground">{title}</h3>
    <span class="text-ui-sm tabular-nums text-muted-foreground/70">{n}</span>
    <span class="ml-2 h-px flex-1 bg-border/50" aria-hidden="true"></span>
  </div>
{/snippet}

{#snippet stat(/** @type {any} */ s)}
  <!-- One metric as a list row: status glyph; label over its breakdown; the
       trend; value over its status hint on the right. -->
  <div class="group flex items-center gap-3 rounded-lg px-3 py-2.5 transition-colors hover:bg-muted/25" title={s.help}>
    <span class="grid size-4 shrink-0 place-items-center self-start pt-0.5" aria-hidden="true">
      {#if s.tone === 'bad'}<CircleX class="size-4 text-destructive" />
      {:else if s.tone === 'warn'}<CircleAlert class="size-4 text-warning" />
      {:else if s.tone === 'ok'}<CircleCheck class="size-4 text-success" />
      {:else}<Circle class="size-4 text-muted-foreground/40" />{/if}
    </span>
    <div class="min-w-0 flex-1">
      <div class="truncate text-ui-sm text-foreground">{s.label}</div>
      <div class="mt-0.5 flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-0.5 text-ui-xs text-muted-foreground">
        {#each s.rows as r (r.k)}
          <span class="whitespace-nowrap">{r.k} <span class="font-mono tabular-nums text-foreground/75">{r.v}</span></span>
        {/each}
      </div>
    </div>
    <div class="hidden w-40 shrink-0 flex-col gap-1 sm:flex">
      {@render sparkline(s.series ?? [], toneClass(s.tone))}
      {#if s.meter != null}
        <div class="h-1 overflow-hidden rounded-full bg-muted/60">
          <div
            class={cn('h-full rounded-full transition-[width] duration-200 ease-out', s.tone === 'bad' ? 'bg-destructive' : s.tone === 'warn' ? 'bg-warning' : 'bg-success')}
            style="width: {Math.max(0, Math.min(100, s.meter))}%"
          ></div>
        </div>
      {/if}
    </div>
    <div class="flex w-28 shrink-0 flex-col items-end gap-0.5 text-right">
      <span class="flex items-baseline gap-1">
        <span class="font-mono text-ui-sm font-medium tabular-nums text-foreground">{s.value}</span>
        {#if s.unit}<span class="text-ui-2xs text-muted-foreground">{s.unit}</span>{/if}
      </span>
      <span class={cn('truncate text-ui-xs tabular-nums', s.hint ? toneClass(s.tone) : 'text-muted-foreground')}>{s.hint || ' '}</span>
    </div>
  </div>
{/snippet}

{#snippet chartCard(/** @type {any} */ c)}
  <div class="rounded-lg border border-border/40 p-4">
    <div class="mb-2 flex flex-wrap items-start justify-between gap-x-4 gap-y-1">
      <div class="min-w-0">
        <h3 class="text-ui-sm font-medium text-foreground">{c.title}</h3>
        <p class="mt-0.5 text-ui-2xs text-muted-foreground">{c.caption}</p>
      </div>
      <div class="flex shrink-0 items-center gap-3 pt-0.5">
        {#each c.legend as l (l.label)}
          <span class="inline-flex items-center gap-1.5 text-ui-2xs text-muted-foreground">
            <span class="size-1.5 shrink-0 rounded-full" style="background: {l.color}"></span>
            {l.label}
          </span>
        {/each}
      </div>
    </div>
    {#if c.ready}
      <!-- mergeUpdates: these three options are built inline above from a fixed
           set of series with no conditional keys, and are rebuilt on every
           refresh tick - exactly the case merging is for. -->
      <div class="h-52"><EChartPanel option={c.option} mergeUpdates /></div>
    {:else}
      <div class="flex h-52 flex-col items-center justify-center gap-1 text-center">
        <p class="text-ui-xs text-muted-foreground">Collecting samples…</p>
        <p class="max-w-[18rem] text-ui-2xs text-muted-foreground">
          Rates need two readings. {autoRefresh ? 'The next one lands in a few seconds.' : 'Turn on Live, or hit Refresh again.'}
        </p>
      </div>
    {/if}
  </div>
{/snippet}

{#snippet dataSection(/** @type {any} */ s)}
  <div class="mb-2 flex items-end justify-between gap-3">
    <div class="min-w-0">
      <div class="flex items-baseline gap-2">
        <h2 class="text-ui-sm font-semibold text-foreground">{s.title}</h2>
        {#if s.rows?.length}
          <span class="text-ui-2xs tabular-nums text-muted-foreground">{big(s.rows.length)}</span>
        {/if}
      </div>
      {#if s.caption}<p class="mt-0.5 text-ui-2xs text-muted-foreground">{s.caption}</p>{/if}
    </div>
    {#if s.onRefresh}
      <button
        type="button"
        class= "field-surface inline-flex h-7 shrink-0 items-center gap-1.5 px-2.5 text-ui-xs text-foreground/85 transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-50"
        disabled={refreshing}
        onclick={s.onRefresh}
      >
        <RefreshCw class={cn('size-3.5 shrink-0', refreshing && 'animate-spin')} /> Refresh
      </button>
    {/if}
  </div>
  {@render dataGrid(s.rows ?? [], s.empty ?? 'No data.')}
{/snippet}

{#snippet dataGrid(/** @type {any[]} */ rows, /** @type {string} */ empty)}
  {#if !rows || rows.length === 0}
    <div class="rounded-lg border border-dashed border-border/40 py-8 text-center text-ui-xs text-muted-foreground">{empty}</div>
  {:else}
    <!-- Column list computed ONCE per grid. It used to be re-derived inside the
         row loop, so a 200-session table allocated 200 throwaway Object.keys()
         arrays on every render of this page. -->
    {@const cols = keysOf(rows)}
    {@const numeric = numericCols(rows, cols)}
    <!-- `insight-grid` (see <style>) does two things the shared .app-scroll
         cannot: it lets a wheel that runs out of table keep scrolling the page,
         and it gives the horizontal bar enough height to read as an affordance.
         No height cap: a table that scrolled vertically inside the page
         scroller trapped the wheel (two scrollbars, one list). The page owns
         vertical scrolling; a table only scrolls sideways. -->
    <div class="app-scroll insight-grid overflow-x-auto overflow-y-hidden rounded-lg border border-border/40">
      <!-- w-max lets every column take its natural width and the container do the
           scrolling. Under w-full the browser squeezed columns to fit and then
           overflowed anyway, so the widths were wrong AND the tail was cut. -->
      <table class="w-max min-w-full border-collapse text-ui-2xs">
        <thead>
          <tr>
            <th class="w-8 whitespace-nowrap border-b border-border/40 bg-muted/20 px-2 py-2 text-right text-ui-2xs font-medium text-muted-foreground">#</th>
            {#each cols as k (k)}
              <th
                class={cn(
                  'whitespace-nowrap border-b border-border/40 bg-muted/20 px-3 py-2 text-ui-2xs font-medium text-muted-foreground',
                  numeric.has(k) ? 'text-right' : 'text-left',
                )}
              >{humanize(k)}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each rows as row, i (i)}
            <tr class={cn('transition-colors hover:bg-muted/25 [&:last-child>td]:border-b-0', isBlocked(row) && 'bg-destructive/[0.06]')}>
              <td class="border-b border-border/15 px-2 py-2 text-right tabular-nums text-muted-foreground/70">{i + 1}</td>
              {#each cols as k (k)}
                {@const raw = row[k]}
                {@const text = cell(raw)}
                <td
                  class={cn(
                    'max-w-[280px] truncate border-b border-border/15 px-3 py-2 font-mono text-foreground/85',
                    numeric.has(k) && 'text-right tabular-nums',
                  )}
                  title={text.length > 32 ? text : undefined}
                >
                  {#if k === 'state' || k === 'command'}
                    {#if raw}
                      <span class={cn('inline-flex items-center rounded px-1.5 py-px font-sans text-ui-3xs', stateTone(raw))}>{raw}</span>
                    {:else}
                      <span class="text-muted-foreground">–</span>
                    {/if}
                  {:else if typeof raw === 'boolean'}
                    <!-- A column of identical-weight true/false is unreadable at a
                         glance. Only the true half carries information here, so
                         false recedes to the muted tone. -->
                    <span class={raw ? 'text-foreground/85' : 'text-muted-foreground/70'}>{raw}</span>
                  {:else}
                    {text}
                  {/if}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
{/snippet}

<style>
  /* Inline data tables inside the page scroller.
     app.css already carves out horizontal-only scrollers from the blanket
     `overscroll-behavior: contain`, for the reason documented there: a contained
     box swallows every wheel tick under the pointer. These grids scroll on both
     axes, so they missed that carve-out and hit the bug anyway - a wheel over a
     table stopped dead at its last row instead of carrying on down the page.
     Same split as the horizontal case: the page never scrolls sideways, so x
     stays contained, and y chains once the table is out of rows.

     The shared bar is 4px, which on a table that scrolls sideways is not enough
     to read as "there is more to the right". 8px with a visible resting thumb is
     the affordance, since a column cut off at the right edge is otherwise the
     only cue that anything is hidden. */
  .insight-grid {
    overscroll-behavior-x: contain;
    overscroll-behavior-y: auto;
  }
  .insight-grid::-webkit-scrollbar {
    width: 8px;
    height: 8px;
  }
  .insight-grid::-webkit-scrollbar-thumb {
    background-color: color-mix(in oklch, var(--muted-foreground) 32%, transparent);
    border-radius: 9999px;
  }
  .insight-grid::-webkit-scrollbar-thumb:hover {
    background-color: color-mix(in oklch, var(--muted-foreground) 52%, transparent);
  }

  /* Config list perf: content-visibility lets the engine skip layout/paint for
     off-screen rows, so scrolling the full ~350-row pg_settings list stays smooth.
     The intrinsic size is the collapsed row height - `auto` lets the engine
     remember the real height of an expanded row once it has measured it. */
  .cfg-row {
    content-visibility: auto;
    contain-intrinsic-size: auto 48px;
  }

  /* Live indicator: a slow, low-contrast pulse that reads as "polling" without
     pulling the eye away from the data. Held to one ring so it stays quiet. */
  .live-ping {
    animation: live-ping 2s cubic-bezier(0, 0, 0.2, 1) infinite;
  }
  @keyframes live-ping {
    0%   { transform: scale(1);   opacity: 0.7; }
    70%  { transform: scale(2.4); opacity: 0; }
    100% { transform: scale(2.4); opacity: 0; }
  }
  @media (prefers-reduced-motion: reduce) {
    .live-ping { animation: none; }
  }
</style>
