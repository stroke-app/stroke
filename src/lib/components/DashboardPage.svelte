<script>
  import { onDestroy, tick } from "svelte";
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
    reorderItems,
  } from "$lib/stores/dashboards.js";
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

  let {
    /** @type {import('$lib/stores/connections.js').SavedConnection | null} */
    connection = null,
    /** Open the SQL editor - where charts are made - from the empty picker. @type {(() => void) | null} */
    onopensqleditor = null,
  } = $props();

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

  /** @param {string} chartId */
  function pickChart(chartId) {
    if (!activeDash) return;
    addChartToDashboard(activeDash.id, chartId);
    pickerOpen = false;
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
  <div
    class="studio-chrome flex h-10 shrink-0 items-center gap-2 border-b border-border bg-panel px-3"
    data-studio-chrome
  >
    <LayoutGrid class="size-4 shrink-0 text-muted-foreground" />

    <!-- Dashboard selector -->
    <div class="relative">
      <button
        type="button"
        class="flex h-8 items-center gap-1.5 rounded-md bg-sidebar-accent/50 px-3 text-ui-sm hover:bg-sidebar-accent"
        onclick={() => (dashDropdownOpen = !dashDropdownOpen)}
      >
        {#if editingName}
          <span class="flex items-center gap-1">
            <input
              type="text"
              bind:value={editNameInput}
              class="w-32 bg-transparent font-medium outline-none"
              onblur={commitEditName}
              onkeydown={(e) => {
                if (e.key === "Enter") commitEditName();
                if (e.key === "Escape") editingName = false;
              }}
              onclick={(e) => e.stopPropagation()}
            />
            <span
              role="button"
              tabindex="0"
              class="cursor-pointer text-primary"
              onclick={(e) => { e.stopPropagation(); commitEditName(); }}
              onkeydown={(e) => { if (e.key === "Enter") commitEditName(); }}
            >
              <Check class="size-3 shrink-0" />
            </span>
          </span>
        {:else}
          <span>{activeDash?.name ?? "Select dashboard"}</span>
          <ChevronDown class="size-3 shrink-0 text-muted-foreground" />
        {/if}
      </button>

      {#if dashDropdownOpen}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="absolute left-0 top-full z-50 mt-1 min-w-[180px] overflow-hidden rounded-[10px] border border-border/60 bg-popover p-1 elevate-2-rim"
          onmouseleave={() => (dashDropdownOpen = false)}
        >
          {#each $dashboards as d (d.id)}
            <button
              type="button"
              class={cn(
                "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-ui-sm transition-colors hover:bg-accent",
                d.id === $activeDashboardId
                  ? "font-medium text-foreground"
                  : "text-muted-foreground",
              )}
              onclick={() => { activeDashboardId.set(d.id); dashDropdownOpen = false; }}
            >
              <LayoutGrid class="size-3 shrink-0" />
              <span class="flex-1 truncate">{d.name}</span>
              {#if d.id !== $activeDashboardId}
                <span
                  role="button"
                  tabindex="0"
                  class="cursor-pointer text-muted-foreground hover:text-destructive"
                  onclick={(e) => { e.stopPropagation(); deleteDashboard(d.id); }}
                  onkeydown={(e) => { if (e.key === "Enter") { e.stopPropagation(); deleteDashboard(d.id); } }}
                >
                  <X class="size-3 shrink-0" />
                </span>
              {/if}
            </button>
          {/each}

          <div class="border-t border-border/50 px-2 py-1.5">
            {#if namingOpen}
              <div class="flex items-center gap-1">
                <input
                  type="text"
                  bind:value={nameInput}
                  placeholder="Dashboard name…"
                  class="no-field-frame h-8 flex-1 rounded-md border-0 bg-sidebar-accent/50 px-2.5 text-ui-sm outline-none focus:bg-sidebar-accent"
                  onkeydown={(e) => {
                    if (e.key === "Enter") handleCreateDashboard();
                    if (e.key === "Escape") namingOpen = false;
                  }}
                />
                <button type="button" class="text-primary" onclick={handleCreateDashboard}>
                  <Check class="size-3 shrink-0" />
                </button>
              </div>
            {:else}
              <button
                type="button"
                class="flex w-full items-center gap-1.5 rounded-md px-1 py-0.5 text-ui-xs text-muted-foreground transition-colors hover:text-foreground"
                onclick={() => { namingOpen = true; nameInput = ""; }}
              >
                <Plus class="size-3 shrink-0" />
                New
              </button>
            {/if}
          </div>
        </div>
      {/if}
    </div>

    {#if activeDash}
      <!-- Rename -->
      <button
        type="button"
        class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        title="Rename dashboard"
        onclick={startEditName}
      >
        <Pencil class="size-3.5 shrink-0" />
      </button>

      <!-- Column count cycle -->
      <div class="flex items-center gap-px rounded-md border border-border/50">
        {#each COL_OPTIONS as cols (cols)}
          <button
            type="button"
            class={cn(
              "flex h-7 w-7 items-center justify-center text-ui-2xs transition-colors first:rounded-l-md last:rounded-r-md",
              activeDash.columns === cols
                ? "bg-primary/15 text-primary font-medium"
                : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
            )}
            title="{cols} columns"
            onclick={() => updateDashboard(activeDash.id, { columns: cols })}
          >{cols}</button>
        {/each}
      </div>

      <!-- Delete -->
      <button
        type="button"
        class={cn(
          "inline-flex size-7 items-center justify-center rounded-md transition-colors",
          confirmDeleteDash
            ? "bg-destructive/15 text-destructive hover:bg-destructive/25"
            : "text-muted-foreground hover:bg-muted/60 hover:text-destructive",
        )}
        title={confirmDeleteDash ? "Click again to confirm delete" : "Delete dashboard"}
        onclick={handleDeleteDashboard}
      >
        <Trash2 class="size-3.5 shrink-0" />
      </button>
    {/if}

    <div class="ml-auto flex items-center gap-1.5">
      {#if activeDash}
        {#if activeDash.items.length > 0}
          <button
            type="button"
            class="inline-flex h-8 items-center gap-1.5 rounded-md bg-sidebar-accent/50 px-3 text-ui-sm text-muted-foreground hover:bg-sidebar-accent hover:text-foreground"
            title="View only - hide every control (Esc to exit)"
            onclick={() => setViewOnly(true)}
          >
            <Presentation class="size-3.5 shrink-0" />
            View only
          </button>
        {/if}
        <Button
           size="sm"
          onclick={() => (pickerOpen = true)}
>
          <Plus class="size-3.5 shrink-0" />
          Add Chart
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
          <div
            data-swapy-slot={item.id}
            style="grid-column: span {span}"
          >
            <div
              data-swapy-item={item.id}
              class={cn(
                "group flex flex-col overflow-hidden rounded-lg border border-border/60 bg-card transition-colors",
                // Taller cards with no chrome to fight for space when presenting.
                presenting ? "h-[320px]" : "h-[260px] hover:border-border",
              )}
            >
              <!-- Card header. View-only keeps the chart title (it labels the
                   data) and drops every control. -->
              <div
                class={cn(
                  "flex h-9 shrink-0 items-center gap-1 border-b border-border/40 bg-card",
                  presenting ? "px-3" : "px-2",
                )}
              >
                {#if !presenting}
                  <!-- Drag handle -->
                  <div
                    data-swapy-handle
                    class="flex size-6 cursor-grab items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground active:cursor-grabbing"
                    title="Drag to reorder"
                  >
                    <Grip class="size-3.5 shrink-0" />
                  </div>
                {/if}

                <!-- Title -->
                <span class="min-w-0 flex-1 truncate text-ui-xs font-medium text-foreground/80">
                  {chart?.name ?? "Unknown chart"}
                </span>

                {#if !presenting}
                <!-- Span selector (1 … columns) -->
                <div class="flex items-center gap-px opacity-0 transition-opacity group-hover:opacity-100">
                  {#each Array.from({ length: activeDash.columns }, (_, i) => i + 1) as s (s)}
                    <button
                      type="button"
                      onclick={() => setItemSpan(activeDash.id, item.id, /** @type {1|2|3} */ (s))}
                      class={cn(
                        "hit-area flex size-5 items-center justify-center rounded-md text-ui-2xs font-mono transition-colors",
                        span === s
                          ? "bg-primary/15 text-primary"
                          : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                      )}
                      title="{s} column{s > 1 ? 's' : ''}"
                    >{s}</button>
                  {/each}
                </div>

                <!-- Remove -->
                <button
                  type="button"
                  onclick={() => removeChartFromDashboard(activeDash.id, item.id)}
                  class="hit-area ml-1 flex size-5 items-center justify-center rounded-md text-muted-foreground transition-colors group-hover:text-muted-foreground hover:!text-destructive"
                  title="Remove from dashboard"
                >
                  <X class="size-3 shrink-0" />
                </button>
                {/if}
              </div>

              <!-- Chart body -->
              <div class="relative min-h-0 flex-1">
                {#if chart?.previewOption && Object.keys(chart.previewOption).length > 0}
                  <EChartPanel option={restyleSavedOption(chart.previewOption, { isDark: $isCurrentThemeDark, compact: true })} class="absolute inset-0" />
                {:else if chart?.aiSpec}
                  <div class="absolute inset-0">
                    <AiChartRenderer spec={chart.aiSpec} noTitle={true} />
                  </div>
                {:else}
                  <div class="absolute inset-0 flex items-center justify-center gap-2 text-muted-foreground">
                    <BarChart2 class="size-6 shrink-0" />
                    <span class="text-ui-xs">No preview</span>
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
  <Dialog.Content class="flex max-h-[min(36rem,80vh)] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl">
    <Dialog.Header class="shrink-0 gap-1 px-6 pt-6 pb-4">
      <Dialog.Title>Add a chart</Dialog.Title>
      <Dialog.Description class="text-ui-xs text-muted-foreground">
        {$savedCharts.length
          ? `Pick a saved chart to pin to ${activeDash?.name ?? "this dashboard"}.`
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
      <div class="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
        <div class="grid grid-cols-2 gap-3">
          {#each $savedCharts as chart (chart.id)}
            <button
              type="button"
              onclick={() => pickChart(chart.id)}
              class="group flex flex-col overflow-hidden rounded-lg border border-border/60 bg-background text-left transition-colors hover:border-ring/55 hover:bg-accent/30 focus-visible:border-ring/55 focus-visible:ring-2 focus-visible:ring-ring/18 focus-visible:outline-none"
            >
              <div class="relative h-28 border-b border-border/40 bg-sidebar/40">
                {#if chart.previewOption && Object.keys(chart.previewOption).length > 0}
                  <EChartPanel option={restyleSavedOption(chart.previewOption, { isDark: $isCurrentThemeDark, compact: true })} class="absolute inset-0" />
                {:else if chart.aiSpec}
                  <div class="absolute inset-0">
                    <AiChartRenderer spec={chart.aiSpec} noTitle={true} />
                  </div>
                {:else}
                  <div class="absolute inset-0 flex items-center justify-center text-muted-foreground">
                    <BarChart2 class="size-5 shrink-0" aria-hidden="true" />
                  </div>
                {/if}
              </div>
              <div class="flex flex-col gap-0.5 px-3 py-2">
                <p class="truncate text-ui-xs font-medium text-foreground">{chart.name}</p>
                {#if chart.group}<p class="truncate text-ui-3xs text-muted-foreground">{chart.group}</p>{/if}
              </div>
            </button>
          {/each}
        </div>
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
