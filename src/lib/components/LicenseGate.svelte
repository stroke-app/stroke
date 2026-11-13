<script>
  import { onMount } from 'svelte'
  import { isBlocked, refreshLicenseStatus, runLicenseCheck } from '$lib/stores/license.js'
  import TrialExpiredScreen from './TrialExpiredScreen.svelte'
  import SignInScreen from './SignInScreen.svelte'
  import { needsOnboarding, startAccountSync } from '$lib/stores/account.js'

  let { children } = $props()

  onMount(async () => {
    // Load the local status immediately so an expired trial gates on first paint,
    // then phone home in the background to catch server-side revocation.
    await refreshLicenseStatus()
    runLicenseCheck()
    // Account: cached first, then the server, then on focus. Never blocks an
    // existing user; only a brand-new install gets the sign-in screen.
    void startAccountSync()
    // E2E sync of workspaces/connections/settings follows the account store.
    void import('$lib/sync/runtime.js').then((m) => m.startSync()).catch((e) => console.error('[sync] start failed:', e))
  })

  let onboardingDone = $state(false)

  // Dev-only: Alt+Shift+O previews the first-run tour even on the sign-in
  // screen, where the app shell (and its own Alt+Shift+O) is not mounted.
  let tourPreview = $state(false)
  /** @param {KeyboardEvent} e */
  function onDevKey(e) {
    if (!import.meta.env.DEV) return
    if (e.altKey && e.shiftKey && e.code === 'KeyO') { e.preventDefault(); tourPreview = true }
  }

  // Once the gate has shown, it stays up until the screen says it is done.
  // activateLicense() refreshes the status before it resolves, so $isBlocked
  // goes false mid-activation - and dropping the gate right then mounted the
  // whole app in the same tick the confetti started, which is what made the
  // celebration stutter (and hid the "Activated" state it was celebrating).
  let holdGate = $state(false)
  $effect(() => {
    if ($isBlocked) holdGate = true
  })
</script>

<svelte:window onkeydown={onDevKey} />

{#if $needsOnboarding && !onboardingDone}
  <!-- New install with no key and no account: sign in first. -->
  <SignInScreen ondone={() => (onboardingDone = true)} />
  {#if import.meta.env.DEV && tourPreview}
    {#await import('./Onboarding.svelte') then { default: Onboarding }}
      <Onboarding bind:open={tourPreview} />
    {/await}
  {/if}
{:else if $isBlocked || holdGate}
  <!-- Trial over, no license → hard block: the app is not rendered at all. -->
  <TrialExpiredScreen onactivated={() => (holdGate = false)} />
{:else}
  {@render children()}
{/if}
