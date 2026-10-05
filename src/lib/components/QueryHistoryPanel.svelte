<script>
  import { onDestroy, onMount, tick } from 'svelte'
  import History from '@lucide/svelte/icons/history'
  import Bookmark from '@lucide/svelte/icons/bookmark'
  import Play from '@lucide/svelte/icons/play'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Search from '@lucide/svelte/icons/search'
  import X from '@lucide/svelte/icons/x'
  import BarChart2 from '@lucide/svelte/icons/bar-chart-2'
  import Star from '@lucide/svelte/icons/star'
  import CircleAlert from '@lucide/svelte/icons/circle-alert'
  import { cn } from '$lib/utils.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import {
    clearQueryHistory,
    deleteQueryHistoryEntry,
    deleteSavedQuery,
    restoreQueryHistoryEntry,
    restoreSavedQuery,
    setQueryHistoryFavorite,
  } from '$lib/stores/query-history.js'
  import { savedCharts } from '$lib/stores/saved-charts.js'

  /** @typedef {import('$lib/stores/query-history.js').QueryHistoryEntry} QueryHistoryEntry */
  /** @typedef {import('$lib/stores/query-history.js').SavedQuery} SavedQuery */

  /**
   * The SQL console's lists, shown in its results pane: what ran, what was
   * saved, and the saved charts. They belong to the connection, not to a tab,
   * so every editor tab shows the same entries. They were a column beside the
   * editor that took its width; down here they get the whole row.
   */
  let {
    /** @type {'history' | 'saved' | 'charts'} */
    view = 'history',
    /** @type {QueryHistoryEntry[]} */
    history = [],
    /** @type {SavedQuery[]} */
    saved = [],
    /** Load a query into the editor. @param {string} sql */
    onselect = (sql) => {},
    /** Load a query and run it. @param {string} sql */
    onrun = (sql) => {},
    onrefresh = async () => {},
    /** Escape from the list: back to the editor. */
    onescape = () => {},
  } = $props()

  /**
   * @typedef {{ id: string, kind: 'history' | 'saved' | 'charts', label: string,
   *   detail: string, meta: string, sql: string, starred: boolean, entry: any,
   *   error?: string }} Row
   */

  let filter = $state('')
  /** The row the arrow keys are on. */
  let cursor = $state(0)
  let filterEl = $state(/** @type {HTMLInputElement | null} */ (null))
  let listEl = $state(/** @type {HTMLElement | null} */ (null))

  const isMac =
    typeof navigator !== 'undefined' && /Mac|iPhone|iPad/i.test(navigator.platform)
  const modKey = isMac ? '⌘' : 'Ctrl'

  let now = $state(Date.now())
  const ticker = setInterval(() => { now = Date.now() }, 30_000)
  onDestroy(() => clearInterval(ticker))
  // The list opens to be searched or walked with the arrows.
  onMount(() => { filterEl?.focus() })

  /** @param {number} ts */
  function relativeTime(ts) {
    const diff = now - ts
    const sec = Math.floor(diff / 1000)
    if (sec < 60) return 'just now'
    const min = Math.floor(sec / 60)
    if (min < 60) return `${min}m ago`
    const hr = Math.floor(min / 60)
    if (hr < 24) return `${hr}h ago`
    const day = Math.floor(hr / 24)
    if (day < 7) return `${day}d ago`
    return new Date(ts).toLocaleDateString()
  }

  /** A query on one row. @param {string} sql */
  const oneLine = (sql) => String(sql ?? '').replace(/\s+/g, ' ').trim()

  /** @type {Row[]} */
  const rows = $derived.by(() => {
    const q = filter.trim().toLowerCase()
    const has = (/** @type {Array<string | undefined>} */ ...parts) => !q || parts.some((p) => p?.toLowerCase().includes(q))
    if (view === 'history') {
      return history
        .filter((e) => has(e.sql))
        // Starred first; a stable sort keeps the store's newest-first order in each group.
        .sort((a, b) => Number(!!b.favorite) - Number(!!a.favorite))
        .map((e) => ({
          id: e.id, kind: /** @type {const} */ ('history'), label: oneLine(e.sql), detail: '',
          meta: [relativeTime(e.executedAt), e.queryMs ? `${e.queryMs}ms` : '', (e.runCount ?? 1) > 1 ? `${e.runCount}×` : ''].filter(Boolean).join(' · '),
          sql: e.sql, starred: !!e.favorite, entry: e,
          // A failed run is listed with its error, and loads and runs like any other.
          error: e.success === false ? (e.error || 'Failed') : undefined,
        }))
    }
    if (view === 'saved') {
      return saved
        .filter((e) => has(e.name, e.sql))
        .map((e) => ({
          id: e.id, kind: /** @type {const} */ ('saved'), label: e.name, detail: oneLine(e.sql),
          meta: relativeTime(e.updatedAt), sql: e.sql, starred: false, entry: e,
        }))
    }
    return $savedCharts
      .filter((c) => has(c.name, c.sql, c.group))
      .map((c) => ({
        id: c.id, kind: /** @type {const} */ ('charts'), label: c.name, detail: oneLine(c.sql),
        meta: c.group ?? '', sql: c.sql ?? '', starred: false, entry: c,
      }))
  })

  $effect(() => { if (cursor > 0 && cursor >= rows.length) cursor = Math.max(0, rows.length - 1) })

  const EMPTY = {
    history: { Icon: History, title: 'No queries run yet', hint: `${modKey}↵ runs the editor` },
    saved: { Icon: Bookmark, title: 'No saved queries', hint: `${modKey}S in the editor saves it` },
    charts: { Icon: BarChart2, title: 'No saved charts', hint: 'Save one from the Chart view of a result' },
  }

  /** @param {Row | undefined} row */
  function load(row) { if (row?.sql) onselect(row.sql) }
  /** @param {Row | undefined} row */
  function run(row) { if (row?.sql) onrun(row.sql) }

  /** @param {Row} row */
  async function toggleStar(row) {
    await setQueryHistoryFavorite(row.id, !row.starred)
    await onrefresh()
  }

  /**
   * Delete now, with Undo in the toast rather than a confirm before: the
   * common case is one keypress, and a slip costs one click.
   * @param {Row | undefined} row
   */
  async function remove(row) {
    if (!row || row.kind === 'charts') return
    const entry = $state.snapshot(row.entry)
    if (row.kind === 'history') await deleteQueryHistoryEntry(row.id)
    else await deleteSavedQuery(row.id)
    await onrefresh()
    toast.message(row.kind === 'history' ? 'Removed from history' : 'Saved query deleted', {
      description: row.label.slice(0, 120),
      action: {
        label: 'Undo',
        onClick: () => void (async () => {
          if (row.kind === 'history') await restoreQueryHistoryEntry(entry)
          else await restoreSavedQuery(entry)
          await onrefresh()
        })(),
      },
    })
  }

  /** Clear the history, starred queries excepted (starring is how one is kept). */
  async function clearHistory() {
    const connId = history[0]?.connectionId
    const removed = history.filter((e) => !e.favorite).map((e) => $state.snapshot(e))
    if (!connId || !removed.length) return
    await clearQueryHistory(connId, { keepStarred: true })
    await onrefresh()
    toast.message(`Cleared ${removed.length} ${removed.length === 1 ? 'query' : 'queries'}`, {
      description: history.some((e) => e.favorite) ? 'Starred queries stay.' : undefined,
      action: {
        label: 'Undo',
        onClick: () => void (async () => {
          await Promise.all(removed.map((e) => restoreQueryHistoryEntry(e)))
          await onrefresh()
        })(),
      },
    })
  }

  /** Move the arrow-key row and keep it on screen. @param {number} i */
  async function moveTo(i) {
    cursor = Math.max(0, Math.min(rows.length - 1, i))
    await tick()
    listEl?.querySelector(`[data-row="${cursor}"]`)?.scrollIntoView({ block: 'nearest' })
  }

  /** Keys shared by the search box and the list. @param {KeyboardEvent} e */
  function onListKey(e) {
    // Plain keys only: Ctrl+Enter still runs the editor, and the rest of the
    // chords still reach the app.
    if (e.isComposing || e.ctrlKey || e.metaKey || e.altKey) return
    const inSearch = e.currentTarget === filterEl
    if (e.key === 'ArrowDown') {
      if (inSearch) listEl?.focus()
      void moveTo(inSearch ? cursor : cursor + 1)
    } else if (e.key === 'ArrowUp') {
      if (!inSearch && cursor === 0) filterEl?.focus()
      else void moveTo(cursor - 1)
    } else if (!inSearch && e.key === 'Home') void moveTo(0)
    else if (!inSearch && e.key === 'End') void moveTo(rows.length - 1)
    else if (e.key === 'Enter') load(rows[cursor])
    else if (!inSearch && (e.key === 'Delete' || e.key === 'Backspace')) void remove(rows[cursor])
    else if (e.key === 'Escape') {
      if (inSearch && filter) filter = ''
      else onescape()
    } else return
    // Handled here, so no app shortcut acts on it as well.
    e.preventDefault()
    e.stopPropagation()
  }

  const total = $derived(view === 'history' ? history.length : view === 'saved' ? saved.length : $savedCharts.length)
</script>

<div class="flex min-h-0 flex-1 flex-col bg-panel">
  <!-- Search, how many, and Clear for the history. -->
  <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border/50 px-2">
    <div class="relative flex w-72 min-w-0 shrink items-center">
      <Search class="pointer-events-none absolute left-2 size-3.5 shrink-0 text-muted-foreground" />
      <input
        type="search"
        bind:this={filterEl}
        bind:value={filter}
        oninput={() => (cursor = 0)}
        onkeydown={onListKey}
        placeholder={view === 'history' ? 'Search history…' : view === 'saved' ? 'Search saved queries…' : 'Search saved charts…'}
        aria-label={view === 'history' ? 'Search history' : view === 'saved' ? 'Search saved queries' : 'Search saved charts'}
        aria-controls="query-list"
        class="field-surface h-7 w-full min-w-0 bg-input/30 pl-7 pr-6 font-mono text-ui-xs outline-none placeholder:font-sans placeholder:text-muted-foreground [&::-webkit-search-cancel-button]:hidden"
      />
      {#if filter}
        <button
          type="button"
          class="absolute right-1.5 inline-flex size-4 items-center justify-center rounded text-muted-foreground hover:text-foreground"
          aria-label="Clear the search"
          onclick={() => { filter = ''; cursor = 0; filterEl?.focus() }}
        ><X class="size-3 shrink-0" /></button>
      {/if}
    </div>
    <span class="whitespace-nowrap font-mono text-ui-2xs tabular-nums text-muted-foreground">
      {filter ? `${rows.length} of ${total}` : total}
    </span>
    {#if view === 'history' && history.some((e) => !e.favorite)}
      <button
        type="button"
        class="ml-auto inline-flex h-7 shrink-0 items-center rounded-md px-2 text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        title="Remove every query from the history except the starred ones"
        onclick={() => void clearHistory()}
      >Clear</button>
    {/if}
  </div>

  {#if rows.length}
    <!-- One listbox: the arrows move, Enter loads, Delete removes. Clicking a
         row loads it; Run on the row loads it and runs it. -->
    <ul
      id="query-list"
      bind:this={listEl}
      role="listbox"
      tabindex="0"
      aria-label={view === 'history' ? 'Query history' : view === 'saved' ? 'Saved queries' : 'Saved charts'}
      aria-activedescendant="query-row-{cursor}"
      onkeydown={onListKey}
      class="app-scroll min-h-0 flex-1 overflow-y-auto py-1 outline-none"
    >
      {#each rows as row, i (row.id)}
        {@const on = i === cursor}
        <li
          id="query-row-{i}"
          data-row={i}
          role="option"
          aria-selected={on}
          class={cn('group/row flex h-8 items-center gap-2 pl-3 pr-1.5', on ? 'bg-accent/60' : 'hover:bg-muted/50')}
        >
          <button
            type="button"
            tabindex="-1"
            class="flex h-full min-w-0 flex-1 items-center gap-3 text-left"
            title={row.error ? `${row.sql}\n\nFailed: ${row.error}` : row.sql}
            onclick={() => { cursor = i; load(row) }}
          >
            {#if row.kind === 'history'}
              {#if row.error}<CircleAlert class="size-3.5 shrink-0 text-destructive" aria-label="Failed" />{/if}
              <span class="min-w-0 flex-1 truncate font-mono text-ui-xs text-foreground/90">{row.label}</span>
              {#if row.error}
                <span class="min-w-0 max-w-[35%] shrink truncate text-ui-2xs text-destructive">{row.error}</span>
              {/if}
            {:else}
              <span class="min-w-0 max-w-[40%] shrink-0 truncate text-ui-xs text-foreground">{row.label}</span>
              <span class="min-w-0 flex-1 truncate font-mono text-ui-2xs text-muted-foreground">{row.detail}</span>
            {/if}
            <span class="shrink-0 font-mono text-ui-2xs tabular-nums text-muted-foreground">{row.meta}</span>
          </button>
          <div class="flex shrink-0 items-center gap-0.5">
            <button
              type="button"
              tabindex="-1"
              class={cn('inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-[opacity,background-color,color] hover:bg-accent hover:text-foreground focus-visible:opacity-100 group-hover/row:opacity-100', on ? 'opacity-100' : 'opacity-0')}
              title="Load and run"
              aria-label="Load and run"
              onclick={() => { cursor = i; run(row) }}
            ><Play class="size-3.5 shrink-0" /></button>
            {#if row.kind === 'history'}
              <button
                type="button"
                tabindex="-1"
                class={cn(
                  'inline-flex size-6 items-center justify-center rounded-md transition-[opacity,background-color,color] hover:bg-accent focus-visible:opacity-100 group-hover/row:opacity-100',
                  row.starred ? 'text-warning opacity-100' : cn('text-muted-foreground hover:text-warning', on ? 'opacity-100' : 'opacity-0'),
                )}
                title={row.starred ? 'Unstar' : 'Star, so Clear keeps it'}
                aria-label={row.starred ? 'Unstar' : 'Star'}
                aria-pressed={row.starred}
                onclick={() => void toggleStar(row)}
              ><Star class={cn('size-3.5 shrink-0', row.starred && 'fill-current')} /></button>
            {/if}
            {#if row.kind !== 'charts'}
              <button
                type="button"
                tabindex="-1"
                class={cn('inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-[opacity,background-color,color] hover:bg-destructive/10 hover:text-destructive focus-visible:opacity-100 group-hover/row:opacity-100', on ? 'opacity-100' : 'opacity-0')}
                title="Delete (Del)"
                aria-label="Delete"
                onclick={() => void remove(row)}
              ><Trash2 class="size-3.5 shrink-0" /></button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    {@const empty = EMPTY[view]}
    <div class="flex min-h-0 flex-1 flex-col items-center justify-center gap-1.5 px-6 text-center">
      <empty.Icon class="size-5 shrink-0 text-muted-foreground" />
      <p class="text-ui-xs text-foreground">{filter ? 'Nothing matches' : empty.title}</p>
      <p class="text-ui-2xs text-muted-foreground">{filter ? 'Fewer words find more' : empty.hint}</p>
    </div>
  {/if}
</div>
