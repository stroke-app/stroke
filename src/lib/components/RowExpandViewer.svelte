<script>
  import { appJsonWordWrap, loadSettings, saveSettings } from '$lib/stores/settings.js'
  import { tick } from 'svelte'
  import Copy from '@lucide/svelte/icons/copy'
  import Check from '@lucide/svelte/icons/check'
  import WrapText from '@lucide/svelte/icons/wrap-text'
  import ListTree from '@lucide/svelte/icons/list-tree'
  import Braces from '@lucide/svelte/icons/braces'
  import X from '@lucide/svelte/icons/x'
  import JsonTree from './JsonTree.svelte'
  import { formatJsonValue } from '$lib/row-inspector.js'
  import {
    escapeHtml,
    highlightJson,
    linkifyJsonInElement,
    getTextOffsetInRoot,
    getJsonValueRangeAtOffset,
  } from '$lib/json-inspector.js'

  const TRUNCATE_LIMIT = 200
  // Max array items / object keys kept in the raw-text preview. Bounds the
  // stringified size for wide values (e.g. a Buffer's `data` byte array with
  // thousands of numbers) so the preview never balloons. Copy JSON still copies
  // the full, untruncated value.
  const PREVIEW_CHILD_LIMIT = 200
  const HIGHLIGHT_LIMIT = 12000

  let {
    record,
    rowLabel = '',
    /** Open a value in the Monaco JSON lightbox. @type {((value: unknown, label: string) => void) | null} */
    onopenjson = null,
    /** Collapse this expanded row. @type {(() => void) | null} */
    onclose = null,
    /** Width of the grid's left gutters, so the panel's content starts under the
     *  first column instead of under the checkbox. */
    indent = 0,
  } = $props()

  let html = $state('')

  // Tree = collapsible fold view (issue #2: wrap/unwrap json inside row data);
  // Raw = the original highlighted text block.
  const VIEW_KEY = 'stroke:row-expand-view'
  function loadView() {
    try { return localStorage.getItem(VIEW_KEY) === 'raw' ? 'raw' : 'tree' } catch { return 'tree' }
  }

  let copied = $state(false)
  let viewMode = $state(loadView())

  /** @param {'tree' | 'raw'} mode */
  function setViewMode(mode) {
    viewMode = mode
    try { localStorage.setItem(VIEW_KEY, mode) } catch { /* ignore */ }
  }

  /** Copy a single tree-node value (raw string as-is, everything else as JSON). */
  function copyNodeValue(/** @type {unknown} */ v) {
    copyText(typeof v === 'string' ? v : formatJsonValue(v))
  }
  /** @type {ReturnType<typeof setTimeout> | null} */
  let copiedTimer = null
  /** @type {HTMLDivElement | null} */
  let rootEl = $state(null)

  /** @type {{ x: number, y: number, value: string | null } | null} */
  let contextMenu = $state(null)

  // Bounded preview of the record: strings truncated to TRUNCATE_LIMIT and
  // arrays/objects capped to PREVIEW_CHILD_LIMIT children. `truncated` records
  // whether anything was cut, which drives whether we highlight at all.
  const displayInfo = $derived.by(() => {
    let truncated = false
    /** @param {unknown} v */
    const walk = (v) => {
      if (typeof v === 'string') {
        if (v.length > TRUNCATE_LIMIT) { truncated = true; return v.slice(0, TRUNCATE_LIMIT) + '…' }
        return v
      }
      if (Array.isArray(v)) {
        const head = v.slice(0, PREVIEW_CHILD_LIMIT).map(walk)
        if (v.length > PREVIEW_CHILD_LIMIT) { truncated = true; head.push(`… ${v.length - PREVIEW_CHILD_LIMIT} more items`) }
        return head
      }
      if (v !== null && typeof v === 'object') {
        /** @type {Record<string, unknown>} */
        const out = {}
        let i = 0
        for (const k in v) {
          if (i >= PREVIEW_CHILD_LIMIT) { truncated = true; break }
          out[k] = walk(/** @type {Record<string, unknown>} */ (v)[k])
          i++
        }
        return out
      }
      return v
    }
    return { text: formatJsonValue(walk(record)), truncated }
  })
  const displayText = $derived(displayInfo.text)

  function fullJsonText() {
    return formatJsonValue(record)
  }

  $effect(() => {
    // Tree mode never touches the stringified text - skip the format +
    // highlight work entirely until the raw view is actually shown.
    if (viewMode !== 'raw') return
    const { text: source, truncated } = displayInfo
    // Skip per-token highlighting for large values (anything we had to truncate,
    // or a still-long preview) - the span-per-token HTML is the memory hog. Plain
    // escaped text stays cheap to build and render.
    html = truncated || source.length > HIGHLIGHT_LIMIT
      ? `<pre class="m-0 p-0 font-mono text-ui-xs leading-relaxed whitespace-pre text-foreground">${escapeHtml(source)}</pre>`
      : highlightJson(source)
  })

  // keep the wordWrap reactive - the pre's whitespace class is toggled via CSS var
  const wrapClass = $derived($appJsonWordWrap ? 'whitespace-pre-wrap break-all' : 'whitespace-pre')

  $effect(() => {
    if (!html || !rootEl) return
    const source = displayText
    // Plain (non-highlighted) render for large values - skip the link scan too.
    if (displayInfo.truncated || source.length > HIGHLIGHT_LIMIT) return
    void tick().then(() => {
      const pre = rootEl?.querySelector('pre')
      if (pre instanceof HTMLElement) linkifyJsonInElement(pre, source)
    })
  })

  async function copyJson() {
    try {
      await navigator.clipboard.writeText(fullJsonText())
      copied = true
      if (copiedTimer) clearTimeout(copiedTimer)
      copiedTimer = setTimeout(() => { copied = false }, 2000)
    } catch {
      /* clipboard unavailable */
    }
  }

  /** @param {string} text */
  async function copyText(text) {
    try {
      await navigator.clipboard.writeText(text)
      copied = true
      if (copiedTimer) clearTimeout(copiedTimer)
      copiedTimer = setTimeout(() => { copied = false }, 2000)
    } catch {
      /* clipboard unavailable */
    }
  }

  /**
   * @param {number} clientX
   * @param {number} clientY
   * @returns {string | null}
   */
  function valueAtPoint(clientX, clientY) {
    const pre = rootEl?.querySelector('pre')
    if (!pre) return null

    /** @type {Node | null} */
    let node = null
    let offset = 0

    if ('caretPositionFromPoint' in document) {
      const pos = /** @type {any} */ (document).caretPositionFromPoint(clientX, clientY)
      if (pos) { node = pos.offsetNode; offset = pos.offset }
    } else if ('caretRangeFromPoint' in document) {
      const r = /** @type {any} */ (document).caretRangeFromPoint(clientX, clientY)
      if (r) { node = r.startContainer; offset = r.startOffset }
    }

    if (!node || !pre.contains(node)) return null

    const srcOffset = getTextOffsetInRoot(pre, node, offset)
    const range = getJsonValueRangeAtOffset(displayText, srcOffset)
    if (!range) return null

    const raw = displayText.slice(range.start, range.end)

    if (raw.length >= 2 && raw[0] === '"' && raw[raw.length - 1] === '"') {
      try {
        const parsed = JSON.parse(raw)
        if (typeof parsed === 'string') return parsed
      } catch {
        return raw.slice(1, -1)
      }
    }

    return raw
  }

  /** @param {MouseEvent} e */
  function handleContextMenu(e) {
    e.preventDefault()
    const value = valueAtPoint(e.clientX, e.clientY)
    contextMenu = { x: e.clientX, y: e.clientY, value }
  }

  function dismissMenu() {
    contextMenu = null
  }

  /**
   * When wrap is off the box is `overflow-x-auto`, which makes the browser
   * promote `overflow-y` to `auto` too - turning it into a nested scroll
   * container. WebKitGTK won't chain a trapped vertical wheel up to the
   * virtual-scroll table, so vertical scrolling stalls over the JSON. Forward
   * vertical deltas to the table's native scroller; leave genuine horizontal
   * gestures to scroll the JSON box itself. (With wrap on the box isn't a
   * scroll container, so native chaining handles everything smoothly and this
   * never runs.)
   * @param {WheelEvent} e
   */
  function handleWheel(e) {
    const el = rootEl
    if (!el) return
    const canScrollH = el.scrollWidth > el.clientWidth
    if (!canScrollH) return // not a scroll trap - let native chaining scroll the table
    if (Math.abs(e.deltaX) > Math.abs(e.deltaY)) return // horizontal gesture → scroll the JSON
    const scroller = el.closest('[data-canvas-table]')
    if (!(scroller instanceof HTMLElement)) return
    // Normalize line/page deltas to pixels so forwarded scroll matches native feel.
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? scroller.clientHeight : 1
    if (e.shiftKey) scroller.scrollLeft += (e.deltaY || e.deltaX) * unit
    else scroller.scrollTop += e.deltaY * unit
    e.preventDefault()
  }

  /**
   * @param {HTMLElement} node
   */
  function portal(node) {
    document.body.appendChild(node)
    return {
      destroy() {
        if (node.isConnected) node.remove()
      },
    }
  }
</script>

<!-- Elevated card so the expanded row reads as a distinct section over the grid
     (bg-background matched the grid and had no bottom edge, so it blended in).
     It lines up with the grid rather than fighting it: content starts where the
     first column starts, and the frozen gutter continues down the left as a
     filled strip (matching the grid's row-number column) so its right edge reads
     as the gutter rule - not a hairline floating in empty space. -->
<!-- The tone is mixed from the theme's muted and background colours rather than
     taken from --card, which several themes set to the grid's own background,
     so the panel reads as set into the grid in every theme. -->
<div class="relative border-y border-border/50 bg-[color-mix(in_oklch,var(--muted)_32%,var(--background))]">
  {#if indent > 0}
    <div class="pointer-events-none absolute inset-y-0 left-0 border-r border-border/40 bg-muted/20" style="width:{indent}px"></div>
  {/if}
  <div style={indent > 0 ? `padding-left:${indent}px` : undefined}>
  <!-- Toolbar -->
  <div class="flex items-center gap-2 border-b border-border/40 px-3 py-1">
    {#if rowLabel}
      <span class="font-mono text-ui-xs text-muted-foreground select-none">{rowLabel}</span>
      <span class="text-border/40 select-none text-ui-xs">·</span>
    {/if}
    <button
      type="button"
      class="inline-flex items-center gap-1.5 rounded-md px-2 py-0.5 font-mono text-ui-xs text-muted-foreground transition-colors hover:bg-accent/40 hover:text-foreground"
      onclick={copyJson}
    >
      {#if copied}
        <Check class="size-3.5 shrink-0 text-success" />
        <span>Copied</span>
      {:else}
        <Copy class="size-3.5 shrink-0" />
        <span>Copy JSON</span>
      {/if}
    </button>
    {#if viewMode === 'raw'}
      <button
        type="button"
        title="Toggle word wrap"
        class={[
          'inline-flex items-center gap-1.5 rounded px-2 py-0.5 font-mono text-ui-xs transition-colors hover:bg-accent/40 hover:text-foreground',
          $appJsonWordWrap ? 'text-foreground bg-accent/30' : 'text-muted-foreground',
        ].join(' ')}
        onclick={() => {
          const next = !$appJsonWordWrap
          appJsonWordWrap.set(next)
          saveSettings({ ...loadSettings(), jsonWordWrap: next })
        }}
      >
        <WrapText class="size-3.5 shrink-0" />
        <span>Wrap</span>
      </button>
    {/if}

    <!-- Tree / Raw view toggle -->
    <div class="ml-auto flex items-center gap-0.5 rounded bg-muted/30 p-0.5">
      <button
        type="button"
        title="Tree view, expand and collapse nested values"
        class={[
          'inline-flex items-center gap-1 rounded px-1.5 py-0.5 font-mono text-ui-xs transition-colors',
          viewMode === 'tree' ? 'bg-background text-foreground' : 'text-muted-foreground hover:text-foreground',
        ].join(' ')}
        onclick={() => setViewMode('tree')}
      >
        <ListTree class="size-3.5 shrink-0" />
        <span>Tree</span>
      </button>
      <button
        type="button"
        title="Raw JSON text"
        class={[
          'inline-flex items-center gap-1 rounded px-1.5 py-0.5 font-mono text-ui-xs transition-colors',
          viewMode === 'raw' ? 'bg-background text-foreground' : 'text-muted-foreground hover:text-foreground',
        ].join(' ')}
        onclick={() => setViewMode('raw')}
      >
        <Braces class="size-3.5 shrink-0" />
        <span>Raw</span>
      </button>
    </div>

    <!-- Close (collapse this row), also bound to Esc in the grid. -->
    {#if onclose}
      <button
        type="button"
        title="Close (Esc)"
        aria-label="Close expanded row"
        class="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={onclose}
      >
        <X class="size-3.5 shrink-0" />
      </button>
    {/if}
  </div>

  <!-- JSON content -->
  {#if viewMode === 'tree'}
    <div class="px-4 py-2" data-studio-selectable="text">
      <JsonTree value={record} defaultDepth={1} oncopy={copyNodeValue} onopen={onopenjson} />
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      bind:this={rootEl}
      data-studio-selectable="text"
      class={['px-5 py-2', $appJsonWordWrap ? '' : 'overflow-x-auto'].join(' ')}
      onwheel={handleWheel}
      oncontextmenu={handleContextMenu}
    >
      {#if !html}
        <p class="loading-reveal font-mono text-ui-xs text-muted-foreground">Loading…</p>
      {:else}
        <div
          class="[&_pre]:m-0 [&_pre]:bg-transparent! [&_pre]:p-0 [&_pre]:font-mono [&_pre]:text-ui-xs [&_pre]:leading-relaxed [&_.json-inspector-url]:text-link [&_.json-inspector-url]:hover:underline [&_.json-inspector-url]:underline-offset-2 {wrapClass === 'whitespace-pre-wrap break-all' ? '[&_pre]:whitespace-pre-wrap [&_pre]:break-all' : '[&_pre]:whitespace-pre'}"
        >
          {@html html}
        </div>
      {/if}
    </div>
  {/if}
  </div>
</div>

<!-- context menu, portalled to body to escape will-change:transform on DataTable -->
{#if contextMenu}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    use:portal
    style="position:fixed;inset:0;z-index:9999"
    onmousedown={dismissMenu}
    oncontextmenu={(e) => e.preventDefault()}
  >
    <div
      class="min-w-36 overflow-hidden rounded border border-border/60 bg-popover py-0.5 elevate-2-rim"
      style="position:fixed;left:{contextMenu.x}px;top:{contextMenu.y}px"
      onmousedown={(e) => e.stopPropagation()}
    >
      {#if contextMenu.value !== null}
        <button
          type="button"
          class="flex w-full items-center gap-2 px-2.5 py-1 text-left text-ui-2xs text-foreground hover:bg-accent hover:text-accent-foreground"
          onclick={() => { copyText(/** @type {string} */ (contextMenu?.value)); dismissMenu() }}
        >
          <Copy class="size-3.5 shrink-0 text-muted-foreground" />
          Copy value
        </button>
      {/if}
      <button
        type="button"
        class="flex w-full items-center gap-2 px-2.5 py-1 text-left text-ui-2xs text-foreground hover:bg-accent hover:text-accent-foreground"
        onclick={() => { copyJson(); dismissMenu() }}
      >
        <Copy class="size-3.5 shrink-0 text-muted-foreground" />
        Copy JSON
      </button>
    </div>
  </div>
{/if}
