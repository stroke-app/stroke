<script>
  import { onMount } from 'svelte'
  import { cn } from '$lib/utils.js'
  import { detectOs } from '$lib/platform.js'

  // Windows/Linux only - macOS keeps the real OS-drawn traffic lights
  // (see src-tauri/src/lib.rs), so there's nothing for this component to draw there.
  const os = typeof navigator !== 'undefined' ? detectOs() : 'linux'
  const isMac = os === 'macos'
  const isLinux = os === 'linux'

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
  // with a window they cannot move, resize or close.
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

  // Windows 11 caption buttons: 46px wide, full titlebar height, square, crisp 10px
  // geometric Fluent glyphs, light hover fill and Windows red behind Close.
  const winBtn =
    'inline-flex h-full w-[46px] items-center justify-center text-foreground/80 hover:bg-foreground/[0.08] hover:text-foreground active:bg-foreground/[0.14]'

  // Linux (GNOME / GTK / Adwaita style): circular subtle controls inset cleanly
  // from the window corner instead of Windows' sharp rectangular caption strip.
  const linuxBtn =
    'inline-flex size-6 items-center justify-center rounded-full bg-foreground/[0.05] text-foreground/75 hover:bg-foreground/[0.12] hover:text-foreground active:bg-foreground/[0.18]'
</script>

{#if isTauri && !isMac}
  {#if isLinux}
    <div class="flex shrink-0 items-center gap-1.5 px-2">
      <button type="button" class={linuxBtn} onclick={minimizeWindow} title="Minimize" aria-label="Minimize">
        <svg class="size-2.5 shrink-0" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.15" aria-hidden="true">
          <path d="M1.5 5.5h7" stroke-linecap="round" />
        </svg>
      </button>
      <button type="button" class={linuxBtn} onclick={toggleMaximizeWindow} title={isMaximized ? 'Restore' : 'Maximize'} aria-label={isMaximized ? 'Restore' : 'Maximize'}>
        {#if isMaximized}
          <svg class="size-2.5 shrink-0" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.1" aria-hidden="true">
            <rect x="2" y="3.5" width="4.5" height="4.5" rx="0.5" />
            <path d="M3.5 3.5V2.25a.5.5 0 0 1 .5-.5h3.75a.5.5 0 0 1 .5.5V6a.5.5 0 0 1-.5.5H6.5" />
          </svg>
        {:else}
          <svg class="size-2.5 shrink-0" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.15" aria-hidden="true">
            <rect x="2" y="2" width="6" height="6" rx="0.75" />
          </svg>
        {/if}
      </button>
      <button
        type="button"
        class={cn(linuxBtn, 'hover:bg-destructive/85 hover:text-white active:bg-destructive')}
        onclick={closeWindow}
        title="Close"
        aria-label="Close"
      >
        <svg class="size-2.5 shrink-0" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.2" aria-hidden="true">
          <path d="M2.25 2.25l5.5 5.5M7.75 2.25l-5.5 5.5" stroke-linecap="round" />
        </svg>
      </button>
    </div>
  {:else}
    <div class="flex shrink-0 self-stretch">
      <button type="button" class={winBtn} onclick={minimizeWindow} title="Minimize" aria-label="Minimize">
        <svg class="size-2.5 shrink-0" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1" aria-hidden="true">
          <path d="M0 5.5h10" />
        </svg>
      </button>
      <button type="button" class={winBtn} onclick={toggleMaximizeWindow} title={isMaximized ? 'Restore' : 'Maximize'} aria-label={isMaximized ? 'Restore' : 'Maximize'}>
        {#if isMaximized}
          <svg class="size-2.5 shrink-0" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1" aria-hidden="true">
            <rect x="0.5" y="2.5" width="7" height="7" />
            <path d="M2.5 2.5V0.5h7v7H7.5" />
          </svg>
        {:else}
          <svg class="size-2.5 shrink-0" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1" aria-hidden="true">
            <rect x="0.5" y="0.5" width="9" height="9" />
          </svg>
        {/if}
      </button>
      <button
        type="button"
        class={cn(winBtn, 'hover:bg-[#c42b1c] hover:text-white active:bg-[#c42b1c]/85')}
        onclick={closeWindow}
        title="Close"
        aria-label="Close"
      >
        <svg class="size-2.5 shrink-0" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.05" aria-hidden="true">
          <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" />
        </svg>
      </button>
    </div>
  {/if}
{/if}
