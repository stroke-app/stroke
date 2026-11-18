<script>
  import { onMount } from 'svelte'
  import { isBlocked, refreshLicenseStatus } from '$lib/stores/license.js'
  import TrialExpiredScreen from './TrialExpiredScreen.svelte'
  import SignInScreen from './SignInScreen.svelte'
  import { needsOnboarding, startAccountSync, account } from '$lib/stores/account.js'

  let { children } = $props()

  /** The cached account and its status are loaded: the gate can decide. */
  let ready = $state(false)

  onMount(async () => {
    // Pro comes from the account alone. Load the cached account and status
    // before deciding, so a signed-out launch goes straight to sign-in and a
    // signed-in one never flashes a gate. The server refresh follows in the
    // background (and again on focus).
    try {
      await startAccountSync()
      await refreshLicenseStatus()
    } finally {
      ready = true
    }
    // E2E sync of workspaces/connections/settings follows the account store.
    void import('$lib/sync/runtime.js').then((m) => m.startSync()).catch((e) => console.error('[sync] start failed:', e))
  })

  let onboardingDone = $state(false)

  // Signing out goes back to the sign-in screen. `onboardingDone` stays true
  // from the sign-in earlier in the session, so reset it here, or a signed-out
  // user would see "Your trial has ended" instead of "Sign in".
  let wasSignedIn = false
  $effect(() => {
    const signedIn = !!$account?.signed_in
    if (wasSignedIn && !signedIn) onboardingDone = false
    wasSignedIn = signedIn
  })

  // Dev-only: Alt+Shift+O previews the first-run tour even on the sign-in
  // screen, where the app shell (and its own Alt+Shift+O) is not mounted.
  let tourPreview = $state(false)
  /** @param {KeyboardEvent} e */
  function onDevKey(e) {
    if (!import.meta.env.DEV) return
    if (e.altKey && e.shiftKey && e.code === 'KeyO') { e.preventDefault(); tourPreview = true }
  }

  // Once the trial gate has shown, it stays up until its welcome has played
  // (it calls onactivated); dropping it the moment Pro arrives would cut that.
  let holdGate = $state(false)
  $effect(() => {
    if (ready && $isBlocked && !$needsOnboarding) holdGate = true
  })
</script>

<svelte:window onkeydown={onDevKey} />

{#if !ready}
  <!-- A moment at launch while the cached account loads. -->
{:else if $needsOnboarding && !onboardingDone}
  <!-- Not signed in: sign in first. There is no other way into the app. -->
  <SignInScreen ondone={() => (onboardingDone = true)} />
  {#if import.meta.env.DEV && tourPreview}
    {#await import('./Onboarding.svelte') then { default: Onboarding }}
      <Onboarding bind:open={tourPreview} />
    {/await}
  {/if}
{:else if $isBlocked || holdGate}
  <!-- Trial over and no Pro on the account: the app is not rendered at all. -->
  <TrialExpiredScreen onactivated={() => (holdGate = false)} />
{:else}
  {@render children()}
{/if}
