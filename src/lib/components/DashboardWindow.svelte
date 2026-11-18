<script>
  // One dashboard in a window of its own, for a second screen: the cards and
  // nothing else. Each card runs its query on the connection the app has open,
  // refreshes on the dashboard's interval, and says in its footer whether the
  // last run was OK. Edits made in the main window's Dashboard tab show up here
  // as they're saved (the `storage` event), so this window has no editing of
  // its own.
  import { onMount, untrack } from 'svelte'
  import EChartPanel from './EChartPanel.svelte'
  import WindowControls from './WindowControls.svelte'
  import FieldSelect from './FieldSelect.svelte'
  import { revealApp } from '$lib/app-reveal.js'
  import { dashboards, switchDashboardsConnection, reloadDashboards, updateDashboard } from '$lib/stores/dashboards.js'
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

  let {
    dashboardId = '',
    connectionId = '',
  } = $props()

  const isMac = detectOs() === 'macos'

  onMount(() => {
    switchDashboardsConnection(connectionId)
    switchChartsConnection(connectionId)
    document.documentElement.classList.add('dark')
    revealApp()
    /** @param {StorageEvent} e */
    const onStorage = (e) => {
      if (!e.key) return
      if (e.key.startsWith('stroke:dashboards')) reloadDashboards()
      if (e.key.includes('chart')) reloadChartsConnection()
    }
    window.addEventListener('storage', onStorage)
    return () => window.removeEventListener('storage', onStorage)
  })

  const dash = $derived($dashboards.find((d) => d.id === dashboardId) ?? null)
  const connName = $derived(loadSavedConnections().find((c) => c.id === connectionId)?.name ?? '')
  $effect(() => { document.title = dash?.name ?? 'Dashboard' })

  // ── Live data (as the Dashboard tab runs it) ───────────────────────────────
  const runQueue = createQueue(2)
  /** @type {Record<string, { result?: any, at?: number, loading?: boolean, error?: string }>} */
  let live = $state({})
  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 15_000)
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
  /** A little taller than in the tab: the window has the room and a footer. @param {DashItem} item */
  const heightOf = (item) => Math.round((item.h ?? SIZE_H[item.size ?? 'm']) * (item.h ? 1.2 : 1)) + 44

  const REFRESH = [[0, 'Off'], [30, '30 s'], [60, '1 min'], [300, '5 min'], [900, '15 min']]
</script>

<div class="flex h-screen flex-col overflow-hidden bg-[#141414] text-[#e8e8e8]" style="color-scheme: dark">
  <!-- Title bar: the dashboard's name in the middle, the way a report is headed. -->
  <header class="relative flex h-11 shrink-0 items-center gap-2 px-3" data-tauri-drag-region>
    <div class={cn('flex items-center gap-1', isMac && 'ps-[72px]')} data-tauri-drag-region>
      <button
        type="button"
        class="flex size-7 items-center justify-center rounded-md text-[#9a9a9a] transition-colors hover:bg-white/[0.06] hover:text-white"
        title="Refresh every card"
        aria-label="Refresh every card"
        onclick={refreshAll}
      >
        <RefreshCw class={cn('size-3.5 shrink-0', anyLoading && 'animate-pulse')} />
      </button>
      {#if dash}
        <FieldSelect
          size="sm"
          class="w-40 border-0 bg-transparent text-[#c8c8c8] hover:bg-white/[0.06]"
          aria-label="Auto refresh"
          value={String(dash.refreshSec ?? 0)}
          options={REFRESH.map(([v, l]) => ({ value: String(v), label: v ? `Every ${l}` : 'Auto refresh off' }))}
          onchange={(v) => dash && updateDashboard(dash.id, { refreshSec: Number(v) })}
        />
      {/if}
    </div>
    <h1 class="pointer-events-none absolute inset-x-0 text-center text-ui-sm font-medium tracking-[0.08em] text-[#e8e8e8] uppercase" data-tauri-drag-region>
      {dash?.name ?? 'Dashboard'}
    </h1>
    <div class="flex-1" data-tauri-drag-region></div>
    {#if connName}
      <span class="flex items-center gap-2 text-ui-xs text-[#9a9a9a]" title={failed ? `${failed} card${failed === 1 ? '' : 's'} failed` : 'Every card is OK'}>
        <span class={cn('size-2 rounded-full', failed ? 'bg-[#ff5f57]' : 'bg-[#28c840]')}></span>
        {connName}
      </span>
    {/if}
    {#if !isMac}<WindowControls />{/if}
  </header>

  <main class="app-scroll min-h-0 flex-1 overflow-y-auto px-6 pt-6 pb-10 md:px-14 md:pt-10">
    {#if !dash}
      <div class="flex h-full flex-col items-center justify-center gap-2 text-center">
        <LayoutGrid class="size-6 shrink-0 text-[#6b6b6b]" />
        <p class="text-ui-sm text-[#c8c8c8]">This dashboard was deleted</p>
      </div>
    {:else if dash.items.length === 0}
      <div class="flex h-full flex-col items-center justify-center gap-2 text-center">
        <LayoutGrid class="size-6 shrink-0 text-[#6b6b6b]" />
        <p class="text-ui-sm text-[#c8c8c8]">Nothing on {dash.name} yet</p>
        <p class="text-ui-xs text-[#8a8a8a]">Add charts or saved queries from the Dashboard tab; they show up here.</p>
      </div>
    {:else}
      <div class="grid gap-7" style="grid-template-columns: repeat({dash.columns}, minmax(0, 1fr))">
        {#each dash.items as item, i (item.id)}
          {@const chart = getChart(item.chartId)}
          {@const l = live[keyOf(item)]}
          {@const view = viewOf(item)}
          {@const title = item.query?.name ?? chart?.name ?? 'Missing chart'}
          {@const color = REPORT_COLORS[i % REPORT_COLORS.length]}
          {@const status = l?.loading && !l?.result ? 'loading' : l?.error ? 'error' : l?.at ? 'ok' : sqlOf(item) ? 'loading' : 'idle'}
          <section
            class="flex flex-col overflow-hidden rounded-lg bg-[#1b1b1b] shadow-[0_1px_0_rgb(255_255_255/0.04)_inset,0_20px_40px_-24px_rgb(0_0_0/0.8)]"
            style="grid-column: span {Math.min(item.span, dash.columns)}; height: {heightOf(item)}px"
            aria-label={title}
          >
            <div class="relative min-h-0 flex-1">
              {#if l?.error}
                <div class="absolute inset-0 flex flex-col items-center justify-center gap-1.5 px-8 text-center">
                  <p class="text-ui-sm font-medium text-[#ff8a80]">Query failed</p>
                  <p class="line-clamp-3 max-w-lg text-ui-xs break-words text-[#9a9a9a]">{l.error}</p>
                </div>
              {:else if view?.kpi}
                <div class="absolute inset-0 flex flex-col items-center justify-center gap-2">
                  <span class="text-[clamp(2.5rem,6vw,4.5rem)] leading-none font-semibold tracking-tight tabular-nums" style="color: {color}">{formatKpi(view.kpi.value)}</span>
                  <span class="text-ui-xs tracking-[0.06em] text-[#9a9a9a] uppercase">{view.kpi.label}</span>
                </div>
              {:else if view?.table}
                <div class="app-scroll absolute inset-0 overflow-auto px-4 pt-3">
                  <table class="w-full border-collapse text-ui-xs">
                    <thead class="sticky top-0 bg-[#1b1b1b]">
                      <tr>
                        {#each view.table.columns as c, ci (ci)}
                          <th class="border-b border-white/10 px-3 py-2 text-left font-medium whitespace-nowrap text-[#9a9a9a]">{c}</th>
                        {/each}
                      </tr>
                    </thead>
                    <tbody>
                      {#each view.table.rows as row, ri (ri)}
                        <tr class="border-b border-white/[0.05] last:border-0">
                          {#each row as cell, ci (ci)}
                            <td class="max-w-72 truncate px-3 py-1.5 font-mono whitespace-nowrap tabular-nums">{cell === null ? 'NULL' : typeof cell === 'number' ? reportNumber(cell) : typeof cell === 'object' ? JSON.stringify(cell) : String(cell)}</td>
                          {/each}
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                </div>
              {:else if view?.option}
                <EChartPanel option={reportOption(view.option, color)} class="absolute inset-x-2 top-3 bottom-0" />
              {:else if status === 'idle'}
                <div class="absolute inset-0 flex items-center justify-center text-ui-xs text-[#8a8a8a]">
                  {chart ? 'This chart writes, so it only runs from the editor' : 'This chart was deleted'}
                </div>
              {/if}
            </div>
            <!-- Footer: state, refresh, the name, and the verdict. -->
            <footer class="grid h-11 shrink-0 grid-cols-[1fr_auto_1fr] items-center px-4">
              <div class="flex items-center gap-3">
                <span
                  class={cn(
                    'size-2.5 rounded-full',
                    status === 'ok' && 'bg-[#28c840]',
                    status === 'loading' && 'animate-pulse bg-[#f5b400]',
                    status === 'error' && 'bg-[#ff5f57]',
                    status === 'idle' && 'bg-[#4a4a4a]',
                  )}
                  role="img"
                  aria-label={status === 'ok' ? 'OK' : status === 'loading' ? 'Running' : status === 'error' ? 'Failed' : 'Not running'}
                ></span>
                {#if sqlOf(item)}
                  <button
                    type="button"
                    class="flex size-6 items-center justify-center rounded-md text-[#8a8a8a] transition-colors hover:bg-white/[0.06] hover:text-white"
                    title={l?.at ? `Refresh · updated ${agoText(l.at, now)}` : 'Refresh'}
                    aria-label="Refresh {title}"
                    onclick={() => refreshItem(item)}
                  >
                    <RefreshCw class="size-3.5 shrink-0" />
                  </button>
                {/if}
              </div>
              <p class="max-w-[40ch] truncate text-center text-ui-sm font-medium tracking-[0.06em] uppercase">{title}</p>
              <p class="justify-self-end text-ui-xs tracking-[0.04em] text-[#9a9a9a] uppercase tabular-nums">
                {status === 'ok' ? 'OK' : status === 'error' ? 'Failed' : status === 'loading' ? 'Running' : '·'}
              </p>
            </footer>
          </section>
        {/each}
      </div>
    {/if}
  </main>
</div>
