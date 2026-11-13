<script>
  let { rows = 12 } = $props()

  const nameWidths = ['72%', '58%', '85%', '64%', '78%', '52%', '90%', '66%', '74%', '48%', '82%', '61%']
</script>

<ul class="loading-reveal flex w-full min-w-full flex-col gap-0.5 px-1.5 pb-2" role="status" aria-label="Loading tables">
  {#each Array(rows) as _, i}
    <li class="flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5">
      <div class="skeleton size-3 shrink-0 rounded-md" aria-hidden="true"></div>
      <div
        class="skeleton h-3.5 min-w-0 flex-1 rounded-md"
        style="max-width: {nameWidths[i % nameWidths.length]}"
        aria-hidden="true"
      ></div>
      <div class="skeleton h-3 w-9 shrink-0 rounded-md" aria-hidden="true"></div>
    </li>
  {/each}
  <span class="sr-only">Loading tables…</span>
</ul>
