<script>
  import JsonWrapToggle from './JsonWrapToggle.svelte'
  import JsonPathSuggest from './JsonPathSuggest.svelte'
  import { appJsonWordWrap } from '$lib/stores/settings.js'
  import { onDestroy, tick, untrack } from 'svelte'
  import CodeEditor from './CodeEditor.svelte'
  import CodeTextView from './CodeTextView.svelte'
  import ResizeHandle from './ResizeHandle.svelte'
  import { loadLayout, saveLayout } from '$lib/stores/layout.js'
  import Copy from '@lucide/svelte/icons/copy'
  import CheckCheck from '@lucide/svelte/icons/check-check'
  import Braces from '@lucide/svelte/icons/braces'
  import Wand2 from '@lucide/svelte/icons/wand-2'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import { evalJsonPath, getCompletionItems, applyCompletion, describeResult } from '$lib/jsonpath.js'

  let { active = false } = $props()

  // ── Layout ────────────────────────────────────────────────────────────────
  const PANEL_MIN = 100
  const stored = loadLayout()
  let inputHeight = $state(untrack(() => Math.max(PANEL_MIN, Math.min(stored.sqlEditorHeight ?? 320, 520))))
  let resizeStart = $state(0)
  /** @type {HTMLElement | null} */
  let pageEl = $state(null)

  function clampHeight(h) {
    const total = pageEl?.clientHeight ?? 0
    const max = total > 0 ? Math.max(PANEL_MIN, total - PANEL_MIN - 32) : 600
    return Math.round(Math.min(max, Math.max(PANEL_MIN, h)))
  }

  // ── Editors ───────────────────────────────────────────────────────────────
  /** @type {CodeEditor | null} */  let inputEditor = $state(null)
  /** The input's text as this page sets it (Format, Clear); typing comes back through `onInput`. */
  let inputValue = $state('')

  // ── Raw JSON: split into "immediate" (for header) + "debounced" (for parse).
  //    JSON.parse() on every keystroke for large documents is the main CPU hog.
  let rawJson = $state('')   // updated immediately - drives header
  let rawJsonDebounced = $state('')   // updated 250ms after typing stops
  /** @type {ReturnType<typeof setTimeout> | null} */
  let parseDebounceTimer = null

  // ── JSONPath ──────────────────────────────────────────────────────────────
  let jsonPath = $state('')
  let pathFocused = $state(false)
  let activeIdx = $state(-1)
  /** @type {HTMLInputElement | null} */
  let pathInput = $state(null)
  let copied = $state(false)
  let copiedInput = $state(false)
  /** @type {ReturnType<typeof setTimeout> | null} */
  let copiedTimer = null
  /** @type {ReturnType<typeof setTimeout> | null} */
  let copiedInputTimer = null

  // ── Derived - use debounced JSON so parse runs at most once per 250ms ─────
  const parsedJson = $derived.by(() => {
    try { return JSON.parse(rawJsonDebounced) } catch { return null }
  })

  const pathResult = $derived.by(() => {
    const p = jsonPath.trim()
    if (!p || p === '$') return null
    if (parsedJson === null) return null
    return evalJsonPath(parsedJson, p.startsWith('$') ? p : '$' + p)
  })

  const resultJson = $derived.by(() => {
    if (!pathResult?.ok) return null
    return JSON.stringify(pathResult.value, null, 2)
  })

  /** @type {import('$lib/jsonpath.js').CompletionItem[]} */
  const completionItems = $derived.by(() => {
    if (!pathFocused || parsedJson === null) return []
    return getCompletionItems(parsedJson, jsonPath).slice(0, 10)
  })

  const inputSummary = $derived.by(() => {
    if (!rawJson.trim()) return null
    if (parsedJson === null) return rawJson.trim() ? 'invalid JSON' : null
    if (Array.isArray(parsedJson)) return `${parsedJson.length} ${parsedJson.length === 1 ? 'item' : 'items'}`
    if (typeof parsedJson === 'object' && parsedJson !== null) return `${Object.keys(parsedJson).length} keys`
    return typeof parsedJson
  })

  // ── Effects ───────────────────────────────────────────────────────────────

  $effect(() => {
    activeIdx = pathFocused && completionItems.length ? 0 : -1
  })

  $effect(() => {
    if (active && inputEditor && !rawJson.trim()) {
      void tick().then(() => inputEditor?.focus())
    }
  })

  // ── Completion helpers ────────────────────────────────────────────────────

  /** @param {import('$lib/jsonpath.js').CompletionItem} item */
  function pickCompletion(insert) {
    jsonPath = applyCompletion(jsonPath, insert)
    activeIdx = -1
    pathInput?.focus()
  }

  /** @param {KeyboardEvent} e */
  function handlePathKeydown(e) {
    if (completionItems.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        activeIdx = (activeIdx + 1) % completionItems.length
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        activeIdx = (activeIdx - 1 + completionItems.length) % completionItems.length
        return
      }
      // Tab: accept first item if nothing selected, else accept selected
      if (e.key === 'Tab') {
        e.preventDefault()
        pickCompletion(completionItems[activeIdx >= 0 ? activeIdx : 0].insert)
        return
      }
      // Enter: only accept if a row is explicitly highlighted
      if (e.key === 'Enter' && activeIdx >= 0) {
        e.preventDefault()
        pickCompletion(completionItems[activeIdx].insert)
        return
      }
    }
    if (e.key === 'Escape') {
      activeIdx = -1
      pathFocused = false
      pathInput?.blur()
    }
  }

  // ── Kind icon + color (minimal - single letter, no text chars like "{}") ──
  /** @param {import('$lib/jsonpath.js').CompletionItem['kind']} kind */


  // ── Actions ───────────────────────────────────────────────────────────────
  function handleCopyResult() {
    if (!resultJson) return
    navigator.clipboard.writeText(resultJson).then(() => {
      copied = true
      if (copiedTimer) clearTimeout(copiedTimer)
      copiedTimer = setTimeout(() => { copied = false }, 2000)
    })
  }

  function handleCopyInput() {
    navigator.clipboard.writeText(rawJson).then(() => {
      copiedInput = true
      if (copiedInputTimer) clearTimeout(copiedInputTimer)
      copiedInputTimer = setTimeout(() => { copiedInput = false }, 2000)
    })
  }

  /** Put text in the input from here: a new document, parsed at once. @param {string} text */
  function setInput(text) {
    inputValue = text
    if (parseDebounceTimer !== null) { clearTimeout(parseDebounceTimer); parseDebounceTimer = null }
    rawJson = text
    rawJsonDebounced = text
  }

  function formatJson() {
    if (parsedJson === null) return
    setInput(JSON.stringify(parsedJson, null, 2))
    inputEditor?.focus()
  }

  function clearInput() {
    setInput('')
    jsonPath = ''
    inputEditor?.focus()
  }

  /** Typing in the input: the header follows at once, the parse 250ms after it stops. @param {string} val */
  function onInput(val) {
    rawJson = val
    // Debounce the expensive parse so it runs at most once per 250ms
    // instead of on every single keystroke. For a 500kb JSON file this
    // prevents multiple full parse passes per second.
    if (parseDebounceTimer !== null) clearTimeout(parseDebounceTimer)
    parseDebounceTimer = setTimeout(() => {
      parseDebounceTimer = null
      rawJsonDebounced = val
    }, 250)
  }

  onDestroy(() => { if (parseDebounceTimer !== null) clearTimeout(parseDebounceTimer) })
</script>

<div bind:this={pageEl} class="flex min-h-0 flex-1 flex-col overflow-hidden">

  <!-- ── Input panel header ────────────────────────────────────────────── -->
  <div class="studio-chrome flex h-8 shrink-0 items-center gap-2 border-b border-border bg-panel px-3" data-studio-chrome>
    <Braces class="size-3.5 shrink-0 text-muted-foreground" />
    <span class="font-mono text-ui-xs font-medium text-foreground/70">JSON Input</span>

    {#if inputSummary === 'invalid JSON'}
      <span class="font-mono text-ui-2xs text-destructive">invalid JSON</span>
    {:else if inputSummary}
      <span class="font-mono text-ui-2xs text-muted-foreground">{inputSummary}</span>
    {:else}
      <span class="font-mono text-ui-2xs text-muted-foreground">paste or type JSON here</span>
    {/if}

    <div class="ml-auto flex shrink-0 items-center gap-0.5">
      {#if parsedJson !== null}
        <JsonWrapToggle />
        <button type="button"
          class="inline-flex items-center gap-1 rounded px-2 py-0.5 font-mono text-ui-2xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          onclick={formatJson}
        ><Wand2 class="size-3" />Format</button>
      {/if}
      {#if rawJson.trim()}
        <button type="button"
          class="inline-flex items-center gap-1 rounded px-2 py-0.5 font-mono text-ui-2xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          onclick={handleCopyInput}
        >
          {#if copiedInput}<CheckCheck class="size-3 text-success" />{:else}<Copy class="size-3" />{/if}
          Copy
        </button>
        <button type="button"
          class="inline-flex size-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
          onclick={clearInput}
        ><Trash2 class="size-3" /></button>
      {/if}
    </div>
  </div>

  <!-- ── Input ─────────────────────────────────────────────────────────── -->
  <div class="relative flex shrink-0 flex-col overflow-hidden" style="height: {inputHeight}px">
    <CodeEditor bind:this={inputEditor} value={inputValue} lang="json" wrap={$appJsonWordWrap} onchange={onInput} ariaLabel="JSON input" />
    {#if !rawJson.trim()}
      <div class="pointer-events-none absolute inset-0 flex items-center justify-center">
        <p class="font-mono text-ui-sm text-muted-foreground">Paste or type JSON here</p>
      </div>
    {/if}
  </div>

  <ResizeHandle
    axis="y"
    edge="end"
    onresizestart={() => { resizeStart = inputHeight }}
    onresize={(dy) => { inputHeight = clampHeight(resizeStart + dy) }}
    onresizeend={() => saveLayout({ sqlEditorHeight: inputHeight })}
  />

  <!-- ── JSONPath bar ──────────────────────────────────────────────────── -->
  <div class="studio-chrome relative flex h-8 shrink-0 items-center gap-1.5 border-b border-border bg-panel px-3" data-studio-chrome>
    <span class="select-none font-mono text-ui-xs text-muted-foreground">$</span>
    <input
      bind:this={pathInput}
      type="text"
      bind:value={jsonPath}
      placeholder=".field  ·  [0]  ·  [*].name  ·  ..key  ·  [?(@.x > 0)]"
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

    <!-- Minimal VSCode-style suggestion list -->
    {#if pathFocused && completionItems.length > 0}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <JsonPathSuggest
        items={completionItems}
        query={jsonPath}
        bind:activeIdx
        onpick={(insert) => pickCompletion(insert)}
      />
    {/if}

    <div class="ml-auto flex shrink-0 items-center gap-0.5">
      <button type="button"
        class="inline-flex items-center gap-1 rounded px-2 py-0.5 font-mono text-ui-2xs transition-colors {resultJson ? 'text-muted-foreground hover:bg-muted hover:text-foreground' : 'cursor-default text-muted-foreground'}"
        disabled={!resultJson}
        onclick={handleCopyResult}
      >
        {#if copied}
          <CheckCheck class="size-3 text-success" />Copied
        {:else}
          <Copy class="size-3" />Copy result
        {/if}
      </button>
    </div>
  </div>

  <!-- ── Result ────────────────────────────────────────────────────────── -->
  <div class="relative flex min-h-0 flex-1 flex-col overflow-hidden">
    <CodeTextView text={resultJson ?? ''} language="json" wordWrap={$appJsonWordWrap ? 'on' : 'off'} ariaLabel="JSONPath result" />

    {#if !rawJson.trim()}
      <div class="pointer-events-none absolute inset-0 flex items-center justify-center">
        <p class="font-mono text-ui-xs text-muted-foreground">Paste JSON above, then query it here</p>
      </div>
    {:else if parsedJson === null}
      <div class="pointer-events-none absolute inset-0 flex items-center justify-center">
        <p class="font-mono text-ui-xs text-destructive">Fix JSON errors to run queries</p>
      </div>
    {:else if !jsonPath.trim() || jsonPath.trim() === '$'}
      <div class="pointer-events-none absolute inset-0 flex items-center justify-center text-center">
        <div class="space-y-1">
          <p class="font-mono text-ui-xs text-muted-foreground">Enter a JSONPath expression above</p>
          <p class="font-mono text-ui-2xs text-muted-foreground">[*].id · .name · ..email · [?(@.active)]</p>
        </div>
      </div>
    {:else if pathResult && !pathResult.ok}
      <div class="pointer-events-none absolute inset-0 flex items-center justify-center">
        <p class="font-mono text-ui-xs text-destructive">{pathResult.error}</p>
      </div>
    {/if}
  </div>

</div>
