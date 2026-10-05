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

  const records = $derived(rows.map((r) => rowToRecord(columns, r)))
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
  <div class="studio-chrome flex h-8 shrink-0 items-center gap-2 border-b border-border bg-panel px-2">
    <div
      class={cn(
        'relative flex h-6 min-w-0 flex-1 items-center gap-1.5 rounded-md border border-transparent bg-input/30 px-2 transition-colors',
        pathFocused ? 'border-input' : 'hover:border-border/60',
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
          class="inline-flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:text-foreground"
          onmousedown={(e) => e.preventDefault()}
          onclick={() => { jsonPath = ''; pathInput?.focus() }}
        >
          <Icon name="x" class="size-3" />
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
    {/if}
  </div>

  <!-- JSON body (⌘F to search) -->
  {#if columns.length === 0}
    <div class="flex min-h-0 flex-1 items-center justify-center bg-panel">
      <p class="font-mono text-ui-sm text-muted-foreground">No data to display</p>
    </div>
  {:else}
    <CodeTextView text={displayedJson} language="json" wordWrap={$appJsonWordWrap ? 'on' : 'off'} />
  {/if}
</div>
