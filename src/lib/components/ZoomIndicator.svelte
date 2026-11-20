<script>
  /**
   * The zoom level, shown for a moment after it changes (Ctrl/Cmd + = - 0, the
   * View menu, Settings). The whole interface scales at once and nothing said
   * by how much, or how to get back. A pill at the top centre names the level
   * and offers Reset, then fades. Silent at startup: only a change shows it.
   */
  import { fade } from 'svelte/transition'
  import { appZoom, resetZoom } from '$lib/stores/settings.js'

  let shown = $state(false)
  let level = $state(1)
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let timer
  let first = true
  /** Held open while the pointer is on it, so Reset can be reached. */
  let hovered = false

  const HOLD_MS = 1400

  function hideLater() {
    clearTimeout(timer)
    timer = setTimeout(() => { if (!hovered) shown = false }, HOLD_MS)
  }

  $effect(() => {
    const z = $appZoom
    if (first) {
      first = false
      level = z
      return
    }
    if (z === level && shown) return
    level = z
    shown = true
    hideLater()
  })

  $effect(() => () => clearTimeout(timer))
</script>

{#if shown}
  <div
    class="chrome-font pointer-events-auto fixed top-[calc(48px*var(--app-scale))] left-1/2 z-[70] flex h-8 -translate-x-1/2 items-center gap-1 rounded-lg border border-border/70 bg-popover ps-3 pe-1 text-ui-xs text-popover-foreground elevate-2"
    role="status"
    aria-live="polite"
    in:fade={{ duration: 120 }}
    out:fade={{ duration: 100 }}
    onpointerenter={() => { hovered = true; clearTimeout(timer) }}
    onpointerleave={() => { hovered = false; hideLater() }}
  >
    <span class="min-w-10 font-medium tabular-nums">{Math.round(level * 100)}%</span>
    {#if level !== 1}
      <button
        type="button"
        class="h-6 rounded-md px-2 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
        onclick={() => { resetZoom(); }}
      >
        Reset
      </button>
    {/if}
  </div>
{/if}
