<script>
  /**
   * The zoom level, shown for two seconds after you change it (Ctrl/Cmd + = - 0,
   * the View menu, Settings, Ctrl+wheel). It listens for zoom gestures, not for
   * the zoom value: the value is re-applied by every settings change and every
   * sync, and following it made the pill appear at launch and flicker at random.
   * Centred just under the title bar; the level is centred in the pill; Reset
   * shows beside it when you are off 100%. Hovering keeps it open.
   *
   * Hover is read from the pill itself (`:hover`) when the timer fires, never
   * from a flag set on pointerenter. A flag only cleared on pointerleave stuck at
   * true whenever that event never came (Reset unmounting under the pointer, the
   * window losing focus, the pill opening under a still pointer), and the pill
   * then stayed on screen for good. Reset, Escape, a click elsewhere and the
   * window losing focus all close it.
   */
  import { fade } from 'svelte/transition'
  import { get } from 'svelte/store'
  import { loadSettings, resetZoom, zoomGesture } from '$lib/stores/settings.js'

  const HOLD_MS = 2000

  let shown = $state(false)
  let level = $state(1)
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let timer
  /** @type {HTMLElement | undefined} */
  let pill = $state()
  /** The count at mount: only gestures after it show the pill. */
  let seen = -1

  function hide() {
    clearTimeout(timer)
    shown = false
  }

  /** Close after HOLD_MS, or later while the pointer is still on the pill. */
  function hideLater() {
    clearTimeout(timer)
    timer = setTimeout(() => {
      if (pill?.matches(':hover')) hideLater()
      else shown = false
    }, HOLD_MS)
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

  // resetZoom counts as a gesture; mark it seen so the pill closes instead of
  // reopening at 100%.
  function reset() {
    resetZoom()
    seen = get(zoomGesture)
    hide()
  }

  /** @param {KeyboardEvent} e */
  function onKey(e) {
    if (shown && e.key === 'Escape') hide()
  }

  /** @param {PointerEvent} e */
  function onPointerDown(e) {
    if (shown && !(e.target instanceof Node && pill?.contains(e.target))) hide()
  }

  $effect(() => () => clearTimeout(timer))
</script>

<svelte:window onkeydown={onKey} onpointerdowncapture={onPointerDown} onblur={() => { if (shown) hide() }} />

{#if shown}
  <div
    bind:this={pill}
    class="chrome-font pointer-events-auto fixed top-[calc(var(--app-titlebar-h,38px)_+_8px)] left-1/2 z-[70] flex h-8 -translate-x-1/2 items-center justify-center gap-1 rounded-lg border border-border/70 bg-popover px-1 text-ui-xs text-popover-foreground elevate-2"
    role="status"
    aria-live="polite"
    in:fade={{ duration: 120 }}
    out:fade={{ duration: 160 }}
    onpointerleave={hideLater}
  >
    <span class="min-w-12 px-2 text-center font-medium tabular-nums">{Math.round(level * 100)}%</span>
    {#if level !== 1}
      <button
        type="button"
        class="h-6 rounded-md px-2 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
        onclick={reset}
      >
        Reset
      </button>
    {/if}
  </div>
{/if}
