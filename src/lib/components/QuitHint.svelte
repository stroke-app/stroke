<script>
  /**
   * macOS: the first ⌘Q only shows this. A second press, or holding the keys,
   * quits (the timing lives in quit.rs). Settings > Appearance turns it off.
   */
  import Kbd from './Kbd.svelte'

  let shown = $state(false)
  // Mounted on the first hint only, so the idle app carries no extra layer.
  let ready = $state(false)
  /** @type {ReturnType<typeof setTimeout> | null} */
  let timer = null
  /** How long the second press counts, for the draining bar. */
  let windowMs = $state(1600)
  /** Bumped per hint so the bar restarts from full. */
  let run = $state(0)

  $effect(() => {
    let off = () => {}
    let gone = false
    void import('@tauri-apps/api/event')
      .then(({ listen }) => listen('app-quit-hint', (e) => {
        ready = true
        windowMs = Number(e.payload) || 1600
        run++
        requestAnimationFrame(() => { shown = true })
        if (timer) clearTimeout(timer)
        timer = setTimeout(() => { shown = false }, windowMs)
      }))
      .then((un) => { if (gone) un(); else off = un })
      .catch(() => {})
    return () => { gone = true; off(); if (timer) clearTimeout(timer) }
  })
</script>

{#if ready}
<div
  role="status"
  aria-live="polite"
  class="pointer-events-none fixed inset-x-0 top-[22vh] z-[100] flex justify-center"
>
  <div
    class="relative flex flex-col items-center gap-1 overflow-hidden rounded-lg border border-border/60 bg-popover/95 px-5 pt-3 pb-3.5 text-popover-foreground shadow-2xl backdrop-blur-xl transition-[opacity,scale,translate] duration-150 ease-out"
    style="opacity:{shown ? 1 : 0};scale:{shown ? 1 : 0.96};translate:0 {shown ? 0 : 4}px"
  >
    <p class="flex items-center gap-2 text-ui-sm font-medium">
      Press <Kbd combo="Mod+Q" size="sm" /> again to quit
    </p>
    <p class="text-ui-xs text-muted-foreground">or keep holding it</p>
    {#key run}
      <span
        class="quit-hint-bar absolute inset-x-0 bottom-0 h-0.5 origin-left bg-foreground/40"
        style="animation-duration: {windowMs}ms"
        aria-hidden="true"
      ></span>
    {/key}
  </div>
</div>
{/if}

<style>
  .quit-hint-bar {
    animation-name: quit-hint-drain;
    animation-timing-function: linear;
    animation-fill-mode: forwards;
  }
  @keyframes quit-hint-drain {
    from { transform: scaleX(1); }
    to { transform: scaleX(0); }
  }
  @media (prefers-reduced-motion: reduce) {
    .quit-hint-bar { animation: none; }
  }
</style>
