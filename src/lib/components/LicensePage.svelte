<script>
  import { licenseStatus, deactivateLicense } from '$lib/stores/license.js'
  import LicenseActivation from './LicenseActivation.svelte'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import BadgeCheck from '@lucide/svelte/icons/badge-check'
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
</script>

<div class="app-scroll flex min-h-0 flex-1 flex-col overflow-y-auto bg-background">
  <div class="mx-auto flex w-full max-w-md flex-col gap-8 px-6 py-14">
    <!-- Header -->
    <div class="flex flex-col items-start gap-4">
      <div class="flex size-11 items-center justify-center rounded-xl border border-warning/20 bg-warning/10">
        <KeyRound class="size-5 text-warning" />
      </div>
      <div class="flex flex-col gap-1">
        <h1 class="text-ui-lg font-semibold tracking-tight text-foreground">Stroke Pro</h1>
        <p class="flex items-center gap-1.5 text-ui-sm text-muted-foreground">
          <span
            class="size-1.5 shrink-0 rounded-full {statusLine.tone === 'good'
              ? 'bg-success'
              : statusLine.tone === 'trial'
                ? 'bg-warning'
                : statusLine.tone === 'warn'
                  ? 'bg-destructive'
                  : 'bg-muted-foreground/40'}"
          ></span>
          {statusLine.text}
        </p>
      </div>
    </div>

    {#if status?.status === 'Valid' && !activatedHere}
      <!-- Active license -->
      <div class="flex flex-col gap-3 rounded-xl border border-border/70 bg-card p-4">
        <div class="flex items-center gap-2 text-ui-sm font-medium text-foreground">
          <BadgeCheck class="size-4 text-success" />
          License active
        </div>
        <dl class="flex flex-col gap-1.5 text-ui-xs">
          <div class="flex items-baseline justify-between gap-3">
            <dt class="text-muted-foreground">Email</dt>
            <dd class="min-w-0 truncate font-mono text-foreground">{status.email}</dd>
          </div>
          <div class="flex items-baseline justify-between gap-3">
            <dt class="text-muted-foreground">Plan</dt>
            <dd class="font-mono capitalize text-foreground">{status.plan}</dd>
          </div>
          {#if status.issued_at}
            <div class="flex items-baseline justify-between gap-3">
              <dt class="text-muted-foreground">Activated</dt>
              <dd class="font-mono text-foreground">{formatDate(status.issued_at)}</dd>
            </div>
          {/if}
        </dl>
        <button
          type="button"
          disabled={deactivating}
          onclick={() => void handleDeactivate()}
          class="mt-1 self-start text-ui-xs text-muted-foreground underline-offset-2 transition-colors hover:text-destructive hover:underline disabled:opacity-50"
        >
          {deactivating ? 'Removing…' : 'Remove license from this device'}
        </button>
      </div>
    {:else}
      <!-- Key entry -->
      <div class="flex flex-col gap-2">
        <div class="flex flex-col gap-0.5">
          <p class="text-ui-sm font-medium text-foreground">Enter your license key</p>
          <p class="text-ui-xs text-muted-foreground">Sent to your email after purchase.</p>
        </div>
        <LicenseActivation naked onactivated={() => onactivated()} />
      </div>
    {/if}

    <!-- What's included -->
    <div class="flex flex-col gap-3">
      <p class="text-ui-2xs font-medium uppercase tracking-wider text-muted-foreground">What Pro unlocks</p>
      <ul class="grid grid-cols-2 gap-x-4 gap-y-2.5">
        {#each features as f (f.label)}
          <li class="flex items-center gap-2 text-ui-xs text-muted-foreground">
            <f.icon class="size-3.5 shrink-0 text-muted-foreground" />
            {f.label}
          </li>
        {/each}
      </ul>
    </div>

    {#if status?.status !== 'Valid'}
      <!-- Purchase -->
      <div class="flex items-center justify-between gap-3 rounded-xl border border-border/70 bg-muted/20 px-4 py-3">
        <p class="text-ui-xs text-muted-foreground">No license yet?</p>
        <button
          type="button"
          onclick={() => void openStore()}
          class= "field-surface inline-flex h-7 items-center gap-1.5 bg-background px-3 text-ui-xs font-medium text-foreground transition-colors hover:bg-muted"
        >
          Get Stroke Pro
          <ExternalLink class="size-3 text-muted-foreground" />
        </button>
      </div>
    {/if}
  </div>
</div>
