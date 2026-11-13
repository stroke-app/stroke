<script>
  /**
   * Proportional meter chart - pure SVG/HTML, no extra dependencies.
   * x_col = segment label, y_col = value, z_col = optional total override.
   */
  let {
    spec = /** @type {any} */ (null),
    noTitle = false,
  } = $props()

  const PALETTE = ['#6366f1','#a855f7','#3b82f6','#22c55e','#f59e0b','#ef4444','#14b8a6','#f97316','#ec4899','#64748b']

  const segments = $derived.by(() => {
    if (!spec?.data?.length) return []
    return spec.data.map((row, i) => ({
      name: String(row[spec.x_col] ?? `Segment ${i + 1}`),
      value: Math.max(0, Number(row[spec.y_col] ?? 0)),
      color: PALETTE[i % PALETTE.length],
    }))
  })

  const used = $derived(segments.reduce((s, d) => s + d.value, 0))
  const total = $derived.by(() => {
    if (spec?.z_col && spec.data?.[0]?.[spec.z_col] != null) return Math.max(used, Number(spec.data[0][spec.z_col]))
    return used || 100
  })
  const pct = $derived(segments.map(s => ({ ...s, width: total > 0 ? (s.value / total) * 100 : 0 })))
  const remain = $derived(Math.max(0, total - used))
</script>

{#if !segments.length}
  <div class="flex h-full items-center justify-center text-ui-xs text-muted-foreground">No data</div>
{:else}
  <div class="flex h-full flex-col justify-center gap-3 px-4 py-3">
    {#if !noTitle && spec?.title}
      <p class="font-mono text-ui-sm font-semibold text-foreground">{spec.title}</p>
    {/if}

    <!-- Usage line -->
    <div class="flex items-baseline justify-between">
      <span class="font-mono text-ui-2xs text-muted-foreground">
        {used.toLocaleString()} used
        {#if remain > 0}· {remain.toLocaleString()} available{/if}
      </span>
      <span class="font-mono text-ui-2xs text-muted-foreground">{total.toLocaleString()} total</span>
    </div>

    <!-- Bar -->
    <div class="flex h-6 w-full overflow-hidden rounded-md bg-muted/20">
      {#each pct as seg (seg.name)}
        <div
          class="h-full transition-[width] duration-200"
          style="width:{seg.width}%; background:{seg.color};"
          title="{seg.name}: {seg.value.toLocaleString()} ({seg.width.toFixed(1)}%)"
        ></div>
      {/each}
    </div>

    <!-- Legend -->
    <div class="flex flex-wrap gap-x-4 gap-y-1.5">
      {#each segments as seg (seg.name)}
        <div class="flex items-center gap-1.5">
          <span class="size-2.5 shrink-0 rounded-sm" style="background:{seg.color}"></span>
          <span class="font-mono text-ui-3xs text-muted-foreground">{seg.name}</span>
          <span class="font-mono text-ui-3xs text-foreground/50">{seg.value.toLocaleString()}</span>
        </div>
      {/each}
    </div>
  </div>
{/if}
