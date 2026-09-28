<script>
  // One-flow provider sign-in: authorize (OAuth) or paste a token, list every
  // database on the account, and hand a ready-to-connect spec to the parent.
  // Generalized from CloudflareLogin.svelte across all provider adapters.
  import { onMount, onDestroy } from 'svelte'
  import Check from '@lucide/svelte/icons/check'
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import LogOut from '@lucide/svelte/icons/log-out'
  import AlertTriangle from '@lucide/svelte/icons/alert-triangle'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import ArrowRight from '@lucide/svelte/icons/arrow-right'
  import X from '@lucide/svelte/icons/x'
  import DbIcon from './DbIcon.svelte'
  import ProviderAuthPanel from './ProviderAuthPanel.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import SearchableMenu from './SearchableMenu.svelte'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import Eye from '@lucide/svelte/icons/eye'
  import EyeOff from '@lucide/svelte/icons/eye-off'
  import { cn } from '$lib/utils.js'
  import {
    providerMeta,
    providerStartOAuth,
    providerCancelOAuth,
    providerStoreToken,
    providerOAuthStatus,
    providerLogout,
    providerListDatabases,
    providerBuildConnection,
  } from '$lib/providers.js'

  let {
    /** Provider id: 'neon' | 'supabase' | 'planetscale' | 'prisma' */
    provider,
    /**
     * Called with a ready-to-connect spec once the user picks a database.
     * @type {(conn: import('$lib/providers.js').ProviderConnection) => void}
     */
    onselect = () => {},
    ondisconnect = () => {},
    /**
     * Look up a saved connection for a database by name, so picking a database we
     * already hold credentials for doesn't mint a second set. PlanetScale creates
     * a fresh admin-role branch password on every build_connection call, and
     * Prisma a fresh connection record - going through the picker a dozen times
     * leaves a dozen live credentials behind in the user's account.
     * @type {(dbName: string) => { host: string, user: string, password: string, database: string } | undefined}
     */
    resolveSavedConnection = () => undefined,
    /**
     * Look up a previously-saved password for a database (host + user), so a
     * needs-password provider (Supabase) doesn't prompt again once it's known.
     * @type {(host: string, user: string) => string | undefined}
     */
    resolvePassword = () => undefined,
  } = $props()

  const meta = $derived(providerMeta(provider))

  /** @type {'idle'|'authorizing'|'fetching'|'selecting'|'building'|'password'|'error'} */
  let phase = $state('idle')
  let errorMsg = $state('')
  let tokenInput = $state('')
  /** Resolved spec awaiting a password (providers that don't expose it, e.g. Supabase). */
  let resolved = $state(/** @type {import('$lib/providers.js').ProviderConnection | null} */ (null))
  let pw = $state('')
  let showPw = $state(false)

  /** @type {import('$lib/providers.js').ProviderDatabase[]} */
  let databases = $state([])
  let selectedRef = $state('')

  // `SearchableMenu` owns the query, the filtering and the keyboard: the input
  // autofocuses on open, Arrow keys move the highlight, Enter selects, Esc closes.
  // This file used to hand-roll all of that around an always-open list, which is
  // why the same picker behaved differently here and in the D1 flow.
  //
  // `value` is the db_ref because that is what selection needs, and cmdk scores a
  // row against its value and keywords rather than its rendered content — so the
  // label goes in `keywords` too, or typing a database's name would filter it out.
  const dbItems = $derived(
    databases.map((d) => ({
      value: d.db_ref,
      label: d.name,
      keywords: d.region ? [d.name, d.region] : [d.name],
      region: d.region,
    })),
  )
  const selectedDbName = $derived(databases.find((d) => d.db_ref === selectedRef)?.name ?? '')

  /** Turn a raw backend error into a calm title + one-line explanation. */
  function friendlyError(msg) {
    const m = String(msg ?? '')
    const name = meta?.name ?? 'the provider'
    if (/not signed in/i.test(m))
      return { title: 'Session expired', detail: `Your ${name} sign-in is no longer valid. Sign in again to continue.` }
    if (/timed out/i.test(m))
      return { title: 'Authorization timed out', detail: 'The browser sign-in took too long. Start again when you are ready.' }
    if (/cancel/i.test(m))
      return { title: 'Sign-in cancelled', detail: 'The browser closed before authorizing. Try again to connect.' }
    if (/denied/i.test(m))
      return { title: 'Authorization denied', detail: `${name} declined the request. Try again and approve access.` }
    if (/port .*in use|bind any callback/i.test(m))
      return { title: 'Callback port in use', detail: m }
    if (/non-JSON|proxy|token exchange/i.test(m))
      return { title: 'Sign-in service unavailable', detail: 'Could not reach the sign-in service. Check your network and try again.' }
    return { title: 'Something went wrong', detail: m || 'Please try again.' }
  }
  const shownError = $derived(friendlyError(errorMsg))
  // The backend already cleared an expired or revoked sign-in; retrying the
  // list can only fail again, so the way forward is signing in.
  const sessionExpired = $derived(/not signed in/i.test(errorMsg))

  onMount(async () => {
    try {
      const status = await providerOAuthStatus(provider)
      if (status.connected) {
        phase = 'fetching'
        await loadDatabases()
      }
    } catch { /* stay idle */ }
  })

  // Unmounting mid-authorize (dialog closed, provider switched via {#key}) must
  // free the backend callback port, or the next sign-in attempt hits
  // "Callback port in use" until the 5-minute timeout expires.
  onDestroy(() => {
    if (phase === 'authorizing') void providerCancelOAuth().catch(() => {})
  })

  async function startAuth() {
    phase = 'authorizing'
    errorMsg = ''
    try {
      await providerStartOAuth(provider)
      phase = 'fetching'
      await loadDatabases()
    } catch (e) {
      // User-cancelled aborts quietly back to the start; anything else is an error.
      if (String(e).includes('cancelled')) {
        phase = 'idle'
        errorMsg = ''
      } else {
        phase = 'error'
        errorMsg = String(e)
      }
    }
  }

  /** Abort the in-flight browser auth and return to the start. */
  async function cancelAuth() {
    try { await providerCancelOAuth() } catch { /* ignore */ }
    phase = 'idle'
    errorMsg = ''
  }

  async function saveToken() {
    if (!tokenInput.trim()) return
    phase = 'fetching'
    errorMsg = ''
    try {
      await providerStoreToken(provider, tokenInput.trim())
      await loadDatabases()
    } catch (e) {
      phase = 'error'
      errorMsg = String(e)
    }
  }

  /**
   * Nothing loads forever.
   *
   * Same guard as CloudflareLogin, for the same reason: the shared provider HTTP
   * client had no timeout, so a stalled Neon/Supabase/PlanetScale/Prisma call
   * left this `await` pending and the panel on its spinner with no error and no
   * way back. The client is bounded now; this backstops the rest of the path.
   * @template T
   * @param {Promise<T>} work
   * @param {number} ms
   * @param {string} what
   * @returns {Promise<T>}
   */
  function withTimeout(work, ms, what) {
    /** @type {ReturnType<typeof setTimeout>} */
    let timer
    return Promise.race([
      work,
      new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`Timed out ${what}. Check your connection and try again.`)),
          ms,
        )
      }),
    ]).finally(() => clearTimeout(timer))
  }

  async function loadDatabases() {
    phase = 'fetching'
    errorMsg = ''
    try {
      databases = await withTimeout(
        providerListDatabases(provider),
        20_000,
        `listing your ${meta?.name ?? provider} databases`,
      )
      phase = 'selecting'
    } catch (e) {
      phase = 'error'
      errorMsg = String(e)
    }
  }

  /** Providers whose build_connection *creates* a credential rather than reading one. */
  const MINTS_CREDENTIALS = ['planetscale', 'prisma']

  async function pick(ref) {
    selectedRef = ref
    phase = 'building'
    try {
      // Already hold working credentials for this database? Use them. Minting
      // again would work too, and leave another admin-role password behind.
      if (MINTS_CREDENTIALS.includes(provider)) {
        const dbName = databases.find((d) => d.db_ref === ref)?.name ?? ''
        const known = dbName ? resolveSavedConnection(dbName) : undefined
        if (known) {
          phase = 'selecting'
          onselect({
            db_type: meta?.engine === 'mysql' ? 'mysql' : 'postgres',
            host: known.host,
            port: meta?.engine === 'mysql' ? 3306 : 5432,
            username: known.user,
            password: known.password,
            database: known.database,
            ssl: true,
            needs_password: false,
            name: dbName,
            // Lets the caller mint fresh credentials if these turn out to be
            // revoked provider-side, instead of failing for good.
            reusedSaved: ref,
          })
          return
        }
      }
      const conn = await providerBuildConnection(provider, ref)
      if (conn.needs_password) {
        // Reuse a previously-saved password for this exact database (host + user)
        // so we don't ask again. Otherwise ask inline, then connect.
        const known = resolvePassword(conn.host, conn.username)
        if (known) {
          phase = 'selecting'
          onselect({ ...conn, password: known })
          return
        }
        resolved = conn
        pw = ''
        phase = 'password'
      } else {
        phase = 'selecting'
        onselect(conn)
      }
    } catch (e) {
      phase = 'error'
      errorMsg = String(e)
    }
  }

  /** Merge the entered password into the resolved spec and connect. */
  function confirmPassword() {
    if (!resolved || !pw.trim()) return
    onselect({ ...resolved, password: pw })
  }

  async function handleLogout() {
    await providerLogout(provider)
    phase = 'idle'
    databases = []
    selectedRef = ''
    tokenInput = ''
    resolved = null
    pw = ''
    errorMsg = ''
    ondisconnect()
  }
</script>

<div class="flex flex-col gap-3">
  {#if phase === 'idle'}
    {#if meta?.mode === 'token'}
      <!-- Providers with no OAuth: one field and one button, same frame. -->
      <ProviderAuthPanel
        title="Connect to {meta?.name}"
        subtitle="Paste the connection string from your {meta?.name} dashboard. It is stored locally, in this app only."
        hint="Nothing leaves your machine"
      >
        {#snippet mark()}<DbIcon id={provider} class="size-4 shrink-0" />{/snippet}
        {#snippet action()}
          <Button disabled={!tokenInput.trim()} onclick={saveToken}>
            <KeyRound class="size-3.5 shrink-0" /> Continue
          </Button>
        {/snippet}
      </ProviderAuthPanel>
      <input
        type="text"
        bind:value={tokenInput}
        placeholder="postgres://…"
        aria-label="{meta?.name} connection string"
        class= "field-surface h-9 w-full bg-muted/25 px-3 font-mono text-ui-2xs outline-none transition-[border-color] focus:"
        onkeydown={(e) => { if (e.key === 'Enter') saveToken() }}
      />
    {:else}
      <ProviderAuthPanel
        title="Sign in with {meta?.name}"
        subtitle="Opens your browser to authorize. Stroke receives a scoped token - your {meta?.name} password never passes through it."
        hint="Secure PKCE flow"
      >
        {#snippet mark()}<DbIcon id={provider} class="size-4 shrink-0" />{/snippet}
        {#snippet action()}
          <Button class="group" onclick={startAuth}>
            Sign in
            <ArrowRight class="size-3.5 transition-transform duration-150 ease-out group-hover:translate-x-0.5" />
          </Button>
        {/snippet}
      </ProviderAuthPanel>
    {/if}

  {:else if phase === 'authorizing'}
    <ProviderAuthPanel
      tone="busy"
      progress
      title="Waiting for {meta?.name}…"
      subtitle="Finish authorizing in the browser tab, then come back here."
      hint="Times out in 5 min"
    >
      {#snippet mark()}<DbIcon id={provider} class="size-4 shrink-0" />{/snippet}
      {#snippet action()}
        <Button variant="ghost" size="sm" onclick={cancelAuth}>
          <X class="size-3.5" /> Cancel
        </Button>
      {/snippet}
    </ProviderAuthPanel>

  {:else if phase === 'fetching'}
    <ProviderAuthPanel
      tone="busy"
      progress
      title="Signed in"
      subtitle="Loading your databases…"
      hint={meta?.name ?? ''}
    >
      {#snippet mark()}<Check class="size-4 shrink-0 text-success" />{/snippet}
    </ProviderAuthPanel>

  {:else if phase === 'error'}
    <!-- Retry the step that failed. Dropping back to 'idle' made the user sign
         in through the browser again even when the sign-in was fine and only
         the database list fell over. -->
    <ProviderAuthPanel tone="error" title={shownError.title} subtitle={shownError.detail} hint="Nothing was saved">
      {#snippet mark()}<AlertTriangle class="size-4 shrink-0 text-destructive" />{/snippet}
      {#snippet action()}
        <div class="flex shrink-0 items-center gap-2">
          {#if sessionExpired}
            <Button onclick={startAuth}>Sign in again</Button>
          {:else}
            <Button variant="outline" class="group" onclick={() => loadDatabases()}>
              <RefreshCw class="size-3.5 transition-transform duration-500 ease-[var(--ease-out)] group-hover:rotate-180" />
              Try again
            </Button>
          {/if}
          <Button variant="ghost" class="text-muted-foreground" onclick={() => (phase = 'idle')}>
            Start over
          </Button>
        </div>
      {/snippet}
    </ProviderAuthPanel>

  {:else if phase === 'selecting' || phase === 'building' || phase === 'password'}
    <!-- Connected header -->
    <div class="flex items-center gap-2.5 rounded-lg border border-border/40 bg-muted/[0.04] px-3 py-2.5">
      <div class="flex size-7 shrink-0 items-center justify-center rounded-lg border border-border/40 bg-background">
        <DbIcon id={provider} class="size-4 text-foreground" />
      </div>
      <div class="min-w-0 flex-1">
        <p class="flex items-center gap-1.5 text-ui-xs font-medium text-foreground">
          {meta?.name}
          <span class="inline-flex items-center gap-1 text-ui-3xs font-normal text-success"><Check class="size-3" />Connected</span>
        </p>
        <p class="text-ui-3xs text-muted-foreground">Pick a database to connect</p>
      </div>
      <button
        type="button"
        title="Disconnect"
        class="shrink-0 rounded p-1 text-muted-foreground transition-colors hover:text-destructive"
        onclick={handleLogout}
      >
        <LogOut class="size-3.5" />
      </button>
    </div>

    {#if phase === 'password'}
      <!-- Inline password step, providers that don't expose the DB password -->
      <div class="flex flex-col gap-2.5 rounded-lg border border-border/50 p-3">
        <p class="text-ui-xs text-foreground">
          Database password for <span class="font-medium">{resolved?.name}</span>
        </p>
        <p class="text-ui-2xs leading-relaxed text-muted-foreground">
          {meta?.name} doesn't expose the database password through its API, enter it once.
          Find or reset it in your {meta?.name} dashboard under Database settings.
        </p>
        <div class="relative">
          <!-- svelte-ignore a11y_autofocus -->
          <input
            type={showPw ? 'text' : 'password'}
            bind:value={pw}
            autocomplete="current-password"
            autofocus
            placeholder="Database password"
            class= "field-surface h-9 w-full bg-muted/30 pl-3 pr-9 text-ui-xs outline-none transition-[border-color] focus:"
            onkeydown={(e) => { if (e.key === 'Enter') confirmPassword() }}
          />
          <button
            type="button"
            tabindex={-1}
            aria-label={showPw ? 'Hide password' : 'Show password'}
            class="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground transition-colors hover:text-foreground"
            onclick={() => (showPw = !showPw)}
          >
            {#if showPw}<EyeOff class="size-3.5" />{:else}<Eye class="size-3.5" />{/if}
          </button>
        </div>
        <div class="flex gap-2">
          <button
            type="button"
            class="flex-1 rounded-lg border border-border px-3 py-2 text-center text-ui-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            onclick={() => { phase = 'selecting'; selectedRef = ''; resolved = null }}
          >
            Back
          </button>
          <button
            type="button"
            class="flex-[2] inline-flex items-center justify-center gap-1.5 rounded-lg bg-primary px-3 py-2 text-ui-xs font-semibold text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-40"
            disabled={!pw.trim() || phase === 'building'}
            onclick={confirmPassword}
          >
            {#if phase === 'building'}<Loader2 class="size-3.5 shrink-0 animate-spin" />Connecting…{:else}Connect{/if}
          </button>
        </div>
      </div>

    <!-- Database picker: the same searchable dropdown the D1 flow uses. -->
    {:else if databases.length > 0}
      <div class="flex flex-col gap-1.5">
        <span class="text-ui-3xs font-semibold tracking-[0.06em] text-muted-foreground uppercase">
          {meta?.name} database
        </span>
        <SearchableMenu
          items={dbItems}
          placeholder="Search databases…"
          empty="No matching database"
          contentClass="w-[var(--bits-popover-anchor-width)] min-w-[240px]"
          align="start"
          onselect={(it) => pick(it.value)}
        >
          {#snippet trigger(props)}
            <button
              {...props}
              type="button"
              disabled={phase === 'building'}
              class="field-surface flex h-9 w-full items-center gap-2 bg-muted/25 pl-3 pr-2.5 text-left text-ui-xs transition-[border-color,box-shadow] hover:border-border focus:outline-none disabled:opacity-60 data-[state=open]:border-ring"
            >
              <DbIcon id={provider} class={cn('size-4 shrink-0', selectedDbName ? 'text-foreground' : 'text-muted-foreground')} />
              <span class={cn('min-w-0 flex-1 truncate font-mono', !selectedDbName && 'font-sans text-muted-foreground')}>
                {selectedDbName || '- select database -'}
              </span>
              <!-- The spinner belongs on the trigger now. It used to sit on the
                   selected row, which the dropdown closes over on select, so
                   connecting would have looked like nothing happening. -->
              {#if phase === 'building'}
                <Loader2 class="size-3.5 shrink-0 animate-spin text-primary" />
              {:else}
                <ChevronDown class="size-3.5 shrink-0 text-muted-foreground" />
              {/if}
            </button>
          {/snippet}
          {#snippet item(it)}
            <DbIcon id={provider} class={cn('size-4 shrink-0', it.value === selectedRef ? 'text-foreground' : 'text-muted-foreground')} />
            <span class="min-w-0 flex-1 truncate font-mono leading-snug">{it.label}</span>
            {#if it.region}<span class="shrink-0 text-ui-3xs text-muted-foreground">{it.region}</span>{/if}
            {#if it.value === selectedRef}<Check class="size-3.5 shrink-0 text-primary" />{/if}
          {/snippet}
        </SearchableMenu>
      </div>
    {:else}
      <div class="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border/50 px-4 py-5 text-center">
        <DbIcon id={provider} class="size-5 text-muted-foreground" />
        <p class="text-ui-2xs text-muted-foreground">No databases found on this account.</p>
        <button type="button" class="flex items-center gap-1 text-ui-3xs text-muted-foreground hover:text-muted-foreground" onclick={loadDatabases}>
          <RefreshCw class="size-3" /> Retry
        </button>
      </div>
    {/if}

  {/if}
</div>

