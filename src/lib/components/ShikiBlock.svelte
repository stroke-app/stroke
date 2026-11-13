<script>
  import { tick, onDestroy } from 'svelte'
  import { cn } from '$lib/utils.js'
  import { appThemeId } from '$lib/stores/settings.js'
  import {
    escapeHtml,
    highlightJson,
    getJsonValueRangeAtOffset,
    getTextOffsetInRoot,
    linkifyJsonInElement,
    selectTextOffsets,
  } from '$lib/json-inspector.js'
  import Copy from '@lucide/svelte/icons/copy'
  import Check from '@lucide/svelte/icons/check'

  let {
    code = '',
    lang = 'plaintext',
    /** Enable JSON value select-on-dblclick and clickable URLs */
    jsonInteractive = false,
    /** Compact embed (AI chat SQL blocks) - no full-height panel chrome */
    embedded = false,
    /** Disable soft wrapping - long lines scroll horizontally instead of
     *  breaking mid-token (dialog code surfaces like DDL / generated SQL). */
    nowrap = false,
    class: className = '',
  } = $props()

  let html = $state('')
  let loading = $state(false)
  let copied = $state(false)
  /** @type {HTMLDivElement | null} */
  let rootEl = $state(null)

  /** @type {ReturnType<typeof setTimeout> | null} */
  let copyTimer = null

  async function copyCode() {
    await navigator.clipboard.writeText(code ?? '').catch(() => {})
    copied = true
    if (copyTimer) clearTimeout(copyTimer)
    copyTimer = setTimeout(() => { copied = false }, 1500)
  }

  onDestroy(() => {
    if (copyTimer) clearTimeout(copyTimer)
  })

  const appTheme = $derived($appThemeId)
  const isJsonInteractive = $derived(jsonInteractive && lang === 'json')

  $effect(() => {
    const source = code ?? ''
    const language = lang
    const theme = appTheme

    // JSON / plaintext never touch shiki - keeps the heavy highlighter (and its
    // wasm engine) out of the startup bundle, since the data-browsing path only
    // ever renders these. Other languages lazy-load shiki on demand.
    if (language === 'json') { html = highlightJson(source); loading = false; return }
    if (!language || language === 'plaintext') {
      html = `<pre class="p-3 font-mono text-ui-sm text-foreground whitespace-pre-wrap break-all">${escapeHtml(source)}</pre>`
      loading = false
      return
    }

    let cancelled = false
    loading = true
    import('$lib/shiki-highlighter.js')
      .then(({ highlightCode }) => highlightCode(source, language, theme))
      .then((result) => {
        if (!cancelled) html = result
      })
      .catch(() => {
        if (!cancelled) {
          html = `<pre class="p-3 font-mono text-ui-sm text-foreground whitespace-pre-wrap break-all">${escapeHtml(source)}</pre>`
        }
      })
      .finally(() => {
        if (!cancelled) loading = false
      })
    return () => {
      cancelled = true
    }
  })

  $effect(() => {
    if (!isJsonInteractive || !html || !rootEl) return
    const source = code ?? ''
    void tick().then(() => {
      const pre = rootEl?.querySelector('pre')
      if (pre instanceof HTMLElement) linkifyJsonInElement(pre, source)
    })
  })

  /** @param {MouseEvent} e */
  function caretRangeFromEvent(e) {
    if (document.caretRangeFromPoint) {
      return document.caretRangeFromPoint(e.clientX, e.clientY)
    }
    const pos = document.caretPositionFromPoint?.(e.clientX, e.clientY)
    if (!pos) return null
    const range = document.createRange()
    range.setStart(pos.offsetNode, pos.offset)
    range.collapse(true)
    return range
  }

  /** @param {MouseEvent} e */
  function handleDblClick(e) {
    if (!isJsonInteractive || !rootEl) return
    const pre = rootEl.querySelector('pre')
    if (!(pre instanceof HTMLElement)) return

    const range = caretRangeFromEvent(e)
    if (!range) return

    const offset = getTextOffsetInRoot(pre, range.startContainer, range.startOffset)
    const valueRange = getJsonValueRangeAtOffset(code ?? '', offset)
    if (!valueRange) return

    e.preventDefault()
    selectTextOffsets(pre, valueRange.start, valueRange.end)
  }

</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!--
  Embedded mode: no scroll container. Long lines wrap so users see the whole
  code without horizontal scroll. This eliminates the WebKitGTK wheel-absorption
  issue (overflow-x:auto on inner blocks blocks vertical chat scroll) and keeps
  the browser's native momentum/smoothness on the AI chat scroll.

  Full mode (SQL console): keeps the scroll container, that surface is wide
  and benefits from horizontal scroll without a vertical-scroll parent above it.
-->
<div
  bind:this={rootEl}
  data-studio-selectable="text"
  class={cn(
    embedded
      ? 'shiki-block-embedded relative bg-transparent'
      : 'app-scroll group relative min-h-0 flex-1 overflow-auto bg-panel',
    className,
  )}
  ondblclick={handleDblClick}
>
  {#if !embedded}
    <button
      type="button"
      onclick={copyCode}
      class="absolute right-2 top-2 z-10 inline-flex size-6 items-center justify-center rounded-md border border-border bg-background text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 hover:text-foreground"
      aria-label="Copy code"
    >
      {#if copied}
        <Check class="size-3 shrink-0 text-success" />
      {:else}
        <Copy class="size-3 shrink-0" />
      {/if}
    </button>
  {/if}
  {#if loading && !html}
    <p class="px-3 py-4 font-mono text-ui-sm text-muted-foreground">Highlighting…</p>
  {:else}
    <div
      class={cn(
        'shiki-block contents [&_pre]:m-0 [&_pre]:bg-transparent! [&_pre]:font-mono [&_.json-inspector-url]:cursor-pointer [&_.json-inspector-url]:text-link [&_.json-inspector-url]:underline [&_.json-inspector-url]:underline-offset-2 [&_.json-inspector-url]:decoration-link/45 hover:[&_.json-inspector-url]:text-link-hover hover:[&_.json-inspector-url]:decoration-link',
        // The row-inspector JSON view (jsonInteractive) reads denser than a
        // marketing code block - smaller text + normal leading.
        isJsonInteractive ? '[&_pre]:leading-normal' : '[&_pre]:leading-relaxed',
        embedded
          ? '[&_pre]:p-3 [&_pre]:text-ui-xs [&_pre]:whitespace-pre'
          : isJsonInteractive
            ? '[&_pre]:p-3 [&_pre]:text-ui-xs [&_pre]:whitespace-pre-wrap [&_pre]:break-all'
            : nowrap
              ? '[&_pre]:p-4 [&_pre]:text-ui-sm [&_pre]:whitespace-pre'
              : '[&_pre]:p-3 [&_pre]:text-ui-sm [&_pre]:whitespace-pre-wrap [&_pre]:break-all',
      )}
    >
      {@html html}
    </div>
  {/if}
</div>
