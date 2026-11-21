<script>
  /**
   * The zoom level, shown for two seconds after you change it (Ctrl/Cmd + = - 0,
   * the View menu, Settings, Ctrl+wheel). It listens for zoom gestures, not for
   * the zoom value: the value is re-applied by every settings change and every
   * sync, and following it made the pill appear at launch and flicker at random.
   * Centred over the window; the level is centred in the pill; Reset shows
   * beside it when you are off 100%. Hovering keeps it open.
   */
  import { fade } from 'svelte/transition'
  import { loadSettings, resetZoom, zoomGesture } from '$lib/stores/settings.js'

  const HOLD_MS = 2000

  let shown = $state(false)
  let level = $state(1)
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let timer
  let hovered = false
  /** The count at mount: only gestures after it show the pill. */
  let seen = -1

  function hideLater() {
    clearTimeout(timer)
    timer = setTimeout(() => { if (!hovered) shown = false }, HOLD_MS)
  }

  $effect(() => {
    const n = $zoomGesture
    if (seen === -1) { seen = n; return }
    if (n === seen) return
    seen = n
    level = loadSettings().zoom
    shown = true
    hideLater()
  })

  $effect(() => () => clearTimeout(timer))
</script>

{#if shown}
  <div
    class="chrome-font pointer-events-auto fixed top-[calc(48px*var(--app-scale))] left-1/2 z-[70] flex h-8 -translate-x-1/2 items-center justify-center gap-1 rounded-lg border border-border/70 bg-popover px-1 text-ui-xs text-popover-foreground elevate-2"
    role="status"
    aria-live="polite"
    in:fade={{ duration: 120 }}
    out:fade={{ duration: 160 }}
    onpointerenter={() => { hovered = true; clearTimeout(timer) }}
    onpointerleave={() => { hovered = false; hideLater() }}
  >
    <span class="min-w-12 px-2 text-center font-medium tabular-nums">{Math.round(level * 100)}%</span>
    {#if level !== 1}
      <button
        type="button"
        class="h-6 rounded-md px-2 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
        onclick={() => resetZoom()}
      >
        Reset
      </button>
    {/if}
  </div>
{/if}
