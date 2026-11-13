<script>
  import { onMount } from 'svelte'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { licenseStatus, deactivateLicense } from '$lib/stores/license.js'
  import LicenseActivation from './LicenseActivation.svelte'
  import Logo from './Logo.svelte'
  import ShieldCheck from '@lucide/svelte/icons/shield-check'
  import Clock from '@lucide/svelte/icons/clock'
  import AlertTriangle from '@lucide/svelte/icons/alert-triangle'
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

    <!-- Hero -->
    <div class="flex flex-col items-center gap-4 px-8 pt-8 pb-6 text-center">
      <div class="flex size-12 items-center justify-center rounded-lg bg-foreground/[0.06] ring-1 ring-border/60">
        <Logo class="size-7" />
      </div>
      <div class="flex flex-col items-center gap-1">
        <h2 class="text-ui-lg font-semibold tracking-tight text-foreground">Stroke</h2>
        <p class="text-ui-2xs text-muted-foreground">Database management, reimagined</p>
        <span class="mt-1 rounded-full border border-border/50 bg-muted/40 px-2.5 py-0.5 font-mono text-ui-3xs text-muted-foreground">
          v{appVersion}
        </span>
      </div>
    </div>

    <!-- Divider -->
    <div class="mx-6 h-px bg-border/50"></div>

    <!-- License -->
    <div class="px-6 py-5">

      {#if !$licenseStatus}
        <!-- Loading -->
        <div class="flex items-center gap-2 text-ui-2xs text-muted-foreground">
          <span class="size-3 animate-spin rounded-full border border-border border-t-muted-foreground inline-block shrink-0"></span>
          Checking license…
        </div>

      {:else if isValid}
        <!-- Licensed -->
        <div class="flex flex-col gap-3">
          <div class="flex items-center gap-2.5">
            <div class="flex size-6 shrink-0 items-center justify-center rounded-full bg-success/12">
              <ShieldCheck class="size-3 shrink-0 text-success" />
            </div>
            <span class="text-ui-xs font-medium text-foreground">Pro license active</span>
          </div>
          <div class="flex flex-col divide-y divide-border/40 rounded-lg border border-border/50 bg-muted/20 text-ui-2xs">
            <div class="flex justify-between px-3 py-2">
              <span class="text-muted-foreground">Email</span>
              <span class="font-mono text-foreground truncate max-w-[160px]">{$licenseStatus.email}</span>
            </div>
            <div class="flex justify-between px-3 py-2">
              <span class="text-muted-foreground">Plan</span>
              <span class="text-foreground capitalize">{$licenseStatus.plan ?? 'Pro'}</span>
            </div>
            <div class="flex justify-between px-3 py-2">
              <span class="text-muted-foreground">Expires</span>
              <span class="text-foreground">
                {$licenseStatus.expires_at
                  ? new Date($licenseStatus.expires_at * 1000).toLocaleDateString()
                  : 'Lifetime'}
              </span>
            </div>
          </div>
          <button
            type="button"
            onclick={handleDeactivate}
            disabled={deactivating}
            class= "field-surface group inline-flex h-8 w-full items-center justify-center gap-1.5 text-ui-xs font-medium text-muted-foreground transition-[color,background-color,border-color,transform] duration-150 ease-[var(--ease-out)] hover:border-destructive/35 hover:bg-destructive/[0.06] hover:text-destructive active:scale-[0.98] disabled:pointer-events-none disabled:opacity-40"
          >
            {#if deactivating}
              <span class="size-3 shrink-0 animate-spin rounded-full border border-current/30 border-t-current"></span>
              Deactivating…
            {:else}
              <LogOut class="size-3.5 shrink-0 opacity-50 transition-opacity duration-150 group-hover:opacity-100" />
              Deactivate
            {/if}
          </button>
        </div>

      {:else if isTrial}
        <!-- Trial active -->
        <div class="flex flex-col gap-3">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-2.5">
              <div class="flex size-6 shrink-0 items-center justify-center rounded-full bg-warning/12">
                <Clock class="size-3 shrink-0 text-warning" />
              </div>
              <span class="text-ui-xs font-medium text-foreground">Free trial</span>
            </div>
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
          <div class="flex items-center gap-2.5">
            <div class="flex size-6 shrink-0 items-center justify-center rounded-full bg-muted/50">
              <AlertTriangle class="size-3 shrink-0 text-muted-foreground" />
            </div>
            <div>
              <p class="text-ui-xs font-medium text-foreground">No active license</p>
              <p class="text-ui-3xs text-muted-foreground">Free features only</p>
            </div>
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
    <div class="flex items-center justify-between border-t border-border/40 px-6 py-3">
      <button
        type="button"
        onclick={() => { open = false; onopenreport() }}
        class="text-ui-3xs text-muted-foreground transition-colors hover:text-foreground"
      >
        Report an issue
      </button>
      <div class="flex items-center gap-3.5">
        <a
          href="https://github.com/stroke-app/stroke"
          target="_blank"
          rel="noopener noreferrer"
          class="inline-flex items-center gap-1 text-ui-3xs text-muted-foreground transition-colors hover:text-foreground"
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
          class="inline-flex items-center gap-1 text-ui-3xs text-muted-foreground transition-colors hover:text-foreground"
        >
          <ExternalLink class="size-3 shrink-0" />
          Website
        </a>
      </div>
    </div>

  </Dialog.Content>
</Dialog.Root>
