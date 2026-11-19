<script>
  import { onMount } from 'svelte'
  import Minus  from '@lucide/svelte/icons/minus'
  import Square from '@lucide/svelte/icons/square'
  import Copy   from '@lucide/svelte/icons/copy'
  import X      from '@lucide/svelte/icons/x'
  import { cn } from '$lib/utils.js'
  import { detectOs } from '$lib/platform.js'

  // Windows/Linux only - macOS keeps the real OS-drawn traffic lights
  // (see src-tauri/src/lib.rs), so there's nothing for this component to draw there.
  const isMac = typeof navigator !== 'undefined' && detectOs() === 'macos'

  let isTauri = $state(false)
  let isMaximized = $state(false)

  onMount(() => {
    isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
    if (!isTauri || isMac) return
    let unlisten = () => {}
    ;(async () => {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      const win = getCurrentWindow()
      isMaximized = await win.isMaximized()
      unlisten = await win.onResized(async () => { isMaximized = await win.isMaximized() })
    })()
    return () => unlisten()
  })

  // Every one of these is an ACL-gated core command, and the window is frameless,
  // so these buttons are the only chrome there is - a denied call leaves the user
  // with a window they cannot move, resize or close. That is how the missing
  // `core:window:allow-destroy` shipped in 2.1.0: `close()` runs through to
  // `destroy`, the ACL refused it, and the rejection went nowhere. Await and log
  // so the next gap in src-tauri/capabilities/default.json is visible.
  /** @param {string} name @param {(w: import('@tauri-apps/api/window').Window) => Promise<unknown>} fn */
  async function windowAction(name, fn) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await fn(getCurrentWindow())
    } catch (err) {
      console.error(`window ${name} failed:`, err)
    }
  }

  const minimizeWindow = () => windowAction('minimize', (w) => w.minimize())
  const toggleMaximizeWindow = () => windowAction('toggleMaximize', (w) => w.toggleMaximize())
  const closeWindow = () => windowAction('close', (w) => w.close())

  // Windows' own caption buttons: 46px wide, the bar's full height, square, a
  // light fill on hover and red behind close. The small rounded icon buttons
  // read as app toolbar buttons, not as the window's.
  const winBtn = 'inline-flex h-full w-[46px] items-center justify-center text-foreground/75 transition-colors duration-100 hover:bg-foreground/[0.08] hover:text-foreground active:bg-foreground/[0.14]'
</script>

{#if isTauri && !isMac}
  <div class="flex shrink-0 self-stretch">
    <button type="button" class={winBtn} onclick={minimizeWindow} title="Minimize" aria-label="Minimize">
      <Minus class="size-4 shrink-0" strokeWidth={1.25} />
    </button>
    <button type="button" class={winBtn} onclick={toggleMaximizeWindow} title={isMaximized ? 'Restore' : 'Maximize'} aria-label={isMaximized ? 'Restore' : 'Maximize'}>
      {#if isMaximized}
        <Copy class="size-3.5 shrink-0" strokeWidth={1.25} />
      {:else}
        <Square class="size-3 shrink-0" strokeWidth={1.25} />
      {/if}
    </button>
    <button
      type="button"
      class={cn(winBtn, 'hover:bg-[#c42b1c] hover:text-white active:bg-[#c42b1c]/85')}
      onclick={closeWindow}
      title="Close"
      aria-label="Close"
    >
      <X class="size-4 shrink-0" strokeWidth={1.25} />
    </button>
  </div>
{/if}
