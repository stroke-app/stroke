<script>
  // The title bar's app menu (the gear at the right): who you are, then
  // Settings, Theme and shortcuts, then the help links. Account details used to
  // sit in the status bar; this is the one place for them now.
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { account, signInState, planLabel, signOut, signInWithToast, openManageAccount } from '$lib/stores/account.js'
  import { licenseStatus } from '$lib/stores/license.js'
  import { syncStatus, syncLabel } from '$lib/sync/runtime.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import AccountAvatar from './AccountAvatar.svelte'
  import PasteCodeDialog from './PasteCodeDialog.svelte'
  import { cn } from '$lib/utils.js'
  import Settings from '@lucide/svelte/icons/settings'
  import Palette from '@lucide/svelte/icons/palette'
  import Keyboard from '@lucide/svelte/icons/keyboard'
  import Bug from '@lucide/svelte/icons/bug'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import LogIn from '@lucide/svelte/icons/log-in'
  import LogOut from '@lucide/svelte/icons/log-out'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import UserRound from '@lucide/svelte/icons/user-round'
  import ClipboardPaste from '@lucide/svelte/icons/clipboard-paste'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'

  /** @type {{ actions?: Record<string, () => void>, class?: string }} */
  let { actions = {}, class: className = '' } = $props()

  const signedIn = $derived(!!$account?.signed_in)
  const user = $derived($account?.user ?? null)
  const plan = $derived(planLabel($account, $licenseStatus))
  const waiting = $derived($signInState.phase === 'waiting')
  let pasteOpen = $state(false)

  /** @param {string} name */
  const run = (name) => () => actions[name]?.()

  async function handleSignOut() {
    const r = await signOut()
    if (r.ok) toast.success('Signed out', { description: 'Your local data and any license key stay on this device.' })
    else toast.error('Could not sign out', { description: r.error })
  }
</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger
    class={cn(
      'relative inline-flex size-7 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-[color,scale] duration-[120ms] ease-[var(--ease-out)] hover:text-foreground active:scale-[0.94] data-[state=open]:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring',
      className,
    )}
    title="Account and settings"
    aria-label="Account and settings"
  >
    {#if waiting}
      <LoaderCircle class="size-3.5 shrink-0 animate-spin" />
    {:else if signedIn && user}
      <AccountAvatar {user} class="size-[calc(22px*var(--app-scale))]" />
    {:else}
      <Settings class="size-[calc(17px*var(--app-scale))] shrink-0" />
    {/if}
  </DropdownMenu.Trigger>

  <DropdownMenu.Content align="end" class="min-w-60">
    <!-- Who you are -->
    {#if signedIn && user}
      <div class="flex items-center gap-2.5 px-2 py-1.5">
        <AccountAvatar {user} class="size-7" />
        <div class="min-w-0 flex-1">
          <p class="truncate text-ui-xs font-medium text-foreground">{user.name || user.email}</p>
          <p class="truncate text-ui-2xs text-muted-foreground">
            <span class={cn(plan.tone === 'pro' && 'text-success', plan.tone === 'trial' && 'text-warning')}>{plan.label}</span>
            · {syncLabel($syncStatus)}
          </p>
        </div>
      </div>
      <DropdownMenu.Item onSelect={() => void openManageAccount()}>
        <UserRound class="size-3.5 shrink-0" />
        <span data-slot="menu-label">Manage account</span>
        <ExternalLink class="ml-auto size-3 shrink-0 text-muted-foreground" />
      </DropdownMenu.Item>
    {:else if signedIn}
      <!-- Signed in, profile not loaded (offline, or it failed verification). -->
      <div class="flex items-center gap-2.5 px-2 py-1.5">
        <span class="grid size-7 shrink-0 place-items-center rounded-full bg-muted">
          <UserRound class="size-3.5 shrink-0 text-muted-foreground" />
        </span>
        <div class="min-w-0 flex-1">
          <p class="truncate text-ui-xs font-medium text-foreground">Signed in</p>
          <p class="truncate text-ui-2xs text-muted-foreground">{plan.label} · profile unavailable</p>
        </div>
      </div>
    {:else}
      <DropdownMenu.Item disabled={waiting} onSelect={() => void signInWithToast()}>
        <LogIn class="size-3.5 shrink-0" />
        <span data-slot="menu-label">{waiting ? 'Waiting for the browser…' : 'Sign in'}</span>
        <span data-slot="menu-trailing" class="ml-auto text-ui-2xs text-muted-foreground">{plan.label}</span>
      </DropdownMenu.Item>
      {#if waiting}
        <DropdownMenu.Item onSelect={() => (pasteOpen = true)}>
          <ClipboardPaste class="size-3.5 shrink-0" />
          <span data-slot="menu-label">Paste sign-in code…</span>
        </DropdownMenu.Item>
      {/if}
    {/if}
    <DropdownMenu.Item onSelect={run('account')}>
      <KeyRound class="size-3.5 shrink-0" />
      <span data-slot="menu-label">{signedIn ? 'Account and license' : 'Enter license key'}</span>
    </DropdownMenu.Item>

    <DropdownMenu.Separator />
    <DropdownMenu.Item onSelect={run('openSettings')}>
      <Settings class="size-3.5 shrink-0" />
      <span data-slot="menu-label">Settings</span>
      <DropdownMenu.Shortcut command="settings.open" />
    </DropdownMenu.Item>
    <DropdownMenu.Item onSelect={run('theme')}>
      <Palette class="size-3.5 shrink-0" />
      <span data-slot="menu-label">Theme…</span>
    </DropdownMenu.Item>
    <DropdownMenu.Item onSelect={run('keybindings')}>
      <Keyboard class="size-3.5 shrink-0" />
      <span data-slot="menu-label">Keyboard shortcuts</span>
    </DropdownMenu.Item>

    <DropdownMenu.Separator />
    <DropdownMenu.Item onSelect={run('reportIssue')}>
      <Bug class="size-3.5 shrink-0" />
      <span data-slot="menu-label">Report an issue</span>
    </DropdownMenu.Item>
    <DropdownMenu.Item onSelect={run('changelog')}>
      <ExternalLink class="size-3.5 shrink-0" />
      <span data-slot="menu-label">Changelog</span>
    </DropdownMenu.Item>

    {#if signedIn}
      <DropdownMenu.Separator />
      <DropdownMenu.Item onSelect={() => void handleSignOut()}>
        <LogOut class="size-3.5 shrink-0" />
        <span data-slot="menu-label">Sign out</span>
      </DropdownMenu.Item>
    {/if}
  </DropdownMenu.Content>
</DropdownMenu.Root>

<PasteCodeDialog bind:open={pasteOpen} />
