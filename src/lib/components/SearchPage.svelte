<script>
  import { getTableRows } from '$lib/api.js'
  import { buildSearchQuery, searchOptionsSupported, searchOptionHotkey, SEARCH_OPTION_KEYS } from '$lib/search-options.js'
  import Search from '@lucide/svelte/icons/search'
  import X from '@lucide/svelte/icons/x'
  import Table2 from '@lucide/svelte/icons/table-2'
  import Eye from '@lucide/svelte/icons/eye'
  import Loader from '@lucide/svelte/icons/loader'
  import ArrowRight from '@lucide/svelte/icons/arrow-right'
  import { untrack } from 'svelte'
  import { cn } from '$lib/utils.js'

  /**
   * @typedef {{ name: string, rowCount?: number, tableKind?: string }} TableInfo
   * @typedef {{ table: string, count: number, columns: any[], sampleRow: any[] | null, tableKind?: string }} SearchHit
   */

  let {
    /** @type {TableInfo[]} */
    tables = [],
    schema = 'public',
    dialect = 'postgres',
    active = false,
    /** @param {string} tableName @param {string} searchTerm */
    onopentable = (tableName, searchTerm) => {},
    /** Assigned here; the shell calls it for ⌘F. */
    focusSearch = $bindable(/** @type {() => void} */ (() => {})),
  } = $props()

  let query = $state('')
  let useRegex = $state(false)
  let matchCase = $state(false)
  let wholeWord = $state(false)
  const optionsSupported = $derived(searchOptionsSupported(dialect))
  /** @type {SearchHit[]} */
  let results = $state([])
  let searching = $state(false)
  let searched = $state(false)
  let progress = $state({ done: 0, total: 0 })
  let searchGeneration = 0
  let regexError = $state('')
  /** @type {HTMLInputElement | null} */
  let inputEl = $state(null)

  $effect(() => {
    // Selects as well as focuses, so ⌘F on a page that already has a query
    // replaces it by typing rather than appending to it.
    focusSearch = () => { inputEl?.focus(); inputEl?.select() }
  })

  $effect(() => {
    if (active && inputEl) {
      // Small delay so the tab transition finishes before focusing
      setTimeout(() => inputEl?.focus(), 30)
    }
  })

  // Cancel an in-flight search when the page deactivates, so workers stop
  // issuing per-table queries in the background.
  $effect(() => {
    if (!active && searching) {
      searchGeneration++
      searching = false
    }
  })

  function validateRegex(/** @type {string} */ pattern) {
    try {
      new RegExp(pattern)
      return ''
    } catch (e) {
      return e instanceof Error ? e.message : 'Invalid regex'
    }
  }

  /**
   * A search is one query per table, ten at a time, so it is not something to
   * fire on a keystroke: on a 135-table schema that is 135 round trips for a
   * letter that is about to be followed by another. Waiting out a pause in the
   * typing is what makes searching-as-you-type affordable here.
   */
  const DEBOUNCE_MS = 400
  /** One character matches most of the database; it is not a search yet. */
  const MIN_QUERY = 2
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let debounceTimer

  /** Drop results and stop anything in flight. */
  function resetResults() {
    searchGeneration++
    results = []
    searched = false
    searching = false
    progress = { done: 0, total: 0 }
  }

  $effect(() => {
    const q = query.trim()
    // Re-run when an option changes too: they change what the query means.
    void matchCase; void wholeWord; void useRegex
    clearTimeout(debounceTimer)
    if (q.length < MIN_QUERY) {
      untrack(() => resetResults())
      return
    }
    debounceTimer = setTimeout(() => void runSearch(), DEBOUNCE_MS)
    return () => clearTimeout(debounceTimer)
  })

  async function runSearch() {
    const q = query.trim()
    // No `searching` guard: a newer search supersedes an older one rather than
    // being dropped by it, and the generation check below stops the old workers.
    if (!q) return

    if (useRegex && optionsSupported) {
      const err = validateRegex(q)
      if (err) { regexError = err; return }
    }
    regexError = ''
    const searchParams = buildSearchQuery(q, { matchCase, wholeWord, regex: useRegex }, optionsSupported)

    const gen = ++searchGeneration
    searching = true
    searched = false
    results = []

    const searchable = tables.filter(
      (t) =>
        !t.tableKind ||
        ['table', 'view', 'foreign_table', 'materialized_view'].includes(t.tableKind),
    )
    progress = { done: 0, total: searchable.length }

    // Run with concurrency cap to avoid overwhelming the DB connection pool.
    // Results stream in as each table finishes.
    const CONCURRENCY = 10

    let index = 0
    async function worker() {
      while (index < searchable.length && searchGeneration === gen) {
        const t = searchable[index++]
        if (!t) continue
        try {
          const res = await getTableRows(schema, t.name, 1, 0, {
            ...searchParams,
            includeMeta: false,
          })
          if (searchGeneration !== gen) return
          if (res.total > 0) {
            results = [...results, /** @type {SearchHit} */ ({
              table: t.name,
              count: res.total,
              columns: res.columns ?? [],
              sampleRow: res.rows?.[0] ?? null,
              tableKind: t.tableKind,
            })].sort((a, b) => b.count - a.count)
          }
        } catch {
          /* ignore per-table errors */
        }
        if (searchGeneration === gen) {
          progress = { done: progress.done + 1, total: progress.total }
        }
      }
    }

    const workers = Array.from({ length: Math.min(CONCURRENCY, searchable.length) }, worker)
    await Promise.all(workers)
    if (searchGeneration !== gen) return

    searched = true
    searching = false
  }

  function handleKeydown(/** @type {KeyboardEvent} */ e) {
    // Enter is "do not wait for the pause", not a separate way to search.
    if (e.key === 'Enter') {
      e.preventDefault()
      clearTimeout(debounceTimer)
      if (query.trim()) void runSearch()
      return
    }
    // Escape empties the box, and the effect above clears the results with it.
    // Only when there is something to clear, so an empty box lets Escape reach
    // whatever else is listening.
    if (e.key === 'Escape' && query) {
      e.preventDefault()
      e.stopPropagation()
      query = ''
      regexError = ''
      return
    }
    const opt = searchOptionHotkey(e)
    if (opt && optionsSupported) {
      e.preventDefault()
      if (opt === 'matchCase') matchCase = !matchCase
      else if (opt === 'wholeWord') wholeWord = !wholeWord
      else { useRegex = !useRegex; regexError = '' }
    }
  }

  /** @param {unknown} v */
  function displayValue(v) {
    if (v === null || v === undefined) return 'NULL'
    if (typeof v === 'object') {
      const s = JSON.stringify(v)
      return s.length > 40 ? s.slice(0, 40) + '…' : s
    }
    const s = String(v)
    return s.length > 40 ? s.slice(0, 40) + '…' : s
  }

  const tableIcon = (/** @type {string | undefined} */ kind) =>
    kind === 'view' || kind === 'materialized_view' ? Eye : Table2

  const progressPct = $derived(
    progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0
  )
</script>

<div class="flex h-full min-h-0 flex-col">
  <!-- Search bar. Same 72rem column as the results underneath it: a field that
       runs the full width of a wide window has its caret in one place and its
       controls a thousand pixels away. -->
  <!-- h-9 with no vertical padding, so this row is the same height as the
       sidebar header beside it (which is h-9 too) and the two line up across
       the split. The control inside is h-7, the same as the schema picker and
       the filter box over there. -->
  <div class="flex h-9 shrink-0 items-center border-b border-border/50 px-2">
    <div
      class={cn(
        'mx-auto flex h-7 w-full max-w-[72rem] items-center gap-1.5 rounded-md bg-muted/30 px-2 transition-colors',
        'border-[length:var(--field-border-width)]',
        regexError ? 'border-destructive/50' : 'border-border/40 focus-within:border-ring/60',
      )}
    >
      <Search class="size-3.5 shrink-0 text-muted-foreground" />
      <input
        bind:this={inputEl}
        type="text"
        placeholder={useRegex ? 'Regex pattern…' : `Search ${schema}…`}
        class="no-focus-ring min-w-0 flex-1 bg-transparent text-ui-xs outline-none placeholder:text-muted-foreground"
        bind:value={query}
        onkeydown={handleKeydown}
        oninput={() => { regexError = '' }}
        spellcheck="false"
        autocomplete="off"
      />
      <!-- VS Code-style option toggles (Postgres regex path only) -->
      {#if optionsSupported}
        <button
          type="button"
          title="Match case ({SEARCH_OPTION_KEYS.matchCase})"
          aria-label="Match case ({SEARCH_OPTION_KEYS.matchCase})"
          aria-keyshortcuts={SEARCH_OPTION_KEYS.matchCase}
          aria-pressed={matchCase}
          class={cn(
            'flex size-6 shrink-0 items-center justify-center rounded-md text-ui-3xs font-mono transition-colors',
            matchCase
              ? 'bg-primary/15 text-primary ring-1 ring-inset ring-primary/30'
              : 'text-muted-foreground hover:bg-accent hover:text-foreground',
          )}
          onclick={() => (matchCase = !matchCase)}
        >Aa</button>
        <button
          type="button"
          title="Match whole word ({SEARCH_OPTION_KEYS.wholeWord})"
          aria-label="Match whole word ({SEARCH_OPTION_KEYS.wholeWord})"
          aria-keyshortcuts={SEARCH_OPTION_KEYS.wholeWord}
          aria-pressed={wholeWord}
          class={cn(
            'flex size-6 shrink-0 items-center justify-center rounded-md text-ui-3xs font-mono transition-colors',
            wholeWord
              ? 'bg-primary/15 text-primary ring-1 ring-inset ring-primary/30'
              : 'text-muted-foreground hover:bg-accent hover:text-foreground',
          )}
          onclick={() => (wholeWord = !wholeWord)}
        ><span class="underline underline-offset-2">ab</span></button>
        <button
          type="button"
          title="Use regular expression ({SEARCH_OPTION_KEYS.regex})"
          aria-label="Use regular expression ({SEARCH_OPTION_KEYS.regex})"
          aria-keyshortcuts={SEARCH_OPTION_KEYS.regex}
          aria-pressed={useRegex}
          class={cn(
            'flex size-6 shrink-0 items-center justify-center rounded-md text-ui-3xs font-mono transition-colors',
            useRegex
              ? 'bg-primary/15 text-primary ring-1 ring-inset ring-primary/30'
              : 'text-muted-foreground hover:bg-accent hover:text-foreground',
          )}
          onclick={() => { useRegex = !useRegex; regexError = '' }}
        >.*</button>
      {/if}
      {#if searching}
        <Loader class="size-3.5 shrink-0 animate-spin text-muted-foreground" />
      {:else if query}
        <button
          type="button"
          class="flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="Clear (Esc)"
          aria-label="Clear search"
          onclick={() => { query = ''; regexError = '' }}
        >
          <X class="size-3.5 shrink-0" />
        </button>
      {/if}
    </div>
  </div>
  <!-- Its own row: the bar above is a fixed h-9 so it lines up with the sidebar
       header, and a second child in there would sit beside the field, not under
       it. -->
  {#if regexError}
    <div class="shrink-0 border-b border-border/50 px-2 pb-1.5">
      <p class="mx-auto w-full max-w-[72rem] px-1 text-ui-xs text-destructive">{regexError}</p>
    </div>
  {/if}

  <!-- Progress bar (only while searching) -->
  {#if searching}
    <div class="shrink-0 border-b border-border/30 px-3 py-2">
      <div class="mb-1.5 flex items-center justify-between text-ui-xs text-muted-foreground">
        <span>{progress.done} / {progress.total} tables searched</span>
        {#if results.length > 0}
          <span class="text-primary">{results.length} with matches</span>
        {/if}
      </div>
      <div class="h-0.5 overflow-hidden rounded-full bg-muted">
        <div
          class="h-full rounded-full bg-primary transition-[width] duration-150"
          style="width: {progressPct}%"
        ></div>
      </div>
    </div>
  {/if}

  <!-- Results
       One column, not the width of the window. A result is a table name, a
       count and a sample of the row it matched - three things that belong
       together, and flinging the count to the far edge of a 1400px pane put
       1300px between a number and the thing it counts. The count now sits with
       the name; the arrow keeps the right edge and says the row opens. -->
  <div class="flex-1 overflow-y-auto">
    {#if results.length > 0}
      <div class="mx-auto w-full max-w-[72rem] divide-y divide-border/25">
        {#each results as hit (hit.table)}
          {@const Icon = tableIcon(hit.tableKind)}
          <button
            type="button"
            class="group flex w-full flex-col gap-0.5 px-4 py-2 text-left outline-none transition-colors hover:bg-accent/40 focus-visible:bg-accent/40 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
            onclick={() => onopentable(hit.table, query.trim())}
          >
            <div class="flex min-w-0 items-center gap-2">
              <Icon class="size-3.5 shrink-0 text-muted-foreground" />
              <span class="min-w-0 truncate font-mono text-ui-xs font-medium text-foreground">{hit.table}</span>
              <span class="shrink-0 rounded-[3px] bg-primary/10 px-1.5 py-px font-mono text-ui-3xs tabular-nums text-primary">
                {hit.count.toLocaleString()}
              </span>
              <span class="shrink-0 text-ui-3xs text-muted-foreground">{hit.count === 1 ? 'match' : 'matches'}</span>
              <ArrowRight class="ml-auto size-3.5 shrink-0 text-muted-foreground/60 transition-colors group-hover:text-foreground" />
            </div>
            {#if hit.sampleRow && hit.columns.length > 0}
              <!-- The sample row reads as one line of a record: name in the
                   quiet weight, value in the readable one, every pair the same
                   width so the eye can run down the column instead of chasing a
                   ragged edge. -->
              <div class="ml-[22px] flex min-w-0 flex-wrap items-baseline gap-x-4 gap-y-0.5">
                {#each hit.columns.slice(0, 5) as col, ci (col.name)}
                  {@const val = hit.sampleRow[ci]}
                  <div class="flex min-w-0 max-w-[26ch] items-baseline gap-1.5">
                    <span class="shrink-0 text-ui-3xs text-muted-foreground/70">{col.name}</span>
                    <span
                      class={cn(
                        'min-w-0 truncate font-mono text-ui-3xs',
                        val === null || val === undefined
                          ? 'italic text-muted-foreground/60'
                          : 'text-foreground/75',
                      )}
                    >
                      {displayValue(val)}
                    </span>
                  </div>
                {/each}
              </div>
            {/if}
          </button>
        {/each}
      </div>
    {:else if searching && progress.done === 0}
      <!-- initial loading state before first result -->
      <div class="loading-reveal flex flex-col items-center justify-center gap-2 py-12 text-center">
        <Loader class="size-6 shrink-0 animate-spin text-muted-foreground" />
        <p class="text-ui-xs text-muted-foreground">Searching…</p>
      </div>
    {:else if searched && !searching}
      <div class="flex flex-col items-center justify-center gap-2 py-12 text-center">
        <Search class="size-8 shrink-0 text-muted-foreground" />
        <p class="text-ui-sm text-muted-foreground">
          No matches for <span class="font-mono">"{query}"</span>
        </p>
        <p class="text-ui-xs text-muted-foreground">
          Searched {progress.total} table{progress.total === 1 ? '' : 's'}
        </p>
      </div>
    {:else if !searching}
      <div class="flex flex-col items-center justify-center gap-2 py-12 text-center">
        <Search class="size-8 shrink-0 text-muted-foreground" />
        <p class="text-ui-sm text-muted-foreground">Search across all {tables.length} tables</p>
        <p class="text-ui-xs text-muted-foreground">
          {query.trim().length ? 'Keep typing…' : 'Start typing'}
        </p>
      </div>
    {/if}
  </div>

  {#if searched && !searching && results.length > 0}
    <div class="shrink-0 border-t border-border/30 px-4 py-1.5 text-ui-xs text-muted-foreground">
      {results.length} table{results.length === 1 ? '' : 's'} with matches · click to open with search pre-filled
    </div>
  {/if}
</div>
