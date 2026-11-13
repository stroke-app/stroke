<script>
  // Hard trial gate. Rendered in place of the whole app once the trial has
  // expired and no valid license is present - there is no dismiss path. The
  // ways through: sign in to an account that has Stroke Pro (a purchase on
  // stroke.click shows up there), or activate a license key.
  import { onMount } from 'svelte'
  import { revealApp } from '$lib/app-reveal.js'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import LicenseActivation from './LicenseActivation.svelte'
  import AccountSignInButtons from './AccountSignInButtons.svelte'
  import WindowControls from './WindowControls.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { licenseStatus, isBlocked } from '$lib/stores/license.js'
  import { account, isSignedIn, signInState, refreshAccount, signOut } from '$lib/stores/account.js'
  import { detectOs } from '$lib/platform.js'
  import { cn } from '$lib/utils.js'
  import ArrowLeft from '@lucide/svelte/icons/arrow-left'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import ShieldCheck from '@lucide/svelte/icons/shield-check'

  /** Called once activation has finished celebrating; LicenseGate drops the gate on it. */
  let { onactivated = () => {} } = $props()

  /** Account sign-in is the main path; the key field is the fallback. */
  let mode = $state(/** @type {'account' | 'key'} */ ('account'))
  /** Pro was found (key or account): the copy turns into a welcome. */
  let activated = $state(false)
  /** How Pro arrived, for the welcome line. */
  let activatedVia = $state(/** @type {'key' | 'account'} */ ('key'))
  /** The celebration is over: fade out, then let the gate hand over to the app. */
  let leaving = $state(false)
  function finish() {
    leaving = true
    setTimeout(onactivated, 220)
  }

  // Follow licenseStatus, which the backend merges with the account's
  // entitlement: once it stops being TrialExpired (sign-in, a later refresh
  // after buying, a focus refresh), hand over to the app. The key flow runs its
  // own celebration and calls finish() itself, so it is left alone here.
  $effect(() => {
    if (mode === 'key' || activated) return
    if ($licenseStatus && !$isBlocked) {
      activatedVia = 'account'
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

  <div class="relative flex min-h-0 flex-1 flex-col items-center justify-center overflow-y-auto px-4 py-10">
    <!-- Ambient glow -->
    <div
      class="pointer-events-none absolute inset-x-0 top-0 h-[55%]"
      style="background: radial-gradient(ellipse 55% 50% at 50% -8%, color-mix(in oklch, var(--primary) 9%, transparent), transparent 70%);"
    ></div>

    <div class="relative flex w-full max-w-[25rem] flex-col gap-3">
      <div class="flex flex-col gap-5 rounded-lg border border-border/60 bg-card/60 p-6 shadow-sm">
        <div class="flex items-center gap-3">
          <div class="grid size-9 shrink-0 place-items-center rounded-lg border border-border/60 bg-background">
            <Logo class="size-4.5" />
          </div>
          <div class="flex min-w-0 flex-col">
            <span class="text-ui-sm font-medium text-foreground">Stroke Pro</span>
            <span class="truncate text-ui-xs text-muted-foreground">
              {#if activated}Active{:else if $isSignedIn && email}{email}{:else}Trial ended{/if}
            </span>
          </div>
        </div>

        <div class="flex flex-col gap-1.5">
          {#if activated}
            <h1 class="text-ui-2xl font-semibold tracking-tight text-foreground text-balance animate-in fade-in duration-200">Welcome to Stroke</h1>
            <p class="text-ui-sm leading-normal text-muted-foreground text-pretty animate-in fade-in duration-200">
              {activatedVia === 'account' ? 'Stroke Pro is on your account.' : 'Your license is active.'} Opening your workspace.
            </p>
          {:else if mode === 'key'}
            <h1 class="text-ui-2xl font-semibold tracking-tight text-foreground text-balance">Enter your license key</h1>
            <p class="text-ui-sm leading-normal text-muted-foreground text-pretty">It was sent to your email after purchase.</p>
          {:else if $isSignedIn}
            <h1 class="text-ui-2xl font-semibold tracking-tight text-foreground text-balance">Your free trial has ended</h1>
            <p class="text-ui-sm leading-normal text-muted-foreground text-pretty">This account doesn't have Stroke Pro yet. Upgrade to keep working, or check again if you just bought it.</p>
          {:else}
            <h1 class="text-ui-2xl font-semibold tracking-tight text-foreground text-balance">Your free trial has ended</h1>
            <p class="text-ui-sm leading-normal text-muted-foreground text-pretty">Sign in to continue. A purchase on stroke.click shows up in your account.</p>
          {/if}
        </div>

        {#if mode === 'key' || (activated && activatedVia === 'key')}
          <LicenseActivation naked onsuccess={() => { activatedVia = 'key'; activated = true }} onactivated={finish} />
        {:else if activated}
          <div class="flex items-center gap-2 text-ui-sm text-muted-foreground animate-in fade-in duration-200">
            <LoaderCircle class="size-3.5 shrink-0 animate-spin" /> Loading
          </div>
        {:else if $isSignedIn}
          <div class="flex flex-col gap-2">
            <Button size="lg" class="w-full" onclick={() => void openStore()}>
              Get Stroke Pro <Icon name="external-link" class="size-3.5 shrink-0" />
            </Button>
            <div class="grid grid-cols-2 gap-2">
              <Button variant="secondary" class="w-full" disabled={checking} onclick={() => void checkAgain()}>
                {#if checking}<LoaderCircle class="size-3.5 shrink-0 animate-spin" />{:else}<RefreshCw class="size-3.5 shrink-0" />{/if}
                Check again
              </Button>
              <Button variant="secondary" class="w-full" onclick={() => (mode = 'key')}>
                <KeyRound class="size-3.5 shrink-0" /> License key
              </Button>
            </div>
          </div>
        {:else}
          <div class="flex flex-col gap-2">
            <AccountSignInButtons size="lg" />
            <Button variant="ghost" class="w-full text-muted-foreground" onclick={() => (mode = 'key')}>
              <KeyRound class="size-3.5 shrink-0" /> Use a license key instead
            </Button>
          </div>
        {/if}

        {#if !activated}
          <div class="flex items-center gap-2 border-t border-border/50 pt-4 text-ui-xs text-muted-foreground">
            <ShieldCheck class="size-3.5 shrink-0" />
            Your connections and settings are saved on this device.
          </div>
        {/if}
      </div>

      <div class={cn('flex items-center justify-between gap-3 px-1 text-ui-xs transition-opacity duration-200', (activated || waiting) && 'pointer-events-none opacity-0')}>
        {#if mode === 'key'}
          <button type="button" onclick={() => (mode = 'account')} class="inline-flex items-center gap-1.5 text-muted-foreground transition-colors hover:text-foreground">
            <ArrowLeft class="size-3.5 shrink-0" /> Back
          </button>
        {:else if $isSignedIn}
          <button type="button" onclick={() => void signOut()} class="text-muted-foreground transition-colors hover:text-foreground">
            Use a different account
          </button>
        {:else}
          <button type="button" onclick={() => void openStore()} class="inline-flex items-center gap-1.5 text-muted-foreground transition-colors hover:text-foreground">
            Get Stroke Pro <Icon name="external-link" class="size-3.5 shrink-0" />
          </button>
        {/if}
        <button type="button" onclick={quit} class="text-muted-foreground transition-colors hover:text-foreground">
          Quit Stroke
        </button>
      </div>
    </div>
  </div>
</div>
