<script>
  import FileText from '@lucide/svelte/icons/file-text'
  import ImageOff from '@lucide/svelte/icons/image-off'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import Maximize2 from '@lucide/svelte/icons/maximize-2'

  let {
    url,
    /** @type {'image' | 'pdf' | 'link'} */
    type,
    /** @type {DOMRect} */
    anchorRect,
    onmouseenter = () => {},
    onmouseleave = () => {},
    onopen = () => {},
    onexpand = () => {},
  } = $props()

  let imgLoaded = $state(false)
  let imgError = $state(false)

  $effect(() => {
    url
    imgLoaded = false
    imgError = false
  })

  const style = $derived.by(() => {
    const GAP = 8
    const MAX_W = 268
    const vw = typeof window !== 'undefined' ? window.innerWidth : 1200
    const vh = typeof window !== 'undefined' ? window.innerHeight : 800

    // Center horizontally over anchor, clamped to viewport edges
    let left = anchorRect.left + anchorRect.width / 2
    left = Math.max(MAX_W / 2 + GAP, Math.min(vw - MAX_W / 2 - GAP, left))

    // Prefer showing above, fall back to below if not enough room
    const APPROX_H = type === 'image' ? 260 : 100
    const showAbove = anchorRect.top - GAP > APPROX_H
    const top = showAbove
      ? anchorRect.top - GAP
      : anchorRect.bottom + GAP

    const transform = showAbove
      ? 'translateX(-50%) translateY(-100%)'
      : 'translateX(-50%)'

    return `left:${left}px;top:${top}px;transform:${transform}`
  })

  const filename = $derived(url.split('/').pop()?.split('?')[0] ?? url)
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed z-[9999] max-w-[268px] overflow-hidden rounded-[10px] border border-border/60 bg-popover elevate-2-rim"
  style={style}
  onmouseenter={onmouseenter}
  onmouseleave={onmouseleave}
>
  {#if type === 'image'}
    <!-- Image area -->
    <div class="relative min-h-[80px] bg-muted/20">
      {#if !imgLoaded && !imgError}
        <div class="flex h-[120px] w-[200px] items-center justify-center">
          <span class="inline-flex gap-1">
            <span class="size-1.5 animate-bounce rounded-full bg-muted-foreground/40" style="animation-delay:0ms"></span>
            <span class="size-1.5 animate-bounce rounded-full bg-muted-foreground/40" style="animation-delay:120ms"></span>
            <span class="size-1.5 animate-bounce rounded-full bg-muted-foreground/40" style="animation-delay:240ms"></span>
          </span>
        </div>
      {/if}
      {#if imgError}
        <div class="flex h-[80px] w-[200px] flex-col items-center justify-center gap-1.5 text-muted-foreground">
          <ImageOff class="size-5 shrink-0 opacity-40" />
          <span class="text-ui-2xs">Could not load image</span>
        </div>
      {:else}
        <img
          src={url}
          alt=""
          class="block max-h-[200px] max-w-[268px] object-contain"
          class:hidden={!imgLoaded}
          loading="eager"
          onload={() => { imgLoaded = true }}
          onerror={() => { imgError = true }}
        />
      {/if}
    </div>

  {:else if type === 'pdf'}
    <div class="flex items-center gap-2.5 px-3 py-3">
      <FileText class="size-6 shrink-0 text-destructive" />
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">PDF Document</p>
        <p class="truncate text-ui-xs text-muted-foreground">{filename}</p>
      </div>
    </div>
  {/if}

  <!-- Footer: URL + action buttons -->
  <div class="flex items-center gap-1 border-t border-border bg-muted/20 px-2 py-1.5">
    <p class="min-w-0 flex-1 truncate font-mono text-ui-2xs text-muted-foreground">{url}</p>
    {#if type === 'image' || type === 'pdf'}
      <button
        type="button"
        class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        title="Open full screen"
        onclick={(e) => { e.stopPropagation(); onexpand() }}
      >
        <Maximize2 class="size-3.5 shrink-0" />
      </button>
    {/if}
    <button
      type="button"
      class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      title="Open in browser"
      onclick={(e) => { e.stopPropagation(); onopen() }}
    >
      <ExternalLink class="size-3.5 shrink-0" />
    </button>
  </div>
</div>
