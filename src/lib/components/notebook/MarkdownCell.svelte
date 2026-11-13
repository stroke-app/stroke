<script>
  import { marked } from 'marked'
  import { tick } from 'svelte'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import { cn } from '$lib/utils.js'

  /**
   * @type {{
   *   cellIndex: number,
   *   content: string,
   *   onchange: (v: string) => void,
   *   onmoveup?: () => void,
   *   onmovedown?: () => void,
   *   onremove?: () => void,
   *   canmoveup?: boolean,
   *   canmovedown?: boolean,
   *   canremove?: boolean,
   * }}
   */
  let {
    cellIndex,
    content,
    onchange,
    onmoveup,
    onmovedown,
    onremove,
    canmoveup = true,
    canmovedown = true,
    canremove = true,
  } = $props()

  let editing = $state(false)
  // svelte-ignore state_referenced_locally
  let draft = $state(content)
  /** @type {HTMLTextAreaElement | null} */
  let textareaEl = $state(null)
  /** @type {HTMLDivElement | null} */
  let cellEl = $state(null)

  $effect(() => {
    if (!editing) draft = content
  })

  /** Live preview (uses draft while editing for instant feedback) */
  const previewHtml = $derived.by(() => {
    const text = editing ? draft : content
    if (!text.trim()) return ''
    try {
      return /** @type {string} */ (marked.parse(text, { breaks: true, gfm: true }))
    } catch {
      return ''
    }
  })

  async function startEdit() {
    if (editing) return
    draft = content
    editing = true
    await tick()
    textareaEl?.focus()
    autoResize()
  }

  function commit() {
    onchange(draft)
    editing = false
  }

  /** @param {KeyboardEvent} e */
  function onKeydown(e) {
    if (e.key === 'Escape') { e.preventDefault(); commit() }
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); commit() }
  }

  function onInput() {
    onchange(draft)
    autoResize()
  }

  function autoResize() {
    if (!textareaEl) return
    textareaEl.style.height = 'auto'
    textareaEl.style.height = `${Math.max(textareaEl.scrollHeight, 140)}px`
  }

  /** Blur fires before the new element gets focus - use a tick to check if still inside cell */
  function onBlur() {
    setTimeout(() => {
      if (!cellEl?.contains(document.activeElement)) commit()
    }, 80)
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
<div
  bind:this={cellEl}
  class={cn(
    'group/md relative',
    editing && 'bg-muted/[0.04]',
  )}
>
  {#if editing}
    <!-- ── Split view: source left, live preview right ── -->
    <div class="flex min-h-[140px] border-b border-border/20">
      <!-- Source pane -->
      <div class="relative flex w-1/2 min-w-0 flex-col border-r border-border/25">
        <div
          class="flex h-7 shrink-0 items-center gap-2 border-b border-border/20 px-6"
        >
          <span
            class="rounded px-1.5 py-px text-ui-3xs font-semibold uppercase tracking-widest text-success"
          >
            md source
          </span>
          <span class="text-ui-3xs text-muted-foreground">Esc or ⌘↵ to preview</span>
        </div>
        <textarea
          bind:this={textareaEl}
          bind:value={draft}
          oninput={onInput}
          onkeydown={onKeydown}
          onblur={onBlur}
          class="flex-1 resize-none bg-transparent px-6 py-4 font-mono text-ui-sm leading-relaxed text-muted-foreground outline-none placeholder:text-muted-foreground"
          placeholder="Write markdown here…"
          spellcheck="false"
        ></textarea>
      </div>

      <!-- Live preview pane -->
      <div class="flex w-1/2 min-w-0 flex-col">
        <div
          class="flex h-7 shrink-0 items-center gap-2 border-b border-border/20 px-6"
        >
          <span
            class="text-ui-3xs font-semibold uppercase tracking-widest text-muted-foreground"
          >preview</span>
        </div>
        <div class="flex-1 overflow-auto px-6 py-4">
          {#if previewHtml}
            <div class="nb-prose">
              {@html previewHtml}
            </div>
          {:else}
            <span class="text-ui-sm italic text-muted-foreground">Preview will appear here…</span>
          {/if}
        </div>
      </div>
    </div>

    <!-- Bottom bar: actions -->
    <div
      class="flex h-7 items-center gap-1 px-4"
    >
      <div class="ml-auto flex items-center gap-1">
        <button
          onclick={onmoveup}
          disabled={!canmoveup}
          class="rounded px-1 py-0.5 text-ui-xs text-muted-foreground hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-20"
          title="Move up"
        >↑</button>
        <button
          onclick={onmovedown}
          disabled={!canmovedown}
          class="rounded px-1 py-0.5 text-ui-xs text-muted-foreground hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-20"
          title="Move down"
        >↓</button>
        <button
          onclick={onremove}
          disabled={!canremove}
          class="rounded p-0.5 text-muted-foreground hover:bg-destructive/10 hover:text-destructive disabled:pointer-events-none disabled:opacity-20"
          title="Delete cell"
        >
          <Trash2 class="size-3 shrink-0" />
        </button>
      </div>
    </div>
  {:else}
    <!-- ── Preview mode ── -->
    <div
      class="relative cursor-text px-8 py-5"
      onclick={startEdit}
      role="button"
      tabindex="0"
      onkeydown={(e) => e.key === 'Enter' && startEdit()}
      aria-label="Click to edit markdown"
    >
      {#if previewHtml}
        <div class="nb-prose">
          {@html previewHtml}
        </div>
      {:else}
        <span class="text-ui-sm italic text-muted-foreground">Click to add markdown…</span>
      {/if}

      <!-- Hover actions -->
      <div
        class="absolute right-4 top-2 flex items-center gap-1 opacity-0 transition-opacity group-hover/md:opacity-100"
      >
        <button
          onclick={(e) => { e.stopPropagation(); onmoveup?.() }}
          disabled={!canmoveup}
          class="rounded px-1 py-0.5 text-ui-xs text-muted-foreground hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-20"
          title="Move up"
        >↑</button>
        <button
          onclick={(e) => { e.stopPropagation(); onmovedown?.() }}
          disabled={!canmovedown}
          class="rounded px-1 py-0.5 text-ui-xs text-muted-foreground hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-20"
          title="Move down"
        >↓</button>
        <button
          onclick={(e) => { e.stopPropagation(); onremove?.() }}
          disabled={!canremove}
          class="rounded p-0.5 text-muted-foreground hover:bg-destructive/10 hover:text-destructive disabled:pointer-events-none disabled:opacity-20"
          title="Delete cell"
        >
          <Trash2 class="size-3 shrink-0" />
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  /* Notebook prose - markdown rendering styles */
  :global(.nb-prose) {
    font-size: 0.9rem;
    line-height: 1.7;
    color: var(--foreground);
    word-break: break-word;
  }
  :global(.nb-prose > *:first-child) { margin-top: 0; }
  :global(.nb-prose > *:last-child) { margin-bottom: 0; }
  :global(.nb-prose p) { margin: 0.5rem 0; }
  :global(.nb-prose strong) { font-weight: 650; }
  :global(.nb-prose em) { font-style: italic; opacity: 0.85; }
  :global(.nb-prose h1) {
    font-size: 1.5rem; font-weight: 700; line-height: 1.3;
    margin: 1.2rem 0 0.4rem; color: var(--foreground);
    border-bottom: 1px solid color-mix(in oklch, var(--border) 60%, transparent);
    padding-bottom: 0.35rem;
  }
  :global(.nb-prose h2) {
    font-size: 1.2rem; font-weight: 650; line-height: 1.35;
    margin: 1rem 0 0.35rem; color: var(--foreground);
  }
  :global(.nb-prose h3) {
    font-size: 1.05rem; font-weight: 600; line-height: 1.4;
    margin: 0.8rem 0 0.3rem; color: var(--foreground);
  }
  :global(.nb-prose h4, .nb-prose h5, .nb-prose h6) {
    font-size: 0.95rem; font-weight: 600;
    margin: 0.7rem 0 0.25rem; color: var(--foreground);
  }
  :global(.nb-prose ul) {
    padding-left: 1.4rem; list-style-type: disc; margin: 0.4rem 0;
  }
  :global(.nb-prose ol) {
    padding-left: 1.4rem; list-style-type: decimal; margin: 0.4rem 0;
  }
  :global(.nb-prose li) { margin: 0.2rem 0; }
  :global(.nb-prose blockquote) {
    border-left: 3px solid color-mix(in oklch, var(--border) 80%, var(--foreground) 10%);
    margin: 0.6rem 0;
    padding: 0.1rem 0 0.1rem 1rem;
    color: color-mix(in oklch, var(--foreground) 70%, transparent);
    font-style: italic;
  }
  :global(.nb-prose code) {
    font-family: "Geist Mono Variable", ui-monospace, monospace;
    font-size: 0.8em;
    background: color-mix(in oklch, var(--muted) 90%, var(--foreground) 5%);
    border: 1px solid color-mix(in oklch, var(--border) 70%, transparent);
    border-radius: 4px;
    padding: 0.15em 0.4em;
    white-space: nowrap;
  }
  :global(.nb-prose pre) {
    background: color-mix(in oklch, var(--muted) 80%, transparent);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.85rem 1rem;
    overflow-x: auto;
    margin: 0.6rem 0;
  }
  :global(.nb-prose pre code) {
    background: none; border: none; padding: 0;
    font-size: 0.83rem; line-height: 1.6; white-space: pre;
  }
  :global(.nb-prose hr) {
    border: none;
    border-top: 1px solid color-mix(in oklch, var(--border) 70%, transparent);
    margin: 1.2rem 0;
  }
  :global(.nb-prose table) {
    border-collapse: collapse; width: 100%; margin: 0.6rem 0;
    font-size: 0.85rem;
  }
  :global(.nb-prose th) {
    border: 1px solid var(--border);
    padding: 0.4rem 0.75rem;
    background: color-mix(in oklch, var(--muted) 60%, transparent);
    font-weight: 600; text-align: left;
  }
  :global(.nb-prose td) {
    border: 1px solid color-mix(in oklch, var(--border) 60%, transparent);
    padding: 0.35rem 0.75rem;
  }
  :global(.nb-prose a) {
    color: color-mix(in oklch, var(--primary) 80%, var(--foreground) 10%);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  :global(.nb-prose img) { max-width: 100%; height: auto; border-radius: 6px; }
</style>
