<script>
  // The title bar's app menu (the gear at the right): who you are, then
  // Settings, Theme and shortcuts, then the help links. Account details used to
  // sit in the status bar; this is the one place for them now.
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { account, signInState, planLabel, signInWithToast } from '$lib/stores/account.js'
  import { licenseStatus } from '$lib/stores/license.js'
  import { syncStatus, syncLabel } from '$lib/sync/runtime.js'
  import AccountAvatar from './AccountAvatar.svelte'
  import PasteCodeDialog from './PasteCodeDialog.svelte'
  import { cn } from '$lib/utils.js'
  import Settings from '@lucide/svelte/icons/settings'
  import Palette from '@lucide/svelte/icons/palette'
  import Keyboard from '@lucide/svelte/icons/keyboard'
  import Bug from '@lucide/svelte/icons/bug'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import LogIn from '@lucide/svelte/icons/log-in'
  import Info from '@lucide/svelte/icons/info'
  import CircleUserRound from '@lucide/svelte/icons/circle-user-round'
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

</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger
    class={cn(
      'group relative inline-flex size-7 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-colors duration-[120ms] hover:text-foreground data-[state=open]:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring',
      className,
    )}
    title="Account and settings"
    aria-label="Account and settings"
  >
    {#if waiting}
      <LoaderCircle class="size-3.5 shrink-0 animate-spin" />
    {:else if signedIn && user}
      <AccountAvatar {user} class="size-[calc(22px*var(--app-scale))] transition-[scale,filter] duration-150 ease-out group-hover:brightness-110 group-hover:ring-2 group-hover:ring-foreground/15 group-active:scale-90 group-data-[state=open]:ring-2 group-data-[state=open]:ring-ring/50" />
    {:else}
      <Settings class="size-[calc(17px*var(--app-scale))] shrink-0 transition-[scale] duration-150 ease-out group-active:scale-90" />
    {/if}
  </DropdownMenu.Trigger>

  <DropdownMenu.Content align="end" class="chrome-font min-w-60">
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
      <CircleUserRound class="size-3.5 shrink-0" />
      <span data-slot="menu-label">Account and plan</span>
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
      <DropdownMenu.Shortcut command="theme.pick" />
    </DropdownMenu.Item>
    <DropdownMenu.Item onSelect={run('shortcuts')}>
      <Keyboard class="size-3.5 shrink-0" />
      <span data-slot="menu-label">Keyboard shortcuts</span>
      <DropdownMenu.Shortcut command="help.shortcuts" />
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
    <!-- Sign out lives with the account (Account and license, Settings ▸
         Account): a menu that opens on every click of the avatar is no place
         for a one-way action. -->
    <DropdownMenu.Item onSelect={run('about')}>
      <Info class="size-3.5 shrink-0" />
      <span data-slot="menu-label">About Stroke</span>
    </DropdownMenu.Item>
  </DropdownMenu.Content>
</DropdownMenu.Root>

<PasteCodeDialog bind:open={pasteOpen} />
