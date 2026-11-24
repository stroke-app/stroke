<script>
  // Hard trial gate. Rendered in place of the whole app once the trial has
  // expired and the account has no Pro - there is no dismiss path. The
  // way through is an account that has Stroke Pro: a purchase on stroke.click
  // shows up there, under the email it was bought with.
  import { onMount } from 'svelte'
  import { revealApp } from '$lib/app-reveal.js'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import AccountSignInButtons from './AccountSignInButtons.svelte'
  import WindowControls from './WindowControls.svelte'
  import GateUpdateCheck from './GateUpdateCheck.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { licenseStatus, isBlocked } from '$lib/stores/license.js'
  import { account, isSignedIn, signInState, refreshAccount, signOut } from '$lib/stores/account.js'
  import { detectOs } from '$lib/platform.js'
  import { cn } from '$lib/utils.js'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import ShieldCheck from '@lucide/svelte/icons/shield-check'

  /** Called once activation has finished celebrating; LicenseGate drops the gate on it. */
  let { onactivated = () => {} } = $props()

  /** Pro was found on the account: the copy turns into a welcome. */
  let activated = $state(false)
  /** The celebration is over: fade out, then let the gate hand over to the app. */
  let leaving = $state(false)
  function finish() {
    leaving = true
    setTimeout(onactivated, 220)
  }

  // Follow licenseStatus, which comes from the account's entitlement: once it
  // stops being TrialExpired (sign-in, a later refresh after buying, a focus
  // refresh), hand over to the app.
  $effect(() => {
    if (activated) return
    if ($licenseStatus && !$isBlocked) {
      activated = true
      setTimeout(finish, 1200)
    }
  })

  const email = $derived($account?.user?.email ?? null)
  const waiting = $derived($signInState.phase === 'waiting')

  let checking = $state(false)
  async function checkAgain() {
    checking = true
    try { await refreshAccount({ network: true }) } finally { checking = false }
  }

  async function openStore() {
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl('https://stroke.click')
    } catch {
      window.open('https://stroke.click', '_blank', 'noopener,noreferrer')
    }
  }

  const isMac = typeof navigator !== 'undefined' && detectOs() === 'macos'

  // The window is frameless (see src-tauri/src/lib.rs) and this screen replaces
  // the whole app - including TitleBar.svelte - so without its own bare drag
  // region here, a user on this screen would have no way to move the window to
  // another monitor or minimize it.
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

  async function quit() {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().close()
    } catch { /* browser/dev - no-op */ }
  }
</script>

<div class={cn('fixed inset-0 z-[9999] flex flex-col bg-background transition-opacity duration-200 ease-out', leaving && 'opacity-0')}>
  <!-- Bare title bar: just the drag region, the mark, and window controls -->
  <div
    class="flex h-(--app-titlebar-h) shrink-0 items-center border-b border-border/50 select-none {isMac ? 'px-2.5' : 'pl-2.5'}"
    data-tauri-drag-region
    role="toolbar"
    aria-label="Window title bar"
    tabindex="-1"
    ondblclick={onDragRegionDblClick}
  >
    {#if isMac}
      <div class="w-[78px] shrink-0"></div>
    {/if}
    <Logo class="size-4 shrink-0" />
    <div class="min-w-0 flex-1" data-tauri-drag-region></div>
    <WindowControls />
  </div>

  <div class="relative flex min-h-0 flex-1 flex-col items-center justify-center overflow-y-auto px-4 py-10">
    <!-- Ambient glow -->
    <div
      class="pointer-events-none absolute inset-x-0 top-0 h-[55%]"
      style="background: radial-gradient(ellipse 55% 50% at 50% -8%, color-mix(in oklch, var(--primary) 9%, transparent), transparent 70%);"
    ></div>

    <div class="relative flex w-full max-w-[22rem] flex-col items-center text-center">
      <div class="grid size-11 place-items-center rounded-lg border border-border/60 bg-card">
        <Logo class="size-5" />
      </div>

      <div class="mt-5 flex flex-col gap-2">
        {#if activated}
          <h1 class="text-ui-xl font-semibold tracking-tight text-foreground text-balance animate-in fade-in duration-200">Welcome to Stroke Pro</h1>
          <p class="text-ui-sm leading-relaxed text-muted-foreground text-pretty animate-in fade-in duration-200">
            Pro is on your account. Opening your workspace.
          </p>
        {:else}
          <h1 class="text-ui-xl font-semibold tracking-tight text-foreground text-balance">Your trial has ended</h1>
          <p class="text-ui-sm leading-relaxed text-muted-foreground text-pretty">
            {$isSignedIn
              ? 'Get Stroke Pro to keep working. Bought it with another email? Switch to that account.'
              : 'Sign in with the email you bought Stroke Pro with, or get it on stroke.click.'}
          </p>
        {/if}
      </div>

      <div class="mt-6 flex w-full flex-col gap-2 text-left">
        {#if activated}
          <div class="flex items-center justify-center gap-2 text-ui-sm text-muted-foreground animate-in fade-in duration-200">
            <LoaderCircle class="size-3.5 shrink-0 animate-spin" /> Loading
          </div>
        {:else if $isSignedIn}
          <button type="button" class="keycap-btn inline-flex h-10 w-full items-center justify-center gap-2 rounded-md px-4 text-ui-sm font-medium" onclick={() => void openStore()}>
            Get Stroke Pro <Icon name="external-link" class="size-3.5 shrink-0 text-muted-foreground" />
          </button>
          <button type="button" class="inline-flex h-8 w-full items-center justify-center gap-1.5 rounded-md text-ui-xs text-muted-foreground transition-colors hover:text-foreground disabled:pointer-events-none" disabled={checking} aria-busy={checking} onclick={() => void checkAgain()}>
            {#if checking}<LoaderCircle class="size-3.5 shrink-0 animate-spin" /> Checking{:else}<RefreshCw class="size-3.5 shrink-0" /> I bought it, check again{/if}
          </button>
        {:else}
          <!-- One way in: an account that has Pro. Buying is a quiet link
               below, not a second big button. -->
          <AccountSignInButtons size="lg" />
        {/if}
      </div>

      {#if !activated && !$isSignedIn}
        <div class={cn('mt-4 flex items-center justify-center text-ui-xs text-muted-foreground', waiting && 'pointer-events-none opacity-0')}>
          <button type="button" onclick={() => void openStore()} class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 transition-[color,scale] duration-[120ms] hover:text-foreground active:scale-[0.97]">
            Get Stroke Pro <Icon name="external-link" class="size-3 shrink-0" />
          </button>
        </div>
      {/if}

      {#if $isSignedIn && email && !activated}
        <!-- Who is signed in, and the way to switch: a different account may
             be the one that has Pro. -->
        <div class={cn('mt-6 flex w-full items-center gap-2.5 rounded-lg border border-border/60 bg-card/50 px-3 py-2 text-left transition-opacity duration-200', waiting && 'pointer-events-none opacity-0')}>
          <span class="grid size-6 shrink-0 place-items-center rounded-full bg-muted text-ui-2xs font-medium uppercase text-foreground" aria-hidden="true">{email.slice(0, 1)}</span>
          <span class="min-w-0 flex-1">
            <span class="block text-ui-3xs text-muted-foreground">Signed in as</span>
            <span class="block truncate text-ui-xs text-foreground">{email}</span>
          </span>
          <button type="button" onclick={() => void signOut()} class="inline-flex h-7 shrink-0 items-center rounded-md px-2 text-ui-xs text-muted-foreground transition-[background-color,color,scale] duration-[120ms] hover:bg-muted/60 hover:text-foreground active:scale-[0.97]">
            Switch
          </button>
        </div>
      {/if}

      {#if !activated}
        <p class="mt-6 flex items-center justify-center gap-1.5 text-ui-2xs text-muted-foreground">
          <ShieldCheck class="size-3.5 shrink-0" />
          Connections and settings stay on this device.
        </p>
        <button type="button" onclick={quit} class={cn('mt-2 inline-flex h-7 items-center rounded-md px-2 text-ui-2xs text-muted-foreground transition-[color,scale] duration-[120ms] hover:text-foreground active:scale-[0.97]', waiting && 'pointer-events-none opacity-0')}>
          Quit Stroke
        </button>
      {/if}
    </div>
  </div>
  <!-- The app (and its update dialog) is not rendered behind this screen, so
       updates are checked from here. -->
  <div class="flex h-10 shrink-0 items-center justify-center text-ui-2xs text-muted-foreground">
    <GateUpdateCheck />
  </div>
</div>
