<script>
  // Owns the app-lock lifecycle: read the status before first paint, hold the
  // lock screen in front of everything while locked, and host the single
  // re-auth dialog that `requireUnlock()` drives from anywhere in the app.
  import { onMount } from 'svelte'
  import AppLockScreen from './AppLockScreen.svelte'
  import PinChallengeDialog from './PinChallengeDialog.svelte'
  import { locked, initAppLock, installAutoLock } from '$lib/stores/app-lock.js'

  let { children } = $props()

  /** Status known (or given up on) - see the timeout below. */
  let ready = $state(false)
  /** The PIN has been accepted at least once this run. */
  let opened = $state(false)

  // A cold start behind the lock must not mount the app at all: StudioShell
  // auto-reconnects on mount, and "the app is locked" has to mean the databases
  // are untouched. An auto-lock later is the opposite case - the session is
  // already live and must survive, so the screen goes over it instead.
  const mountApp = $derived(opened || !$locked)

  $effect(() => {
    if (!$locked && ready) opened = true
  })

  // While locked, the app underneath is covered but still listening: its hotkeys
  // are document-level, and inert/pointer-events do nothing about those. Swallow
  // every key that isn't headed for the lock screen itself.
  $effect(() => {
    if (!$locked || !opened) return
    /** @param {KeyboardEvent} e */
    const swallow = (e) => {
      const target = /** @type {Element | null} */ (e.target)
      if (target?.closest?.('[data-app-lock]')) return
      e.stopImmediatePropagation()
    }
    const events = /** @type {const} */ (['keydown', 'keyup', 'keypress'])
    for (const t of events) window.addEventListener(t, swallow, true)
    return () => {
      for (const t of events) window.removeEventListener(t, swallow, true)
    }
  })

  onMount(() => {
    let teardown = () => {}
    // Fail open on a slow read rather than sit on a blank window: an OS keychain
    // call can block on a "let this app in?" prompt. If the status lands late
    // and says locked, the screen still comes up - just a beat later.
    const giveUp = setTimeout(() => (ready = true), 1500)
    initAppLock().finally(() => {
      performance.mark('stroke:lock-ready')
      clearTimeout(giveUp)
      ready = true
      teardown = installAutoLock()
    })
    return () => {
      clearTimeout(giveUp)
      teardown()
    }
  })
</script>

{#if ready}
  {#if mountApp}
    {@render children()}
    <PinChallengeDialog />
  {/if}
  {#if $locked}
    <AppLockScreen />
  {/if}
{/if}
