<script>
  // A code block inside a scrolling page (an AI answer, a tool step). It never
  // scrolls on its own: a vertical scroll box inside the chat's scroll caught
  // the wheel halfway down the page, so reading a long answer meant fighting
  // every query card on the way. Long code is cut at `max` with a fade and a
  // "Show all" toggle instead; the page is the only thing that scrolls.
  import { cn } from '$lib/utils.js'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'

  /** @type {{ text: string, max?: number, class?: string }} */
  let { text, max = 160, class: className = '' } = $props()

  let open = $state(false)
  /** @type {HTMLPreElement | null} */
  let el = $state(null)
  /** The code's natural height; scrollHeight ignores the clamp, so this never flips back and forth. */
  let natural = $state(0)
  const lines = $derived(text.split('\n').length)
  const long = $derived(natural > max + 24)
  $effect(() => {
    void text
    if (!el) return
    const measure = () => { if (el) natural = el.scrollHeight }
    measure()
    const ro = new ResizeObserver(measure)
    ro.observe(el)
    return () => ro.disconnect()
  })
</script>

<div>
  <div class="relative">
  <pre
    bind:this={el}
    class={cn('overflow-x-auto overflow-y-hidden', className)}
    style={long && !open ? `max-height: ${max}px` : undefined}
  >{text}</pre>
  {#if long && !open}
    <div
      class="pointer-events-none absolute inset-x-0 bottom-0 h-12 rounded-b-md bg-linear-to-t from-background/90 to-transparent"
      aria-hidden="true"
    ></div>
  {/if}
  </div>
  {#if long}
    <button
      type="button"
      class="mt-1 inline-flex h-6 items-center gap-1 rounded-md px-1.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      aria-expanded={open}
      onclick={() => (open = !open)}
    >
      <ChevronDown class={cn('size-3 shrink-0 transition-transform duration-150', open && 'rotate-180')} />
      {open ? 'Show less' : `Show all ${lines} lines`}
    </button>
  {/if}
</div>
