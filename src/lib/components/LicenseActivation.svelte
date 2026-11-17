<script>
  import { onDestroy, tick } from 'svelte'
  import { activateLicense } from '$lib/stores/license.js'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'

  let {
    /** Fired once the celebration is over - the caller moves on from here. */
    onactivated = () => {},
    /** Fired the moment the key is accepted, before the celebration. */
    onsuccess = () => {},
    compact = false, naked = false, inline = false,
  } = $props()

  // canvas-confetti is loaded on demand. Onboarding mounts this component, so a
  // static import puts the whole library in the boot chunk for a one-off burst
  // that most sessions never fire. It is fetched the moment the key field is
  // touched, not at the moment of success: importing it then put a chunk load
  // between "Activated" and the first particle.
  /** @type {Promise<any> | null} */
  let confettiModule = null
  function loadConfetti() {
    confettiModule ??= import('canvas-confetti').then((m) => m.default).catch(() => null)
    return confettiModule
  }

  const sleep = (/** @type {number} */ ms) => new Promise((r) => setTimeout(r, ms))

  /**
   * The confetti draws on a canvas this component owns, not the one
   * canvas-confetti appends to <body> by default. The body canvas outlived the
   * screen: the app mounted underneath it and the last particles fell across
   * the workspace. This canvas is removed with the component, so the
   * celebration can only ever happen here.
   * @type {HTMLCanvasElement | null}
   */
  let confettiCanvas = $state(null)
  /** @type {any} */
  let cannon = null
  onDestroy(() => cannon?.reset())

  /**
   * Two side cannons, then a smaller pair: 180 particles living ~2s. Drawn in a
   * worker where OffscreenCanvas exists (WebView2, WKWebView); WebKitGTK has
   * none and draws on the main thread, which is why the count stays modest.
   * Resolves when the last particle has settled.
   */
  async function fireConfetti() {
    const confetti = await loadConfetti()
    await tick() // the canvas renders with `success`
    if (!confetti || !confettiCanvas) return // a missed celebration must never fail the activation
    cannon ??= confetti.create(confettiCanvas, { resize: true, useWorker: true, disableForReducedMotion: true })
    /** @param {number} angle @param {{ x: number, y: number }} origin @param {number} particleCount */
    const burst = (angle, origin, particleCount) =>
      cannon({ angle, origin, particleCount, spread: 60, startVelocity: 42, decay: 0.91, scalar: 1, ticks: 130 })
    const first = [burst(60, { x: 0, y: 0.65 }, 60), burst(120, { x: 1, y: 0.65 }, 60)]
    await sleep(150)
    await Promise.all([...first, burst(75, { x: 0.15, y: 0.5 }, 30), burst(105, { x: 0.85, y: 0.5 }, 30)])
  }

  let key = $state('')
  let loading = $state(false)
  let error = $state('')
  let success = $state(false)

  const ready = $derived(!!key.trim() && !loading && !success)

  /** The text the error line shows. It outlives `error` so the line can
   *  collapse with its words still in it instead of emptying first. */
  let shownError = $state('')

  /** "Verifying" only once a check has taken 200ms. A local check answers in a
   *  frame, and swapping the label for that frame was its own flicker. */
  let verifyingVisible = $state(false)
  $effect(() => {
    if (!loading) { verifyingVisible = false; return }
    const t = setTimeout(() => (verifyingVisible = true), 200)
    return () => clearTimeout(t)
  })

  async function submit() {
    if (!key.trim() || loading || success) return
    // The previous error stays up until this attempt's result replaces it.
    // Clearing it here collapsed the line and jumped the button up, and a
    // second failure then dropped it straight back down: a flicker per click.
    loading = true
    const result = await activateLicense(key.trim())
    loading = false
    if (result.ok) {
      error = ''
      success = true
      onsuccess()
      // Hand over once the last particle has landed - never sooner than 1.2s,
      // so the success state is readable, and never later than 3.2s if the
      // animation hangs. Nothing falls past this point: the canvas goes with us.
      await Promise.all([Promise.race([fireConfetti(), sleep(3200)]), sleep(1200)])
      onactivated()
    } else {
      error = friendlyError(result.error)
      shownError = error
    }
  }

  /**
   * The Rust side reports what failed to parse (`invalid key format (expected
   * <payload>.<signature>)`, `key payload is not valid base64url`, ...). That is
   * the right detail for a log and the wrong one for someone pasting a key they
   * paid for: every malformed-key case means the same thing to them.
   * @param {string} raw
   */
  function friendlyError(raw) {
    const msg = raw.replace(/^Error invoking remote method '[^']+': /, '').trim()
    if (/revoked/i.test(msg)) return 'This license has been revoked. Get in touch if that is unexpected.'
    if (/expired/i.test(msg)) return 'This license has expired.'
    if (/format|base64|signature|utf-8|timestamp|payload/i.test(msg)) {
      return "That isn't a valid license key. Paste the whole key from your purchase email."
    }
    return msg.charAt(0).toUpperCase() + msg.slice(1)
  }

  /** @param {KeyboardEvent} e */
  function handleKeydown(e) {
    if (e.key === 'Enter') void submit()
  }
</script>

{#snippet keyField(extraClass)}
  <!-- The field is a well a step below the page; the button above it is a
       key. Typed keys are mono so each group lines up; the placeholder is
       plain words, since a row of Xs never fit the width and got cut off. -->
  <label class={cn('key-well flex h-10 items-center gap-2 rounded-md px-3', extraClass)} data-invalid={error ? '' : undefined}>
    <Icon name="key-round" class="size-4 shrink-0 text-muted-foreground" />
    <input
      type="text"
      bind:value={key}
      onkeydown={handleKeydown}
      onfocus={loadConfetti}
      oninput={() => { error = ''; void loadConfetti() }}
      aria-label="License key"
      aria-invalid={error ? 'true' : undefined}
      aria-describedby={error ? 'license-key-error' : undefined}
      placeholder="Paste your license key"
      spellcheck="false"
      autocomplete="off"
      readonly={loading}
      disabled={success}
      class="no-field-frame h-full min-w-0 flex-1 bg-transparent text-ui-sm text-foreground outline-none placeholder:font-sans placeholder:tracking-normal placeholder:text-muted-foreground/70 {key ? 'font-mono tracking-wide' : ''}"
    />
  </label>
{/snippet}

{#snippet submitBtn(fullWidth, label)}
  <button
    type="button"
    disabled={!key.trim() && !success}
    aria-busy={loading || undefined}
    aria-keyshortcuts="Enter"
    onclick={() => void submit()}
    class={cn(
      'keycap-btn inline-flex h-10 shrink-0 items-center justify-center gap-2 rounded-md px-4 text-ui-sm font-medium',
      fullWidth && 'w-full',
      (loading || success) && 'pointer-events-none',
      success && 'text-success',
    )}
  >
    {#if loading && verifyingVisible}
      <Icon name="loader-2" class="size-3.5 shrink-0 animate-spin" />
      Verifying
    {:else if success}
      <Icon name="check" class="size-3.5 shrink-0" />
      License activated
    {:else}
      {label}
      <span aria-hidden="true" class="chip-gloss chip-muted h-5 px-1.5 text-ui-3xs font-semibold">↵</span>
    {/if}
  </button>
{/snippet}

<div class="flex flex-col gap-3 {naked || inline ? '' : compact ? 'p-5' : 'p-6'}">
  {#if !compact && !naked && !inline}
    <div class="flex flex-col gap-0.5">
      <p class="text-ui-sm font-semibold text-foreground">Enter your license key</p>
      <p class="text-ui-xs text-muted-foreground">Sent to your email after purchase.</p>
    </div>
  {/if}

  <!-- Input + button: side-by-side (inline) or stacked -->
  {#if inline}
    <div class="flex items-stretch gap-2">
      {@render keyField('flex-1 min-w-0')}
      {@render submitBtn(false, 'Activate')}
    </div>
  {:else}
    {@render keyField('')}
  {/if}

  <!-- Error: help text under the field it is about, not a red box. The
       field already turns red (aria-invalid); a red panel on top of that was
       the same signal three times. The line is always in the tree and opens
       and closes on its grid row, so the button below glides rather than
       jumping, and `-mt-3` cancels the column gap while it is closed. -->
  <div
    class={cn(
      'grid transition-[grid-template-rows,opacity,margin] duration-200 ease-out motion-reduce:transition-none',
      error ? 'grid-rows-[1fr] opacity-100' : '-mt-3 grid-rows-[0fr] opacity-0',
    )}
  >
    <div class="min-h-0 overflow-hidden">
      <p id="license-key-error" aria-live="polite" class="flex items-start gap-1.5 text-left text-ui-xs leading-normal text-muted-foreground text-pretty">
        <Icon name="alert-circle" class="mt-px size-3.5 shrink-0 text-destructive" />
        <span>{shownError}</span>
      </p>
    </div>
  </div>

  {#if !inline}
    {@render submitBtn(true, 'Activate')}
  {/if}
</div>

{#if success}
  <canvas bind:this={confettiCanvas} class="pointer-events-none fixed inset-0 z-[10000] size-full" aria-hidden="true"></canvas>
{/if}
