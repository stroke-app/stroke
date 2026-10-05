<script>
  import { comboTitle } from '$lib/shortcuts.js'
  import { onMount } from 'svelte'
  import { getAppScale } from '$lib/app-zoom.js'
  import X from '@lucide/svelte/icons/x'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import CircleDot from '@lucide/svelte/icons/circle-dot'
  import Table2 from '@lucide/svelte/icons/table-2'
  import Terminal from '@lucide/svelte/icons/terminal'
  import Pencil from '@lucide/svelte/icons/pencil'
  import Trash from '@lucide/svelte/icons/trash'
  import Plus from '@lucide/svelte/icons/plus'
  import Unplug from '@lucide/svelte/icons/unplug'
  import FileDown from '@lucide/svelte/icons/file-down'
  import ResizeHandle from './ResizeHandle.svelte'
  import { cn } from '$lib/utils.js'
  import { subscribeActivityLog, clearActivityLog } from '$lib/stores/activity-log.js'
  import { clampLogPanelWidth, loadLayout, saveLayout } from '$lib/stores/layout.js'

  let { onclose = () => {} } = $props()

  const initialLayout = loadLayout()
  let width = $state(initialLayout.logPanelWidth)
  let resizeStartWidth = initialLayout.logPanelWidth
  /** App scale sampled at drag start - `dx` is screen px, `width` is px at 100%. */
  let resizeScale = 1

  /** @type {import('$lib/stores/activity-log.js').ActivityEntry[]} */
  let entries = $state([])
  let autoScroll = $state(true)
  /** @type {HTMLDivElement | null} */
  let logEl = $state(null)
  /** @type {'all' | 'error'} */
  let filter = $state('all')

  onMount(() => subscribeActivityLog((log) => { entries = log }))

  // rAF-based auto-scroll - no tick() needed
  $effect(() => {
    const count = entries.length
    if (!autoScroll || !logEl || count === 0) return
    requestAnimationFrame(() => {
      if (logEl && autoScroll) logEl.scrollTop = logEl.scrollHeight
    })
  })

  const displayed = $derived(filter === 'error' ? entries.filter((e) => !e.success) : entries)
  const errorCount = $derived(entries.filter((e) => !e.success).length)

  const TYPE_TAG = /** @type {Record<string,string>} */ ({
    connect: 'CON', disconnect: 'DIS', table_open: 'TAB', row_fetch: 'ERR',
    row_save: 'SAV', row_delete: 'DEL', row_insert: 'INS', sql_exec: 'SQL',
    export: 'EXP', schema_change: 'SCH', error: 'ERR',
  })

  /** @param {import('$lib/stores/activity-log.js').ActivityEntry} e */
  function entryIcon(e) {
    if (e.type === 'connect' || e.type === 'disconnect') return Unplug
    if (e.type === 'table_open' || e.type === 'row_fetch') return Table2
    if (e.type === 'row_save')   return Pencil
    if (e.type === 'row_delete') return Trash
    if (e.type === 'row_insert') return Plus
    if (e.type === 'sql_exec')   return Terminal
    if (e.type === 'export')     return FileDown
    return CircleDot
  }

  /** @param {import('$lib/stores/activity-log.js').ActivityEntry} e */
  function entryColor(e) {
    if (!e.success)              return 'text-destructive'
    if (e.type === 'connect')    return 'text-primary'
    if (e.type === 'disconnect') return 'text-muted-foreground'
    if (e.type === 'row_delete') return 'text-warning'
    if (e.type === 'row_insert') return 'text-success'
    if (e.type === 'row_save')   return 'text-info'
    if (e.type === 'sql_exec')   return 'text-foreground/80'
    return 'text-muted-foreground'
  }

  /** @param {number} ts */
  function hms(ts) {
    return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false })
  }

  let _scrollRaf = 0
  function handleScroll() {
    if (!logEl || _scrollRaf) return
    _scrollRaf = requestAnimationFrame(() => {
      _scrollRaf = 0
      if (!logEl) return
      autoScroll = logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight < 40
    })
  }
</script>

<div
  class="flex h-full min-w-0 shrink-0 flex-col border-l border-border bg-panel"
  style="width: calc({width}px * var(--app-scale, 1)); min-width: calc({width}px * var(--app-scale, 1)); max-width: calc({width}px * var(--app-scale, 1))"
>
  <ResizeHandle
    edge="start"
    onresizestart={() => { resizeStartWidth = width; resizeScale = getAppScale() }}
    onresize={(dx) => { width = clampLogPanelWidth(resizeStartWidth - dx / resizeScale) }}
    onresizeend={() => saveLayout({ logPanelWidth: width })}
  />

  <!-- Header -->
  <div class="studio-chrome flex h-9 shrink-0 items-center gap-1.5 border-b border-border px-3">
    <CircleDot class="size-3.5 shrink-0 text-primary" />
    <span class="flex-1 font-mono text-ui-sm font-medium">Activity</span>

    <!-- Segmented filter -->
    <div class="inline-flex h-6 items-center rounded border border-border/60 bg-muted/40 p-px">
      <button
        type="button"
        class={cn('h-5 rounded-[3px] px-2 font-mono text-ui-2xs transition-all',
          filter === 'all' ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground')}
        onclick={() => { filter = 'all' }}
      >All</button>
      <button
        type="button"
        class={cn('h-5 rounded-[3px] px-2 font-mono text-ui-2xs transition-all',
          filter === 'error' ? 'bg-card text-destructive shadow-sm' : 'text-muted-foreground hover:text-foreground')}
        onclick={() => { filter = 'error' }}
      >Err</button>
    </div>

    <button
      type="button"
      class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-destructive disabled:opacity-30"
      title="Clear"
      onclick={() => clearActivityLog()}
      disabled={entries.length === 0}
    ><Trash2 class="size-3" /></button>

    <button
      type="button"
      class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground"
      title={`Close (${comboTitle('Mod+Shift+L')})`}
      onclick={onclose}
    ><X class="size-3" /></button>
  </div>

  <!-- Log feed -->
  <div
    bind:this={logEl}
    class="app-scroll min-h-0 flex-1 overflow-y-auto p-1 [will-change:transform] [overflow-anchor:none]"
    onscroll={handleScroll}
  >
    {#if displayed.length === 0}
      <div class="flex flex-col items-center justify-center gap-2 py-10 text-center">
        <CircleDot class="size-7 text-muted-foreground" />
        <p class="font-mono text-ui-2xs text-muted-foreground">
          {entries.length === 0 ? 'Waiting for activity…' : 'No errors logged'}
        </p>
      </div>
    {:else}
      {#each displayed as entry (entry.id)}
        {@const Icon = entryIcon(entry)}
        {@const color = entryColor(entry)}
        {@const tag = TYPE_TAG[entry.type] ?? '???'}
        <div
          class={cn(
            'flex min-w-0 items-baseline gap-1.5 rounded px-1.5 py-[3px] font-mono text-ui-2xs leading-5 transition-colors hover:bg-accent/25',
            !entry.success && 'bg-destructive/5',
          )}
          title={entry.detail || entry.error || entry.title}
        >
          <span class="w-[52px] shrink-0 select-none text-ui-3xs tabular-nums text-muted-foreground">
            {hms(entry.timestamp)}
          </span>
          <span class={cn('w-7 shrink-0 select-none text-ui-3xs font-bold uppercase tracking-wider', color)}>
            {tag}
          </span>
          <span class={cn('min-w-0 flex-1 truncate', color)}>
            {entry.title}
            {#if entry.table && entry.type !== 'table_open'}
              <span class="text-muted-foreground"> · {entry.table}</span>
            {/if}
          </span>
          {#if entry.durationMs != null}
            <span class="shrink-0 text-ui-3xs tabular-nums text-muted-foreground">{entry.durationMs}ms</span>
          {/if}
        </div>
        {#if entry.error}
          <div class="truncate rounded px-1.5 py-px font-mono text-ui-3xs text-destructive" title={entry.error}>
            &nbsp;&nbsp;↳ {entry.error.split('\n')[0]}
          </div>
        {/if}
      {/each}
    {/if}
  </div>

  <!-- Footer -->
  <div class="flex shrink-0 items-center justify-between border-t border-border/50 px-3 py-1">
    <span class="font-mono text-ui-2xs text-muted-foreground tabular-nums">
      {entries.length} events
      {#if errorCount > 0}
        · <span class="text-destructive">{errorCount} err</span>
      {/if}
    </span>
    {#if !autoScroll}
      <button
        type="button"
        class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 font-mono text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={() => { autoScroll = true; if (logEl) logEl.scrollTop = logEl.scrollHeight }}
      >
        <ChevronDown class="size-3" />Latest
      </button>
    {/if}
  </div>
</div>
