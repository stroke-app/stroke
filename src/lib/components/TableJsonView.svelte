<script module>
  // JSONPath filters are per table-tab: keyed by table identity, module-scoped
  // so a filter survives view switches/remounts but never leaks across tables.
  /** @type {Map<string, string>} */
  const pathByTable = new Map()
</script>

<script>
  import { untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import CodeTextView from './CodeTextView.svelte'
  import JsonWrapToggle from './JsonWrapToggle.svelte'
  import JsonPathSuggest from './JsonPathSuggest.svelte'
  import { appJsonWordWrap } from '$lib/stores/settings.js'
  import { cn } from '$lib/utils.js'
  import { rowToRecord, formatJsonValue } from '$lib/row-inspector.js'
  import { evalJsonPath, getCompletionItems, applyCompletion, describeResult } from '$lib/jsonpath.js'
  import { searchJson } from '$lib/json-search.js'
  import JsonTree from './JsonTree.svelte'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'

  /**
   * JSON mode for the data table - a read-only CodeMirror surface (smooth
   * virtualized scrolling, ⌘F find, full selection) with adaptive large-doc
   * settings, plus a JSONPath bar evaluated against the live records so the
   * document is never re-parsed.
   */
  let {
    /** @type {Array<{ name: string }>} */
    columns = [],
    /** @type {unknown[][]} */
    rows = [],
    /** Identity of the table shown - scopes the JSONPath filter to this tab. */
    tableKey = '',
  } = $props()

  // ── View mode: a collapsible tree (default) or the raw text document ──────
  const MODE_KEY = 'stroke:table-json-mode'
  const DECODE_KEY = 'stroke:table-json-decode'
  /** @param {string} k @param {string} d */
  function load(k, d) {
    try { return localStorage.getItem(k) ?? d } catch { return d }
  }
  /** @param {string} k @param {string} v */
  function save(k, v) {
    try { localStorage.setItem(k, v) } catch {}
  }
  let mode = $state(/** @type {'tree' | 'text'} */ (load(MODE_KEY, 'tree') === 'text' ? 'text' : 'tree'))
  /** Show JSON stored in text columns as the object it holds, not an escaped string. */
  let decode = $state(load(DECODE_KEY, '1') === '1')
  $effect(() => save(MODE_KEY, mode))
  $effect(() => save(DECODE_KEY, decode ? '1' : '0'))

  /**
   * A text value that is itself a JSON object or array, decoded. Only strings
   * that start with `{` or `[` are tried (one char check, no parse attempt on
   * ordinary text), and anything over 1MB is left alone.
   * @param {unknown} v
   */
  function decodeJsonText(v) {
    if (typeof v !== 'string' || v.length < 2 || v.length > 1_000_000) return v
    const c = v.charCodeAt(0) === 32 || v.charCodeAt(0) === 10 ? v.trimStart()[0] : v[0]
    if (c !== '{' && c !== '[') return v
    try { return JSON.parse(v) } catch { return v }
  }

  const records = $derived.by(() => {
    if (!decode) return rows.map((r) => rowToRecord(columns, r))
    return rows.map((r) => {
      const rec = rowToRecord(columns, r)
      for (const k in rec) rec[k] = decodeJsonText(rec[k])
      return rec
    })
  })
  // Deriveds are lazy: in tree mode the whole-page string is never built.
  const fullJson = $derived(formatJsonValue(records))

  // ── JSONPath (scoped per table via pathByTable) ───────────────────────────
  // Initial values deliberately capture the mount-time tableKey (untracked);
  // later key changes are handled by the restore effect below.
  const _initKey = untrack(() => tableKey)
  let jsonPath = $state(pathByTable.get(_initKey) ?? '')
  let _prevKey = _initKey
  $effect(() => {
    const key = tableKey
    untrack(() => {
      if (key === _prevKey) return
      pathByTable.set(_prevKey, jsonPath) // stash the outgoing tab's filter
      jsonPath = pathByTable.get(key) ?? '' // restore the incoming tab's
      evalPath = jsonPath // snap immediately - no debounce lag on tab switch
      _prevKey = key
    })
  })
  // Keep the stash current so a view-switch remount restores the same filter.
  $effect(() => {
    const p = jsonPath
    untrack(() => pathByTable.set(_prevKey, p))
  })
  let pathFocused = $state(false)
  let activeIdx = $state(-1)
  /** @type {HTMLInputElement | null} */
  let pathInput = $state(null)

  // Evaluation is debounced off the input: a full-path eval (recursive descent
  // can walk every record) plus re-stringifying the result document per
  // KEYSTROKE would stall typing on large pages. The input stays instant; the
  // document updates ~160ms after the user pauses.
  let evalPath = $state(pathByTable.get(_initKey) ?? '')
  $effect(() => {
    const p = jsonPath
    if (p === untrack(() => evalPath)) return
    const t = setTimeout(() => { evalPath = p }, 160)
    return () => clearTimeout(t)
  })

  const pathResult = $derived.by(() => {
    const p = evalPath.trim()
    if (!p || p === '$') return null
    return evalJsonPath(records, p)
  })

  const displayedJson = $derived.by(() => {
    if (!pathResult?.ok) return fullJson
    return formatJsonValue(pathResult.value)
  })

  /** What the tree shows: the JSONPath result when there is one, else the records. */
  const shownValue = $derived(pathResult?.ok ? pathResult.value : records)

  /**
   * The page as the tree's root: one entry per row, labelled `#n · <first
   * column>` so a collapsed row still says which row it is (a bare `0`, `1`,
   * `2` made the collapsed list useless).
   */
  const treeRoot = $derived.by(() => {
    if (pathResult?.ok) return shownValue
    const first = columns[0]?.name
    /** @type {Record<string, unknown>} */
    const out = {}
    records.forEach((rec, i) => {
      const v = first != null ? rec[first] : undefined
      const tag = v === null || v === undefined || typeof v === 'object' ? '' : ` · ${first}: ${String(v).slice(0, 40)}`
      out[`#${i + 1}${tag}`] = rec
    })
    return out
  })

  // ── Search in the tree (keys and values) ─────────────────────────────────
  let query = $state('')
  let evalQuery = $state('')
  $effect(() => {
    const q = query
    const t = setTimeout(() => { evalQuery = q.trim() }, 140)
    return () => clearTimeout(t)
  })
  const found = $derived(mode === 'tree' && evalQuery ? searchJson(treeRoot, evalQuery) : null)

  /** @param {unknown} v */
  async function copyValue(v) {
    try {
      await navigator.clipboard.writeText(typeof v === 'string' ? v : formatJsonValue(v))
      toast.success('Copied')
    } catch {
      toast.error('Could not copy')
    }
  }

  const completions = $derived.by(() => {
    if (!pathFocused) return []
    return getCompletionItems(records, jsonPath).slice(0, 12)
  })

  $effect(() => {
    // Arm the top match: a filtered list where Enter does nothing is a list
    // you have to arrow into before it is any use.
    activeIdx = pathFocused && completions.length ? 0 : -1
  })

  /** @param {string} completion */
  function pickCompletion(completion) {
    jsonPath = applyCompletion(jsonPath, completion)
    activeIdx = -1
    pathInput?.focus()
  }

  /** @param {KeyboardEvent} e */
  function handlePathKeydown(e) {
    if (!completions.length) return
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      activeIdx = (activeIdx + 1) % completions.length
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      activeIdx = (activeIdx - 1 + completions.length) % completions.length
    } else if ((e.key === 'Tab' || e.key === 'Enter') && activeIdx >= 0) {
      e.preventDefault()
      pickCompletion(completions[activeIdx].insert)
    } else if (e.key === 'Escape') {
      activeIdx = -1
      pathFocused = false
      pathInput?.blur()
    }
  }

</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
  <!-- JSONPath filter bar, a quiet filter field (SQL Studio-style): glyph +
       mono input in a rounded inset, result hint right-aligned. Actions like
       copy/export/view-switch live in the table toolbar, not here. -->
  <div class="studio-chrome flex h-9 shrink-0 items-center gap-2 border-b border-border bg-panel px-2">
    <!-- Same segmented control as the text view's format switch. -->
    <div class="field-surface inline-flex h-7 shrink-0 items-center gap-0.5 bg-muted/20 p-0.5" role="radiogroup" aria-label="JSON view">
      {#each [['tree', 'Tree'], ['text', 'Text']] as [id, label] (id)}
        <button
          type="button"
          role="radio"
          aria-checked={mode === id}
          class={cn(
            'inline-flex h-full items-center rounded-sm px-2.5 text-ui-xs font-medium transition-colors',
            mode === id ? 'bg-background text-foreground elevate-1' : 'text-muted-foreground hover:text-foreground',
          )}
          onclick={() => (mode = /** @type {'tree' | 'text'} */ (id))}
        >{label}</button>
      {/each}
    </div>
    <div
      class={cn(
        'relative flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-md bg-muted/40 px-2.5 transition-colors',
        pathFocused ? 'bg-muted/70' : 'hover:bg-muted/60',
      )}
    >
      <Icon name="list-filter" class="size-3 shrink-0 text-muted-foreground" />
      <span class="select-none font-mono text-ui-xs text-muted-foreground">$</span>
      <input
        bind:this={pathInput}
        type="text"
        bind:value={jsonPath}
        aria-label="JSONPath filter"
        placeholder=".field  ·  [0]  ·  .items[*].name  ·  ..key"
        class="no-focus-ring min-w-0 flex-1 bg-transparent font-mono text-ui-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
        spellcheck="false"
        autocomplete="off"
        onfocus={() => { pathFocused = true }}
        onblur={() => setTimeout(() => { pathFocused = false }, 120)}
        onkeydown={handlePathKeydown}
      />
      {#if jsonPath}
        <button
          type="button"
          aria-label="Clear filter"
          class="hit-area inline-flex size-4 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground"
          onmousedown={(e) => e.preventDefault()}
          onclick={() => { jsonPath = ''; pathInput?.focus() }}
        >
          <Icon name="x" class="size-3 shrink-0" />
        </button>
      {/if}

      {#if pathFocused && completions.length > 0}
        <JsonPathSuggest
          items={completions}
          query={jsonPath}
          bind:activeIdx
          onpick={(insert) => pickCompletion(insert)}
        />
      {/if}
    </div>

    {#if pathResult && !pathResult.ok}
      <span class="shrink-0 pr-1 font-mono text-ui-2xs text-destructive">{pathResult.error}</span>
    {:else if pathResult?.ok}
      <span class="shrink-0 pr-1 font-mono text-ui-2xs tabular-nums text-muted-foreground">{describeResult(pathResult.value)}</span>
    {:else}
      <span class="shrink-0 pr-1 text-ui-2xs tabular-nums text-muted-foreground">{rows.length === 1 ? '1 row' : `${rows.length} rows`}</span>
    {/if}

    {#if mode === 'tree'}
      <div class="flex h-7 w-48 shrink-0 items-center gap-1.5 rounded-md bg-muted/40 px-2.5 focus-within:bg-muted/70">
        <Icon name="search" class="size-3 shrink-0 text-muted-foreground" />
        <input
          type="text"
          bind:value={query}
          aria-label="Search keys and values"
          placeholder="Search keys, values"
          class="no-focus-ring min-w-0 flex-1 bg-transparent text-ui-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
          spellcheck="false"
          autocomplete="off"
          onkeydown={(e) => { if (e.key === 'Escape') query = '' }}
        />
        {#if found}
          <span class="shrink-0 text-ui-3xs tabular-nums text-muted-foreground">{found.count}{found.truncated ? '+' : ''}</span>
        {/if}
      </div>
    {/if}

    <button
      type="button"
      title={decode ? 'JSON stored as text is shown decoded' : 'Show JSON stored as text decoded'}
      aria-pressed={decode}
      class={cn(
        'inline-flex h-7 shrink-0 items-center gap-1 rounded-md px-2 text-ui-xs transition-colors',
        decode ? 'bg-muted/60 text-foreground' : 'text-muted-foreground hover:bg-muted/40 hover:text-foreground',
      )}
      onclick={() => (decode = !decode)}
    >
      <Icon name="braces" class="size-3.5 shrink-0" />
      Decode
    </button>
    <button
      type="button"
      title="Copy as JSON"
      aria-label="Copy as JSON"
      class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground"
      onclick={() => copyValue(mode === 'text' ? displayedJson : shownValue)}
    >
      <Icon name="copy" class="size-3.5 shrink-0" />
    </button>
    {#if mode === 'text'}<JsonWrapToggle />{/if}
  </div>

  <!-- JSON body (⌘F to search) -->
  {#if columns.length === 0}
    <div class="flex min-h-0 flex-1 items-center justify-center bg-panel">
      <p class="font-mono text-ui-sm text-muted-foreground">No data to display</p>
    </div>
  {:else if mode === 'tree'}
    <div class="min-h-0 flex-1 overflow-auto bg-panel px-3 py-2 font-mono text-ui-xs" data-studio-selectable="text">
      {#key pathResult?.ok ? evalPath : ''}
        <JsonTree
          value={treeRoot}
          defaultDepth={1}
          oncopy={copyValue}
          query={found ? evalQuery : ''}
          matchPaths={found?.paths ?? null}
          openPaths={found?.open ?? null}
        />
      {/key}
      {#if found && found.count === 0}
        <p class="px-1 py-3 text-ui-xs text-muted-foreground">No key or value matches “{evalQuery}”.</p>
      {/if}
    </div>
  {:else}
    <CodeTextView text={displayedJson} language="json" wordWrap={$appJsonWordWrap ? 'on' : 'off'} />
  {/if}
</div>
