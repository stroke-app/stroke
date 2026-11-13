<script>
  // Status-bar account control: avatar + plan, and a menu with the email, plan,
  // devices, "Manage account" and "Sign out". Signed out, it offers "Sign in"
  // and keeps a way to the license page (where a key is entered or managed).
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { account, signInState, planLabel, signOut, signInWithToast, openManageAccount, refreshAccount } from '$lib/stores/account.js'
  import { licenseStatus } from '$lib/stores/license.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import AccountAvatar from './AccountAvatar.svelte'
  import { cn } from '$lib/utils.js'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import LogOut from '@lucide/svelte/icons/log-out'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import LogIn from '@lucide/svelte/icons/log-in'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import Laptop from '@lucide/svelte/icons/laptop'
  import UserRound from '@lucide/svelte/icons/user-round'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import Cloud from '@lucide/svelte/icons/cloud'
  import CloudOff from '@lucide/svelte/icons/cloud-off'
  import ClipboardPaste from '@lucide/svelte/icons/clipboard-paste'
  import { syncStatus, syncLabel } from '$lib/sync/runtime.js'
  import PasteCodeDialog from './PasteCodeDialog.svelte'

  let {
    /** Opens the license page (key entry / management). */
    onopenlicense = () => {},
    /** Trigger classes from the host bar, so the control matches its neighbours. */
    triggerClass = '',
  } = $props()

  const signedIn = $derived(!!$account?.signed_in)
  const plan = $derived(planLabel($account, $licenseStatus))
  const user = $derived($account?.user ?? null)
  const devices = $derived($account?.devices?.length ?? 0)
  const waiting = $derived($signInState.phase === 'waiting')
  let pasteOpen = $state(false)
  const syncOk = $derived($syncStatus.phase === 'ready' || $syncStatus.phase === 'syncing')

  async function handleSignOut() {
    const r = await signOut()
    if (r.ok) toast.success('Signed out', { description: 'Your local data and any license key stay on this device.' })
    else toast.error('Could not sign out', { description: r.error })
  }
</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger
    class={cn(triggerClass, 'text-muted-foreground')}
    title={signedIn ? `${user?.email ?? 'Account'} · ${plan.label}` : 'Account'}
    aria-label="Account"
  >
    {#if waiting}
      <LoaderCircle class="size-3 shrink-0 animate-spin" />
    {:else if signedIn}
      <AccountAvatar {user} class="size-4" />
    {:else}
      <UserRound class="size-3 shrink-0" />
    {/if}
    <span
      class={cn(
        'font-medium @max-[900px]/sb:hidden',
        plan.tone === 'pro' && 'text-success',
        plan.tone === 'trial' && 'text-warning',
      )}
    >
      {plan.tone === 'trial' ? plan.label.replace('Trial · ', '').replace(/ days?$/, 'd trial') : plan.label}
    </span>
  </DropdownMenu.Trigger>

  <DropdownMenu.Content side="top" align="end" class="min-w-56">
    {#if signedIn && !user}
      <!-- Signed in, but the account couldn't be loaded or verified: say so
           and offer a retry, rather than an anonymous "?" profile. -->
      <div class="flex flex-col gap-1 px-1.5 py-1.5">
        <p class="text-ui-xs font-medium text-foreground">Couldn't load your account</p>
        <p class="text-ui-2xs leading-relaxed text-muted-foreground">{$account?.offline ? 'Stroke is offline.' : ($account?.error ?? 'The server answer could not be verified.')}</p>
      </div>
      <DropdownMenu.Separator />
      <DropdownMenu.Item onSelect={() => void refreshAccount()}>
        <RefreshCw class="size-3.5 shrink-0" />
        <span data-slot="menu-label">Try again</span>
      </DropdownMenu.Item>
      <DropdownMenu.Item onSelect={onopenlicense}>
        <KeyRound class="size-3.5 shrink-0" />
        <span data-slot="menu-label">License</span>
      </DropdownMenu.Item>
      <DropdownMenu.Separator />
      <DropdownMenu.Item onSelect={() => void handleSignOut()}>
        <LogOut class="size-3.5 shrink-0" />
        <span data-slot="menu-label">Sign out</span>
      </DropdownMenu.Item>
    {:else if signedIn}
      <div class="flex items-center gap-2 px-1.5 py-1.5">
        <AccountAvatar {user} class="size-7" />
        <div class="min-w-0">
          {#if user?.name}<p class="truncate text-ui-xs font-medium text-foreground">{user.name}</p>{/if}
          <p class="truncate text-ui-2xs text-muted-foreground">{user?.email ?? ''}</p>
        </div>
      </div>
      <DropdownMenu.Separator />
      <div class="flex flex-col gap-1 px-1.5 py-1 text-ui-2xs">
        <div class="flex items-center justify-between gap-3">
          <span class="text-muted-foreground">Plan</span>
          <span class="font-medium text-foreground">{plan.label}</span>
        </div>
        <div class="flex items-center justify-between gap-3">
          <span class="flex items-center gap-1.5 text-muted-foreground"><Laptop class="size-3 shrink-0" /> Devices</span>
          <span class="font-mono tabular-nums text-foreground">{devices}</span>
        </div>
        <div class="flex items-center justify-between gap-3" role="status">
          <span class="flex items-center gap-1.5 text-muted-foreground">
            {#if syncOk}<Cloud class="size-3 shrink-0" />{:else}<CloudOff class="size-3 shrink-0" />{/if} Sync
          </span>
          <span class={cn('text-foreground', $syncStatus.phase === 'locked' && 'text-warning', $syncStatus.phase === 'error' && 'text-destructive')}>{syncLabel($syncStatus)}</span>
        </div>
        {#if $account?.offline}
          <p class="text-muted-foreground">Offline, showing the last saved plan.</p>
        {/if}
      </div>
      <DropdownMenu.Separator />
      <DropdownMenu.Item onSelect={() => void openManageAccount()}>
        <ExternalLink class="size-3.5 shrink-0" />
        <span data-slot="menu-label">Manage account</span>
      </DropdownMenu.Item>
      <DropdownMenu.Item onSelect={onopenlicense}>
        <KeyRound class="size-3.5 shrink-0" />
        <span data-slot="menu-label">License</span>
      </DropdownMenu.Item>
      <DropdownMenu.Separator />
      <DropdownMenu.Item onSelect={() => void handleSignOut()}>
        <LogOut class="size-3.5 shrink-0" />
        <span data-slot="menu-label">Sign out</span>
      </DropdownMenu.Item>
    {:else}
      <div class="flex flex-col gap-0.5 px-1.5 py-1.5">
        <p class="text-ui-xs font-medium text-foreground">Stroke {plan.label}</p>
        <p class="text-ui-2xs leading-relaxed text-muted-foreground">Sign in to keep your license with your account.</p>
      </div>
      <DropdownMenu.Separator />
      <DropdownMenu.Item disabled={waiting} onSelect={() => void signInWithToast()}>
        <LogIn class="size-3.5 shrink-0" />
        <span data-slot="menu-label">{waiting ? 'Waiting for the browser…' : 'Sign in'}</span>
      </DropdownMenu.Item>
      {#if waiting}
        <DropdownMenu.Item onSelect={() => (pasteOpen = true)}>
          <ClipboardPaste class="size-3.5 shrink-0" />
          <span data-slot="menu-label">Paste sign-in code…</span>
        </DropdownMenu.Item>
      {/if}
      <DropdownMenu.Item onSelect={onopenlicense}>
        <KeyRound class="size-3.5 shrink-0" />
        <span data-slot="menu-label">{$licenseStatus?.status === 'Valid' ? 'License' : 'Enter license key'}</span>
      </DropdownMenu.Item>
    {/if}
  </DropdownMenu.Content>
</DropdownMenu.Root>

<PasteCodeDialog bind:open={pasteOpen} />
