<script>
  import { comboTitle } from '$lib/shortcuts.js'
  /**
   * One SQL cell: a header (run counter, the {{variables}} it uses, status,
   * actions), the editor sized to its query, and the result.
   */
  import Play from '@lucide/svelte/icons/play'
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import Download from '@lucide/svelte/icons/download'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import ArrowUp from '@lucide/svelte/icons/arrow-up'
  import ArrowDown from '@lucide/svelte/icons/arrow-down'
  import Copy from '@lucide/svelte/icons/copy'
  import CopyPlus from '@lucide/svelte/icons/copy-plus'
  import Braces from '@lucide/svelte/icons/braces'
  import { cn, errorText } from '$lib/utils.js'
  import { saveExportAs } from '$lib/api.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { variableNames } from '$lib/notebook.js'
  import SqlEditor from '$lib/components/SqlEditor.svelte'

  /**
   * @type {{
   *   cellIndex: number,
   *   content: string,
   *   result: import('$lib/notebook.js').CellResult | null,
   *   running: boolean,
   *   runCount?: number | null,
   *   variables?: string[],
   *   onchange: (v: string) => void,
   *   onrun: () => void,
   *   onrunadvance?: () => void,
   *   onduplicate?: () => void,
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
    runCount = null,
    variables = [],
    onchange,
    onrun,
    onrunadvance = () => {},
    onduplicate = () => {},
    onmoveup,
    onmovedown,
    onremove,
    canmoveup = true,
    canmovedown = true,
    canremove = true,
  } = $props()

  // The editor fits the query (3 to 24 lines); a drag on the handle wins.
  const LINE = 20
  const autoHeight = $derived(Math.min(24 * LINE + 16, Math.max(3 * LINE + 16, String(content).split('\n').length * LINE + 16)))
  let manualHeight = $state(/** @type {number | null} */ (null))
  const editorHeight = $derived(manualHeight ?? autoHeight)
  let resizing = $state(false)
  let resizeStartY = 0
  let resizeStartH = 0
  let resultsCollapsed = $state(false)

  const MAX_DISPLAY_ROWS = 500
  const displayRows = $derived(result ? result.rows.slice(0, MAX_DISPLAY_ROWS) : [])
  const truncated = $derived(result ? result.rows.length > MAX_DISPLAY_ROWS : false)
  const hasError = $derived(!!result?.error)
  const usedVars = $derived(variableNames(content))
  const known = $derived(new Set(variables))

  /** @param {unknown} v */
  const show = (v) => (v !== null && typeof v === 'object' ? JSON.stringify(v) : String(v))

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
    manualHeight = Math.max(60, Math.min(900, resizeStartH + e.clientY - resizeStartY))
  }
  function onResizeEnd() { resizing = false }

  /** Shift+Enter runs and moves on, as in Jupyter. @param {KeyboardEvent} e */
  function onKeydownCapture(e) {
    if (e.key === 'Enter' && e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault()
      e.stopPropagation()
      onrunadvance()
    }
  }

  async function exportCsv() {
    if (!result?.columns.length) return
    const header = result.columns.map((c) => c.name).join(',')
    const rows = result.rows.map((r) => r.map((v) => (v === null ? '' : JSON.stringify(show(v)))).join(','))
    try {
      const path = await saveExportAs([header, ...rows].join('\n'), `query-${cellIndex + 1}.csv`, { name: 'CSV', extensions: ['csv'] })
      if (path) toast.success('Exported as CSV', { description: `Saved to ${path}` })
    } catch (e) {
      toast.error('Export failed', { description: errorText(e) })
    }
  }

  async function copyTsv() {
    if (!result?.columns.length) return
    const text = [result.columns.map((c) => c.name).join('\t'), ...result.rows.map((r) => r.map((v) => (v === null ? '' : show(v).replace(/[\t\n]/g, ' '))).join('\t'))].join('\n')
    try {
      await navigator.clipboard.writeText(text)
      toast.success(`Copied ${result.rows.length} rows`, { description: 'Tab separated: pastes into a spreadsheet.' })
    } catch {
      toast.error('Could not copy to the clipboard')
    }
  }

  const ranLabel = $derived.by(() => {
    if (!result?.ranAt) return ''
    const s = Math.round((Date.now() - result.ranAt) / 1000)
    return s < 60 ? 'just now' : s < 3600 ? `${Math.floor(s / 60)}m ago` : new Date(result.ranAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  })
  const iconBtn = 'inline-flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-30'
</script>

<svelte:window onmousemove={onResizeMove} onmouseup={onResizeEnd} />

<div class="group/sql relative flex flex-col" onkeydowncapture={onKeydownCapture} role="group" aria-label="SQL cell {cellIndex + 1}">
  <!-- Header -->
  <div class="flex h-8 shrink-0 items-center gap-2 px-4">
    <span class="w-9 shrink-0 select-none font-mono text-ui-2xs tabular-nums text-muted-foreground" title="Run order">
      [{running ? '*' : runCount ?? ' '}]
    </span>
    <span class="select-none text-ui-3xs font-semibold tracking-wide text-info">SQL</span>
    {#each usedVars as v (v)}
      <span
        class={cn('inline-flex items-center gap-1 rounded-md px-1.5 py-px font-mono text-ui-3xs', known.has(v) ? 'bg-muted/60 text-muted-foreground' : 'bg-warning/10 text-warning')}
        title={known.has(v) ? `Filled from the variable ${v}` : `No variable named ${v}: add it above`}
      ><Braces class="size-3 shrink-0" />{v}</span>
    {/each}
    <div class="ml-auto flex items-center gap-1.5">
      {#if running}
        <span class="flex items-center gap-1 text-ui-3xs text-muted-foreground"><Loader2 class="size-3 shrink-0 animate-spin" />running…</span>
      {:else if result && !hasError}
        <span class="text-ui-3xs tabular-nums text-muted-foreground">
          {result.columns.length ? `${result.rows.length}${truncated ? '+' : ''} row${result.rows.length === 1 ? '' : 's'} · ` : ''}{result.queryMs} ms{ranLabel ? ` · ${ranLabel}` : ''}
        </span>
      {:else if hasError}
        <span class="text-ui-3xs text-destructive">failed</span>
      {/if}
      <span class="flex items-center gap-0.5 opacity-0 transition-opacity group-hover/sql:opacity-100 group-focus-within/sql:opacity-100">
        <button type="button" onclick={onmoveup} disabled={!canmoveup} class={iconBtn} title="Move up" aria-label="Move up"><ArrowUp class="size-3.5 shrink-0" /></button>
        <button type="button" onclick={onmovedown} disabled={!canmovedown} class={iconBtn} title="Move down" aria-label="Move down"><ArrowDown class="size-3.5 shrink-0" /></button>
        <button type="button" onclick={onduplicate} class={iconBtn} title="Duplicate cell" aria-label="Duplicate cell"><CopyPlus class="size-3.5 shrink-0" /></button>
        <button type="button" onclick={onremove} disabled={!canremove} class={cn(iconBtn, 'hover:bg-destructive/10 hover:text-destructive')} title="Delete cell" aria-label="Delete cell"><Trash2 class="size-3.5 shrink-0" /></button>
      </span>
      <button
        type="button"
        onclick={onrun}
        disabled={running || !content.trim()}
        title={`Run (${comboTitle('Mod+Enter')}) · run and go to the next cell (${comboTitle('Shift+Enter')})`}
        class="inline-flex h-6 items-center gap-1 rounded-md bg-primary/90 px-2 text-ui-2xs font-medium text-primary-foreground transition-colors hover:bg-primary disabled:pointer-events-none disabled:opacity-40"
      >
        <Play class="size-3 shrink-0" />Run
      </button>
    </div>
  </div>

  <!-- Editor, sized to its query -->
  <div style="height:{editorHeight}px" class="relative w-full overflow-hidden">
    <SqlEditor value={content} onchange={onchange} onmodenter={onrun} class="h-full w-full" />
  </div>

  <!-- Resize handle (double-click: back to fitting the query) -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    role="separator"
    aria-orientation="horizontal"
    tabindex="-1"
    class={cn(
      'flex h-1.5 w-full cursor-row-resize select-none items-center justify-center border-y border-border/20 transition-colors hover:border-primary/30 hover:bg-primary/5',
      resizing && 'border-primary/30 bg-primary/5',
    )}
    onmousedown={onResizeStart}
    ondblclick={() => (manualHeight = null)}
    title="Drag to resize · double-click to fit the query"
  >
    <div class="h-px w-8 rounded-full bg-border/50"></div>
  </div>

  <!-- Result -->
  {#if result}
    <div>
      <div class="flex items-center gap-2 px-4 py-1.5 text-ui-xs">
        <button type="button" onclick={() => (resultsCollapsed = !resultsCollapsed)} class="flex min-w-0 flex-1 items-center gap-1.5 hover:opacity-80" aria-expanded={!resultsCollapsed}>
          {#if resultsCollapsed}<ChevronRight class="size-3 shrink-0 text-muted-foreground" />{:else}<ChevronDown class="size-3 shrink-0 text-muted-foreground" />{/if}
          {#if hasError}
            <span class="font-medium text-destructive">Error</span>
          {:else if result.columns.length}
            <span class="text-muted-foreground">Output</span>
          {:else}
            <span class="text-success">{result.message || 'Done'}</span>
          {/if}
        </button>
        {#if !resultsCollapsed && result.columns.length && !hasError}
          <button type="button" onclick={() => void copyTsv()} class="flex shrink-0 items-center gap-1 rounded-md px-1.5 py-0.5 text-muted-foreground hover:bg-muted hover:text-foreground" title="Copy as tab-separated text">
            <Copy class="size-3 shrink-0" /><span class="text-ui-3xs">Copy</span>
          </button>
          <button type="button" onclick={() => void exportCsv()} class="flex shrink-0 items-center gap-1 rounded-md px-1.5 py-0.5 text-muted-foreground hover:bg-muted hover:text-foreground" title="Export CSV">
            <Download class="size-3 shrink-0" /><span class="text-ui-3xs">CSV</span>
          </button>
        {/if}
      </div>

      {#if !resultsCollapsed}
        {#if hasError}
          <div class="mx-4 mb-3 rounded-md border border-destructive/20 bg-destructive/[0.04] px-4 py-3 font-mono text-ui-xs leading-relaxed whitespace-pre-wrap text-destructive">{result.error}</div>
        {:else if result.columns.length}
          <div class="max-h-[360px] overflow-auto border-t border-border/20">
            <table class="w-full min-w-max border-collapse text-ui-xs">
              <thead class="sticky top-0 z-10">
                <tr class="bg-background">
                  <th class="w-8 border-r border-b border-border/30 px-3 py-1.5 text-right font-normal text-muted-foreground select-none">#</th>
                  {#each result.columns as col, ci (ci)}
                    <th class="border-r border-b border-border/30 px-4 py-1.5 text-left text-muted-foreground">
                      <span class="font-medium">{col.name}</span>
                      {#if col.dataType}<span class="ml-1.5 text-ui-3xs font-normal text-muted-foreground">{col.dataType}</span>{/if}
                    </th>
                  {/each}
                </tr>
              </thead>
              <tbody>
                {#each displayRows as row, ri (ri)}
                  <tr class={cn('border-b border-border/10 hover:bg-muted/20', ri % 2 === 1 && 'bg-muted/[0.04]')}>
                    <td class="border-r border-border/20 px-3 py-1 text-right font-mono text-ui-3xs text-muted-foreground select-none">{ri + 1}</td>
                    {#each row as cell, ci (ci)}
                      <td class="max-w-[20rem] border-r border-border/10 px-4 py-1 font-mono">
                        {#if cell === null || cell === undefined}
                          <span class="text-muted-foreground italic">NULL</span>
                        {:else}
                          <span class="block truncate" title={show(cell)}>{show(cell)}</span>
                        {/if}
                      </td>
                    {/each}
                  </tr>
                {/each}
                {#if !result.rows.length}
                  <tr><td colspan={result.columns.length + 1} class="px-4 py-3 text-center text-ui-xs text-muted-foreground">The query returned no rows</td></tr>
                {/if}
                {#if truncated}
                  <tr><td colspan={result.columns.length + 1} class="px-4 py-2 text-center text-ui-xs text-muted-foreground italic">Showing first {MAX_DISPLAY_ROWS} of {result.rows.length} rows</td></tr>
                {/if}
              </tbody>
            </table>
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</div>
