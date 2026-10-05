<script>
  import Play from '@lucide/svelte/icons/play'
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import Download from '@lucide/svelte/icons/download'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import { cn } from '$lib/utils.js'
  import { saveExportAs } from '$lib/api.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import SqlEditor from '$lib/components/SqlEditor.svelte'

  /**
   * @type {{
   *   cellIndex: number,
   *   content: string,
   *   result: import('$lib/notebook.js').CellResult | null,
   *   running: boolean,
   *   onchange: (v: string) => void,
   *   onrun: () => void,
   *   onmoveup?: () => void,
   *   onmovedown?: () => void,
   *   onremove?: () => void,
   *   canmoveup?: boolean,
   *   canmovedown?: boolean,
   *   canremove?: boolean,
   * }}
   */
  let {
    cellIndex,
    content,
    result,
    running,
    onchange,
    onrun,
    onmoveup,
    onmovedown,
    onremove,
    canmoveup = true,
    canmovedown = true,
    canremove = true,
  } = $props()

  let editorHeight = $state(180)
  let resizing = $state(false)
  let resizeStartY = 0
  let resizeStartH = 0
  let resultsCollapsed = $state(false)

  const MAX_DISPLAY_ROWS = 500
  const hasResults = $derived(result !== null)
  const displayRows = $derived(result ? result.rows.slice(0, MAX_DISPLAY_ROWS) : [])
  const truncated = $derived(result ? result.rows.length > MAX_DISPLAY_ROWS : false)
  const hasError = $derived(!!result?.error)

  /** @param {MouseEvent} e */
  function onResizeStart(e) {
    resizing = true
    resizeStartY = e.clientY
    resizeStartH = editorHeight
    e.preventDefault()
  }

  /** @param {MouseEvent} e */
  function onResizeMove(e) {
    if (!resizing) return
    editorHeight = Math.max(80, Math.min(800, resizeStartH + e.clientY - resizeStartY))
  }

  function onResizeEnd() { resizing = false }

  async function exportCsv() {
    if (!result?.columns.length) return
    const header = result.columns.map((c) => c.name).join(',')
    const rows = result.rows.map((r) =>
      r.map((v) => (v === null ? '' : JSON.stringify(String(v)))).join(','),
    )
    // `<a download>` is ignored by WKWebView, so the anchor this replaced did
    // nothing at all in the desktop app.
    try {
      const path = await saveExportAs(
        [header, ...rows].join('\n'),
        `query-${cellIndex + 1}.csv`,
        { name: 'CSV', extensions: ['csv'] },
      )
      if (!path) return  // dialog cancelled
      toast.success('Exported as CSV', { description: `Saved to ${path}` })
    } catch (e) {
      toast.error('Export failed', { description: String(e) })
    }
  }
</script>

<svelte:window onmousemove={onResizeMove} onmouseup={onResizeEnd} />

<div class="group/sql relative flex flex-col">
  <!-- Cell toolbar -->
  <div
    class="flex h-7 shrink-0 items-center gap-2 px-4 transition-opacity"
  >
    <span
      class="select-none rounded px-1.5 py-px text-ui-3xs font-semibold uppercase tracking-widest text-blue-500/60"
    >
      [{cellIndex + 1}] sql
    </span>
    <span class="text-ui-3xs text-muted-foreground">⌘↵ run</span>
    {#if running}
      <span class="flex items-center gap-1 text-ui-3xs text-muted-foreground">
        <Loader2 class="size-3 animate-spin" />running…
      </span>
    {/if}
    <div class="ml-auto flex items-center gap-1">
      <button
        onclick={onmoveup}
        disabled={!canmoveup}
        class="rounded px-1 py-0.5 text-ui-xs text-muted-foreground opacity-0 transition-opacity group-hover/sql:opacity-100 hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-0"
        title="Move up"
      >↑</button>
      <button
        onclick={onmovedown}
        disabled={!canmovedown}
        class="rounded px-1 py-0.5 text-ui-xs text-muted-foreground opacity-0 transition-opacity group-hover/sql:opacity-100 hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-0"
        title="Move down"
      >↓</button>
      <button
        onclick={onremove}
        disabled={!canremove}
        class="rounded p-0.5 text-muted-foreground opacity-0 transition-opacity group-hover/sql:opacity-100 hover:bg-destructive/10 hover:text-destructive disabled:pointer-events-none disabled:opacity-0"
        title="Delete cell"
      >
        <Trash2 class="size-3" />
      </button>
      <button
        onclick={onrun}
        disabled={running}
        class={cn(
          'flex items-center gap-1 rounded px-2 py-0.5 text-ui-xs font-medium transition-colors',
          'bg-primary/90 text-primary-foreground hover:bg-primary disabled:pointer-events-none disabled:opacity-40',
        )}
      >
        <Play class="size-3" />Run
      </button>
    </div>
  </div>

  <!-- SQL editor -->
  <div style="height:{editorHeight}px" class="relative w-full overflow-hidden">
    <SqlEditor
      value={content}
      onchange={onchange}
      onmodenter={onrun}
      class="h-full w-full"
    />
  </div>

  <!-- Resize handle -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    role="none"
    class={cn(
      'flex h-1.5 w-full cursor-row-resize select-none items-center justify-center border-y border-border/20',
      'transition-colors hover:border-primary/30 hover:bg-primary/5',
      resizing && 'border-primary/30 bg-primary/5',
    )}
    onmousedown={onResizeStart}
  >
    <div class="h-px w-8 rounded-full bg-border/50"></div>
  </div>

  <!-- Results -->
  {#if hasResults}
    <div>
      <!-- Results header -->
      <div class="flex items-center gap-2 px-4 py-1.5 text-ui-xs">
        <button
          onclick={() => { resultsCollapsed = !resultsCollapsed }}
          class="flex min-w-0 flex-1 items-center gap-1.5 hover:opacity-80"
        >
          {#if resultsCollapsed}
            <ChevronRight class="size-3 shrink-0 text-muted-foreground" />
          {:else}
            <ChevronDown class="size-3 shrink-0 text-muted-foreground" />
          {/if}

          {#if hasError}
            <span class="font-medium text-destructive">Error</span>
          {:else}
            <span class="text-muted-foreground">Output</span>
            {#if result?.columns.length}
              <span class="text-muted-foreground">
                {result.rows.length}{truncated ? '+' : ''} row{result.rows.length !== 1 ? 's' : ''}
              </span>
              <span class="text-muted-foreground">·</span>
              <span class="text-muted-foreground">{result.queryMs}ms</span>
            {:else if result?.message}
              <span class="text-emerald-500/80">{result.message}</span>
            {/if}
          {/if}
        </button>

        {#if !resultsCollapsed && result?.columns.length && !hasError}
          <button
            onclick={() => void exportCsv()}
            class="ml-auto flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
            title="Export CSV"
          >
            <Download class="size-3" />
            <span class="text-ui-3xs">CSV</span>
          </button>
        {/if}
      </div>

      {#if !resultsCollapsed}
        {#if hasError}
          <div
            class="mx-4 mb-3 rounded-md border border-destructive/20 bg-destructive/[0.04] px-4 py-3 font-mono text-ui-xs leading-relaxed text-destructive whitespace-pre-wrap"
          >
            {result?.error}
          </div>
        {:else if result?.columns.length}
          <div class="max-h-[360px] overflow-auto border-t border-border/20">
            <table class="w-full min-w-max border-collapse text-ui-xs">
              <thead class="sticky top-0 z-10">
                <tr class="bg-background/95 backdrop-blur-sm">
                  <th
                    class="w-8 border-b border-r border-border/30 px-3 py-1.5 text-right font-normal text-muted-foreground select-none"
                  >#</th>
                  {#each result.columns as col}
                    <th
                      class="border-b border-r border-border/30 px-4 py-1.5 text-left text-muted-foreground"
                    >
                      <span class="font-medium">{col.name}</span>
                      {#if col.dataType}
                        <span class="ml-1.5 font-normal text-muted-foreground text-ui-3xs"
                          >{col.dataType}</span
                        >
                      {/if}
                    </th>
                  {/each}
                </tr>
              </thead>
              <tbody>
                {#each displayRows as row, ri}
                  <tr
                    class={cn(
                      'border-b border-border/10 hover:bg-muted/20',
                      ri % 2 === 1 && 'bg-muted/[0.04]',
                    )}
                  >
                    <td
                      class="border-r border-border/20 px-3 py-1 text-right font-mono text-muted-foreground text-ui-3xs select-none"
                    >{ri + 1}</td>
                    {#each row as cell}
                      <td class="max-w-[320px] border-r border-border/10 px-4 py-1 font-mono">
                        {#if cell === null}
                          <span class="italic text-muted-foreground">NULL</span>
                        {:else}
                          <span class="block truncate">{String(cell)}</span>
                        {/if}
                      </td>
                    {/each}
                  </tr>
                {/each}
                {#if truncated}
                  <tr>
                    <td
                      colspan={result.columns.length + 1}
                      class="px-4 py-2 text-center text-ui-xs italic text-muted-foreground"
                    >
                      Showing first {MAX_DISPLAY_ROWS} of {result.rows.length} rows
                    </td>
                  </tr>
                {/if}
              </tbody>
            </table>
          </div>
        {:else if result?.message}
          <div class="px-4 pb-3 text-ui-xs text-muted-foreground">{result.message}</div>
        {/if}
      {/if}
    </div>
  {/if}
</div>
