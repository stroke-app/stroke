<script>
  import { licenseStatus, deactivateLicense } from '$lib/stores/license.js'
  import LicenseActivation from './LicenseActivation.svelte'
  import AccountAvatar from './AccountAvatar.svelte'
  import AccountSignInButtons from './AccountSignInButtons.svelte'
  import SyncSection from './SyncSection.svelte'
  import { account, planLabel, offlineGraceNote, signOut, openManageAccount, refreshAccount } from '$lib/stores/account.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { cn } from '$lib/utils.js'
  import Laptop from '@lucide/svelte/icons/laptop'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import Bot from '@lucide/svelte/icons/bot'
  import LayoutDashboard from '@lucide/svelte/icons/layout-dashboard'
  import Code2 from '@lucide/svelte/icons/code-2'
  import LayoutTemplate from '@lucide/svelte/icons/layout-template'
  import BarChart2 from '@lucide/svelte/icons/bar-chart-2'
  import Workflow from '@lucide/svelte/icons/workflow'
  import DatabaseBackup from '@lucide/svelte/icons/database-backup'
  import NotebookPen from '@lucide/svelte/icons/notebook-pen'

  let {
    /** Called after a successful activation (once the success state has shown). */
    onactivated = () => {},
  } = $props()

  const status = $derived($licenseStatus)

  /**
   * True when activation happened on this page. The instant a key activates,
   * the license store flips to Valid - without this flag the page would swap
   * the key-entry UI for the "License active" card mid-celebration, cutting
   * off the success banner and confetti. Keeping the activation UI mounted
   * lets the moment play out; onactivated then closes the tab.
   */
  let activatedHere = $state(false)
  let sawNonValid = false
  $effect(() => {
    const s = $licenseStatus?.status
    if (s && s !== 'Valid') {
      sawNonValid = true
    } else if (s === 'Valid' && sawNonValid && !activatedHere) {
      activatedHere = true
      // Page-owned center volley, alongside LicenseActivation's side bursts.
      void firePageConfetti()
    }
  })

  async function firePageConfetti() {
    try {
      const { default: confetti } = await import('canvas-confetti')
      confetti({
        origin: { x: 0.5, y: 0.4 },
        spread: 100,
        particleCount: 140,
        startVelocity: 38,
        decay: 0.92,
        scalar: 1.05,
        ticks: 220,
      })
    } catch {
      /* celebration is best-effort */
    }
  }

  const statusLine = $derived.by(() => {
    if (status?.status === 'Valid') return { tone: 'good', text: `Licensed to ${status.email}` }
    if (status?.status === 'Trial') {
      const d = status.days_remaining
      return { tone: 'trial', text: `Trial, ${d} ${d === 1 ? 'day' : 'days'} remaining` }
    }
    if (status?.status === 'TrialExpired') return { tone: 'warn', text: 'Trial expired, activate a license to keep Pro features' }
    return { tone: 'free', text: 'Free plan' }
  })

  /** What a license unlocks - mirrors PRO_FEATURES in $lib/stores/license.js. */
  const features = [
    { icon: Bot, label: 'AI assistant' },
    { icon: LayoutDashboard, label: 'Dashboards' },
    { icon: Code2, label: 'ORM runner' },
    { icon: LayoutTemplate, label: 'Schema explorer' },
    { icon: BarChart2, label: 'Charts & diagrams' },
    { icon: Workflow, label: 'Schema diagram' },
    { icon: DatabaseBackup, label: 'Backup & restore' },
    { icon: NotebookPen, label: 'SQL notebooks' },
  ]

  let deactivating = $state(false)
  /** The key field is secondary to the account; it opens on request. */
  let showKeyEntry = $state(false)

  async function handleDeactivate() {
    if (deactivating) return
    deactivating = true
    const result = await deactivateLicense()
    deactivating = false
    if (result.ok) toast.success('License removed from this device')
    else toast.error('Could not deactivate', { description: result.error })
  }

  async function openStore() {
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl('https://stroke.click')
    } catch {
      window.open('https://stroke.click', '_blank', 'noopener,noreferrer')
    }
  }

  /** @param {number} secs */
  function formatDate(secs) {
    try {
      return new Date(secs * 1000).toLocaleDateString('en-US', { year: 'numeric', month: 'long', day: 'numeric' })
    } catch {
      return ''
    }
  }

  // ── Account ──────────────────────────────────────────────────────────────
  const signedIn = $derived(!!$account?.signed_in)
  const user = $derived($account?.user ?? null)
  const plan = $derived(planLabel($account, $licenseStatus))
  const graceNote = $derived(offlineGraceNote($account))
  const devices = $derived($account?.devices ?? [])
  let signingOut = $state(false)
  async function handleSignOut() {
    if (signingOut) return
    signingOut = true
    const r = await signOut()
    signingOut = false
    if (!r.ok) toast.error('Could not sign out', { description: r.error })
  }
  const LINK_ERRORS = /** @type {Record<string, string>} */ ({
    key_owned_elsewhere: 'The license key on this device belongs to another account. It still works here.',
    revoked: 'The license key on this device has been revoked.',
    not_found: 'The license key on this device was not found on the server.',
    invalid_key: 'The license key on this device could not be linked.',
  })
  /** @param {number} secs */
  /** macos → macOS, the way the OS names itself. @param {string | null | undefined} os */
  function osName(os) {
    const k = String(os ?? '').toLowerCase()
    return ({ macos: 'macOS', windows: 'Windows', linux: 'Linux', ios: 'iOS', android: 'Android' })[k] ?? (os ?? '')
  }

  function ago(secs) {
    const d = Math.max(0, Date.now() / 1000 - secs)
    if (d < 120) return 'just now'
    if (d < 3600) return `${Math.floor(d / 60)} min ago`
    if (d < 86400) return `${Math.floor(d / 3600)} h ago`
    return `${Math.floor(d / 86400)} d ago`
  }
  const sectionLabel = 'text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground/60'
</script>

<div class="app-scroll flex min-h-0 flex-1 flex-col overflow-y-auto bg-background">
  <div class="mx-auto flex w-full max-w-xl flex-col gap-10 px-8 py-12">
    <!-- Header -->
    <header class="flex flex-col gap-1.5">
      <h1 class="text-ui-xl font-semibold tracking-tight text-foreground">Account</h1>
      <p class="flex items-center gap-2 text-ui-sm text-muted-foreground">
        <span class={cn('size-1.5 shrink-0 rounded-full', statusLine.tone === 'good' ? 'bg-success' : statusLine.tone === 'trial' ? 'bg-warning' : statusLine.tone === 'warn' ? 'bg-destructive' : 'bg-muted-foreground/40')}></span>
        {statusLine.text}
      </p>
    </header>

    <!-- Profile -->
    <section class="flex flex-col gap-3">
      <h2 class={sectionLabel}>Profile</h2>
      {#if signedIn && user}
        <div class="flex items-center gap-3">
          <AccountAvatar {user} class="size-10 text-ui-xs" />
          <div class="min-w-0 flex-1">
            <p class="truncate text-ui-sm font-medium text-foreground">{user.name || user.email}</p>
            {#if user.name}<p class="truncate text-ui-xs text-muted-foreground">{user.email}</p>{/if}
          </div>
          <span class={cn('shrink-0 rounded-md px-2 py-0.5 text-ui-2xs font-medium', plan.tone === 'pro' ? 'bg-success/10 text-success' : plan.tone === 'trial' ? 'bg-warning/10 text-warning' : 'bg-muted text-muted-foreground')}>{plan.label}</span>
        </div>
        {#if graceNote}<p class="text-ui-xs leading-relaxed text-warning">{graceNote.text}</p>
        {:else if $account?.offline}<p class="text-ui-xs text-muted-foreground">Offline. Showing the last saved plan.</p>{/if}
        {#if $account?.link_error}<p class="text-ui-xs leading-relaxed text-muted-foreground">{LINK_ERRORS[$account.link_error] ?? 'The license key on this device could not be linked.'}</p>{/if}
        <div class="flex items-center gap-1">
          <Button variant="secondary" size="sm" onclick={() => void openManageAccount()}>Manage account <ExternalLink class="size-3.5 shrink-0" /></Button>
          <Button variant="ghost" size="sm" class="text-muted-foreground" disabled={signingOut} onclick={() => void handleSignOut()}>{signingOut ? 'Signing out…' : 'Sign out'}</Button>
        </div>
      {:else if signedIn}
        <div class="flex items-start justify-between gap-3">
          <div class="flex flex-col gap-0.5">
            <p class="text-ui-sm text-foreground">Couldn't load your account</p>
            <p class="text-ui-xs text-muted-foreground">{$account?.error ?? 'The server answer could not be verified.'}</p>
          </div>
          <div class="flex shrink-0 items-center gap-1">
            <Button variant="secondary" size="sm" onclick={() => void refreshAccount()}><RefreshCw class="size-3.5 shrink-0" /> Try again</Button>
            <Button variant="ghost" size="sm" class="text-muted-foreground" onclick={() => void handleSignOut()}>Sign out</Button>
          </div>
        </div>
      {:else}
        <div class="flex flex-col gap-3">
          <p class="text-ui-sm leading-relaxed text-muted-foreground">
            {$account?.has_license_key ? 'Your license key keeps working. Sign in to sync your connections; your license comes with you.' : 'Your license and trial come with your account, on every device you sign in on.'}
          </p>
          <AccountSignInButtons size="sm" />
        </div>
      {/if}
    </section>

    {#if signedIn && devices.length}
      <section class="flex flex-col gap-2">
        <h2 class={sectionLabel}>Devices · {devices.length}</h2>
        <ul class="flex flex-col">
          {#each devices as d (d.device_id)}
            <li class="flex h-10 items-center gap-3 border-b border-border/40 last:border-b-0">
              <Laptop class="size-4 shrink-0 text-muted-foreground" />
              <div class="flex min-w-0 flex-1 items-center gap-2">
                <span class="min-w-0 truncate text-ui-sm text-foreground">{d.name || 'Unnamed device'}</span>
                {#if d.current}<span class="shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-ui-3xs font-medium text-muted-foreground">This device</span>{/if}
              </div>
              <span class="shrink-0 whitespace-nowrap text-right text-ui-2xs tabular-nums text-muted-foreground">
                {[osName(d.os), d.last_seen_at ? ago(d.last_seen_at) : ''].filter(Boolean).join(' · ')}
              </span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if signedIn}
      <SyncSection {sectionLabel} />
    {/if}

    <!-- License key on this device -->
    <section class="flex flex-col gap-3">
      <h2 class={sectionLabel}>License key</h2>
      {#if status?.status === 'Valid' && !activatedHere}
        <dl class="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-2 text-ui-sm">
          <dt class="text-muted-foreground">Licensed to</dt>
          <dd class="min-w-0 truncate text-foreground">{status.email}</dd>
          <dt class="text-muted-foreground">Plan</dt>
          <dd class="capitalize text-foreground">{status.plan}</dd>
          {#if status.issued_at}
            <dt class="text-muted-foreground">Activated</dt>
            <dd class="text-foreground">{formatDate(status.issued_at)}</dd>
          {/if}
        </dl>
        <Button variant="ghost" size="sm" disabled={deactivating} onclick={() => void handleDeactivate()} class="-ml-2 self-start text-muted-foreground hover:text-destructive">
          <KeyRound class="size-3.5 shrink-0" />
          {deactivating ? 'Removing…' : 'Remove the key from this device'}
        </Button>
      {:else if !showKeyEntry && !activatedHere}
        <div class="flex items-center justify-between gap-3">
          <p class="text-ui-sm text-muted-foreground">{signedIn ? 'Not needed when your account has Pro.' : 'No key on this device.'}</p>
          <div class="flex shrink-0 items-center gap-1">
            <Button variant="ghost" size="sm" onclick={() => (showKeyEntry = true)}><KeyRound class="size-3.5 shrink-0" /> Enter a key</Button>
            {#if status?.status !== 'Valid'}<Button variant="secondary" size="sm" onclick={() => void openStore()}>Get Stroke Pro <ExternalLink class="size-3.5 shrink-0" /></Button>{/if}
          </div>
        </div>
      {:else}
        <div class="flex flex-col gap-2">
          <p class="text-ui-xs text-muted-foreground">The key was emailed to you after purchase.</p>
          <LicenseActivation naked onactivated={() => onactivated()} />
        </div>
      {/if}
    </section>

    <!-- Included with Pro -->
    <section class="flex flex-col gap-3">
      <h2 class={sectionLabel}>Included with Pro</h2>
      <ul class="grid grid-cols-2 gap-x-6 gap-y-2.5">
        {#each features as f (f.label)}
          <li class="flex items-center gap-2 text-ui-sm text-muted-foreground">
            <f.icon class="size-4 shrink-0 text-muted-foreground/70" />
            {f.label}
          </li>
        {/each}
      </ul>
    </section>
  </div>
</div>
