<script>
  // The sign-in screen everyone sees before the app until they sign in (see
  // account.js `needsOnboarding`); a device with an activated license key skips
  // it. Sign-in goes through the browser; "I have a license key" is the
  // fallback to the existing activation flow.
  import { onMount } from 'svelte'
  import { revealApp } from '$lib/app-reveal.js'
  import Logo from './Logo.svelte'
  import WindowControls from './WindowControls.svelte'
  import LicenseActivation from './LicenseActivation.svelte'
  import AccountSignInButtons from './AccountSignInButtons.svelte'
  import { skipOnboarding, signInState, isReturningUser } from '$lib/stores/account.js'
  import { detectOs } from '$lib/platform.js'
  import { cn } from '$lib/utils.js'
  import ArrowLeft from '@lucide/svelte/icons/arrow-left'

  /** Called once signed in (or a key was activated); the gate hands over to the app. */
  let { ondone = () => {} } = $props()

  let showKey = $state(false)
  let leaving = $state(false)
  function finish() {
    leaving = true
    setTimeout(ondone, 200)
  }

  const isMac = typeof navigator !== 'undefined' && detectOs() === 'macos'
  let isTauri = $state(false)
  onMount(() => {
    isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
    revealApp()
  })

  /** @param {MouseEvent} e */
  function onDragRegionDblClick(e) {
    if (!isTauri || e.target !== e.currentTarget) return
    import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().toggleMaximize())
  }
</script>

<div class={cn('fixed inset-0 z-[9999] flex flex-col bg-background transition-opacity duration-200 ease-out', leaving && 'opacity-0')}>
  <!-- Bare title bar: this screen replaces the app, TitleBar included. -->
  <div
    class="flex h-[52px] shrink-0 items-center px-[10px] select-none"
    data-tauri-drag-region
    role="toolbar"
    aria-label="Window title bar"
    tabindex="-1"
    ondblclick={onDragRegionDblClick}
  >
    {#if isMac}<div class="w-[92px] shrink-0"></div>{/if}
    <div class="min-w-0 flex-1" data-tauri-drag-region></div>
    <WindowControls />
  </div>

  <!-- One calm centre: a small mark, one headline, one quiet line, one block of
       actions. No tiles, no cards, nothing competing with the question. -->
  <div class="relative flex min-h-0 flex-1 flex-col items-center justify-center overflow-y-auto px-6 pb-16">
    <div class="welcome-hero flex w-full max-w-[22rem] flex-col items-center text-center">
      <Logo class="mb-6 size-6 shrink-0 opacity-90" />

      {#if showKey}
        <h1 class="text-ui-2xl font-medium tracking-tight text-foreground text-balance">Enter your license key</h1>
        <p class="mt-2 text-ui-sm text-muted-foreground text-pretty">It was sent to your email after purchase.</p>
        <div class="mt-7 w-full">
          <LicenseActivation naked onsuccess={() => void skipOnboarding()} onactivated={finish} />
        </div>
        <button
          type="button"
          class="mt-5 inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:text-foreground"
          onclick={() => (showKey = false)}
        >
          <ArrowLeft class="size-3.5 shrink-0" /> Back
        </button>
      {:else}
        {#if $isReturningUser}
          <h1 class="text-ui-2xl font-medium tracking-tight text-foreground text-balance">Sign in to continue</h1>
          <p class="mt-2 text-ui-sm text-muted-foreground text-pretty">Your connections and settings stay on this Mac.</p>
        {:else}
          <h1 class="text-ui-2xl font-medium tracking-tight text-foreground text-balance">Welcome to Stroke</h1>
          <p class="mt-2 text-ui-sm text-muted-foreground text-pretty">Sign in to start your free trial.</p>
        {/if}

        <div class="mt-7 w-full">
          <AccountSignInButtons size="lg" onsignedin={finish} />
        </div>

        {#if $signInState.phase !== 'waiting'}
          <button
            type="button"
            class="mt-4 h-7 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:text-foreground"
            onclick={() => (showKey = true)}
          >
            I have a license key
          </button>
        {/if}
      {/if}
    </div>
  </div>

  <!-- Fine print, out of the way. -->
  <div class="flex h-10 shrink-0 items-center justify-center gap-3 text-ui-2xs text-muted-foreground/80">
    <a href="https://stroke.click/terms" target="_blank" rel="noopener noreferrer" class="transition-colors hover:text-foreground">Terms</a>
    <span aria-hidden="true">·</span>
    <a href="https://stroke.click/privacy" target="_blank" rel="noopener noreferrer" class="transition-colors hover:text-foreground">Privacy</a>
  </div>
</div>
