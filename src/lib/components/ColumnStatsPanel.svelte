<script>
  /**
   * Side panel showing quick stats for a single column.
   * @typedef {{ column: string, count: number, nullCount: number, distinctCount: number|null, min: string|null, max: string|null, avg: number|null }} ColStats
   */
  import { getColumnStats } from "$lib/api.js";
  import X from "@lucide/svelte/icons/x";
  import BarChart2 from "@lucide/svelte/icons/bar-chart-2";

  /** @type {{ schema: string, table: string, column: string, dataType?: string, onclose: () => void }} */
  let { schema, table, column, dataType = "", onclose } = $props();

  /** @type {ColStats | null} */
  let stats = $state(null);
  let error = $state("");
  let loading = $state(true);

  $effect(() => {
    // Re-run whenever the column changes
    void column;
    loading = true; error = ""; stats = null;
    getColumnStats(schema, table, column)
      .then((s) => { stats = s; loading = false; })
      .catch((e) => { error = String(e); loading = false; });
  });

  const nullPct = $derived(
    stats && stats.count > 0
      ? Math.round((stats.nullCount / stats.count) * 100)
      : 0
  );
  const filledPct = $derived(100 - nullPct);
</script>

<div class="flex h-full w-64 shrink-0 flex-col border-l border-border/50 bg-panel text-ui-sm">
  <!-- Header -->
  <div class="flex items-center gap-2 border-b border-border/40 px-3 py-2">
    <BarChart2 class="size-3.5 shrink-0 text-muted-foreground" />
    <span class="min-w-0 flex-1 truncate font-mono font-medium text-foreground">{column}</span>
    {#if dataType}
      <span class="shrink-0 font-mono text-ui-2xs text-muted-foreground">{dataType}</span>
    {/if}
    <button
      type="button"
      class="ml-1 inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
      onclick={onclose}
      aria-label="Close stats"
    >
      <X class="size-3 shrink-0" />
    </button>
  </div>

  <!-- Body -->
  <div class="flex-1 overflow-y-auto px-3 py-3">
    {#if loading}
      <div class="flex items-center gap-2 text-muted-foreground">
        <span class="inline-block size-3 animate-spin rounded-full border-2 border-current border-t-transparent"></span>
        Computing…
      </div>
    {:else if error}
      <p class="text-ui-xs text-destructive">{error}</p>
    {:else if stats}
      <!-- Fill/null bar -->
      <div class="mb-4">
        <div class="mb-1 flex justify-between text-ui-2xs text-muted-foreground">
          <span>Filled {filledPct}%</span>
          <span>Null {nullPct}%</span>
        </div>
        <div class="flex h-1.5 w-full overflow-hidden rounded-full bg-muted">
          <div
            class="h-full rounded-full bg-primary transition-[width]"
            style="width:{filledPct}%"
          ></div>
        </div>
      </div>

      <!-- Stats rows -->
      <dl class="space-y-2">
        {@render statRow("Total rows", stats.count.toLocaleString())}
        {@render statRow("Non-null", (stats.count - stats.nullCount).toLocaleString())}
        {@render statRow("Null", stats.nullCount.toLocaleString())}
        {#if stats.distinctCount !== null}
          {@render statRow("Distinct", stats.distinctCount.toLocaleString())}
        {/if}
        {#if stats.min !== null}
          {@render statRow("Min", stats.min)}
        {/if}
        {#if stats.max !== null}
          {@render statRow("Max", stats.max)}
        {/if}
        {#if stats.avg !== null}
          {@render statRow("Avg", stats.avg.toFixed(4).replace(/\.?0+$/, ""))}
        {/if}
      </dl>
    {/if}
  </div>
</div>

{#snippet statRow(label, value)}
  <div class="flex items-baseline justify-between gap-2">
    <dt class="shrink-0 text-ui-xs text-muted-foreground">{label}</dt>
    <dd class="min-w-0 truncate text-right font-mono text-ui-xs text-foreground" title={String(value)}>{value}</dd>
  </div>
{/snippet}
