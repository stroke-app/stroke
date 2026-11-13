<script>
  import { Button } from '$lib/components/ui/button/index.js'
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

  // Charts per group (derived)
  const chartsByGroup = $derived(
    $chartGroups.map((g) => ({
      group: g,
      charts: $savedCharts.filter((c) => c.group === g),
    })),
  );

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
  <!-- ── Header bar ────────────────────────────────────────────────────── -->
  <div
    class="studio-chrome flex h-12 shrink-0 items-center gap-2 border-b border-border/40 bg-panel px-4"
    data-studio-chrome
  >
    <BarChart2 class="size-4 shrink-0 text-muted-foreground" />
    <span class="font-medium">Saved charts</span>
    <span
      class="ml-1 rounded-full bg-muted px-1.5 py-0.5 font-mono text-ui-2xs text-muted-foreground"
    >
      {totalCharts}
    </span>
    <div class="ml-auto flex items-center gap-1">
      {#if !newGroupOpen}
        <button
          type="button"
          class= "field-surface inline-flex h-6 items-center gap-1.5 px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onclick={() => {
            newGroupOpen = true;
            newGroupInput = "";
          }}
        >
          <FolderPlus class="size-3.5 shrink-0" />
          New Group
        </button>
      {:else}
        <div class="flex items-center gap-1">
          <input
            type="text"
            placeholder="Group name…"
            bind:value={newGroupInput}
            class="no-field-frame h-8 rounded-md border-0 bg-sidebar-accent/50 px-2.5 text-ui-sm text-foreground outline-none placeholder:text-muted-foreground focus:bg-sidebar-accent"
            style="width:150px"
            onkeydown={(e) => {
              if (e.key === "Enter") commitNewGroup();
              if (e.key === "Escape") {
                newGroupOpen = false;
              }
            }}
          />
          <button type="button" class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-sidebar-accent hover:text-foreground" title="Create group" aria-label="Create group" onclick={commitNewGroup}>
            <Check class="size-4 shrink-0" />
          </button>
          <button
            type="button"
            class={cn(btnIconSm)}
            onclick={() => {
              newGroupOpen = false;
            }}
          >
            <X class="size-3 shrink-0" />
          </button>
        </div>
      {/if}
    </div>
  </div>

  <!-- ── Content ───────────────────────────────────────────────────────── -->
  <div class="min-h-0 flex-1 overflow-y-auto p-4">
    {#if totalCharts === 0}
      <!-- Empty state -->
      <div
        class="flex h-full min-h-[280px] flex-col items-center justify-center gap-4"
      >
        <div
          class="flex size-16 items-center justify-center rounded-lg bg-muted/30"
        >
          <BarChart2 class="size-8 shrink-0 text-muted-foreground" />
        </div>
        <div class="text-center">
          <p class="font-medium text-foreground/60">No saved charts yet</p>
          <p class="mt-1 text-ui-sm text-muted-foreground">
            Save charts from the Query Editor to see them here
          </p>
        </div>
        <div
          class="flex items-center gap-1.5 rounded-lg border border-border/40 bg-muted/20 px-3 py-2 text-ui-sm text-muted-foreground"
        >
          <span>Run a query</span>
          <ArrowRight class="size-3.5 shrink-0" />
          <span>Chart tab</span>
          <ArrowRight class="size-3.5 shrink-0" />
          <span>Save chart</span>
        </div>
      </div>
    {:else}
      <div class="flex flex-col gap-6">
        {#each chartsByGroup as grpData (grpData.group)}
          {#if grpData.charts.length > 0}
            <section>
              <!-- Group header -->
              <div class="mb-3 flex items-center gap-2">
                <FolderOpen class="size-3.5 shrink-0 text-muted-foreground" />
                {#if renamingGroupName === grpData.group}
                  <div class="flex items-center gap-1">
                    <input
                      type="text"
                      bind:value={renameGroupInput}
                      class="field-surface h-7 bg-background px-2 font-mono text-ui-xs"
                      style="width:130px"
                      onkeydown={(e) => {
                        if (e.key === "Enter") commitRenameGroup();
                        if (e.key === "Escape") {
                          renamingGroupName = null;
                        }
                      }}
                    />
                    <button
                      type="button"
                      class={cn(btnIconSm, "size-5")}
                      onclick={commitRenameGroup}
                      ><Check class="size-3 shrink-0" /></button
                    >
                    <button
                      type="button"
                      class={cn(btnIconSm, "size-5")}
                      onclick={() => {
                        renamingGroupName = null;
                      }}><X class="size-3 shrink-0" /></button
                    >
                  </div>
                {:else}
                  <h3 class="text-ui-sm font-semibold text-foreground/70">
                    {grpData.group}
                  </h3>
                  <span
                    class="rounded-full bg-muted/60 px-1.5 py-0 font-mono text-ui-2xs text-muted-foreground"
                    >{grpData.charts.length}</span
                  >
                  {#if grpData.group !== "Default"}
                    <button
                      type="button"
                      class={cn(btnIconSm, "size-5 ml-0.5")}
                      title="Rename group"
                      onclick={() => startRenameGroup(grpData.group)}
                    >
                      <Pencil class="size-3 shrink-0" />
                    </button>
                    <button
                      type="button"
                      class={cn(btnIconSm, "size-5 hover:text-destructive")}
                      title="Delete group"
                      onclick={() => handleDeleteGroup(grpData.group)}
                    >
                      <Trash2 class="size-3 shrink-0" />
                    </button>
                  {/if}
                {/if}
              </div>

              <!-- Chart cards grid, @container so columns adapt to sidebar state -->
              <div class="@container">
              <div
                class="grid grid-cols-1 gap-2.5 @[380px]:grid-cols-2 @[580px]:grid-cols-3 @[820px]:grid-cols-4"
              >
                {#each grpData.charts as chart (chart.id)}
                  {@const entry = catalogEntry(chart.config.type)}
                  <div
                    class="group relative flex flex-col overflow-hidden rounded-lg border border-border/50 bg-card transition-colors hover:border-border"
                  >
                    <!-- Preview: dominant, fills top of card. Absolute child for crisp canvas sizing -->
                    <div class="relative h-36 shrink-0 bg-muted/10">
                      {#if chart.previewOption && Object.keys(chart.previewOption).length > 0}
                        <EChartPanel
                          option={cleanPreviewOption(chart.previewOption)}
                          class="absolute inset-0"
                        />
                      {:else if chart.aiSpec}
                        <!-- AI-generated chart types (meter, choropleth, etc.) that don't
                             produce an ECharts option, render via AiChartRenderer -->
                        <div class="absolute inset-0">
                          <AiChartRenderer spec={chart.aiSpec} noTitle={true} />
                        </div>
                      {:else}
                        <div
                          class="absolute inset-0 flex items-center justify-center"
                        >
                          <BarChart2 class="size-8 shrink-0 text-muted-foreground" />
                        </div>
                      {/if}

                      <!-- Hover overlay with action buttons -->
                      <div
                        class="absolute inset-0 flex items-end justify-between bg-gradient-to-t from-card/95 via-card/20 to-transparent p-2 opacity-0 transition-opacity group-hover:opacity-100"
                      >
                        <div class="flex items-center gap-1">
                          <button
                            type="button"
                            class="inline-flex h-6 items-center gap-1 rounded-md border border-border/60 bg-card px-1.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                            title="Open SQL in editor"
                            onclick={() => handleRunSql(chart)}
                          >
                            <Play class="size-3 shrink-0" />
                            Run
                          </button>
                          <div class="relative">
                            <button
                              type="button"
                              class="inline-flex h-6 items-center gap-1 rounded-md border border-border/60 bg-card px-1.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                              title="Add to dashboard"
                              onclick={(e) => {
                                e.stopPropagation();
                                dashPickerChartId =
                                  dashPickerChartId === chart.id
                                    ? null
                                    : chart.id;
                              }}
                            >
                              <LayoutDashboard class="size-3 shrink-0" />
                              Dashboard
                            </button>
                            {#if dashPickerChartId === chart.id}
                              <div
                                class="absolute bottom-full left-0 z-30 mb-1 min-w-[160px] overflow-hidden rounded-[10px] border border-border/60 bg-popover p-1 elevate-2-rim"
                              >
                                {#each $dashboards as d (d.id)}
                                  <button
                                    type="button"
                                    class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-ui-sm transition-colors hover:bg-accent hover:text-foreground"
                                    onclick={(e) => {
                                      e.stopPropagation();
                                      addToDashboardById(chart.id, d.id);
                                    }}
                                  >
                                    <LayoutGrid
                                      class="size-3 shrink-0 text-muted-foreground"
                                    />
                                    <span class="truncate">{d.name}</span>
                                    {#if d.id === $activeDashboardId}
                                      <span
                                        class="ml-auto font-mono text-ui-3xs text-muted-foreground"
                                        >active</span
                                      >
                                    {/if}
                                  </button>
                                {/each}
                                <div class="border-t border-border/50">
                                  <button
                                    type="button"
                                    class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-ui-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                                    onclick={(e) => {
                                      e.stopPropagation();
                                      addToNewDashboard(chart.id);
                                    }}
                                  >
                                    <Plus class="size-3 shrink-0" />
                                    New
                                  </button>
                                </div>
                              </div>
                            {/if}
                          </div>
                        </div>
                        <div class="flex items-center gap-0.5">
                          <button
                            type="button"
                            class={cn(
                              btnIconSm,
                              "size-6 bg-card border border-border/40",
                            )}
                            title="Rename"
                            onclick={() => startRenameChart(chart.id)}
                          >
                            <Pencil class="size-3 shrink-0" />
                          </button>
                          <div class="relative">
                            <button
                              type="button"
                              class={cn(
                                btnIconSm,
                                "size-6 bg-card border border-border/40",
                              )}
                              title="Move to group"
                              onclick={() => {
                                moveChartId =
                                  moveChartId === chart.id ? null : chart.id;
                              }}
                            >
                              <FolderOpen class="size-3 shrink-0" />
                            </button>
                            {#if moveChartId === chart.id}
                              <div
                                class="absolute bottom-full right-0 z-20 mb-1 min-w-[130px] rounded-[10px] border border-border/60 bg-popover p-1 elevate-2-rim"
                              >
                                {#each $chartGroups.filter((g) => g !== chart.group) as g (g)}
                                  <button
                                    type="button"
                                    class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-ui-sm text-muted-foreground hover:bg-accent hover:text-foreground"
                                    onclick={() => handleMoveChart(chart.id, g)}
                                  >
                                    <FolderOpen class="size-3 shrink-0" />{g}
                                  </button>
                                {/each}
                                {#if $chartGroups.filter((g) => g !== chart.group).length === 0}
                                  <p
                                    class="px-2 py-1 text-ui-2xs text-muted-foreground"
                                  >
                                    No other groups
                                  </p>
                                {/if}
                              </div>
                            {/if}
                          </div>
                          <button
                            type="button"
                            class={cn(
                              btnIconSm,
                              "size-6 bg-card border border-border/40",
                              confirmDeleteId === chart.id
                                ? "text-destructive"
                                : "hover:text-destructive",
                            )}
                            title={confirmDeleteId === chart.id
                              ? "Click again to confirm"
                              : "Delete"}
                            onclick={() => handleDeleteChart(chart.id)}
                          >
                            <Trash2 class="size-3 shrink-0" />
                          </button>
                        </div>
                      </div>
                    </div>

                    <!-- Footer: name + type badge -->
                    <div class="flex items-center gap-2 px-2.5 py-2">
                      {#if renamingChartId === chart.id}
                        <div class="flex min-w-0 flex-1 items-center gap-1">
                          <input
                            type="text"
                            bind:value={renameChartInput}
                            class="field-surface h-7 min-w-0 flex-1 bg-background px-2 font-mono text-ui-xs"
                            onkeydown={(e) => {
                              if (e.key === "Enter") commitRenameChart();
                              if (e.key === "Escape") {
                                renamingChartId = null;
                              }
                            }}
                          />
                          <button
                            type="button"
                            class={cn(btnIconSm, "size-4 shrink-0")}
                            onclick={commitRenameChart}
                            ><Check class="size-3 shrink-0" /></button
                          >
                          <button
                            type="button"
                            class={cn(btnIconSm, "size-4 shrink-0")}
                            onclick={() => {
                              renamingChartId = null;
                            }}><X class="size-3 shrink-0" /></button
                          >
                        </div>
                      {:else}
                        <span
                          class="min-w-0 flex-1 truncate text-ui-xs font-medium text-foreground/90"
                          title={chart.name}>{chart.name}</span
                        >
                      {/if}
                      <span
                        class="shrink-0 rounded bg-muted/50 px-1 py-px font-mono text-ui-3xs text-muted-foreground"
                        >{entry?.label ?? chart.config.type}</span
                      >
                    </div>
                  </div>
                {/each}
              </div>
              </div><!-- end @container -->
            </section>
          {/if}
        {/each}

        <!-- Groups with 0 charts (show placeholder) -->
        {#each chartsByGroup as grpData (grpData.group)}
          {#if grpData.charts.length === 0}
            <section>
              <div class="mb-2 flex items-center gap-2">
                <FolderOpen class="size-3.5 shrink-0 text-muted-foreground" />
                <h3 class="text-ui-sm font-semibold text-foreground/40">
                  {grpData.group}
                </h3>
                <span
                  class="rounded-full bg-muted/40 px-1.5 font-mono text-ui-2xs text-muted-foreground"
                  >0</span
                >
                {#if grpData.group !== "Default"}
                  <button
                    type="button"
                    class={cn(btnIconSm, "size-5 hover:text-destructive")}
                    title="Delete empty group"
                    onclick={() => handleDeleteGroup(grpData.group)}
                  >
                    <Trash2 class="size-3 shrink-0" />
                  </button>
                {/if}
              </div>
              <div
                class="rounded-lg border border-dashed border-border/40 py-4 text-center text-ui-xs text-muted-foreground"
              >
                No charts in this group
              </div>
            </section>
          {/if}
        {/each}
      </div>
    {/if}
  </div>
</div>
{/if}
