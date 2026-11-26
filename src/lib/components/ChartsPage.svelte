<script>
  import { Button } from '$lib/components/ui/button/index.js'
  import EmptyState from './EmptyState.svelte'
  import EChartPanel from "./EChartPanel.svelte";
  import AiChartRenderer from "./AiChartRenderer.svelte";
  import {
    savedCharts,
    chartGroups,
    deleteChart,
    updateChart,
    addGroup,
    deleteGroup,
    renameGroup,
  } from "$lib/stores/saved-charts.js";
  import {
    addChartToDashboard,
    dashboards,
    activeDashboardId,
    createDashboard,
  } from "$lib/stores/dashboards.js";
  import { CHART_CATALOG, restyleSavedOption } from "$lib/chart-utils.js";
  import { isCurrentThemeDark } from "$lib/stores/settings.js";
  import { cn } from "$lib/utils.js";
  import { toast } from "$lib/components/ui/sonner/toast.svelte.js";
  import BarChart2 from "@lucide/svelte/icons/bar-chart-2";
  import Play from "@lucide/svelte/icons/play";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import X from "@lucide/svelte/icons/x";
  import Check from "@lucide/svelte/icons/check";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import LayoutDashboard from "@lucide/svelte/icons/layout-dashboard";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import Plus from "@lucide/svelte/icons/plus";
  import Share2 from "@lucide/svelte/icons/share-2";
  import MoreHorizontal from "@lucide/svelte/icons/more-horizontal";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import SearchField from "./SearchField.svelte";
  import SelectMenu from "./SelectMenu.svelte";
  import { executeSql } from "$lib/api.js";
  import { isWriteSql } from "$lib/sql-write.js";
  import { requestShare } from "$lib/share-flow.js";

  /**
   * @typedef {import('$lib/stores/connections.js').SavedConnection} SavedConnection
   */

  let {
    /** @type {SavedConnection | null} */
    connection = null,
    /** @type {(sql: string) => void} */
    onrunsql = (_sql) => {},
  } = $props();

  // ── State ─────────────────────────────────────────────────────────────────
  /** @type {string | null} */
  let renamingGroupName = $state(null);
  let renameGroupInput = $state("");

  /** @type {string | null} */
  let renamingChartId = $state(null);
  let renameChartInput = $state("");

  let newGroupOpen = $state(false);
  let newGroupInput = $state("");

  /** @type {string | null} */
  let moveChartId = $state(null);

  /** @type {string | null} */
  let confirmDeleteId = $state(null);

  // Dashboard picker popover per chart
  /** @type {string | null} */
  let dashPickerChartId = $state(null);

  let query = $state("");
  /** "" = every group. */
  let groupFilter = $state("");
  /** The chart whose share is being prepared (its query re-runs). */
  let sharingId = $state(/** @type {string | null} */ (null));

  // Charts per group, after the search and the group filter.
  const chartsByGroup = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return $chartGroups
      .filter((g) => !groupFilter || g === groupFilter)
      .map((g) => ({
        group: g,
        charts: $savedCharts.filter(
          (c) => c.group === g && (!q || c.name.toLowerCase().includes(q) || (catalogEntry(c.config.type)?.label ?? c.config.type).toLowerCase().includes(q)),
        ),
      }));
  });
  const shownCount = $derived(chartsByGroup.reduce((n, g) => n + g.charts.length, 0));

  /**
   * Share a saved chart: its query runs again (read only) so the link carries
   * the rows as they are now, then the share dialog opens with them.
   * @param {import('$lib/stores/saved-charts.js').SavedChart} chart
   */
  async function shareChart(chart) {
    if (!chart.sql?.trim()) { toast.error("This chart has no query to share"); return; }
    if (isWriteSql(chart.sql)) { toast.error("Only charts made from a read-only query can be shared"); return; }
    sharingId = chart.id;
    try {
      const r = await executeSql(chart.sql);
      const columns = (r?.columns ?? []).map((/** @type {any} */ c) => ({ name: c.name, type: c.dataType ?? c.data_type }));
      const rows = r?.rows ?? [];
      if (!rows.length) { toast.message("The query returned no rows to share"); return; }
      requestShare({
        kind: "chart",
        title: chart.name,
        chart: { type: chart.config.type, x: chart.config.xCol, y: [chart.config.yCol].filter(Boolean), group: chart.config.groupCol ?? null },
        columns,
        rows,
      });
    } catch (e) {
      toast.error("Couldn't run the chart's query", { description: String(/** @type {any} */ (e)?.message ?? e) });
    } finally {
      sharingId = null;
    }
  }

  const totalCharts = $derived($savedCharts.length);

  // ── Helpers ───────────────────────────────────────────────────────────────
  /** @param {string} typeId */
  function catalogEntry(typeId) {
    return CHART_CATALOG.find((c) => c.id === typeId);
  }

  /**
   * Clean a saved previewOption for display:
   * - Strips the in-chart title (shown in the card footer already)
   * - Re-attaches timestamp formatters lost during JSON serialisation
   * @param {any} option
   */
  function cleanPreviewOption(option) {
    // Shared with the dashboard: a saved option also carries the theme colours
    // it was built in, so re-theming has to happen here too - a chart saved in
    // light mode used to render its axis labels as dark text on a dark card.
    return restyleSavedOption(option, { isDark: $isCurrentThemeDark })
  }

  /** @param {number} ts */
  function fmtDate(ts) {
    return new Date(ts).toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  }

  function commitNewGroup() {
    const name = newGroupInput.trim();
    if (!name) return;
    addGroup(name);
    newGroupInput = "";
    newGroupOpen = false;
    toast.success(`Group "${name}" created`);
  }

  /** @param {string} oldName */
  function startRenameGroup(oldName) {
    renamingGroupName = oldName;
    renameGroupInput = oldName;
  }

  function commitRenameGroup() {
    if (renamingGroupName) {
      renameGroup(renamingGroupName, renameGroupInput.trim());
      toast.success("Group renamed");
    }
    renamingGroupName = null;
    renameGroupInput = "";
  }

  /** @param {string} name */
  function handleDeleteGroup(name) {
    deleteGroup(name);
    toast.success(`Group "${name}" deleted`);
  }

  /** @param {string} id */
  function startRenameChart(id) {
    const c = $savedCharts.find((cc) => cc.id === id);
    renamingChartId = id;
    renameChartInput = c?.name ?? "";
  }

  function commitRenameChart() {
    if (renamingChartId) {
      const name = renameChartInput.trim();
      if (name) updateChart(renamingChartId, { name });
    }
    renamingChartId = null;
    renameChartInput = "";
  }

  /** @param {string} id */
  function handleDeleteChart(id) {
    if (confirmDeleteId === id) {
      deleteChart(id);
      confirmDeleteId = null;
      toast.success("Chart deleted");
    } else {
      confirmDeleteId = id;
    }
  }

  /** @param {string} id @param {string} toGroup */
  function handleMoveChart(id, toGroup) {
    updateChart(id, { group: toGroup });
    moveChartId = null;
    toast.success("Chart moved");
  }

  /** @param {import('$lib/stores/saved-charts.js').SavedChart} chart */
  function handleRunSql(chart) {
    onrunsql(chart.sql);
    toast.success("SQL opened in Query Editor");
  }

  /** Focus a field as it appears (rename, new group). @param {HTMLInputElement} el */
  function focusOnMount(el) {
    queueMicrotask(() => { el.focus(); el.select(); });
  }

  const btnIconSm =
    "inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground";

  /**
   * Add chart directly to a specific dashboard (or create one if none exist).
   * @param {string} chartId
   * @param {string} dashId
   */
  function addToDashboardById(chartId, dashId) {
    addChartToDashboard(dashId, chartId);
    activeDashboardId.set(dashId);
    dashPickerChartId = null;
    toast.success("Added to dashboard");
  }

  /** @param {string} chartId */
  function addToNewDashboard(chartId) {
    const dash = createDashboard("Dashboard");
    addChartToDashboard(dash.id, chartId);
    dashPickerChartId = null;
    toast.success("Created dashboard & added chart");
  }
</script>

{#if !connection}
  <div class="flex min-h-0 flex-1 items-center justify-center bg-panel">
    <div class="flex flex-col items-center gap-2 text-center">
      <BarChart2 class="size-8 shrink-0 text-muted-foreground" />
      <p class="text-ui-sm text-muted-foreground">Connect to a database to view saved charts</p>
    </div>
  </div>
{:else}
<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  <!-- ── Toolbar: what this is, search, group, new group ──────────────── -->
  <div class="studio-chrome flex h-12 shrink-0 items-center gap-2 border-b border-border/40 bg-panel px-4" data-studio-chrome>
    <h1 class="text-ui-sm font-medium text-foreground">Saved charts</h1>
    <span class="text-ui-xs tabular-nums text-muted-foreground">{totalCharts}</span>
    <span class="flex-1"></span>
    {#if totalCharts > 0}
      <SearchField bind:value={query} placeholder="Search charts" class="w-52" />
      {#if $chartGroups.length > 1}
        <SelectMenu
          ariaLabel="Group"
          triggerClass="h-8 w-36"
          contentClass="w-[var(--bits-popover-anchor-width)] min-w-[10rem]"
          align="end"
          value={groupFilter}
          onValueChange={(v) => (groupFilter = v)}
          items={[{ value: "", label: "All groups" }, ...$chartGroups.map((g) => ({ value: g, label: g }))]}
        />
      {/if}
    {/if}
    {#if newGroupOpen}
      <div class="flex items-center gap-1">
        <input
          type="text"
          placeholder="Group name"
          bind:value={newGroupInput}
          class="no-field-frame h-8 w-36 rounded-lg border-0 bg-muted/55 px-2.5 text-ui-xs text-foreground outline-none placeholder:text-muted-foreground focus:bg-muted/80"
          onkeydown={(e) => {
            if (e.key === "Enter") commitNewGroup();
            if (e.key === "Escape") newGroupOpen = false;
          }}
          use:focusOnMount
        />
        <button type="button" class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" title="Create group" aria-label="Create group" onclick={commitNewGroup}>
          <Check class="size-3.5 shrink-0" />
        </button>
        <button type="button" class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Cancel" onclick={() => (newGroupOpen = false)}>
          <X class="size-3.5 shrink-0" />
        </button>
      </div>
    {:else}
      <Button variant="outline" size="sm" class="gap-1.5" onclick={() => { newGroupOpen = true; newGroupInput = ""; }}>
        <FolderPlus class="size-3.5 shrink-0" />New group
      </Button>
    {/if}
  </div>

  <div class="app-scroll min-h-0 flex-1 overflow-y-auto px-4 py-4">
    {#if totalCharts === 0}
      <EmptyState
        class="h-full min-h-[280px] justify-center"
        icon={BarChart2}
        title="No saved charts yet"
        description="Run a query, switch its result to the chart view, and save it. It shows up here, ready to share or put on a dashboard."
      >
        <Button size="sm" variant="secondary" onclick={() => onrunsql('')}>Open SQL editor</Button>
      </EmptyState>
    {:else if shownCount === 0}
      <p class="py-16 text-center text-ui-xs text-muted-foreground">No charts match.</p>
    {:else}
      <div class="flex flex-col gap-6">
        {#each chartsByGroup as grpData (grpData.group)}
          {#if grpData.charts.length > 0}
            <section aria-label={grpData.group}>
              <div class="mb-2 flex h-7 items-center gap-2">
                {#if renamingGroupName === grpData.group}
                  <input
                    type="text"
                    bind:value={renameGroupInput}
                    class="no-field-frame h-7 w-40 rounded-md border-0 bg-muted/60 px-2 text-ui-xs outline-none"
                    onkeydown={(e) => {
                      if (e.key === "Enter") commitRenameGroup();
                      if (e.key === "Escape") renamingGroupName = null;
                    }}
                    onblur={commitRenameGroup}
                    use:focusOnMount
                  />
                {:else}
                  <h2 class="text-ui-xs font-medium text-muted-foreground">{grpData.group}</h2>
                  <span class="text-ui-2xs tabular-nums text-muted-foreground">{grpData.charts.length}</span>
                  {#if grpData.group !== "Default"}
                    <DropdownMenu.DropdownMenu>
                      <DropdownMenu.DropdownMenuTrigger class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Group actions">
                        <MoreHorizontal class="size-3.5 shrink-0" />
                      </DropdownMenu.DropdownMenuTrigger>
                      <DropdownMenu.DropdownMenuContent align="start" class="min-w-40">
                        <DropdownMenu.DropdownMenuItem onclick={() => startRenameGroup(grpData.group)}><Pencil class="size-3.5 shrink-0" />Rename group</DropdownMenu.DropdownMenuItem>
                        <DropdownMenu.DropdownMenuItem variant="destructive" onclick={() => handleDeleteGroup(grpData.group)}><Trash2 class="size-3.5 shrink-0" />Delete group</DropdownMenu.DropdownMenuItem>
                      </DropdownMenu.DropdownMenuContent>
                    </DropdownMenu.DropdownMenu>
                  {/if}
                {/if}
              </div>

              <div class="@container">
                <div class="grid grid-cols-1 gap-3 @[420px]:grid-cols-2 @[680px]:grid-cols-3 @[960px]:grid-cols-4">
                  {#each grpData.charts as chart (chart.id)}
                    {@const entry = catalogEntry(chart.config.type)}
                    <article class="group relative flex flex-col overflow-hidden rounded-lg border border-border/50 bg-card transition-colors hover:border-border">
                      <button type="button" class="relative block h-40 shrink-0 bg-muted/10 text-left" title="Open the query" onclick={() => handleRunSql(chart)}>
                        {#if chart.previewOption && Object.keys(chart.previewOption).length > 0}
                          <EChartPanel option={cleanPreviewOption(chart.previewOption)} class="pointer-events-none absolute inset-0" />
                        {:else if chart.aiSpec}
                          <div class="pointer-events-none absolute inset-0"><AiChartRenderer spec={chart.aiSpec} noTitle={true} /></div>
                        {:else}
                          <div class="absolute inset-0 flex items-center justify-center"><BarChart2 class="size-6 shrink-0 text-muted-foreground" /></div>
                        {/if}
                      </button>
                      <div class="flex items-center gap-2 border-t border-border/40 py-2 ps-3 pe-1.5">
                        <div class="min-w-0 flex-1">
                          {#if renamingChartId === chart.id}
                            <input
                              type="text"
                              bind:value={renameChartInput}
                              class="no-field-frame h-6 w-full rounded-md border-0 bg-muted/60 px-1.5 text-ui-xs outline-none"
                              onkeydown={(e) => {
                                if (e.key === "Enter") commitRenameChart();
                                if (e.key === "Escape") renamingChartId = null;
                              }}
                              onblur={commitRenameChart}
                              use:focusOnMount
                            />
                          {:else}
                            <p class="truncate text-ui-xs font-medium text-foreground" title={chart.name}>{chart.name}</p>
                          {/if}
                          <p class="truncate text-ui-3xs text-muted-foreground">{entry?.label ?? chart.config.type} · {fmtDate(chart.createdAt)}</p>
                        </div>
                        {#if sharingId === chart.id}
                          <LoaderCircle class="size-3.5 shrink-0 animate-spin text-muted-foreground" />
                        {/if}
                        <DropdownMenu.DropdownMenu>
                          <DropdownMenu.DropdownMenuTrigger class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground data-[state=open]:bg-accent" aria-label="Chart actions">
                            <MoreHorizontal class="size-3.5 shrink-0" />
                          </DropdownMenu.DropdownMenuTrigger>
                          <DropdownMenu.DropdownMenuContent align="end" class="min-w-48">
                            <DropdownMenu.DropdownMenuItem onclick={() => handleRunSql(chart)}><Play class="size-3.5 shrink-0" />Open the query</DropdownMenu.DropdownMenuItem>
                            <DropdownMenu.DropdownMenuItem onclick={() => void shareChart(chart)}><Share2 class="size-3.5 shrink-0" />Share…</DropdownMenu.DropdownMenuItem>
                            <DropdownMenu.DropdownMenuSub>
                              <DropdownMenu.DropdownMenuSubTrigger><LayoutDashboard class="size-3.5 shrink-0" />Add to dashboard</DropdownMenu.DropdownMenuSubTrigger>
                              <DropdownMenu.DropdownMenuSubContent class="min-w-44">
                                {#each $dashboards as d (d.id)}
                                  <DropdownMenu.DropdownMenuItem onclick={() => addToDashboardById(chart.id, d.id)}><LayoutGrid class="size-3.5 shrink-0" /><span class="truncate">{d.name}</span></DropdownMenu.DropdownMenuItem>
                                {/each}
                                {#if $dashboards.length}<DropdownMenu.DropdownMenuSeparator />{/if}
                                <DropdownMenu.DropdownMenuItem onclick={() => addToNewDashboard(chart.id)}><Plus class="size-3.5 shrink-0" />New dashboard</DropdownMenu.DropdownMenuItem>
                              </DropdownMenu.DropdownMenuSubContent>
                            </DropdownMenu.DropdownMenuSub>
                            {#if $chartGroups.length > 1}
                              <DropdownMenu.DropdownMenuSub>
                                <DropdownMenu.DropdownMenuSubTrigger><FolderOpen class="size-3.5 shrink-0" />Move to group</DropdownMenu.DropdownMenuSubTrigger>
                                <DropdownMenu.DropdownMenuSubContent class="min-w-40">
                                  {#each $chartGroups.filter((g) => g !== chart.group) as g (g)}
                                    <DropdownMenu.DropdownMenuItem onclick={() => handleMoveChart(chart.id, g)}>{g}</DropdownMenu.DropdownMenuItem>
                                  {/each}
                                </DropdownMenu.DropdownMenuSubContent>
                              </DropdownMenu.DropdownMenuSub>
                            {/if}
                            <DropdownMenu.DropdownMenuItem onclick={() => startRenameChart(chart.id)}><Pencil class="size-3.5 shrink-0" />Rename</DropdownMenu.DropdownMenuItem>
                            <DropdownMenu.DropdownMenuSeparator />
                            <DropdownMenu.DropdownMenuItem variant="destructive" onclick={() => { deleteChart(chart.id); toast.success("Chart deleted"); }}><Trash2 class="size-3.5 shrink-0" />Delete</DropdownMenu.DropdownMenuItem>
                          </DropdownMenu.DropdownMenuContent>
                        </DropdownMenu.DropdownMenu>
                      </div>
                    </article>
                  {/each}
                </div>
              </div>
            </section>
          {/if}
        {/each}
      </div>
    {/if}
  </div>
</div>
{/if}
