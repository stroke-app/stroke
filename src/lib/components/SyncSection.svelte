<script>
  // Account page → Sync: set up end-to-end encrypted sync (first device),
  // unlock it (other devices), its status, "Sync now" and "Reset sync".
  // Contract: docs/accounts-and-sync.md, Appendix B. Logic: $lib/sync/runtime.js.
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { syncStatus, syncLabel, syncNow, setupSync, unlockSync, resetSync, lockSync } from '$lib/sync/runtime.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { cn } from '$lib/utils.js'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import LockKeyhole from '@lucide/svelte/icons/lock-keyhole'
  import CircleAlert from '@lucide/svelte/icons/circle-alert'
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw'

  let { sectionLabel = '' } = $props()

  const MIN = 8
  /** @type {'none' | 'setup' | 'reset'} */
  let form = $state('none')
  let pass = $state('')
  let pass2 = $state('')
  let busy = $state(false)
  let formError = $state(/** @type {string | null} */ (null))
  let now = $state(Date.now())

  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 30_000)
    return () => clearInterval(t)
  })

  const s = $derived($syncStatus)
  const active = $derived(s.phase === 'ready' || s.phase === 'syncing' || s.phase === 'offline' || s.phase === 'error')
  const choosing = $derived(form === 'setup' || form === 'reset')
  const mismatch = $derived(choosing && pass2.length > 0 && pass !== pass2)
  const canSubmitNew = $derived(pass.length >= MIN && pass === pass2 && !busy)

  function closeForm() {
    form = 'none'
    pass = ''
    pass2 = ''
    formError = null
  }

  /** @param {() => Promise<void>} fn @param {string} done */
  async function run(fn, done) {
    if (busy) return
    busy = true
    formError = null
    try {
      await fn()
      closeForm()
      toast.success(done)
    } catch (e) {
      formError = String(e)
    } finally {
      busy = false
    }
  }

  /** @param {SubmitEvent} e */
  function submitNew(e) {
    e.preventDefault()
    if (!canSubmitNew) return
    const p = pass
    if (form === 'reset') void run(() => resetSync(p), 'Sync was reset. This device is the new starting point.')
    else void run(() => setupSync(p), 'Sync is on')
  }

  /** @param {SubmitEvent} e */
  function submitUnlock(e) {
    e.preventDefault()
    if (!pass || busy) return
    const p = pass
    void run(() => unlockSync(p), 'Sync unlocked')
  }

  async function lock() {
    try {
      await lockSync()
      toast.success('Sync locked on this device', { description: 'Your synced data stays in your account.' })
    } catch (e) {
      toast.error('Could not lock sync', { description: String(e) })
    }
  }
</script>

<section class="flex flex-col gap-3">
  <h2 class={sectionLabel}>Sync</h2>

  {#if choosing}
    <form class="flex flex-col gap-3" onsubmit={submitNew}>
      <p class="text-ui-sm leading-relaxed text-muted-foreground">
        {#if form === 'reset'}
          Resetting deletes everything synced to your account and uploads this device's workspaces, connections and settings
          as the new starting point. Other devices will ask for the new passphrase.
        {:else}
          Workspaces, connections (passwords included) and settings are encrypted on this device with your passphrase
          before they leave it. Stroke can't read them.
        {/if}
      </p>
      <div class="flex flex-col gap-2">
        <Input type="password" autocomplete="new-password" placeholder="Passphrase" aria-label="Passphrase" bind:value={pass} disabled={busy} />
        <Input type="password" autocomplete="new-password" placeholder="Passphrase again" aria-label="Passphrase again" bind:value={pass2} disabled={busy} aria-invalid={mismatch} />
      </div>
      <p class="flex items-start gap-1.5 text-ui-xs leading-relaxed text-warning">
        <CircleAlert class="mt-0.5 size-3.5 shrink-0" />
        If you lose this passphrase it can't be recovered, not even by Stroke. You would have to reset sync.
      </p>
      {#if mismatch}<p class="text-ui-xs text-destructive">The passphrases don't match.</p>
      {:else if pass.length > 0 && pass.length < MIN}<p class="text-ui-xs text-muted-foreground">Use at least {MIN} characters.</p>{/if}
      {#if formError}<p class="text-ui-xs text-destructive" role="alert">{formError}</p>{/if}
      <div class="flex items-center gap-1">
        <Button type="submit" variant={form === 'reset' ? 'destructive' : 'default'} size="sm" disabled={!canSubmitNew}>
          {busy ? (form === 'reset' ? 'Resetting…' : 'Turning on…') : form === 'reset' ? 'Reset sync' : 'Turn on sync'}
        </Button>
        <Button type="button" variant="ghost" size="sm" class="text-muted-foreground" disabled={busy} onclick={closeForm}>Cancel</Button>
      </div>
    </form>
  {:else if s.phase === 'signed-out'}
    <p class="text-ui-sm text-muted-foreground">Sign in to sync your workspaces, connections and settings across devices.</p>
  {:else if s.phase === 'checking'}
    <p class="text-ui-sm text-muted-foreground">Checking sync…</p>
  {:else if s.phase === 'setup'}
    <div class="flex items-center justify-between gap-3">
      <p class="text-ui-sm leading-relaxed text-muted-foreground">Keep workspaces, connections and settings in step on every device, end-to-end encrypted.</p>
      <Button variant="secondary" size="sm" class="shrink-0" onclick={() => (form = 'setup')}>Set up sync</Button>
    </div>
  {:else if s.phase === 'locked'}
    <form class="flex flex-col gap-3" onsubmit={submitUnlock}>
      <p class="text-ui-sm leading-relaxed text-muted-foreground">
        {s.keyChanged
          ? 'Sync was reset on another device. Enter the new passphrase to keep syncing here.'
          : 'Sync is on for your account. Enter your passphrase to unlock it on this device.'}
      </p>
      <div class="flex items-center gap-2">
        <Input type="password" autocomplete="current-password" placeholder="Passphrase" aria-label="Sync passphrase" bind:value={pass} disabled={busy} class="flex-1" />
        <Button type="submit" size="sm" class="shrink-0" disabled={!pass || busy}><LockKeyhole class="size-3.5 shrink-0" /> {busy ? 'Unlocking…' : 'Unlock'}</Button>
      </div>
      {#if formError}<p class="text-ui-xs text-destructive" role="alert">{formError}</p>{/if}
      <button type="button" class="self-start text-ui-xs text-muted-foreground transition-colors hover:text-destructive" onclick={() => { formError = null; pass = ''; form = 'reset' }}>
        Forgot it? Reset sync
      </button>
    </form>
  {:else if active}
    <dl class="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-2 text-ui-sm">
      <dt class="text-muted-foreground">Status</dt>
      <dd class="flex items-center gap-2 text-foreground">
        <span class={cn('size-1.5 shrink-0 rounded-full', s.phase === 'error' ? 'bg-destructive' : s.phase === 'offline' ? 'bg-muted-foreground/40' : 'bg-success')}></span>
        {syncLabel(s, now)}
      </dd>
      <dt class="text-muted-foreground">Items</dt>
      <dd class="tabular-nums text-foreground">{s.items}</dd>
      <dt class="text-muted-foreground">Encryption</dt>
      <dd class="text-foreground">End-to-end, with your passphrase</dd>
    </dl>
    {#if s.error}<p class="text-ui-xs leading-relaxed text-destructive" role="alert">{s.error}</p>{/if}
    {#if s.undecryptable}<p class="text-ui-xs leading-relaxed text-muted-foreground">{s.undecryptable} {s.undecryptable === 1 ? 'item' : 'items'} couldn't be decrypted and {s.undecryptable === 1 ? 'was' : 'were'} skipped.</p>{/if}
    <div class="mt-1 flex flex-wrap items-center gap-2">
      <Button variant="outline" size="sm" disabled={s.phase === 'syncing'} onclick={() => void syncNow()}>
        <RefreshCw class={cn('size-3.5 shrink-0', s.phase === 'syncing' && 'animate-spin')} /> Sync now
      </Button>
      <Button variant="outline" size="sm" onclick={() => void lock()}>
        <LockKeyhole class="size-3.5 shrink-0" /> Lock on this device
      </Button>
      <Button variant="ghost" size="sm" class="text-muted-foreground hover:text-destructive" onclick={() => (form = 'reset')}>
        <RotateCcw class="size-3.5 shrink-0" /> Reset sync…
      </Button>
    </div>
  {/if}
</section>
