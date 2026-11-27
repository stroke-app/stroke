<script>
  // One dashboard in a window of its own, for a second screen: the cards and
  // nothing else. Each card runs its query on the connection the app has open,
  // refreshes on the dashboard's interval, and shows when it last ran. Edits
  // made in the main window's Dashboard tab show up here as they're saved (the
  // `storage` event); this window only switches dashboards, refreshes and goes
  // full screen.
  import { onMount, untrack } from 'svelte'
  import EmptyState from './EmptyState.svelte'
  import EChartPanel from './EChartPanel.svelte'
  import WindowControls from './WindowControls.svelte'
  import FieldSelect from './FieldSelect.svelte'
  import { revealApp } from '$lib/app-reveal.js'
  import { dashboards, switchDashboardsConnection, reloadDashboards, updateDashboard, findDashboardScope } from '$lib/stores/dashboards.js'
  import { savedCharts, switchChartsConnection, reloadChartsConnection } from '$lib/stores/saved-charts.js'
  import { loadSavedConnections } from '$lib/stores/connections.js'
  import { executeSql } from '$lib/api.js'
  import { isWriteSql } from '$lib/sql-write.js'
  import { cardView, queryView, createQueue, formatKpi, agoText } from '$lib/dashboard-live.js'
  import { reportOption, REPORT_COLORS, reportNumber } from '$lib/report-chart.js'
  import { detectOs } from '$lib/platform.js'
  import { cn } from '$lib/utils.js'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import LayoutGrid from '@lucide/svelte/icons/layout-grid'
  import Maximize2 from '@lucide/svelte/icons/maximize-2'
  import Minimize2 from '@lucide/svelte/icons/minimize-2'

  let {
    dashboardId = '',
    connectionId = '',
  } = $props()

  const isMac = detectOs() === 'macos'

  // svelte-ignore state_referenced_locally
  let currentId = $state(dashboardId)
  // svelte-ignore state_referenced_locally
  let scope = $state(connectionId)
  let ready = $state(false)
  let fullscreen = $state(false)
  let width = $state(1440)

  onMount(() => {
    // The dashboard is stored under the saved connection's id; fall back to
    // any connection that has it, so an older link still opens.
    scope = findDashboardScope(dashboardId, connectionId) ?? connectionId
    switchDashboardsConnection(scope)
    switchChartsConnection(scope)
    ready = true
    // A report on a dark wall: the card colours (report-chart.js) are picked
    // to read on dark, and on a light theme the KPI number was #00b4ff on
    // white, 2.3:1. The dark class alone left the light theme's palette in
    // place, because the palette follows data-theme, so both are pinned, and
    // pinned again if the app's settings are re-applied in this window.
    const pinDark = () => {
      const root = document.documentElement
      if (root.getAttribute('data-theme') !== 'dark') root.setAttribute('data-theme', 'dark')
      if (!root.classList.contains('dark')) root.classList.add('dark')
    }
    pinDark()
    const themeGuard = new MutationObserver(pinDark)
    themeGuard.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'class'] })
    revealApp()
    /** @param {StorageEvent} e */
    const onStorage = (e) => {
      if (!e.key) return
      if (e.key.startsWith('stroke:dashboards')) reloadDashboards()
      if (e.key.includes('chart')) reloadChartsConnection()
    }
    /** @param {KeyboardEvent} e */
    const onKey = (e) => {
      if (e.key === 'F11' || (e.key === 'f' && (e.metaKey || e.ctrlKey) && e.shiftKey)) {
        e.preventDefault()
        void toggleFullscreen()
      } else if (e.key === 'Escape' && fullscreen) {
        void toggleFullscreen()
      } else if (e.key === 'r' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault()
        refreshAll()
      }
    }
    window.addEventListener('storage', onStorage)
    window.addEventListener('keydown', onKey)
    return () => {
      themeGuard.disconnect()
      window.removeEventListener('storage', onStorage)
      window.removeEventListener('keydown', onKey)
    }
  })

  async function toggleFullscreen() {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      const win = getCurrentWindow()
      const next = !(await win.isFullscreen())
      await win.setFullscreen(next)
      fullscreen = next
    } catch { /* not in Tauri */ }
  }

  const dash = $derived($dashboards.find((d) => d.id === currentId) ?? null)
  const connName = $derived(loadSavedConnections().find((c) => c.id === scope)?.name ?? '')
  $effect(() => { document.title = dash?.name ?? 'Dashboard' })

  /** Fewer columns when the window is narrow, so a card is never a sliver. */
  const cols = $derived(dash ? Math.max(1, Math.min(dash.columns, width < 720 ? 1 : width < 1100 ? 2 : dash.columns)) : 1)

  // ── Live data (as the Dashboard tab runs it) ───────────────────────────────
  const runQueue = createQueue(2)
  /** @type {Record<string, { result?: any, at?: number, loading?: boolean, error?: string }>} */
  let live = $state({})
  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 5_000)
    return () => clearInterval(t)
  })

  /** @typedef {import('$lib/stores/dashboards.js').DashItem} DashItem */
  /** @param {string | null} id */
  const getChart = (id) => (id ? $savedCharts.find((c) => c.id === id) ?? null : null)
  /** @param {DashItem} item */
  const keyOf = (item) => (item.query ? `q:${item.id}` : item.chartId ?? `x:${item.id}`)
  /** @param {DashItem} item */
  function sqlOf(item) {
    const sql = item.query?.sql ?? getChart(item.chartId)?.sql ?? ''
    return sql.trim() && !isWriteSql(sql) ? sql : null
  }

  /** @param {DashItem} item */
  async function refreshItem(item) {
    const key = keyOf(item)
    const sql = sqlOf(item)
    if (!sql || live[key]?.loading) return
    live[key] = { ...live[key], loading: true, error: '' }
    try {
      const result = await runQueue(() => executeSql(sql))
      live[key] = { result, at: Date.now(), loading: false, error: '' }
    } catch (e) {
      live[key] = { ...live[key], loading: false, error: String(/** @type {any} */ (e)?.message ?? e) }
    }
  }

  function refreshAll() {
    if (!dash) return
    const seen = new Set()
    for (const item of dash.items) {
      if (seen.has(keyOf(item))) continue
      seen.add(keyOf(item))
      void refreshItem(item)
    }
  }

  $effect(() => {
    if (!dash) return
    const items = dash.items
    untrack(() => { for (const item of items) if (!live[keyOf(item)]) void refreshItem(item) })
  })
  $effect(() => {
    const sec = dash?.refreshSec ?? 0
    if (!sec) return
    const t = setInterval(() => { if (document.visibilityState === 'visible') untrack(refreshAll) }, sec * 1000)
    return () => clearInterval(t)
  })
  const anyLoading = $derived(Object.values(live).some((l) => l.loading))
  const failed = $derived(dash ? dash.items.filter((i) => live[keyOf(i)]?.error).length : 0)
  const lastAt = $derived(dash ? Math.max(0, ...dash.items.map((i) => live[keyOf(i)]?.at ?? 0)) : 0)

  /** What a card draws. @param {DashItem} item */
  function viewOf(item) {
    const r = live[keyOf(item)]?.result
    if (!r) return null
    try {
      if (item.query) return queryView(r, item.query.type, true)
      const chart = getChart(item.chartId)
      return chart ? { ...cardView(chart, r, true), table: null } : null
    } catch {
      return null
    }
  }

  const SIZE_H = { s: 240, m: 340, l: 480 }
  /** A little taller than in the tab: the window has the room. @param {DashItem} item */
  const heightOf = (item) => Math.round((item.h ?? SIZE_H[item.size ?? 'm']) * (item.h ? 1.15 : 1)) + 48

  const REFRESH = [[0, 'Off'], [30, '30 s'], [60, '1 min'], [300, '5 min'], [900, '15 min']]
  const dashOptions = $derived($dashboards.map((d) => ({ value: d.id, label: d.name })))
</script>

<svelte:window bind:innerWidth={width} />

<div class="flex h-screen flex-col overflow-hidden bg-background text-foreground" style="color-scheme: dark">
  <!-- Title bar: which dashboard, its state, and the window's controls. -->
  {#if !fullscreen}
    <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border/50 px-3" data-tauri-drag-region>
      <div class={cn('flex min-w-0 items-center gap-2', isMac && 'ps-[72px]')} data-tauri-drag-region>
        <LayoutGrid class="size-4 shrink-0 text-muted-foreground" />
        {#if dashOptions.length > 1}
          <FieldSelect
            size="sm"
            class="w-52 max-w-[40vw] border-0 bg-transparent font-medium hover:bg-muted/60"
            aria-label="Dashboard"
            value={currentId}
            options={dashOptions}
            onchange={(v) => { currentId = v; live = {} }}
          />
        {:else}
          <h1 class="truncate text-ui-sm font-medium">{dash?.name ?? 'Dashboard'}</h1>
        {/if}
        {#if connName}
          <span class="hidden items-center gap-1.5 truncate text-ui-xs text-muted-foreground sm:flex" title={failed ? `${failed} card${failed === 1 ? '' : 's'} failed` : 'Every card is OK'}>
            <span class={cn('size-1.5 shrink-0 rounded-full', failed ? 'bg-destructive' : 'bg-success')}></span>
            {connName}
          </span>
        {/if}
      </div>
      <div class="flex-1 self-stretch" data-tauri-drag-region></div>
      {#if dash}
        <span class="hidden text-ui-xs text-muted-foreground tabular-nums md:block" aria-live="polite">
          {anyLoading ? 'Updating' : lastAt ? `Updated ${agoText(lastAt, now)}` : ''}
        </span>
        <button
          type="button"
          class="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
          title="Refresh every card (Ctrl+R)"
          aria-label="Refresh every card"
          onclick={refreshAll}
        >
          <RefreshCw class={cn('size-3.5 shrink-0', anyLoading && 'animate-pulse')} />
        </button>
        <FieldSelect
          size="sm"
          class="w-44 border-0 bg-transparent text-muted-foreground hover:bg-muted/60"
          aria-label="Auto refresh"
          value={String(dash.refreshSec ?? 0)}
          options={REFRESH.map(([v, l]) => ({ value: String(v), label: v ? `Every ${l}` : 'Auto refresh off' }))}
          onchange={(v) => dash && updateDashboard(dash.id, { refreshSec: Number(v) })}
        />
        <button
          type="button"
          class="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
          title="Full screen (F11)"
          aria-label="Full screen"
          onclick={() => void toggleFullscreen()}
        >
          <Maximize2 class="size-3.5 shrink-0" />
        </button>
      {/if}
      {#if !isMac}<WindowControls />{/if}
    </header>
  {:else}
    <button
      type="button"
      class="group fixed top-3 right-3 z-10 flex h-8 items-center gap-1.5 rounded-md bg-muted/80 px-2.5 text-ui-xs text-muted-foreground opacity-40 transition-opacity hover:text-foreground hover:opacity-100 focus-visible:opacity-100"
      onclick={() => void toggleFullscreen()}
    >
      <Minimize2 class="size-3.5 shrink-0" />Exit full screen
    </button>
  {/if}

  <main class={cn('app-scroll min-h-0 flex-1 overflow-y-auto', width < 720 ? 'p-4' : 'p-6 lg:p-8')}>
    {#if ready && !dash}
      <EmptyState
        class="h-full justify-center"
        icon={LayoutGrid}
        title="This dashboard isn't here anymore"
        description="It was deleted, or it belongs to another connection."
      >
        {#if dashOptions.length}
          {#each dashOptions as o (o.value)}
            <button type="button" class="h-8 rounded-md border border-border/60 px-3 text-ui-sm transition-colors hover:bg-muted/60" onclick={() => { currentId = o.value; live = {} }}>{o.label}</button>
          {/each}
        {/if}
      </EmptyState>
    {:else if dash && dash.items.length === 0}
      <EmptyState
        class="h-full justify-center"
        icon={LayoutGrid}
        title={`Nothing on ${dash.name} yet`}
        description="Add charts or saved queries in the Dashboard tab. They show up here as you add them."
      />
    {:else if dash}
      <div class="grid gap-4 lg:gap-5" style="grid-template-columns: repeat({cols}, minmax(0, 1fr))">
        {#each dash.items as item, i (item.id)}
          {@const chart = getChart(item.chartId)}
          {@const l = live[keyOf(item)]}
          {@const view = viewOf(item)}
          {@const title = item.query?.name ?? chart?.name ?? 'Missing chart'}
          {@const color = REPORT_COLORS[i % REPORT_COLORS.length]}
          {@const status = l?.loading && !l?.result ? 'loading' : l?.error ? 'error' : l?.at ? 'ok' : sqlOf(item) ? 'loading' : 'idle'}
          <section
            class="group flex flex-col overflow-hidden rounded-lg border border-border/50 bg-card"
            style="grid-column: span {Math.min(item.span, cols)}; height: {heightOf(item)}px"
            aria-label={title}
          >
            <header class="flex h-11 shrink-0 items-center gap-2.5 px-4">
              <span
                class={cn(
                  'size-2 shrink-0 rounded-full',
                  status === 'ok' && 'bg-success',
                  status === 'loading' && 'animate-pulse bg-warning',
                  status === 'error' && 'bg-destructive',
                  status === 'idle' && 'bg-muted-foreground/40',
                )}
                role="img"
                aria-label={status === 'ok' ? 'OK' : status === 'loading' ? 'Running' : status === 'error' ? 'Failed' : 'Not running'}
              ></span>
              <h2 class="min-w-0 flex-1 truncate text-ui-sm font-medium" title={title}>{title}</h2>
              <span class="shrink-0 text-ui-2xs text-muted-foreground tabular-nums">
                {status === 'error' ? 'Failed' : l?.loading ? 'Running' : l?.at ? agoText(l.at, now) : ''}
              </span>
              {#if sqlOf(item)}
                <button
                  type="button"
                  class="flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-[opacity,colors] group-hover:opacity-100 hover:bg-muted/60 hover:text-foreground focus-visible:opacity-100"
                  title="Refresh"
                  aria-label="Refresh {title}"
                  onclick={() => refreshItem(item)}
                >
                  <RefreshCw class={cn('size-3.5 shrink-0', l?.loading && 'animate-pulse')} />
                </button>
              {/if}
            </header>
            <div class="relative min-h-0 flex-1">
              {#if l?.error}
                <div class="absolute inset-0 flex flex-col items-center justify-center gap-2 px-8 text-center">
                  <p class="text-ui-sm font-medium text-destructive">Query failed</p>
                  <p class="line-clamp-3 max-w-lg text-ui-xs break-words text-muted-foreground">{l.error}</p>
                  <button type="button" class="mt-1 h-7 rounded-md border border-border/60 px-2.5 text-ui-xs transition-colors hover:bg-muted/60" onclick={() => refreshItem(item)}>Try again</button>
                </div>
              {:else if view?.kpi}
                <div class="absolute inset-0 flex flex-col items-center justify-center gap-2 pb-4">
                  <span class="text-[clamp(2.5rem,6vw,4.5rem)] leading-none font-semibold tracking-tight tabular-nums" style="color: {color}">{formatKpi(view.kpi.value)}</span>
                  <span class="text-ui-xs text-muted-foreground">{view.kpi.label}</span>
                </div>
              {:else if view?.table}
                <div class="app-scroll absolute inset-0 overflow-auto px-2">
                  <table class="w-full border-collapse text-ui-xs">
                    <thead class="sticky top-0 bg-card">
                      <tr>
                        {#each view.table.columns as c, ci (ci)}
                          <th class="border-b border-border/60 px-3 py-2 text-left font-medium whitespace-nowrap text-muted-foreground">{c}</th>
                        {/each}
                      </tr>
                    </thead>
                    <tbody>
                      {#each view.table.rows as row, ri (ri)}
                        <tr class="border-b border-border/30 last:border-0">
                          {#each row as cell, ci (ci)}
                            <td class="max-w-72 truncate px-3 py-1.5 font-mono whitespace-nowrap tabular-nums">{cell === null ? 'NULL' : typeof cell === 'number' ? reportNumber(cell) : typeof cell === 'object' ? JSON.stringify(cell) : String(cell)}</td>
                          {/each}
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                </div>
              {:else if view?.option}
                <EChartPanel option={reportOption(view.option, color)} class="absolute inset-x-2 top-0 bottom-2" />
              {:else if status === 'loading'}
                <div class="absolute inset-x-4 top-2 bottom-4 animate-pulse rounded-md bg-muted/40" aria-hidden="true"></div>
              {:else if status === 'idle'}
                <div class="absolute inset-0 flex items-center justify-center px-6 text-center text-ui-xs text-muted-foreground">
                  {chart ? 'This chart writes, so it only runs from the editor' : 'This chart was deleted'}
                </div>
              {/if}
            </div>
          </section>
        {/each}
      </div>
    {/if}
  </main>
</div>
