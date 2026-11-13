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

  const winBtn = 'inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors duration-150 hover:bg-foreground/[0.08] hover:text-foreground'
</script>

{#if isTauri && !isMac}
  <div class="flex shrink-0 items-center gap-2.5">
    <button type="button" class={winBtn} onclick={minimizeWindow} title="Minimize">
      <Minus class="size-3.5 shrink-0" />
    </button>
    <button type="button" class={winBtn} onclick={toggleMaximizeWindow} title={isMaximized ? 'Restore' : 'Maximize'}>
      {#if isMaximized}
        <Copy class="size-3 shrink-0" />
      {:else}
        <Square class="size-3 shrink-0" />
      {/if}
    </button>
    <button
      type="button"
      class={cn(winBtn, 'hover:bg-[#e81123] hover:text-white')}
      onclick={closeWindow}
      title="Close"
    >
      <X class="size-3.5 shrink-0" />
    </button>
  </div>
{/if}
