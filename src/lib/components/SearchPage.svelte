<script>
  import Kbd from './Kbd.svelte'
  import { getTableRows, listTables } from '$lib/api.js'
  import { isSystemSchema } from '$lib/system-schemas.js'
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
   * @typedef {{ table: string, schema: string, count: number, columns: any[], sampleRow: any[] | null, tableKind?: string }} SearchHit
   */

  let {
    /** @type {TableInfo[]} */
    tables = [],
    schema = 'public',
    dialect = 'postgres',
    active = false,
    /** Every schema of the connection, for "All schemas". @type {string[]} */
    schemas = [],
    /** @param {string} tableName @param {string} searchTerm @param {string} [inSchema] */
    onopentable = (tableName, searchTerm, inSchema) => {},
    /** Assigned here; the shell calls it for ⌘F. */
    focusSearch = $bindable(/** @type {() => void} */ (() => {})),
  } = $props()

  let query = $state('')
  let useRegex = $state(false)
  let matchCase = $state(false)
  /** This schema, or every user schema of the connection. */
  let scope = $state(/** @type {'schema' | 'all'} */ ('schema'))
  const userSchemas = $derived(schemas.filter((s) => !isSystemSchema(s)))
  const canScope = $derived(userSchemas.length > 1)
  /** Table lists of other schemas, read once per page. @type {Map<string, TableInfo[]>} */
  const schemaTables = new Map()
  let wholeWord = $state(false)
  const optionsSupported = $derived(searchOptionsSupported(dialect))
  /** @type {SearchHit[]} */
  let results = $state([])
  let searching = $state(false)
  let searched = $state(false)
  let progress = $state({ done: 0, total: 0 })
  let searchGeneration = 0
  let regexError = $state('')
  /** Tables whose query failed (permissions, a type the predicate can't read). */
  let failed = $state(0)
  /** The highlighted result, for arrow keys and Enter. */
  let selected = $state(-1)
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
    void matchCase; void wholeWord; void useRegex; void scope
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
    failed = 0
    selected = -1

    /** @param {TableInfo & { kind?: string }} t */
    const searchableKind = (t) => {
      const k = t.tableKind ?? t.kind
      return !k || ['table', 'view', 'foreign_table', 'materialized_view'].includes(k)
    }
    /** @type {{ schema: string, name: string, tableKind?: string }[]} */
    let searchable = tables.filter(searchableKind).map((t) => ({ schema, name: t.name, tableKind: t.tableKind ?? /** @type {any} */ (t).kind }))
    if (scope === 'all' && canScope) {
      // The other schemas' tables, listed once and kept for the page.
      const others = userSchemas.filter((s) => s !== schema)
      await Promise.all(others.filter((s) => !schemaTables.has(s)).map(async (s) => {
        try { schemaTables.set(s, /** @type {TableInfo[]} */ (await listTables(s))) } catch { schemaTables.set(s, []) }
      }))
      if (searchGeneration !== gen) return
      for (const s of others) {
        for (const t of schemaTables.get(s) ?? []) {
          if (searchableKind(t)) searchable.push({ schema: s, name: t.name, tableKind: t.tableKind ?? /** @type {any} */ (t).kind })
        }
      }
    }
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
          const res = await getTableRows(t.schema, t.name, 1, 0, {
            ...searchParams,
            includeMeta: false,
          })
          if (searchGeneration !== gen) return
          if (res.total > 0) {
            results = [...results, /** @type {SearchHit} */ ({
              table: t.name,
              schema: t.schema,
              count: res.total,
              columns: res.columns ?? [],
              sampleRow: res.rows?.[0] ?? null,
              tableKind: t.tableKind,
            })].sort((a, b) => b.count - a.count)
          }
        } catch {
          // Counted and said, not dropped silently: "no matches" from a
          // search that couldn't read half the tables is not true.
          if (searchGeneration === gen) failed++
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
    // Arrows walk the results from the field; Enter opens the highlighted one.
    if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && results.length) {
      e.preventDefault()
      const d = e.key === 'ArrowDown' ? 1 : -1
      selected = Math.max(-1, Math.min(results.length - 1, selected + d))
      document.querySelector(`[data-hit="${selected}"]`)?.scrollIntoView({ block: 'nearest' })
      return
    }
    if (e.key === 'Enter' && selected >= 0 && results[selected]) {
      e.preventDefault()
      onopentable(results[selected].table, query.trim(), results[selected].schema)
      return
    }
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
    const s = typeof v === 'object' ? JSON.stringify(v) : String(v)
    return s.length > 60 ? s.slice(0, 60) + '…' : s
  }

  /** The query as typed, for highlighting in samples (plain searches only). */
  const needle = $derived(useRegex ? '' : query.trim())

  /**
   * The sample row's columns, the ones that matched first: a result is about
   * where the text was found, and five arbitrary columns often missed it.
   * @param {SearchHit} hit
   */
  function sampleCells(hit) {
    if (!hit.sampleRow) return []
    const n = matchCase ? needle : needle.toLowerCase()
    const cells = hit.columns.map((c, i) => {
      const text = displayValue(hit.sampleRow?.[i])
      const hay = matchCase ? String(hit.sampleRow?.[i] ?? '') : String(hit.sampleRow?.[i] ?? '').toLowerCase()
      return { name: c.name, text, hit: !!n && hay.includes(n), isNull: hit.sampleRow?.[i] == null }
    })
    return [...cells.filter((c) => c.hit), ...cells.filter((c) => !c.hit)].slice(0, 6)
  }

  /** `text` split around the needle, for a <mark>. @param {string} text */
  function marked(text) {
    if (!needle) return [{ t: text, m: false }]
    const hay = matchCase ? text : text.toLowerCase()
    const n = matchCase ? needle : needle.toLowerCase()
    const i = hay.indexOf(n)
    if (i < 0) return [{ t: text, m: false }]
    return [{ t: text.slice(0, i), m: false }, { t: text.slice(i, i + n.length), m: true }, { t: text.slice(i + n.length), m: false }]
  }

  const totalMatches = $derived(results.reduce((n, r) => n + r.count, 0))

  const tableIcon = (/** @type {string | undefined} */ kind) =>
    kind === 'view' || kind === 'materialized_view' ? Eye : Table2

  const progressPct = $derived(
    progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0
  )
</script>

<div class="flex h-full min-h-0 flex-col">
  <!-- The field, in the same column as the results. -->
  <div class="shrink-0 px-6 pt-6 pb-3">
    <div class="mx-auto w-full max-w-3xl">
      <div
        class={cn(
          'search-shell flex h-11 items-center gap-2 ps-3.5 pe-1.5',
          regexError && 'border-destructive/50 bg-destructive/8',
        )}
      >
        <Search class="size-4 shrink-0 text-muted-foreground" />
        <input
          bind:this={inputEl}
          type="text"
          data-slash-focus="25"
          aria-keyshortcuts="/"
          placeholder={useRegex ? 'Regex pattern' : `Find in every table of ${schema}`}
          class="peer no-focus-ring no-field-frame min-w-0 flex-1 bg-transparent text-ui-sm text-foreground outline-none placeholder:text-muted-foreground"
          bind:value={query}
          onkeydown={handleKeydown}
          oninput={() => { regexError = ''; selected = -1 }}
          spellcheck="false"
          autocomplete="off"
          role="combobox"
          aria-expanded={results.length > 0}
          aria-controls="find-results"
          aria-activedescendant={selected >= 0 ? `find-hit-${selected}` : undefined}
        />
        <Kbd combo="/" class="pointer-events-none hidden peer-placeholder-shown:inline-flex peer-focus:hidden shrink-0" aria-hidden="true" />
        {#if searching}
          <Loader class="size-3.5 shrink-0 animate-spin text-muted-foreground" />
        {:else if query}
          <button
            type="button"
            class="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            title="Clear (Esc)"
            aria-label="Clear search"
            onclick={() => { query = ''; regexError = ''; inputEl?.focus() }}
          ><X class="size-3.5 shrink-0" /></button>
        {/if}
        {#if optionsSupported}
          <span class="mx-0.5 h-5 w-px shrink-0 bg-border/70" aria-hidden="true"></span>
          {#each [
            { key: 'matchCase', on: matchCase, label: 'Match case', glyph: 'Aa' },
            { key: 'wholeWord', on: wholeWord, label: 'Whole word', glyph: 'ab' },
            { key: 'regex', on: useRegex, label: 'Regular expression', glyph: '.*' },
          ] as o (o.key)}
            <button
              type="button"
              title="{o.label} ({SEARCH_OPTION_KEYS[/** @type {'matchCase'|'wholeWord'|'regex'} */ (o.key)]})"
              aria-label={o.label}
              aria-pressed={o.on}
              class={cn(
                'flex size-7 shrink-0 items-center justify-center rounded-md font-mono text-ui-2xs transition-colors',
                o.on ? 'bg-primary/15 font-semibold text-primary' : 'text-muted-foreground hover:bg-accent hover:text-foreground',
              )}
              onclick={() => {
                if (o.key === 'matchCase') matchCase = !matchCase
                else if (o.key === 'wholeWord') wholeWord = !wholeWord
                else { useRegex = !useRegex; regexError = '' }
                inputEl?.focus()
              }}
            >{#if o.key === 'wholeWord'}<span class="underline underline-offset-2">ab</span>{:else}{o.glyph}{/if}</button>
          {/each}
        {/if}
      </div>
      {#if regexError}
        <p class="mt-1.5 px-1 text-ui-xs text-destructive">{regexError}</p>
      {/if}

      <!-- One status line: where it looks, then progress while it runs and
           totals after. -->
      <div class="mt-2 flex h-6 items-center gap-2 px-1 text-ui-2xs text-muted-foreground tabular-nums">
        {#if canScope}
          <div role="radiogroup" aria-label="Search in" class="flex shrink-0 gap-0.5 rounded-full bg-muted/60 p-0.5">
            {#each [{ v: 'schema', l: schema }, { v: 'all', l: `All schemas (${userSchemas.length})` }] as o (o.v)}
              <button
                type="button"
                role="radio"
                aria-checked={scope === o.v}
                class={cn('h-5 max-w-40 truncate rounded-full px-2 text-ui-2xs font-medium transition-colors', scope === o.v ? 'bg-background text-foreground shadow-xs' : 'text-muted-foreground hover:text-foreground')}
                onclick={() => { scope = /** @type {'schema' | 'all'} */ (o.v); inputEl?.focus() }}
              >{o.l}</button>
            {/each}
          </div>
        {/if}
        {#if searching}
          <span>Searching {progress.done} of {progress.total} tables</span>
          <span class="h-0.5 w-24 overflow-hidden rounded-full bg-muted">
            <span class="block h-full rounded-full bg-primary transition-[width] duration-150" style="width: {progressPct}%"></span>
          </span>
          {#if results.length}<span>· {results.length} with matches</span>{/if}
        {:else if searched && results.length}
          <span><span class="font-medium text-foreground">{totalMatches.toLocaleString()}</span> {totalMatches === 1 ? 'match' : 'matches'} in <span class="font-medium text-foreground">{results.length}</span> of {progress.total} tables</span>
        {:else if !query.trim()}
          {#if !canScope}<span>{tables.length} tables in {schema}</span>{/if}
        {/if}
        {#if failed && !searching}
          <span class="text-warning">· {failed} table{failed === 1 ? '' : 's'} couldn't be searched</span>
        {/if}
        {#if results.length && !searching}
          <span class="ms-auto hidden items-center gap-1 sm:flex"><Kbd combo="ArrowUp" /><Kbd combo="ArrowDown" /> move <Kbd combo="Enter" /> open</span>
        {/if}
      </div>
    </div>
  </div>

  <div class="app-scroll min-h-0 flex-1 overflow-y-auto px-6 pb-8">
    <div class="mx-auto w-full max-w-3xl">
      {#if results.length > 0}
        <ul id="find-results" role="listbox" aria-label="Tables with matches" class="flex flex-col gap-0.5">
          {#each results as hit, hi (hit.schema + '.' + hit.table)}
            {@const TIcon = tableIcon(hit.tableKind)}
            {@const cells = sampleCells(hit)}
            <li id="find-hit-{hi}" role="option" aria-selected={selected === hi}>
              <button
                type="button"
                data-hit={hi}
                class={cn(
                  'group flex w-full flex-col gap-1.5 rounded-lg px-3 py-2.5 text-left outline-none transition-colors hover:bg-muted/50 focus-visible:bg-muted/60',
                  selected === hi && 'bg-muted/60',
                )}
                onclick={() => onopentable(hit.table, query.trim(), hit.schema)}
                onpointerenter={() => (selected = hi)}
              >
                <div class="flex min-w-0 items-center gap-2.5">
                  <span class="flex size-6 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground"><TIcon class="size-3.5 shrink-0" /></span>
                  <span class="min-w-0 truncate font-mono text-ui-sm font-medium text-foreground">{#if scope === 'all' && hit.schema !== schema}<span class="font-normal text-muted-foreground">{hit.schema}.</span>{/if}{hit.table}</span>
                  <span class="shrink-0 text-ui-2xs text-muted-foreground tabular-nums">{hit.count.toLocaleString()} {hit.count === 1 ? 'row' : 'rows'}</span>
                  <span class="ms-auto inline-flex shrink-0 items-center gap-1 text-ui-2xs text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 {selected === hi ? 'opacity-100' : ''}">
                    Open <ArrowRight class="size-3.5 shrink-0" />
                  </span>
                </div>
                {#if cells.length}
                  <div class="ms-8.5 flex min-w-0 flex-wrap gap-x-4 gap-y-1">
                    {#each cells as c (c.name)}
                      <span class="flex min-w-0 max-w-[30ch] items-baseline gap-1.5">
                        <span class={cn('shrink-0 text-ui-3xs', c.hit ? 'text-foreground/80' : 'text-muted-foreground/70')}>{c.name}</span>
                        <span class={cn('min-w-0 truncate font-mono text-ui-2xs', c.isNull ? 'text-muted-foreground/60' : 'text-foreground/75')}>
                          {#each marked(c.text) as part, pi (pi)}{#if part.m}<mark class="rounded-sm bg-warning/25 px-px text-foreground">{part.t}</mark>{:else}{part.t}{/if}{/each}
                        </span>
                      </span>
                    {/each}
                  </div>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {:else if searched && !searching}
        <div class="flex flex-col items-center gap-2 py-20 text-center">
          <span class="flex size-11 items-center justify-center rounded-lg bg-muted/60"><Search class="size-5 shrink-0 text-muted-foreground" /></span>
          <p class="mt-1 text-ui-sm font-medium text-foreground">Nothing matches <span class="font-mono">"{query.trim()}"</span></p>
          <p class="max-w-80 text-ui-xs leading-relaxed text-muted-foreground">
            Searched {progress.total} table{progress.total === 1 ? '' : 's'} in {scope === 'all' && canScope ? `${userSchemas.length} schemas` : schema}.
            {#if matchCase || wholeWord}Try without {matchCase && wholeWord ? 'Match case and Whole word' : matchCase ? 'Match case' : 'Whole word'}.{/if}
          </p>
        </div>
      {:else if !searching}
        <div class="flex flex-col items-center gap-2 py-20 text-center">
          <span class="flex size-11 items-center justify-center rounded-lg bg-muted/60"><Search class="size-5 shrink-0 text-muted-foreground" /></span>
          <p class="mt-1 text-ui-sm font-medium text-foreground">Find a value anywhere</p>
          <p class="max-w-96 text-ui-xs leading-relaxed text-muted-foreground">
            Every column of every table in {schema} is searched at once. Type at least {MIN_QUERY} characters; results open the table with the search already applied.
          </p>
        </div>
      {/if}
    </div>
  </div>
</div>
