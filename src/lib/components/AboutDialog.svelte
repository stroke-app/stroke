<script>
  import { onMount } from 'svelte'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { licenseStatus, deactivateLicense } from '$lib/stores/license.js'
  import LicenseActivation from './LicenseActivation.svelte'
  import Logo from './Logo.svelte'
  import ShieldCheck from '@lucide/svelte/icons/shield-check'
  import Clock from '@lucide/svelte/icons/clock'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import LogOut from '@lucide/svelte/icons/log-out'
  import ExternalLink from '@lucide/svelte/icons/external-link'

  let { open = $bindable(false), onopenreport = () => {} } = $props()

  let appVersion = $state('—')
  let showActivate = $state(false)
  let deactivating = $state(false)

  onMount(async () => {
    try {
      const { getVersion } = await import('@tauri-apps/api/app')
      appVersion = await getVersion()
    } catch {
      appVersion = 'dev'
    }
  })

  async function handleDeactivate() {
    deactivating = true
    await deactivateLicense()
    deactivating = false
    showActivate = false
  }

  function handleOpenChange(next) {
    open = next
    if (!next) showActivate = false
  }

  const isValid = $derived($licenseStatus?.status === 'Valid')
  const isTrial = $derived($licenseStatus?.status === 'Trial')
  const isExpired = $derived(!!$licenseStatus && !isValid && !isTrial)
</script>

<Dialog.Root {open} onOpenChange={handleOpenChange} closeOnEscape={true}>
  <Dialog.Content class="w-full max-w-[25rem] gap-0 overflow-hidden p-0">

    <!-- Header: the mark, the name, the build. No hero, no pill. -->
    <div class="flex items-center gap-3 px-6 pt-6 pe-12">
      <Logo class="size-9 shrink-0" />
      <div class="min-w-0">
        <Dialog.Title class="text-ui-lg">Stroke</Dialog.Title>
        <p class="mt-1 truncate font-mono text-ui-2xs tabular-nums text-muted-foreground" title={`v${appVersion}`}>v{appVersion}</p>
      </div>
    </div>

    <!-- License -->
    <div class="px-6 pt-5 pb-6">

      {#if !$licenseStatus}
        <!-- Loading -->
        <div class="flex items-center gap-2 text-ui-2xs text-muted-foreground">
          <span class="size-3 animate-spin rounded-full border border-border border-t-muted-foreground inline-block shrink-0"></span>
          Checking license…
        </div>

      {:else if isValid}
        <!-- Licensed -->
        <!-- Key–value rows, spaced not boxed. Deactivate is a quiet action at
             the end of the plan row, not a full-width bar. -->
        <dl class="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-2.5 text-ui-xs">
          <dt class="text-muted-foreground">Plan</dt>
          <dd class="flex min-w-0 items-center gap-2 text-foreground">
            <ShieldCheck class="size-3.5 shrink-0 text-success" aria-hidden="true" />
            <span class="capitalize">{$licenseStatus.plan ?? 'Pro'}</span>
            <span class="text-muted-foreground">·</span>
            <span>{$licenseStatus.expires_at ? `until ${new Date($licenseStatus.expires_at * 1000).toLocaleDateString()}` : 'Lifetime'}</span>
          </dd>
          <dt class="text-muted-foreground">Licensed to</dt>
          <dd class="min-w-0 truncate text-foreground" title={$licenseStatus.email}>{$licenseStatus.email}</dd>
        </dl>
        <button
          type="button"
          onclick={handleDeactivate}
          disabled={deactivating}
          class="-ms-2 mt-4 inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors duration-150 hover:bg-destructive/[0.08] hover:text-destructive focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-40"
        >
          {#if deactivating}
            <span class="size-3 shrink-0 animate-spin rounded-full border border-current/30 border-t-current"></span>
            Deactivating…
          {:else}
            <LogOut class="size-3.5 shrink-0" />
            Deactivate on this computer
          {/if}
        </button>

      {:else if isTrial}
        <!-- Trial active -->
        <div class="flex flex-col gap-3">
          <div class="flex items-center justify-between">
            <span class="flex items-center gap-2 text-ui-xs font-medium text-foreground">
              <Clock class="size-3.5 shrink-0 text-warning" aria-hidden="true" />Free trial
            </span>
            <span class="text-ui-3xs text-muted-foreground">
              {$licenseStatus.days_remaining}d left
            </span>
          </div>
          {#if showActivate}
            <LicenseActivation naked onactivated={() => { showActivate = false; open = false }} />
            <button
              type="button"
              onclick={() => (showActivate = false)}
              class="text-ui-2xs text-muted-foreground hover:text-foreground transition-colors text-center"
            >Cancel</button>
          {:else}
            <button
              type="button"
              onclick={() => (showActivate = true)}
              class= "field-surface flex h-8 w-full items-center justify-center gap-1.5 bg-muted/30 text-ui-2xs font-medium text-foreground/70 transition-colors hover:bg-muted/60 hover:text-foreground"
            >
              <KeyRound class="size-3 shrink-0" />
              Enter license key
            </button>
          {/if}
        </div>

      {:else}
        <!-- Expired / no license -->
        <div class="flex flex-col gap-3">
          <div>
            <p class="text-ui-xs font-medium text-foreground">No active license</p>
            <p class="mt-0.5 text-ui-2xs text-muted-foreground">Free features only.</p>
          </div>
          {#if showActivate}
            <LicenseActivation naked onactivated={() => { showActivate = false; open = false }} />
            <button
              type="button"
              onclick={() => (showActivate = false)}
              class="text-ui-2xs text-muted-foreground hover:text-foreground transition-colors text-center"
            >Cancel</button>
          {:else}
            <button
              type="button"
              onclick={() => (showActivate = true)}
              class= "field-surface flex h-8 w-full items-center justify-center gap-1.5 bg-muted/30 text-ui-2xs font-medium text-foreground/70 transition-colors hover:bg-muted/60 hover:text-foreground"
            >
              <KeyRound class="size-3 shrink-0" />
              Enter license key
            </button>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Footer -->
    <div class="flex items-center justify-between border-t border-border/40 px-6 py-3.5">
      <button
        type="button"
        onclick={() => { open = false; onopenreport() }}
        class="text-ui-2xs text-muted-foreground transition-colors hover:text-foreground"
      >
        Report an issue
      </button>
      <div class="flex items-center gap-3.5">
        <a
          href="https://github.com/stroke-app/stroke"
          target="_blank"
          rel="noopener noreferrer"
          class="inline-flex items-center gap-1.5 text-ui-2xs text-muted-foreground transition-colors hover:text-foreground"
        >
          <svg class="size-3" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M12 2C6.477 2 2 6.477 2 12c0 4.418 2.865 8.166 6.839 9.489.5.092.682-.217.682-.482 0-.237-.009-.868-.014-1.703-2.782.604-3.369-1.34-3.369-1.34-.454-1.156-1.11-1.463-1.11-1.463-.908-.62.069-.608.069-.608 1.003.07 1.531 1.03 1.531 1.03.892 1.529 2.341 1.087 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.11-4.555-4.943 0-1.091.39-1.984 1.029-2.683-.103-.253-.446-1.27.098-2.647 0 0 .84-.269 2.75 1.025A9.578 9.578 0 0 1 12 6.836a9.59 9.59 0 0 1 2.504.337c1.909-1.294 2.747-1.025 2.747-1.025.546 1.377.202 2.394.1 2.647.64.699 1.028 1.592 1.028 2.683 0 3.842-2.339 4.687-4.566 4.935.359.309.678.919.678 1.852 0 1.336-.012 2.415-.012 2.743 0 .267.18.578.688.48C19.138 20.163 22 16.418 22 12c0-5.523-4.477-10-10-10z"/>
          </svg>
          GitHub
        </a>
        <a
          href="https://stroke.click"
          target="_blank"
          rel="noopener noreferrer"
          class="inline-flex items-center gap-1.5 text-ui-2xs text-muted-foreground transition-colors hover:text-foreground"
        >
          <ExternalLink class="size-3 shrink-0" />
          Website
        </a>
      </div>
    </div>

  </Dialog.Content>
</Dialog.Root>
