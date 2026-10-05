<script>
  import { onDestroy } from 'svelte'
  import { marked } from 'marked'
  import { appThemeId } from '$lib/stores/settings.js'
  import { highlightMarkdownHtml } from '$lib/markdown-highlight.js'
  import { cn } from '$lib/utils.js'
  import { shortcutKeys } from '$lib/kbd-text.js'
  import { escapeHtml } from '$lib/json-inspector.js'

  let {
    content = '',
    class: className = '',
    debounceMs = 0,
    streaming = false,
    /** Called after each paint, so the caller can stick the view to the bottom
     *  when the content has actually grown instead of polling every frame. */
    onrender = null,
  } = $props()

  // ── Custom marked renderer ────────────────────────────────────────────────
  // Images are NEVER rendered as <img> tags - they cause massive performance
  // problems when DB results contain image URL columns (dozens of network
  // requests fire simultaneously). Show a compact link chip instead.
  const renderer = new marked.Renderer()
  renderer.image = ({ href, title, text }) => {
    const url = href ?? ''
    const label = text || title || url.split('/').pop()?.split('?')[0] || 'image'
    const escaped = url.replace(/"/g, '&quot;')
    return `<a href="${escaped}" target="_blank" rel="noopener noreferrer" class="prose-ai-img-link" title="${escaped}">${label}</a>`
  }
  // A shortcut in backticks renders as key caps; everything else stays code.
  // The `+` stays as text between the caps so a copy reads `Ctrl+Shift+P`.
  const codespan = renderer.codespan.bind(renderer)
  renderer.codespan = (token) => {
    const keys = shortcutKeys(token.text)
    if (!keys) return codespan(token)
    const caps = keys.map((k) => `<kbd>${escapeHtml(k)}</kbd>`).join('+')
    return `<span class="ai-kbd-chord">${caps}</span>`
  }
  const markedOpts = /** @type {marked.MarkedOptions} */ ({ breaks: true, gfm: true, renderer })

  // Injected controls live outside Svelte's template, so they can't render
  // @lucide/svelte components - these are the same two glyphs, inlined.
  const ICON_COPY =
    '<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>'
  const ICON_CHECK =
    '<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>'

  /**
   * A copy button for injected (non-Svelte) content.
   *
   * `read` is called at click time rather than now, because the deferred
   * highlight pass replaces the node this button was created for.
   *
   * @param {string} cls
   * @param {() => string} read
   * @param {string} [label] shown next to the icon; icon-only when omitted
   */
  function makeCopyBtn(cls, read, label = '') {
    const btn = document.createElement('button')
    btn.type = 'button'
    btn.className = cls
    btn.setAttribute('aria-label', label || 'Copy')
    btn.title = label || 'Copy'
    const face = (/** @type {string} */ icon, /** @type {string} */ text) =>
      icon + (text ? `<span>${text}</span>` : '')
    btn.innerHTML = face(ICON_COPY, label)
    /** @type {ReturnType<typeof setTimeout> | undefined} */
    let timer
    btn.addEventListener('click', async (e) => {
      e.preventDefault()
      e.stopPropagation()
      const text = read()
      if (!text) return
      await navigator.clipboard.writeText(text).catch(() => {})
      btn.dataset.copied = '1'
      btn.innerHTML = face(ICON_CHECK, label ? 'Copied' : '')
      clearTimeout(timer)
      timer = setTimeout(() => {
        delete btn.dataset.copied
        btn.innerHTML = face(ICON_COPY, label)
      }, 1400)
    })
    return btn
  }

  /**
   * Serialise a rendered table back to a GFM table, so what lands on the
   * clipboard can be pasted into another chat (or this one) unchanged.
   * @param {HTMLTableElement} table
   */
  function tableToMarkdown(table) {
    const rows = [...table.rows].map((r) =>
      [...r.cells].map((c) => (c.textContent ?? '').trim().replace(/\|/g, '\\|')),
    )
    if (rows.length === 0) return ''
    const [head, ...body] = rows
    const line = (/** @type {string[]} */ cells) => `| ${cells.join(' | ')} |`
    return [line(head), line(head.map(() => '---')), ...body.map(line)].join('\n')
  }

  /** @param {HTMLElement} node */
  function copyButtons(node) {
    function inject() {
      // Any <pre>, not just `pre.shiki`: highlighting is deferred (and skipped
      // outright while streaming), so gating on the highlighted markup left the
      // first paint - the version people actually read - with no copy button.
      node.querySelectorAll('pre:not([data-copy-injected])').forEach((pre) => {
        pre.setAttribute('data-copy-injected', '1')
        pre.appendChild(
          makeCopyBtn(
            'ai-block-copy',
            () => pre.querySelector('code')?.textContent ?? pre.textContent ?? '',
          ),
        )
      })
      // Two nested elements, deliberately: the OUTER wrap is positioned and does
      // not scroll (it anchors the copy button), the INNER one scrolls. Anchoring
      // the button inside the scroller sends it drifting into the middle of the
      // table as soon as the user scrolls sideways.
      node.querySelectorAll('table:not([data-copy-injected])').forEach((el) => {
        const table = /** @type {HTMLTableElement} */ (el)
        table.setAttribute('data-copy-injected', '1')
        const wrap = document.createElement('div')
        wrap.className = 'ai-table-wrap'
        const scroller = document.createElement('div')
        scroller.className = 'ai-table-scroll'
        table.replaceWith(wrap)
        scroller.appendChild(table)
        wrap.appendChild(scroller)
        wrap.appendChild(
          makeCopyBtn('ai-block-copy ai-table-copy', () => tableToMarkdown(table), 'Copy table'),
        )
      })
    }

    /**
     * Forward vertical-dominant wheel events out of <pre> code blocks.
     *
     * The `.prose-ai pre` global CSS sets `overflow-x: auto` so wide code can
     * scroll horizontally. But WebKitGTK treats that as a wheel-event consumer
     * for ALL wheel directions, so vertical scrolling over a code block dies
     * inside the <pre> and never reaches the AI chat scroll container.
     *
     * Capture-phase listener intercepts the wheel before <pre>'s default scroll
     * runs, redirects deltaY to the nearest vertically-scrollable ancestor, and
     * leaves horizontal wheel untouched so horizontal code scroll still works.
     */
    function onWheelCapture(/** @type {WheelEvent} */ e) {
      if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return
      const target = e.target instanceof Element ? e.target : null
      const inner = target?.closest('pre, .ai-table-scroll')
      if (!inner) return
      // If the block itself can still scroll THIS way, it owns the gesture -
      // stealing it made scrollable tables and tall code blocks unusable.
      const canScrollSelf =
        inner.scrollHeight > inner.clientHeight &&
        (e.deltaY < 0 ? inner.scrollTop > 0 : inner.scrollTop + inner.clientHeight < inner.scrollHeight - 1)
      if (canScrollSelf) return
      let parent = /** @type {HTMLElement | null} */ (node.parentElement)
      while (parent) {
        if (parent.scrollHeight > parent.clientHeight) {
          const oy = getComputedStyle(parent).overflowY
          if (oy === 'auto' || oy === 'scroll' || oy === 'overlay') {
            e.preventDefault()
            // scrollBy goes through the scroll API rather than assigning
            // scrollTop, so the engine can coalesce successive wheel events into
            // one composited update instead of one layout write per event.
            parent.scrollBy({ top: e.deltaY, behavior: 'instant' })
            return
          }
        }
        parent = parent.parentElement
      }
    }

    // Syntax highlighting is deferred until the message is near the viewport, so
    // restoring a long conversation doesn't pay Shiki's cost for messages nobody
    // is looking at. No IntersectionObserver → highlight immediately.
    /** @type {IntersectionObserver | null} */
    let io = null
    if (typeof IntersectionObserver === 'function') {
      io = new IntersectionObserver(
        (entries) => {
          if (!entries.some((e) => e.isIntersecting)) return
          nearViewport = true
          io?.disconnect()
          io = null
        },
        { rootMargin: '600px 0px' },
      )
      io.observe(node)
    } else {
      nearViewport = true
    }

    // Not while streaming: the subtree is rewritten every frame, so observing it
    // would re-scan the whole growing message per frame to attach controls to
    // blocks that are still being written. The finished message gets them.
    /** @type {MutationObserver | null} */
    let obs = null
    if (!streaming) {
      obs = new MutationObserver(inject)
      obs.observe(node, { childList: true, subtree: true })
      inject()
    }
    node.addEventListener('wheel', onWheelCapture, { capture: true, passive: false })
    return {
      destroy() {
        obs?.disconnect()
        io?.disconnect()
        node.removeEventListener('wheel', onWheelCapture, true)
      },
    }
  }

  let html = $state('')
  let loading = $state(false)
  /** Set once this message reaches (or nears) the viewport - gates highlighting. */
  let nearViewport = $state(false)

  const appTheme = $derived($appThemeId)

  // Streaming: fast synchronous parse, coalesced to one per animation frame.
  // Chunks arrive faster than frames, and each parse covers the FULL message
  // so far - parsing per chunk is quadratic work for output nobody can see.
  // The rAF callback reads `content` at fire time, so it always renders the
  // latest accumulated text. No syntax highlighting - when streaming ends,
  // the item is replaced with a fully highlighted assistant item anyway.
  let _streamRaf = 0
  $effect(() => {
    if (!streaming) return
    void content // dependency: schedule on every appended chunk
    if (_streamRaf) return // a parse is already queued for this frame
    _streamRaf = requestAnimationFrame(() => {
      _streamRaf = 0
      const md = content
      if (!md.trim()) { html = ''; onrender?.(); return }
      const result = marked.parse(md, markedOpts)
      html = typeof result === 'string' ? result : ''
      onrender?.()
    })
  })
  onDestroy(() => {
    if (_streamRaf) cancelAnimationFrame(_streamRaf)
  })

  // Non-streaming: paint immediately, then upgrade to highlighted code in place.
  //
  // Parsing markdown is sub-millisecond; Shiki is the slow part. Awaiting it
  // before the first paint meant a finished response blanked out at the exact
  // moment streaming handed over, and restoring a conversation stalled on every
  // code block at once. So phase 1 paints the parsed markdown (the same markup
  // streaming was already showing) and phase 2 swaps in highlighted code once
  // this message is near the viewport.
  $effect(() => {
    if (streaming) return
    const md = content
    const theme = appTheme
    const wait = debounceMs
    const canHighlight = nearViewport
    let cancelled = false
    /** @type {ReturnType<typeof setTimeout> | undefined} */
    let timer

    async function render() {
      if (!md.trim()) {
        html = ''
        loading = false
        return
      }
      const raw = /** @type {string} */ (marked.parse(md, markedOpts))
      if (cancelled) return
      html = raw
      // Nothing to highlight, or not worth doing yet.
      if (!canHighlight || !raw.includes('<pre')) {
        loading = false
        return
      }
      loading = true
      try {
        const highlighted = await highlightMarkdownHtml(raw, theme)
        if (!cancelled) html = highlighted
      } catch {
        /* keep the un-highlighted markup already on screen */
      } finally {
        if (!cancelled) loading = false
      }
    }

    if (wait > 0) {
      timer = setTimeout(() => void render(), wait)
      return () => {
        cancelled = true
        clearTimeout(timer)
      }
    }

    void render()
    return () => {
      cancelled = true
    }
  })
</script>

<div class={cn('prose-ai', className, loading && 'prose-ai-loading')} use:copyButtons>
  {@html html}
</div>

<style>
  /* Styling for the injected copy controls. They are created outside the
     template (makeCopyBtn), so they carry plain classes rather than utilities -
     nothing scans generated DOM for Tailwind classes. */
  :global(.prose-ai pre) {
    position: relative;
  }
  :global(.ai-table-wrap) {
    position: relative;
    /* Never wider than the message column. Without the min-width:0 a wide table
       forces the flex parent open and the whole transcript scrolls sideways. */
    max-width: 100%;
    min-width: 0;
  }
  :global(.ai-block-copy) {
    position: absolute;
    top: 0.375rem;
    right: 0.375rem;
    z-index: 10;
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    height: 1.5rem;
    padding: 0 0.375rem;
    border: 1px solid color-mix(in oklch, var(--border) 80%, transparent);
    border-radius: 6px;
    background: color-mix(in oklch, var(--background) 88%, transparent);
    color: var(--muted-foreground);
    font-size: 0.6875rem;
    line-height: 1;
    backdrop-filter: blur(4px);
    opacity: 0;
    transition:
      opacity 120ms ease,
      color 120ms ease;
  }
  :global(.ai-block-copy:hover) {
    color: var(--foreground);
    background: var(--background);
  }
  :global(.ai-block-copy[data-copied]) {
    color: var(--success, #22c55e);
    opacity: 1;
  }
  /* Revealed on hover of the block itself, and always while focused so the
     control is reachable by keyboard. */
  :global(.prose-ai pre:hover .ai-block-copy),
  :global(.ai-table-wrap:hover .ai-block-copy),
  :global(.ai-block-copy:focus-visible) {
    opacity: 1;
  }
  :global(.ai-block-copy:focus-visible) {
    outline: 2px solid var(--ring);
    outline-offset: 1px;
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.ai-block-copy) {
      transition: none;
    }
  }
</style>
