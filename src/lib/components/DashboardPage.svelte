<script>
  import { onDestroy, tick, untrack } from "svelte";
  import { createSwapy } from "swapy";
  import EChartPanel from "./EChartPanel.svelte";
  import AiChartRenderer from "./AiChartRenderer.svelte";
  import { savedCharts } from "$lib/stores/saved-charts.js";
  import { isCurrentThemeDark } from "$lib/stores/settings.js";
  import { restyleSavedOption } from "$lib/chart-utils.js";
  import {
    dashboards,
    activeDashboardId,
    createDashboard,
    deleteDashboard,
    updateDashboard,
    addChartToDashboard,
    removeChartFromDashboard,
    setItemSpan,
    setItemSize,
    reorderItems,
  } from "$lib/stores/dashboards.js";
  import { executeSql } from "$lib/api.js";
  import { isWriteSql } from "$lib/sql-write.js";
  import { cardView, createQueue, agoText, formatKpi } from "$lib/dashboard-live.js";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import SearchIcon from "@lucide/svelte/icons/search";
  import Loader2 from "@lucide/svelte/icons/loader-2";
  import { cn } from "$lib/utils.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import BarChart2 from "@lucide/svelte/icons/bar-chart-2";
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import Grip from "@lucide/svelte/icons/grip";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Check from "@lucide/svelte/icons/check";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import Presentation from "@lucide/svelte/icons/presentation";
  import Minimize2 from "@lucide/svelte/icons/minimize-2";
  import MoreHorizontal from "@lucide/svelte/icons/more-horizontal";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import SelectMenu from "./SelectMenu.svelte";

  let {
    /** @type {import('$lib/stores/connections.js').SavedConnection | null} */
    connection = null,
    /** Open the SQL editor - where charts are made - from the empty picker. @type {(() => void) | null} */
    onopensqleditor = null,
    /** Open a chart's query in a new editor tab. @type {((sql: string, title: string) => void) | null} */
    onopensql = null,
    /** The dashboard tab is the one on screen: only then do queries run. */
    active = true,
  } = $props();

  // ── Live data ──────────────────────────────────────────────────────────────
  // Each card runs its saved chart's query again (read-only queries only), two
  // at a time, and keeps the result here by chart id. Cards fall back to the
  // snapshot taken when the chart was saved until their first run lands, or if
  // the query writes, fails, or there is none.
  const runQueue = createQueue(2);
  /** @type {Record<string, { result?: any, at?: number, loading?: boolean, error?: string }>} */
  let live = $state({});
  let now = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 15_000);
    return () => clearInterval(t);
  });

  /** @param {import('$lib/stores/saved-charts.js').SavedChart | null} chart */
  const canRun = (chart) => !!chart?.sql?.trim() && !isWriteSql(chart.sql);

  /** @param {string} chartId */
  async function refreshChart(chartId) {
    const chart = getChart(chartId);
    if (!canRun(chart) || live[chartId]?.loading) return;
    live[chartId] = { ...live[chartId], loading: true, error: "" };
    try {
      const result = await runQueue(() => executeSql(/** @type {string} */ (chart?.sql)));
      live[chartId] = { result, at: Date.now(), loading: false, error: "" };
    } catch (e) {
      live[chartId] = { ...live[chartId], loading: false, error: String(/** @type {any} */ (e)?.message ?? e) };
    }
  }

  function refreshAll() {
    if (!activeDash) return;
    for (const id of new Set(activeDash.items.map((i) => i.chartId).filter(Boolean))) void refreshChart(/** @type {string} */ (id));
  }

  // First run for cards that have none yet, whenever the dashboard is on screen.
  $effect(() => {
    if (!active || !activeDash) return;
    const ids = activeDash.items.map((i) => i.chartId).filter(Boolean);
    untrack(() => { for (const id of ids) if (!live[/** @type {string} */ (id)]) void refreshChart(/** @type {string} */ (id)); });
  });
  // Auto refresh, only while the tab and the window are in front.
  $effect(() => {
    const sec = activeDash?.refreshSec ?? 0;
    if (!active || !sec) return;
    const t = setInterval(() => { if (document.visibilityState === "visible") untrack(refreshAll); }, sec * 1000);
    return () => clearInterval(t);
  });
  const REFRESH_CHOICES = [[0, "Off"], [30, "30 s"], [60, "1 min"], [300, "5 min"], [900, "15 min"]];
  const anyLoading = $derived(Object.values(live).some((l) => l.loading));

  /** What a card draws: its live result, or the saved snapshot. @param {import('$lib/stores/saved-charts.js').SavedChart | null} chart */
  function viewFor(chart) {
    const l = chart ? live[chart.id] : null;
    if (chart && l?.result) {
      try { return cardView(chart, l.result, $isCurrentThemeDark); } catch { /* fall back to the snapshot */ }
    }
    return null;
  }

  const SIZE_H = { s: 200, m: 280, l: 400 };

  /** A chart option shrunk to a thumbnail: the marks only, no axes, labels or tooltip. @param {any} o */
  function thumbOption(o) {
    if (!o) return o;
    const strip = (/** @type {any} */ a) => (Array.isArray(a) ? a.map((x) => ({ ...x, show: false })) : a ? { ...a, show: false } : a);
    return {
      ...o,
      animation: false,
      tooltip: { show: false },
      legend: { show: false },
      title: { show: false },
      grid: { left: 2, right: 2, top: 4, bottom: 2, containLabel: false },
      xAxis: strip(o.xAxis),
      yAxis: strip(o.yAxis),
      series: (Array.isArray(o.series) ? o.series : o.series ? [o.series] : []).map((/** @type {any} */ x) => ({ ...x, label: { show: false }, symbol: "none", radius: x.type === "pie" ? ["0%", "85%"] : x.radius })),
    };
  }

  // ── Active dashboard ───────────────────────────────────────────────────────
  const activeDash = $derived(
    $dashboards.find((d) => d.id === $activeDashboardId) ?? null,
  );

  // ── View-only (presentation) mode ──────────────────────────────────────────
  // Strips every control - header, drag handles, span pickers, remove buttons -
  // so the tab shows nothing but the charts. Persisted so a dashboard left on a
  // wall display comes back up clean after a restart.
  const VIEW_ONLY_KEY = "stroke:dashboard:view-only";

  let viewOnly = $state(
    (() => {
      try {
        return localStorage.getItem(VIEW_ONLY_KEY) === "1";
      } catch {
        return false;
      }
    })(),
  );

  /** @param {boolean} next */
  function setViewOnly(next) {
    viewOnly = next;
    try {
      localStorage.setItem(VIEW_ONLY_KEY, next ? "1" : "0");
    } catch {
      /* private mode / quota - the mode still applies for this session */
    }
  }

  // Only actually present when there is something to present; otherwise the
  // empty states would be unreachable (no header, no create button).
  const presenting = $derived(viewOnly && (activeDash?.items.length ?? 0) > 0);

  /** @param {KeyboardEvent} e */
  function handleWindowKeydown(e) {
    if (e.key === "Escape" && presenting) {
      e.preventDefault();
      setViewOnly(false);
    }
  }

  // ── Chart picker modal ─────────────────────────────────────────────────────
  let pickerOpen = $state(false);

  let pickerQuery = $state("");
  let pickerGroup = $state("");
  $effect(() => { if (!pickerOpen) { pickerQuery = ""; pickerGroup = ""; } });
  const pickerGroups = $derived([...new Set($savedCharts.map((c) => c.group).filter(Boolean))]);
  const pickerCharts = $derived(
    $savedCharts.filter((c) => {
      const q = pickerQuery.trim().toLowerCase();
      return (!pickerGroup || c.group === pickerGroup) && (!q || c.name.toLowerCase().includes(q) || (c.group ?? "").toLowerCase().includes(q) || (c.config?.type ?? "").includes(q));
    }),
  );
  /** How many times a chart is on this dashboard. @param {string} chartId */
  const onDash = (chartId) => activeDash?.items.filter((i) => i.chartId === chartId).length ?? 0;

  /** Add it, or take it off when it's already there. The picker stays open. @param {string} chartId */
  function pickChart(chartId) {
    if (!activeDash) return;
    const there = activeDash.items.find((i) => i.chartId === chartId);
    if (there) removeChartFromDashboard(activeDash.id, there.id);
    else addChartToDashboard(activeDash.id, chartId);
  }

  // ── Dashboard CRUD ─────────────────────────────────────────────────────────
  let namingOpen = $state(false);
  let nameInput = $state("");
  let editingName = $state(false);
  let editNameInput = $state("");

  function handleCreateDashboard() {
    const n = nameInput.trim() || "Dashboard";
    createDashboard(n);
    nameInput = "";
    namingOpen = false;
  }

  /** A new dashboard, named and ready to rename in place. */
  function newDashboard() {
    const d = createDashboard(`Dashboard ${$dashboards.length + 1}`);
    if (d?.id) activeDashboardId.set(d.id);
    void tick().then(() => startEditName());
  }

  /** Focus and select a field as it appears. @param {HTMLInputElement} el */
  function focusSelect(el) {
    queueMicrotask(() => { el.focus(); el.select(); });
  }

  function startEditName() {
    if (!activeDash) return;
    editNameInput = activeDash.name;
    editingName = true;
  }

  function commitEditName() {
    if (activeDash && editNameInput.trim()) {
      updateDashboard(activeDash.id, { name: editNameInput.trim() });
    }
    editingName = false;
  }

  let dashDropdownOpen = $state(false);

  // ── Column toggle ──────────────────────────────────────────────────────────
  /** @type {(2|3)[]} */
  const COL_OPTIONS = [2, 3];

  function cycleColumns() {
    if (!activeDash) return;
    const idx = COL_OPTIONS.indexOf(activeDash.columns);
    const next = COL_OPTIONS[(idx + 1) % COL_OPTIONS.length];
    updateDashboard(activeDash.id, { columns: next });
  }

  // ── Swapy ─────────────────────────────────────────────────────────────────
  /** @type {HTMLDivElement | null} */
  let gridEl = $state(null);
  /** @type {ReturnType<typeof createSwapy> | null} */
  let swapyInstance = null;

  /**
   * A stable string that captures every layout-affecting piece of state.
   * Changing any of: active dashboard, column count, item order, item spans
   * produces a different key and triggers Swapy re-init.
   */
  const layoutKey = $derived(
    activeDash
      ? `${activeDash.id}|${activeDash.columns}|${presenting}|${activeDash.items
          .map((i) => `${i.id}:${Math.min(i.span, activeDash.columns)}`)
          .join(",")}`
      : "none",
  );

  $effect(() => {
    // Subscribe to layoutKey so Svelte tracks it.
    const _key = layoutKey;
    let cancelled = false;

    // Destroy synchronously first so Swapy releases the DOM before the
    // next tick re-render.
    swapyInstance?.destroy();
    swapyInstance = null;

    void (async () => {
      await tick(); // wait for Svelte to reconcile the DOM
      // No drag-to-reorder in view-only mode - there are no handles to grab.
      if (cancelled || presenting || !gridEl || !activeDash?.items.length) return;

      const grid = gridEl;
      const dash = activeDash;

      swapyInstance = createSwapy(grid, {
        animation: "dynamic",
        swapMode: "hover",
      });

      swapyInstance.onSwapEnd(() => {
        // Read DOM order after swap and persist to store.
        // The store update changes layoutKey → this effect re-runs → Swapy re-inits.
        const newOrder = Array.from(
          grid.querySelectorAll("[data-swapy-item]"),
        )
          .map((el) => el.getAttribute("data-swapy-item"))
          .filter(/** @param {string|null} id */ (id) => id !== null);
        reorderItems(dash.id, /** @type {string[]} */ (newOrder));
      });
    })();

    return () => {
      cancelled = true;
      swapyInstance?.destroy();
      swapyInstance = null;
    };
  });

  onDestroy(() => {
    swapyInstance?.destroy();
    if (confirmTimer) clearTimeout(confirmTimer);
  });

  // ── Grid layout ────────────────────────────────────────────────────────────
  // Fixed column count so Swapy can predict slot positions reliably.
  const gridStyle = $derived(
    `grid-template-columns: repeat(${activeDash?.columns ?? 3}, 1fr)`,
  );

  /**
   * Clamp span to the current column count so items never overflow the grid.
   * @param {import('$lib/stores/dashboards.js').DashItem} item
   */
  function effectiveSpan(item) {
    return Math.min(item.span, activeDash?.columns ?? 3);
  }

  // ── Chart lookup ───────────────────────────────────────────────────────────
  /** @param {string | null} chartId */
  function getChart(chartId) {
    return chartId ? ($savedCharts.find((c) => c.id === chartId) ?? null) : null;
  }

  // ── Delete active dashboard ────────────────────────────────────────────────
  let confirmDeleteDash = $state(false);
  /** @type {ReturnType<typeof setTimeout> | null} */
  let confirmTimer = null;

  function handleDeleteDashboard() {
    if (!activeDash) return;
    if (confirmDeleteDash) {
      deleteDashboard(activeDash.id);
      confirmDeleteDash = false;
    } else {
      confirmDeleteDash = true;
      if (confirmTimer) clearTimeout(confirmTimer);
      confirmTimer = setTimeout(() => { confirmDeleteDash = false; }, 3000);
    }
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

{#if !connection}
  <div class="flex min-h-0 flex-1 items-center justify-center bg-panel">
    <div class="flex flex-col items-center gap-2 text-center">
      <LayoutGrid class="size-8 shrink-0 text-muted-foreground" />
      <p class="text-ui-sm text-muted-foreground">Connect to a database to view dashboards</p>
    </div>
  </div>
{:else}
<div class="group/dash relative flex min-h-0 flex-1 flex-col overflow-hidden">
  {#if presenting}
    <!-- Only affordance left on screen: a corner pill that fades in on hover.
         In the dashboard's own corner: `fixed` to the window put it over
         the sidebar whenever the sidebar was on the right. -->
    <button
      type="button"
      class="absolute right-4 top-3 z-30 inline-flex h-8 items-center gap-1.5 rounded-md bg-sidebar-accent/80 px-3 text-ui-xs text-muted-foreground opacity-0 transition-opacity duration-150 hover:text-foreground focus-visible:opacity-100 group-hover/dash:opacity-100"
      title="Exit view only (Esc)"
      onclick={() => setViewOnly(false)}
    >
      <Minimize2 class="size-3.5 shrink-0" />
      Exit
    </button>
  {/if}
  <!-- Header -->
  {#if !presenting}
  <!-- Toolbar: which dashboard (searchable), New, one menu for the rest,
       then View only and Add chart. -->
  <div
    class="studio-chrome flex h-12 shrink-0 items-center gap-2 border-b border-border/40 bg-panel px-4"
    data-studio-chrome
  >
    {#if editingName && activeDash}
      <input
        type="text"
        bind:value={editNameInput}
        aria-label="Dashboard name"
        class="no-field-frame h-8 w-56 rounded-lg border-0 bg-muted/60 px-2.5 text-ui-sm font-medium text-foreground outline-none focus:bg-muted/80"
        onblur={commitEditName}
        onkeydown={(e) => {
          if (e.key === "Enter") commitEditName();
          if (e.key === "Escape") editingName = false;
        }}
        use:focusSelect
      />
    {:else if $dashboards.length}
      <SelectMenu
        ariaLabel="Dashboard"
        triggerClass="h-8 w-56"
        contentClass="w-[var(--bits-popover-anchor-width)] min-w-[14rem]"
        searchPlaceholder="Search dashboards"
        value={$activeDashboardId ?? ""}
        onValueChange={(v) => activeDashboardId.set(v)}
        items={$dashboards.map((d) => ({ value: d.id, label: d.name, icon: "layout-grid", hint: `${d.items.length}`, keywords: [d.name] }))}
      />
    {/if}
    <Button variant="outline" size="sm" class="gap-1.5" onclick={newDashboard}>
      <Plus class="size-3.5 shrink-0" />New
    </Button>
    {#if activeDash}
      <DropdownMenu.DropdownMenu>
        <DropdownMenu.DropdownMenuTrigger class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground data-[state=open]:bg-accent" aria-label="Dashboard actions">
          <MoreHorizontal class="size-4 shrink-0" />
        </DropdownMenu.DropdownMenuTrigger>
        <DropdownMenu.DropdownMenuContent align="start" class="min-w-48">
          <DropdownMenu.DropdownMenuItem onclick={startEditName}><Pencil class="size-3.5 shrink-0" />Rename</DropdownMenu.DropdownMenuItem>
          <DropdownMenu.DropdownMenuSub>
            <DropdownMenu.DropdownMenuSubTrigger><LayoutGrid class="size-3.5 shrink-0" />Columns</DropdownMenu.DropdownMenuSubTrigger>
            <DropdownMenu.DropdownMenuSubContent class="min-w-32">
              {#each [1, 2, 3, 4] as cols (cols)}
                <DropdownMenu.DropdownMenuItem onclick={() => updateDashboard(activeDash.id, { columns: cols })}>
                  <span class="flex-1">{cols} {cols === 1 ? "column" : "columns"}</span>
                  {#if activeDash.columns === cols}<Check class="size-3.5 shrink-0 text-primary" />{/if}
                </DropdownMenu.DropdownMenuItem>
              {/each}
            </DropdownMenu.DropdownMenuSubContent>
          </DropdownMenu.DropdownMenuSub>
          <DropdownMenu.DropdownMenuSeparator />
          <DropdownMenu.DropdownMenuItem variant="destructive" onclick={() => { if (activeDash) deleteDashboard(activeDash.id); }}><Trash2 class="size-3.5 shrink-0" />Delete dashboard</DropdownMenu.DropdownMenuItem>
        </DropdownMenu.DropdownMenuContent>
      </DropdownMenu.DropdownMenu>
    {/if}

    <div class="ml-auto flex items-center gap-1.5">
      {#if activeDash}
        {#if activeDash.items.length > 0}
          <div class="flex h-8 items-center rounded-md border border-border/60">
            <button type="button" class="inline-flex h-full items-center gap-1.5 rounded-s-md px-2.5 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" title="Run every chart's query again" onclick={refreshAll}>
              <RefreshCw class={cn("size-3.5 shrink-0", anyLoading && "animate-spin")} />Refresh
            </button>
            <span class="h-4 w-px bg-border/60"></span>
            <SelectMenu
              ariaLabel="Auto refresh"
              triggerClass="h-full w-auto gap-1 rounded-none rounded-e-md border-0 bg-transparent px-2 text-muted-foreground hover:text-foreground"
              contentClass="w-40"
              searchThreshold={99}
              value={String(activeDash.refreshSec ?? 0)}
              onValueChange={(v) => updateDashboard(activeDash.id, { refreshSec: Number(v) })}
              items={REFRESH_CHOICES.map(([v, l]) => ({ value: String(v), label: v ? `Every ${l}` : "Auto off" }))}
            />
          </div>
          <Button variant="ghost" size="sm" class="gap-1.5 text-muted-foreground hover:text-foreground" title="View only: hide every control (Esc to exit)" onclick={() => setViewOnly(true)}>
            <Presentation class="size-3.5 shrink-0" />View only
          </Button>
        {/if}
        <Button size="sm" class="gap-1.5" onclick={() => (pickerOpen = true)}>
          <Plus class="size-3.5 shrink-0" />Add chart
        </Button>
      {/if}
    </div>
  </div>
  {/if}

  <!-- Content -->
  <div class="relative min-h-0 flex-1 overflow-y-auto p-4">
    {#if !activeDash}
      <div class="flex h-full min-h-[300px] flex-col items-center justify-center gap-4">
        <div class="flex size-16 items-center justify-center rounded-lg bg-muted/30">
          <LayoutGrid class="size-8 shrink-0 text-muted-foreground" />
        </div>
        <div class="text-center">
          <p class="font-medium text-foreground/60">No dashboard yet</p>
          <p class="mt-1 text-ui-sm text-muted-foreground">
            Create a dashboard to arrange your saved charts
          </p>
        </div>
        <Button
          
          onclick={() => createDashboard("My Dashboard")}
>
          <Plus class="size-4 shrink-0" />
          Create Dashboard
        </Button>
      </div>

    {:else if activeDash.items.length === 0}
      <div class="flex h-full min-h-[300px] flex-col items-center justify-center gap-4">
        <div class="flex size-16 items-center justify-center rounded-lg border-2 border-dashed border-border/40">
          <BarChart2 class="size-8 shrink-0 text-muted-foreground" />
        </div>
        <p class="text-ui-sm text-muted-foreground">Add charts to build your dashboard</p>
        <button
          type="button"
          class="inline-flex h-8 items-center gap-1.5 rounded-md bg-sidebar-accent/50 px-4 text-ui-sm text-muted-foreground hover:bg-sidebar-accent hover:text-foreground"
          onclick={() => (pickerOpen = true)}
        >
          <Plus class="size-4 shrink-0" />
          Add Chart
        </button>
      </div>

    {:else}
      <!-- Dashboard grid, fixed columns so Swapy can compute slot positions reliably -->
      <div bind:this={gridEl} class="grid gap-3" style={gridStyle}>
        {#each activeDash.items as item (item.id)}
          {@const chart = getChart(item.chartId)}
          {@const span = effectiveSpan(item)}
          {@const l = chart ? live[chart.id] : null}
          {@const view = viewFor(chart)}
          {@const h = SIZE_H[item.size ?? "m"] + (presenting ? 40 : 0)}
          <div
            data-swapy-slot={item.id}
            style="grid-column: span {span}"
          >
            <div
              data-swapy-item={item.id}
              class={cn(
                "group flex flex-col overflow-hidden rounded-lg border border-border/60 bg-card transition-colors",
                !presenting && "hover:border-border",
              )}
              style="height:{h}px"
            >
              <div class={cn("flex h-9 shrink-0 items-center gap-1.5", presenting ? "px-3" : "ps-1.5 pe-1.5")}>
                {#if !presenting}
                  <div
                    data-swapy-handle
                    class="flex size-6 shrink-0 cursor-grab items-center justify-center rounded-md text-muted-foreground/60 transition-colors hover:text-foreground active:cursor-grabbing"
                    title="Drag to reorder"
                  >
                    <Grip class="size-3.5 shrink-0" />
                  </div>
                {/if}
                <span class="min-w-0 flex-1 truncate text-ui-xs font-medium text-foreground">{chart?.name ?? "Missing chart"}</span>
                {#if l?.loading}
                  <Loader2 class="size-3 shrink-0 animate-spin text-muted-foreground" />
                {:else if l?.error}
                  <button type="button" class="shrink-0 text-ui-3xs text-destructive hover:underline" title={l.error} onclick={() => chart && refreshChart(chart.id)}>Failed · retry</button>
                {:else if l?.at}
                  <span class="shrink-0 text-ui-3xs tabular-nums text-muted-foreground" title={new Date(l.at).toLocaleString()}>{agoText(l.at, now)}</span>
                {:else if chart && !canRun(chart)}
                  <span class="shrink-0 text-ui-3xs text-muted-foreground" title="Saved snapshot: the query isn't read-only, so it isn't run again">snapshot</span>
                {/if}
                {#if !presenting}
                  <div class="flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
                    {#if chart && canRun(chart)}
                      <button type="button" class="flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" title="Refresh" aria-label="Refresh chart" onclick={() => refreshChart(chart.id)}>
                        <RefreshCw class="size-3.5 shrink-0" />
                      </button>
                    {/if}
                    <DropdownMenu.DropdownMenu>
                      <DropdownMenu.DropdownMenuTrigger class="flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground data-[state=open]:bg-accent" aria-label="Card options">
                        <MoreHorizontal class="size-3.5 shrink-0" />
                      </DropdownMenu.DropdownMenuTrigger>
                      <DropdownMenu.DropdownMenuContent align="end" class="min-w-48">
                        <DropdownMenu.DropdownMenuSub>
                          <DropdownMenu.DropdownMenuSubTrigger>Width</DropdownMenu.DropdownMenuSubTrigger>
                          <DropdownMenu.DropdownMenuSubContent class="min-w-36">
                            {#each Array.from({ length: activeDash.columns }, (_, i) => i + 1) as w (w)}
                              <DropdownMenu.DropdownMenuItem onclick={() => setItemSpan(activeDash.id, item.id, /** @type {1|2|3} */ (w))}>
                                <span class="flex-1">{w === activeDash.columns ? "Full width" : `${w} of ${activeDash.columns}`}</span>
                                {#if span === w}<Check class="size-3.5 shrink-0 text-primary" />{/if}
                              </DropdownMenu.DropdownMenuItem>
                            {/each}
                          </DropdownMenu.DropdownMenuSubContent>
                        </DropdownMenu.DropdownMenuSub>
                        <DropdownMenu.DropdownMenuSub>
                          <DropdownMenu.DropdownMenuSubTrigger>Height</DropdownMenu.DropdownMenuSubTrigger>
                          <DropdownMenu.DropdownMenuSubContent class="min-w-36">
                            {#each [["s", "Small"], ["m", "Medium"], ["l", "Large"]] as [k, label] (k)}
                              <DropdownMenu.DropdownMenuItem onclick={() => setItemSize(activeDash.id, item.id, /** @type {'s'|'m'|'l'} */ (k))}>
                                <span class="flex-1">{label}</span>
                                {#if (item.size ?? "m") === k}<Check class="size-3.5 shrink-0 text-primary" />{/if}
                              </DropdownMenu.DropdownMenuItem>
                            {/each}
                          </DropdownMenu.DropdownMenuSubContent>
                        </DropdownMenu.DropdownMenuSub>
                        {#if chart?.sql && onopensql}
                          <DropdownMenu.DropdownMenuItem onclick={() => chart && onopensql?.(chart.sql, chart.name)}><SquareTerminal class="size-3.5 shrink-0" />Open query</DropdownMenu.DropdownMenuItem>
                        {/if}
                        <DropdownMenu.DropdownMenuSeparator />
                        <DropdownMenu.DropdownMenuItem variant="destructive" onclick={() => removeChartFromDashboard(activeDash.id, item.id)}><X class="size-3.5 shrink-0" />Remove from dashboard</DropdownMenu.DropdownMenuItem>
                      </DropdownMenu.DropdownMenuContent>
                    </DropdownMenu.DropdownMenu>
                  </div>
                {/if}
              </div>

              <div class="relative min-h-0 flex-1">
                {#if view?.kpi}
                  <div class="absolute inset-0 flex flex-col items-center justify-center gap-1 px-4 text-center">
                    <span class={cn("font-semibold tracking-tight text-foreground tabular-nums", h >= 280 ? "text-ui-3xl" : "text-ui-2xl")}>{formatKpi(view.kpi.value)}</span>
                    <span class="max-w-full truncate text-ui-xs text-muted-foreground">{view.kpi.label}</span>
                  </div>
                {:else if view?.option}
                  <EChartPanel option={view.option} class="absolute inset-0" />
                {:else if chart?.previewOption && Object.keys(chart.previewOption).length > 0}
                  <EChartPanel option={restyleSavedOption(chart.previewOption, { isDark: $isCurrentThemeDark, compact: true })} class="absolute inset-0" />
                {:else if chart?.aiSpec}
                  <div class="absolute inset-0">
                    <AiChartRenderer spec={chart.aiSpec} noTitle={true} />
                  </div>
                {:else}
                  <div class="absolute inset-0 flex items-center justify-center gap-2 text-muted-foreground">
                    <BarChart2 class="size-5 shrink-0" />
                    <span class="text-ui-xs">{chart ? "No preview" : "This chart was deleted"}</span>
                  </div>
                {/if}
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
{/if}

<!-- Chart picker. On the app's Dialog: portalled to <body>, so it centres on
     the window (a page-level `fixed` overlay centred on a transformed ancestor
     instead and sat off to one side), with the focus trap and Escape handling
     every other dialog has. -->
<Dialog.Root bind:open={pickerOpen}>
  <Dialog.Content class="flex max-h-[min(36rem,80vh)] flex-col gap-0 overflow-hidden p-0 sm:max-w-lg">
    <Dialog.Header class="shrink-0 gap-1 px-6 pt-6 pb-4">
      <Dialog.Title>Add a chart</Dialog.Title>
      <Dialog.Description class="text-ui-xs text-muted-foreground">
        {$savedCharts.length
          ? `Pick charts for ${activeDash?.name ?? "this dashboard"}. Click again to take one off.`
          : "Charts you save from query results show up here."}
      </Dialog.Description>
    </Dialog.Header>

    {#if $savedCharts.length === 0}
      <div class="flex flex-col items-center gap-3 px-6 pt-4 pb-8 text-center">
        <div class="flex size-11 items-center justify-center rounded-lg bg-muted/50 text-muted-foreground">
          <BarChart2 class="size-5 shrink-0" aria-hidden="true" />
        </div>
        <div class="flex max-w-80 flex-col gap-1">
          <p class="text-ui-sm font-medium text-foreground">No saved charts yet</p>
          <p class="text-ui-xs text-muted-foreground">
            Run a query, switch the results to Chart, and save it. It will be listed here.
          </p>
        </div>
        {#if onopensqleditor}
          <Button
            class="mt-1"
            onclick={() => {
              pickerOpen = false;
              onopensqleditor();
            }}
          >
            Open query editor
          </Button>
        {/if}
      </div>
    {:else}
      <div class="flex shrink-0 flex-col gap-2 px-6 pb-2">
        <label class="field-surface flex h-8 items-center gap-2 px-2.5">
          <SearchIcon class="size-3.5 shrink-0 text-muted-foreground" />
          <input bind:value={pickerQuery} placeholder="Search charts" aria-label="Search charts" class="no-field-frame min-w-0 flex-1 bg-transparent text-ui-sm outline-none placeholder:text-muted-foreground" />
        </label>
        {#if pickerGroups.length > 1}
          <div class="flex flex-wrap gap-1">
            {#each ["", ...pickerGroups] as g (g)}
              <button type="button"
                class={cn("h-6 rounded-md px-2 text-ui-2xs transition-colors", pickerGroup === g ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-muted/60 hover:text-foreground")}
                onclick={() => (pickerGroup = g)}>{g || "All"}</button>
            {/each}
          </div>
        {/if}
      </div>
      <div class="app-scroll min-h-0 flex-1 overflow-y-auto px-3 pb-3">
        <ul class="flex flex-col">
          {#each pickerCharts as chart (chart.id)}
            {@const n = onDash(chart.id)}
            {@const thumb = viewFor(chart)}
            <li>
              <button
                type="button"
                onclick={() => pickChart(chart.id)}
                aria-pressed={n > 0}
                class="group flex h-14 w-full items-center gap-3 rounded-lg px-3 text-left transition-colors hover:bg-muted/50 focus-visible:bg-muted/50 focus-visible:outline-none"
              >
                <!-- A picture of the chart, not one to play with: no pointer events. -->
                <span class="pointer-events-none relative h-10 w-16 shrink-0 overflow-hidden rounded-md bg-muted/40">
                  {#if thumb?.kpi}
                    <span class="absolute inset-0 flex items-center justify-center text-ui-xs font-semibold tabular-nums text-foreground">{formatKpi(thumb.kpi.value)}</span>
                  {:else if thumb?.option || (chart.previewOption && Object.keys(chart.previewOption).length > 0)}
                    <EChartPanel option={thumbOption(thumb?.option ?? restyleSavedOption(chart.previewOption, { isDark: $isCurrentThemeDark, compact: true }))} class="absolute inset-0" />
                  {:else}
                    <BarChart2 class="absolute inset-0 m-auto size-4 text-muted-foreground" aria-hidden="true" />
                  {/if}
                </span>
                <span class="min-w-0 flex-1">
                  <span class="block truncate text-ui-sm text-foreground">{chart.name}</span>
                  <span class="block truncate text-ui-2xs text-muted-foreground">{chart.group || "No group"} · {chart.config?.type ?? "chart"}</span>
                </span>
                {#if n > 0}
                  <span class="flex shrink-0 items-center gap-1 text-ui-xs text-foreground"><Check class="size-3.5 shrink-0" />Added</span>
                {:else}
                  <span class="flex shrink-0 items-center gap-1 text-ui-xs text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100"><Plus class="size-3.5 shrink-0" />Add</span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
        {#if !pickerCharts.length}
          <p class="py-8 text-center text-ui-xs text-muted-foreground">No charts match.</p>
        {/if}
      </div>
      <div class="flex shrink-0 items-center justify-between gap-3 border-t border-border/40 px-6 py-3">
        <span class="text-ui-xs text-muted-foreground">{activeDash?.items.length ?? 0} on {activeDash?.name ?? "this dashboard"}</span>
        <Button size="sm" onclick={() => (pickerOpen = false)}>Done</Button>
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
