<script>
  import { onMount, onDestroy } from 'svelte'
  import GitBranch from '@lucide/svelte/icons/git-branch'
  import Check from '@lucide/svelte/icons/check'
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import LogOut from '@lucide/svelte/icons/log-out'
  import AlertTriangle from '@lucide/svelte/icons/alert-triangle'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import {
    startDeviceFlow,
    pollOAuthToken,
    storeOAuthToken,
    loadOAuthToken,
    clearOAuthToken,
    fetchCopilotModels,
  } from '$lib/copilot.js'

  let {
    /** Called when successfully authenticated. */
    onconnect = /** @type {(models: {id:string,name:string}[]) => void} */ (() => {}),
    /** Called when the user logs out. */
    ondisconnect = () => {},
  } = $props()

  /** @type {'idle' | 'starting' | 'waiting' | 'verifying' | 'connected' | 'error'} */
  let phase = $state('idle')

  /** @type {string} */
  let userCode = $state('')
  /** @type {string} */
  let verificationUri = $state('https://github.com/login/device')
  /** @type {string} */
  let errorMsg = $state('')
  /** @type {string} */
  let deviceCode = $state('')
  /** @type {number} */
  let pollInterval = 5

  /** Countdown in seconds until code expires (15 min max). */
  let secondsLeft = $state(0)
  /** @type {ReturnType<typeof setTimeout> | null} */
  let pollTimer = null
  /** @type {ReturnType<typeof setTimeout> | null} */
  let countdownTimer = null
  /** Bumped by stopTimers so a doPoll awaiting the network can tell it was cancelled
   *  and must not re-arm the timer or mutate state (cancel/unmount mid-request). */
  let pollGeneration = 0

  function stopTimers() {
    pollGeneration++
    if (pollTimer) { clearTimeout(pollTimer); pollTimer = null }
    if (countdownTimer) { clearInterval(countdownTimer); countdownTimer = null }
  }

  onMount(async () => {
    // Check if already connected on mount
    const stored = await loadOAuthToken()
    if (stored) {
      phase = 'connected'
    }
  })

  onDestroy(() => stopTimers())

  async function startLogin() {
    phase = 'starting'
    errorMsg = ''
    try {
      const flow = await startDeviceFlow()
      deviceCode = flow.device_code
      userCode = flow.user_code
      verificationUri = flow.verification_uri
      pollInterval = flow.interval ?? 5
      secondsLeft = flow.expires_in ?? 900
      phase = 'waiting'

      // Open the verification URL in the browser automatically
      try {
        const { openUrl } = await import('@tauri-apps/plugin-opener')
        await openUrl(verificationUri)
      } catch {
        // Fallback for dev mode without Tauri
        window.open(verificationUri, '_blank', 'noopener')
      }

      // Start countdown
      countdownTimer = setInterval(() => {
        secondsLeft = Math.max(0, secondsLeft - 1)
        if (secondsLeft === 0) { stopTimers(); phase = 'error'; errorMsg = 'Code expired. Please try again.' }
      }, 1000)

      // Start polling
      schedulePoll()
    } catch (e) {
      phase = 'error'
      errorMsg = String(/** @type {any} */ (e)?.message ?? e)
    }
  }

  function schedulePoll() {
    pollTimer = setTimeout(doPoll, pollInterval * 1000)
  }

  async function doPoll() {
    const gen = pollGeneration
    try {
      const result = await pollOAuthToken(deviceCode)
      if (gen !== pollGeneration) return // cancelled while awaiting
      if (result.status === 'authorized' && result.token) {
        stopTimers()
        phase = 'verifying'
        await storeOAuthToken(result.token)
        // Fetch available models to pass back to the caller
        let models = []
        try { models = await fetchCopilotModels() } catch {}
        phase = 'connected'
        onconnect(models)
        return
      }
      if (result.status === 'expired' || result.status === 'access_denied') {
        stopTimers()
        phase = 'error'
        errorMsg = result.status === 'access_denied'
          ? 'Access was denied. Please try again.'
          : 'Code expired. Please try again.'
        return
      }
      if (result.status === 'slow_down') {
        pollInterval = Math.min(pollInterval * 2, 30)
      }
      schedulePoll()
    } catch (e) {
      if (gen !== pollGeneration) return // cancelled while awaiting
      stopTimers()
      phase = 'error'
      errorMsg = String(/** @type {any} */ (e)?.message ?? e)
    }
  }

  async function handleLogout() {
    stopTimers()
    await clearOAuthToken()
    phase = 'idle'
    userCode = ''
    errorMsg = ''
    ondisconnect()
  }

  function cancel() {
    stopTimers()
    phase = 'idle'
    userCode = ''
    errorMsg = ''
  }

  function fmt(secs) {
    const m = Math.floor(secs / 60)
    const s = secs % 60
    return `${m}:${String(s).padStart(2, '0')}`
  }

  async function openExternal(url) {
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl(url)
    } catch {
      window.open(url, '_blank', 'noopener')
    }
  }
</script>

<div class="flex flex-col gap-3">
  {#if phase === 'idle'}
    <!-- ── Not connected ── -->
    <div class="flex flex-col items-center gap-3 rounded-lg border border-border/60 bg-muted/30 px-4 py-5 text-center">
      <GitBranch class="size-8 shrink-0 text-muted-foreground" />
      <div>
        <p class="text-ui-sm font-medium text-foreground">Connect GitHub Copilot</p>
        <p class="mt-0.5 text-ui-xs text-muted-foreground">Sign in with your GitHub account to use Copilot models.</p>
      </div>
      <button
        type="button"
        class="flex items-center gap-2 rounded-md bg-foreground px-4 py-1.5 text-ui-sm font-medium text-background transition-colors hover:bg-foreground/80"
        onclick={startLogin}
      >
        <GitBranch class="size-3.5 shrink-0" />
        Login with GitHub
      </button>
    </div>

  {:else if phase === 'starting'}
    <!-- ── Starting ── -->
    <div class="loading-reveal flex items-center justify-center gap-2 rounded-lg border border-border/60 bg-muted/30 px-4 py-5 text-ui-xs text-muted-foreground">
      <Loader2 class="size-4 shrink-0 animate-spin" />
      Starting device flow…
    </div>

  {:else if phase === 'waiting'}
    <!-- ── Waiting for user to enter the code on GitHub ── -->
    <div class="flex flex-col gap-3 rounded-lg border border-border/60 bg-muted/30 px-4 py-4">
      <div class="flex items-center justify-between text-ui-xs text-muted-foreground">
        <span>1. Go to GitHub and enter the code</span>
        <span class="font-mono {secondsLeft < 60 ? 'text-destructive' : ''}">{fmt(secondsLeft)}</span>
      </div>

      <!-- User code, large and easy to read -->
      <div class="flex items-center justify-between gap-2 rounded-md border border-border bg-background px-3 py-2.5">
        <span class="font-mono text-ui-lg font-semibold tracking-widest text-foreground">{userCode}</span>
        <button
          type="button"
          class="shrink-0 rounded px-1.5 py-0.5 text-ui-xs text-muted-foreground hover:bg-accent/20 hover:text-foreground"
          onclick={() => navigator.clipboard.writeText(userCode)}
        >Copy</button>
      </div>

      <button
        type="button"
        class="flex items-center justify-center gap-1.5 rounded-md border border-border/60 px-3 py-1.5 text-ui-xs text-muted-foreground transition-colors hover:bg-accent/20 hover:text-foreground"
        onclick={() => openExternal(verificationUri)}
      >
        <ExternalLink class="size-3 shrink-0" />
        Open github.com/login/device
      </button>

      <p class="text-center text-ui-xs text-muted-foreground">
        <Loader2 class="mr-1 inline size-3 shrink-0 animate-spin" />
        Waiting for authorization…
      </p>

      <button
        type="button"
        class="self-center text-ui-xs text-muted-foreground hover:text-foreground"
        onclick={cancel}
      >Cancel</button>
    </div>

  {:else if phase === 'verifying'}
    <!-- ── Exchanging token ── -->
    <div class="loading-reveal flex items-center justify-center gap-2 rounded-lg border border-border/60 bg-muted/30 px-4 py-5 text-ui-xs text-muted-foreground">
      <Loader2 class="size-4 shrink-0 animate-spin" />
      Verifying with GitHub Copilot…
    </div>

  {:else if phase === 'connected'}
    <!-- ── Connected ── -->
    <div class="flex items-center gap-3 rounded-lg border border-success/30 bg-success/10 px-3 py-3">
      <div class="flex size-7 shrink-0 items-center justify-center rounded-full bg-success/30">
        <Check class="size-4 shrink-0 text-success" />
      </div>
      <div class="min-w-0 flex-1">
        <p class="text-ui-sm font-medium text-foreground">Connected to GitHub Copilot</p>
        <p class="text-ui-xs text-muted-foreground">Models are loaded from your subscription.</p>
      </div>
      <button
        type="button"
        class="shrink-0 rounded p-1 text-muted-foreground hover:bg-accent/20 hover:text-destructive"
        title="Log out"
        onclick={handleLogout}
      >
        <LogOut class="size-3.5 shrink-0" />
      </button>
    </div>

  {:else if phase === 'error'}
    <!-- ── Error ── -->
    <div class="flex flex-col gap-2 rounded-lg border border-destructive/30 bg-destructive/8 px-3 py-3">
      <div class="flex items-start gap-2 text-ui-xs text-destructive">
        <AlertTriangle class="mt-0.5 size-3.5 shrink-0" />
        <span class="break-words">{errorMsg || 'Authentication failed.'}</span>
      </div>
      <button
        type="button"
        class="self-start text-ui-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
        onclick={startLogin}
      >Try again</button>
    </div>
  {/if}
</div>
