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

  $effect(() => {
    let off = () => {}
    let gone = false
    void import('@tauri-apps/api/event')
      .then(({ listen }) => listen('app-quit-hint', (e) => {
        ready = true
        requestAnimationFrame(() => { shown = true })
        if (timer) clearTimeout(timer)
        timer = setTimeout(() => { shown = false }, Number(e.payload) || 1600)
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
    class="flex items-center gap-2.5 rounded-full border border-border/60 bg-popover/95 py-2.5 ps-5 pe-4 text-ui-lg font-medium text-popover-foreground shadow-2xl backdrop-blur-xl transition-[opacity,scale,translate] duration-150 ease-out"
    style="opacity:{shown ? 1 : 0};scale:{shown ? 1 : 0.96};translate:0 {shown ? 0 : 4}px"
  >
    <span>Hold</span>
    <Kbd combo="Mod+Q" size="md" />
    <span>or press twice to quit</span>
  </div>
</div>
{/if}
