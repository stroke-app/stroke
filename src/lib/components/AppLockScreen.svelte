<script>
  // Full-screen PIN gate, drawn over everything while the app is locked. On a
  // cold start the app is not mounted behind it at all (see AppLockGate); after
  // an idle auto-lock the session is still there, covered and key-deaf.
  import { onMount } from 'svelte'
  import { revealApp } from '$lib/app-reveal.js'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import PinField from './PinField.svelte'
  import WindowControls from './WindowControls.svelte'
  import { unlockWith, lockStatus } from '$lib/stores/app-lock.js'
  import { detectOs } from '$lib/platform.js'

  const isMac = typeof navigator !== 'undefined' && detectOs() === 'macos'

  let pin = $state('')
  let checking = $state(false)
  let wrong = $state(false)
  let showHelp = $state(false)

  let isTauri = $state(false)
  onMount(() => {
    isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
    revealApp()
  })

  /** @param {MouseEvent} e */
  function onDragRegionDblClick(e) {
    if (!isTauri) return
    if (e.target === e.currentTarget) return
    if (/** @type {Element} */ (e.target).closest('button')) return
    import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().toggleMaximize())
  }

  /** @param {string} entered */
  async function submit(entered) {
    if (checking || entered.length !== $lockStatus.pinLength) return
    checking = true
    wrong = false
    const ok = await unlockWith(entered)
    checking = false
    if (!ok) {
      wrong = true
      pin = ''
      // Long enough to read as a rejection, short enough not to feel punitive.
      setTimeout(() => (wrong = false), 900)
    }
  }

  async function quit() {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().close()
    } catch { /* browser/dev - no-op */ }
  }
</script>

<div data-app-lock class="fixed inset-0 z-[9999] flex flex-col bg-background">
  <div
    class="flex h-[52px] shrink-0 items-center border-b border-border/40 px-2.5 select-none"
    data-tauri-drag-region
    role="toolbar"
    aria-label="Window title bar"
    tabindex="-1"
    ondblclick={onDragRegionDblClick}
  >
    {#if isMac}
      <div class="w-[92px] shrink-0"></div>
    {/if}
    <Logo class="size-4 shrink-0" />
    <div class="min-w-0 flex-1" data-tauri-drag-region></div>
    <WindowControls />
  </div>

  <div class="relative flex min-h-0 flex-1 flex-col items-center justify-center overflow-y-auto px-6 py-10">
    <div
      class="pointer-events-none absolute inset-x-0 top-0 h-[55%]"
      style="background: radial-gradient(ellipse 55% 50% at 50% -8%, color-mix(in oklch, var(--primary) 9%, transparent), transparent 70%);"
    ></div>

    <div class="relative flex w-full max-w-[20rem] flex-col items-center text-center">
      <div class="grid size-11 place-items-center rounded-lg border border-border/50 bg-card/40 shadow-sm">
        <Icon name="lock" class="size-5 shrink-0 text-muted-foreground" />
      </div>

      <h1 class="mt-4 text-ui-lg font-semibold tracking-tight text-foreground">Stroke is locked</h1>
      <p class="mt-1 text-ui-xs text-muted-foreground">
        Enter your {$lockStatus.pinLength}-digit PIN to unlock
      </p>

      <div class="mt-6">
        <PinField
          bind:value={pin}
          length={$lockStatus.pinLength}
          disabled={checking}
          invalid={wrong}
          autofocus
          onsubmit={submit}
          aria-label="App PIN"
        />
      </div>
      <p class="mt-2 h-4 text-ui-2xs {wrong ? 'text-destructive' : 'text-muted-foreground'}">
        {#if checking}Checking…{:else if wrong}Wrong PIN. Try again.{/if}
      </p>

      <div class="mt-4 flex items-center gap-3 text-ui-2xs">
        <button
          type="button"
          class="text-muted-foreground underline underline-offset-2 transition-colors hover:text-foreground"
          onclick={() => (showHelp = !showHelp)}>Forgot your PIN?</button
        >
        <span class="text-border/70">·</span>
        <button
          type="button"
          class="text-muted-foreground transition-colors hover:text-foreground"
          onclick={quit}>Quit</button
        >
      </div>

      {#if showHelp}
        <p
          class="mt-4 rounded-lg border border-border/30 bg-muted/20 px-3.5 py-2.5 text-left text-ui-2xs leading-relaxed text-muted-foreground"
        >
          There is no recovery code, by design: the PIN is only ever stored as a
          salted hash. To get back in, delete the
          <span class="font-mono text-foreground/80">secrets-vault</span> entry for
          <span class="font-mono text-foreground/80">app.stroke.desktop</span> from your
          OS keychain. That clears the PIN and every saved credential with it.
        </p>
      {/if}
    </div>
  </div>
</div>
