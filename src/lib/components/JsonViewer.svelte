<script>
  import CodeTextView from './CodeTextView.svelte'
  import Table2 from '@lucide/svelte/icons/table-2'
  import Copy from '@lucide/svelte/icons/copy'
  import Download from '@lucide/svelte/icons/download'
  import CheckCheck from '@lucide/svelte/icons/check-check'
  import JsonWrapToggle from './JsonWrapToggle.svelte'
  import JsonPathSuggest from './JsonPathSuggest.svelte'
  import { appJsonWordWrap } from '$lib/stores/settings.js'
  import { cn } from '$lib/utils.js'
  import { evalJsonPath, getCompletionItems, applyCompletion, describeResult } from '$lib/jsonpath.js'

  let {
    json = '[]',
    /**
     * The already-parsed value, when the caller has one.
     *
     * Passing it skips a full `JSON.parse` of `json`. The callers build these
     * objects, stringify them for the editor, and this component used to parse
     * that string straight back into the same shape purely so JSONPath had
     * something to walk - a third pass over the whole payload, and a second
     * copy of it in memory, to recover data the caller was already holding.
     */
    data = /** @type {any} */ (undefined),
    /** Rows actually rendered, when the caller bounded what it handed over. */
    shownRows = 0,
    rowCount = 0,
    onshowtable = () => {},
    ondownload = /** @type {(() => void) | undefined} */ (undefined),
  } = $props()

  /** True when the caller trimmed the result to keep this view responsive. */
  const truncated = $derived(shownRows > 0 && rowCount > shownRows)


  let copied = $state(false)
  /** @type {ReturnType<typeof setTimeout> | null} */
  let copiedTimer = null

  // ── JSONPath ──────────────────────────────────────────────────────────────
  let jsonPath = $state('')
  let pathFocused = $state(false)
  let activeIdx = $state(-1)
  /** @type {HTMLInputElement | null} */
  let pathInput = $state(null)

  const parsedJson = $derived.by(() => {
    if (data !== undefined) return data
    try { return JSON.parse(json) } catch { return null }
  })

  const pathResult = $derived.by(() => {
    const p = jsonPath.trim()
    if (!p || p === '$') return null
    if (parsedJson === null) return null
    return evalJsonPath(parsedJson, p)
  })

  const displayedJson = $derived.by(() => {
    if (!pathResult?.ok) return json
    return JSON.stringify(pathResult.value, null, 2)
  })

  // The rich items, not just their insert strings. `kind`, `detail` and
  // `preview` were being computed and then thrown away, which is why every row
  // read `.key auth_name .auth_name` - the widget was reconstructing a worse
  // version of data it already had.
  const completions = $derived.by(() => {
    if (!pathFocused || parsedJson === null) return []
    return getCompletionItems(parsedJson, jsonPath).slice(0, 12)
  })

  // Nothing selected means Enter does nothing, which is the wrong default for a
  // list that is already filtered to what you typed. The top match is armed, so
  // Tab or Enter takes it straight away.
  $effect(() => {
    void completions
    activeIdx = completions.length ? 0 : -1
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


  function handleCopy() {
    navigator.clipboard.writeText(displayedJson).then(() => {
      copied = true
      if (copiedTimer) clearTimeout(copiedTimer)
      copiedTimer = setTimeout(() => { copied = false }, 2000)
    })
  }

</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  <!-- JSONPath bar -->
  <div class="studio-chrome relative flex h-8 shrink-0 items-center gap-1.5 border-b border-border bg-panel px-3">
    <span class="select-none font-mono text-ui-xs text-muted-foreground">$</span>
    <input
      bind:this={pathInput}
      type="text"
      bind:value={jsonPath}
      placeholder=".field  ·  [0]  ·  .items[*].name  ·  ..key"
      class="no-focus-ring min-w-0 flex-1 bg-transparent font-mono text-ui-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
      spellcheck="false"
      autocomplete="off"
      onfocus={() => { pathFocused = true }}
      onblur={() => setTimeout(() => { pathFocused = false }, 120)}
      onkeydown={handlePathKeydown}
    />

    {#if pathResult && !pathResult.ok}
      <span class="shrink-0 font-mono text-ui-2xs text-destructive">{pathResult.error}</span>
    {:else if pathResult?.ok}
      <span class="shrink-0 font-mono text-ui-2xs text-muted-foreground">{describeResult(pathResult.value)}</span>
    {/if}

    {#if pathFocused && completions.length > 0}
      <JsonPathSuggest
        items={completions}
        query={jsonPath}
        bind:activeIdx
        onpick={(insert) => pickCompletion(insert)}
      />
    {/if}

    <!-- toolbar right side -->
    <div class="ml-auto flex shrink-0 items-center gap-0.5">
      {#if truncated}
        <!-- Say what is missing rather than quietly showing a prefix: a JSON
             document that silently stops at 1000 rows reads as the answer. -->
        <span
          class="select-none px-2 font-mono text-ui-2xs text-warning"
          title="The JSON view is capped to keep it responsive. Export to get every row."
        >{shownRows.toLocaleString()} of {rowCount.toLocaleString()} rows</span>
      {:else if rowCount > 0}
        <span class="select-none px-2 font-mono text-ui-2xs text-muted-foreground">{rowCount} rows</span>
        <div class="h-4 w-px bg-border/60"></div>
      {/if}

      <JsonWrapToggle />

      <button
        type="button"
        title="Copy JSON"
        class="inline-flex items-center gap-1.5 rounded-md px-2 py-1 font-mono text-ui-2xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        onclick={handleCopy}
      >
        {#if copied}
          <CheckCheck class="size-3 shrink-0 text-success" />
          <span>Copied</span>
        {:else}
          <Copy class="size-3 shrink-0" />
          <span>Copy</span>
        {/if}
      </button>

      {#if ondownload}
        <button
          type="button"
          title="Download JSON"
          class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          onclick={ondownload}
        >
          <Download class="size-3 shrink-0" />
        </button>
      {/if}

      <div class="h-4 w-px bg-border/60"></div>

      <button
        type="button"
        class="inline-flex items-center gap-1.5 rounded-md px-2 py-1 font-mono text-ui-2xs font-medium text-foreground transition-colors hover:bg-muted"
        onclick={onshowtable}
      >
        <Table2 class="size-3 shrink-0" />
        Show table
      </button>
    </div>
  </div>

  <!-- JSON body (⌘F to search). Wrap is an app setting, so a change made
       anywhere reflows this view too. -->
  <CodeTextView text={displayedJson} language="json" wordWrap={$appJsonWordWrap ? 'on' : 'off'} ariaLabel="Result as JSON" />
</div>
