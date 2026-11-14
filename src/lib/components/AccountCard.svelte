<script>
  // Account-first block for the license page and settings: who is signed in,
  // the plan, devices, Manage account and Sign out; or the sign-in buttons.
  import { account, planLabel, signOut, openManageAccount, signIn, signInState } from '$lib/stores/account.js'
  import AuthProviderIcon from './AuthProviderIcon.svelte'
  import { licenseStatus } from '$lib/stores/license.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import AccountAvatar from './AccountAvatar.svelte'
  import AccountSignInButtons from './AccountSignInButtons.svelte'
  import { cn } from '$lib/utils.js'
  import ExternalLink from '@lucide/svelte/icons/external-link'

  let {
    class: className = '',
    /** `card` on the license page; `row` in Settings, laid out like the rows around it. */
    variant = /** @type {'card' | 'row'} */ ('card'),
  } = $props()

  const idle = $derived($signInState.phase !== 'waiting')
  const rowCls = 'flex items-center justify-between gap-6 border-t border-border/25 py-3.5 first:border-t-0'

  const signedIn = $derived(!!$account?.signed_in)
  const user = $derived($account?.user ?? null)
  const plan = $derived(planLabel($account, $licenseStatus))
  const devices = $derived($account?.devices?.length ?? 0)
  let signingOut = $state(false)

  const LINK_ERRORS = /** @type {Record<string, string>} */ ({
    key_owned_elsewhere: 'The license on this device was bought with another account, so this account is on the free trial. Sign in with the account that bought it to use it here.',
    revoked: 'The license key on this device has been revoked.',
    not_found: 'The license key on this device was not found on the server.',
    invalid_key: 'The license key on this device could not be linked.',
  })

  async function handleSignOut() {
    if (signingOut) return
    signingOut = true
    const r = await signOut()
    signingOut = false
    if (!r.ok) toast.error('Could not sign out', { description: r.error })
  }
</script>

{#snippet planPill()}
  <span
    class={cn(
      'shrink-0 rounded-full border px-2 py-0.5 text-ui-2xs font-medium',
      plan.tone === 'pro' && 'border-success/25 bg-success/10 text-success',
      plan.tone === 'trial' && 'border-warning/25 bg-warning/10 text-warning',
      plan.tone === 'free' && 'border-border/70 bg-muted/40 text-muted-foreground',
    )}
  >
    {plan.label}
  </span>
{/snippet}

{#if variant === 'row'}
  <!-- Settings: one row like its neighbours (label left, actions right). The
       boxed card with two full-width buttons sat in the list like a dialog. -->
  {#if signedIn}
    <div class={cn(rowCls, className)}>
      <div class="flex min-w-0 items-center gap-3">
        <AccountAvatar {user} class="size-8 text-ui-2xs" />
        <div class="min-w-0">
          <p class="flex items-center gap-2 text-ui-sm font-medium text-foreground">
            <span class="truncate">{user?.name || user?.email || 'Signed in'}</span>
            {@render planPill()}
          </p>
          <p class="mt-0.5 truncate text-ui-xs text-muted-foreground">
            {#if user?.name}{user.email} · {/if}<span class="font-mono tabular-nums">{devices}</span> {devices === 1 ? 'device' : 'devices'}{#if $account?.offline} · offline{/if}
          </p>
        </div>
      </div>
      <div class="flex shrink-0 items-center gap-1">
        <Button variant="outline" size="sm" onclick={() => void openManageAccount()}>
          Manage <ExternalLink class="size-3.5 shrink-0" />
        </Button>
        <Button variant="ghost" size="sm" disabled={signingOut} onclick={() => void handleSignOut()}>
          {signingOut ? 'Signing out…' : 'Sign out'}
        </Button>
      </div>
    </div>
    {#if $account?.link_error}
      <p class="-mt-1 pb-2 text-ui-xs leading-relaxed text-muted-foreground">{LINK_ERRORS[$account.link_error] ?? 'The license key on this device could not be linked.'}</p>
    {/if}
  {:else}
    <div class={cn(rowCls, className)}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Stroke account</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          {$account?.has_license_key
            ? 'Your key keeps working. Sign in to sync connections across devices.'
            : 'Your license and trial come with you to every device you sign in on.'}
        </p>
      </div>
      {#if idle}
        <div class="flex shrink-0 items-center gap-1.5">
          <Button variant="outline" size="sm" class="gap-2" onclick={() => void signIn('github')}>
            <AuthProviderIcon name="github" class="size-3.5 shrink-0" />GitHub
          </Button>
          <Button variant="outline" size="sm" class="gap-2" onclick={() => void signIn('google')}>
            <AuthProviderIcon name="google" class="size-3.5 shrink-0" />Google
          </Button>
        </div>
      {/if}
    </div>
    <!-- Waiting (cancel, copy link, paste code) and errors need the room. -->
    {#if !idle || $signInState.phase === 'error'}
      <AccountSignInButtons size="sm" class="pb-3" />
    {/if}
  {/if}
{:else}
<div class={cn('flex flex-col gap-3 rounded-lg border border-border bg-panel p-4', className)}>
  {#if signedIn}
    <div class="flex items-center gap-3">
      <AccountAvatar {user} class="size-8 text-ui-2xs" />
      <div class="min-w-0 flex-1">
        <p class="truncate text-ui-sm font-medium text-foreground">{user?.name || user?.email || 'Signed in'}</p>
        {#if user?.name}<p class="truncate text-ui-xs text-muted-foreground">{user.email}</p>{/if}
      </div>
      {@render planPill()}
    </div>
    <p class="text-ui-xs text-muted-foreground">
      <span class="font-mono tabular-nums">{devices}</span> {devices === 1 ? 'device' : 'devices'} signed in{#if $account?.offline}. Offline, showing the last saved plan{/if}.
    </p>
    {#if $account?.link_error}
      <p class="text-ui-xs leading-relaxed text-muted-foreground">{LINK_ERRORS[$account.link_error] ?? 'The license key on this device could not be linked.'}</p>
    {/if}
    <div class="flex items-center gap-2">
      <Button variant="outline" size="sm" onclick={() => void openManageAccount()}>
        Manage account <ExternalLink class="size-3.5 shrink-0" />
      </Button>
      <Button variant="ghost" size="sm" disabled={signingOut} onclick={() => void handleSignOut()}>
        {signingOut ? 'Signing out…' : 'Sign out'}
      </Button>
    </div>
  {:else}
    <div class="flex flex-col gap-0.5">
      <p class="text-ui-sm font-medium text-foreground">Sign in to Stroke</p>
      <p class="text-ui-xs leading-relaxed text-muted-foreground">
        {$account?.has_license_key
          ? 'Your license key keeps working. Sign in to sync your connections; your license comes with you.'
          : 'Your license and trial come with your account, on every device you sign in on.'}
      </p>
    </div>
    <AccountSignInButtons size="sm" />
  {/if}
</div>
{/if}
