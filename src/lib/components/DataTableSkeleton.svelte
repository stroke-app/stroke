<script>
  import { cn } from '$lib/utils.js'
  import { defaultColumnWidth } from '$lib/table-column-widths.js'

  let {
    columns = [],
    /** @type {Record<string, number>} */
    columnWidths = {},
    columnCount = 6,
    rowCount = 14,
    showRowExpand = true,
    showSelection = true,
  } = $props()

  const displayColumns = $derived(
    columns.length > 0
      ? columns
      : Array.from({ length: Math.max(1, Math.min(columnCount, 10)) }, (_, i) => ({
          name: `col-${i}`,
          data_type: 'text',
        })),
  )

  /** @param {{ name: string, dataType?: string, data_type?: string }} col */
  function colWidth(col) {
    return columnWidths[col.name] ?? defaultColumnWidth(col.dataType ?? col.data_type ?? '')
  }

  /** Bar widths as fraction of cell - short left-aligned pills like real cell text */
  const barFracs = [0.52, 0.68, 0.41, 0.76, 0.48, 0.61, 0.55, 0.37, 0.72, 0.44]

  /** @param {number} rowIdx @param {number} colIdx */
  function barStyle(rowIdx, colIdx) {
    const frac = barFracs[(rowIdx + colIdx) % barFracs.length]
    return `width: ${Math.round(frac * 100)}%`
  }
</script>

<div class="loading-reveal min-h-0 w-full min-w-full" role="status" aria-label="Loading rows">
  <table class="studio-data-table w-max min-w-full table-fixed text-ui-sm">
    <colgroup>
      {#if showRowExpand}
        <col style="width: 34px" />
      {/if}
      {#if showSelection}
        <col style="width: 32px" />
      {/if}
      {#each displayColumns as col (col.name)}
        <col style="width: {colWidth(col)}px" />
      {/each}
    </colgroup>
    <thead class="studio-chrome sticky top-0 z-10 bg-panel">
      <tr>
        {#if showRowExpand}
          <th
            class="studio-table-gutter"
            style="width: 34px; min-width: 34px; max-width: 34px"
            aria-hidden="true"
          ></th>
        {/if}
        {#if showSelection}
          <th
            class="studio-table-gutter"
            style="width: 32px; min-width: 32px; max-width: 32px"
            aria-hidden="true"
          >
            <div class="studio-table-gutter-inner">
              <div class="skeleton size-3.5 rounded-[4px]" aria-hidden="true"></div>
            </div>
          </th>
        {/if}
        {#each displayColumns as col (col.name)}
          {@const w = colWidth(col)}
          <th
            class="overflow-hidden px-3 py-1 text-left font-normal"
            style="width: {w}px; min-width: {w}px; max-width: {w}px"
            aria-hidden="true"
          >
            <div class="flex min-w-0 flex-col gap-px leading-tight">
              <div class="skeleton h-3 max-w-[min(7rem,75%)] rounded-md" aria-hidden="true"></div>
              <div class="skeleton h-2 max-w-[min(4.5rem,50%)] rounded-md opacity-80" aria-hidden="true"></div>
            </div>
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each Array(rowCount) as _, rowIdx}
        <tr>
          {#if showRowExpand}
            <td
              class="studio-table-gutter"
              style="width: 34px; min-width: 34px; max-width: 34px"
              aria-hidden="true"
            ></td>
          {/if}
          {#if showSelection}
            <td
              class="studio-table-gutter"
              style="width: 32px; min-width: 32px; max-width: 32px"
              aria-hidden="true"
            >
              <div class="studio-table-gutter-inner">
                <div class="skeleton size-3.5 rounded-[4px]" aria-hidden="true"></div>
              </div>
            </td>
          {/if}
          {#each displayColumns as col, colIdx (col.name)}
            {@const w = colWidth(col)}
            <td
              class="overflow-hidden px-3 py-0.5 align-middle"
              style="width: {w}px; min-width: {w}px; max-width: {w}px"
              aria-hidden="true"
            >
              <div
                class={cn('skeleton h-3 rounded-md')}
                style={barStyle(rowIdx, colIdx)}
                aria-hidden="true"
              ></div>
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
  <span class="sr-only">Loading rows…</span>
</div>
