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
  import ConfirmDialog from './ConfirmDialog.svelte'
  import { Input } from '$lib/components/ui/input/index.js'
  import PasswordInput from './PasswordInput.svelte'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import SearchableMenu from './SearchableMenu.svelte'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import Eye from '@lucide/svelte/icons/eye'
  import EyeOff from '@lucide/svelte/icons/eye-off'
  import { cn } from '$lib/utils.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
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
  import { readProviderList, writeProviderList, clearProviderLists } from '$lib/provider-list-cache.js'

  let {
    /** Provider id: 'neon' | 'supabase' | 'planetscale' | 'prisma' | 'tidb' | 'turso' */
    provider,
    /**
     * Called with a ready-to-connect spec once the user picks a database.
     * @type {(conn: import('$lib/providers.js').ProviderConnection) => void}
     */
    onselect = () => {},
    ondisconnect = () => {},
    /**
     * Look up the saved connection for a database, by the provider's own
     * reference (`db_ref`), so picking it again connects with no API call and
     * mints nothing new. PlanetScale creates a fresh admin-role branch password
     * on every build_connection call, and Prisma a fresh connection record -
     * going through the picker a dozen times left a dozen live credentials
     * behind in the user's account. `name` is only for entries saved before the
     * reference was recorded.
     * @type {(ref: string, name: string) => any}
     */
    resolveSavedConnection = (/** @type {string} */ _ref, /** @type {string} */ _name) => undefined,
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
  const tokenFields = $derived(meta?.token?.fields ?? [])
  /**
   * Every declared field starts as '' rather than missing: a missing key binds
   * `undefined`, which Svelte rejects for a prop with a fallback (PasswordInput's
   * `value = ''`) and the whole panel crashed on open.
   */
  const emptyTokenValues = () =>
    Object.fromEntries((meta?.token?.fields ?? []).map((/** @type {{ key: string }} */ f) => [f.key, '']))
  /** Values of a token provider's own fields (Upstash: email + API key). @type {Record<string, string>} */
  let tokenValues = $state(emptyTokenValues())
  /** Every field filled, or the single legacy field when a provider declares none. */
  const tokenReady = $derived(
    tokenFields.length
      ? tokenFields.every((/** @type {{ key: string }} */ f) => (tokenValues[f.key] ?? '').trim())
      : !!tokenInput.trim(),
  )
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
      kind: d.kind,
    })),
  )
  const selectedDbName = $derived(databases.find((d) => d.db_ref === selectedRef)?.name ?? '')

  /** Turn a raw backend error into a calm title + one-line explanation. */
  function friendlyError(msg) {
    const m = String(msg ?? '')
    const name = meta?.name ?? 'the provider'
    if (/not signed in/i.test(m) && meta?.mode === 'token')
      return { title: `${name} didn't accept those credentials`, detail: 'Check the email and API key, then enter them again.' }
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

  const cacheKey = $derived(`provider:${provider}`)

  onMount(async () => {
    // Open on the list from last time and refresh it underneath. The backend
    // answers "not signed in" on its own when the session is gone, so the status
    // round-trip is only needed when there is nothing to show yet.
    /** @type {typeof databases | undefined} */
    const cached = readProviderList(cacheKey)
    if (cached?.length) {
      databases = cached
      phase = 'selecting'
      void loadDatabases({ quiet: true })
      return
    }
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

  /**
   * Device-code sign-in (TiDB Cloud): the code the browser page asks you to
   * confirm, pushed by the backend the moment the provider issues it.
   * @type {{ userCode: string, verificationUri: string, verificationUriComplete: string } | null}
   */
  let deviceCode = $state(null)
  let codeCopied = $state(false)

  async function copyDeviceCode() {
    if (!deviceCode) return
    try {
      await navigator.clipboard.writeText(deviceCode.userCode)
      codeCopied = true
      setTimeout(() => (codeCopied = false), 1500)
    } catch { /* clipboard blocked: the code is still on screen */ }
  }

  async function reopenDevicePage() {
    if (!deviceCode) return
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl(deviceCode.verificationUriComplete)
    } catch { /* the code and URL are on screen */ }
  }

  async function startAuth() {
    phase = 'authorizing'
    errorMsg = ''
    deviceCode = null
    /** @type {(() => void) | undefined} */
    let unlisten
    try {
      const { listen } = await import('@tauri-apps/api/event')
      unlisten = await listen('provider-device-code', (ev) => {
        deviceCode = /** @type {any} */ (ev.payload)
      })
    } catch { /* not in Tauri: the flow still works, just without the code */ }
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
    } finally {
      unlisten?.()
      deviceCode = null
    }
  }

  /** Abort the in-flight browser auth and return to the start. */
  async function cancelAuth() {
    try { await providerCancelOAuth() } catch { /* ignore */ }
    phase = 'idle'
    errorMsg = ''
  }

  async function saveToken() {
    if (!tokenReady) return
    const credential = tokenFields.length
      ? (meta?.token?.join?.(tokenValues) ?? '')
      : tokenInput.trim()
    phase = 'fetching'
    errorMsg = ''
    try {
      await providerStoreToken(provider, credential)
      await loadDatabases()
      // The secret now lives in the keychain; don't keep a copy in the form.
      tokenValues = emptyTokenValues()
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

  /**
   * `quiet` refreshes a cached list already on screen: no spinner, and a network
   * blip keeps the cached list rather than replacing it with an error card. An
   * ended session still surfaces, since nothing on that list would connect.
   * @param {{ quiet?: boolean }} [opts]
   */
  async function loadDatabases({ quiet = false } = {}) {
    if (!quiet) phase = 'fetching'
    errorMsg = ''
    try {
      databases = await withTimeout(
        providerListDatabases(provider),
        20_000,
        `listing your ${meta?.name ?? provider} databases`,
      )
      writeProviderList(cacheKey, $state.snapshot(databases))
      phase = 'selecting'
    } catch (e) {
      const msg = String(e)
      const ended = /not signed in/i.test(msg)
      if (ended) clearProviderLists(cacheKey)
      if (quiet && !ended) return
      phase = 'error'
      errorMsg = msg
    }
  }

  /**
   * Title for a database that couldn't be resolved into a connection. The
   * backend's message already says what to do (restore it, wait for it), so it
   * rides along as the description.
   * @param {string} msg @param {string} dbName
   */
  function pickFailureTitle(msg, dbName) {
    const db = dbName || 'This database'
    if (/is paused/i.test(msg)) return `${db} is paused`
    if (/starting up|restoring|restarting|resizing|upgrading|coming.up/i.test(msg)) return `${db} is still starting`
    if (/failed to start/i.test(msg)) return `${db} failed to start`
    if (/timed out|request failed|network|unavailable|dns|connect/i.test(msg))
      return `Couldn't reach ${meta?.name ?? 'the provider'}`
    return `Couldn't connect to ${db}`
  }


  async function pick(ref) {
    selectedRef = ref
    phase = 'building'
    try {
      // Already saved this database? Connect with that entry as it is: no API
      // call at all. For the providers that mint credentials (PlanetScale,
      // Prisma, TiDB, Turso, Nile) that also avoids leaving another password
      // behind; for Neon and Supabase it skips two or three API round trips.
      // The caller mints fresh credentials if the saved ones were revoked.
      const dbName = databases.find((d) => d.db_ref === ref)?.name ?? ''
      const known = resolveSavedConnection(ref, dbName)
      if (known) {
        phase = 'selecting'
        onselect({ reuse: known, providerRef: ref })
        return
      }
      const conn = { ...(await providerBuildConnection(provider, ref)), providerRef: ref }
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
      const msg = String(e).replace(/^Error:\s*/, '')
      // An ended sign-in needs the "Sign in again" card; there is nothing to
      // pick until that's fixed.
      if (/not signed in/i.test(msg)) {
        phase = 'error'
        errorMsg = msg
        return
      }
      // One database being down (paused, starting, unreachable) says nothing
      // about the others: keep the picker and report it in a toast, so the next
      // pick is one click away instead of behind "Try again".
      phase = 'selecting'
      selectedRef = ''
      const dbName = databases.find((d) => d.db_ref === ref)?.name ?? ''
      toast.error(pickFailureTitle(msg, dbName), { description: msg, duration: 9000 })
    }
  }

  /** Merge the entered password into the resolved spec and connect. */
  function confirmPassword() {
    if (!resolved || !pw.trim()) return
    onselect({ ...resolved, password: pw })
  }

  /** Sign-out asks first: it can't be undone from here without the browser. */
  let confirmSignOut = $state(false)

  async function handleLogout() {
    await providerLogout(provider)
    clearProviderLists(cacheKey)
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
      <!-- Providers with no OAuth for third-party apps: the provider's own
           fields (declared in providers.js), labelled, in one form so Enter
           submits from any of them. -->
      <ProviderAuthPanel
        title="Connect to {meta?.name}"
        subtitle={meta?.token?.help ?? `Paste your ${meta?.name} credentials.`}
        hint="Stored in this machine's keychain"
      >
        {#snippet mark()}<DbIcon id={provider} class="size-4 shrink-0" />{/snippet}
        {#snippet action()}
          {#if meta?.token?.helpUrl}
            <Button
              variant="ghost"
              size="sm"
              class="text-muted-foreground"
              onclick={async () => {
                try {
                  const { openUrl } = await import('@tauri-apps/plugin-opener')
                  await openUrl(meta.token.helpUrl)
                } catch { /* the help text says where to go */ }
              }}
            >
              Get a key
              <ArrowRight class="size-3.5 shrink-0" aria-hidden="true" />
            </Button>
          {/if}
        {/snippet}
      </ProviderAuthPanel>
      <form
        class="flex flex-col gap-3 ps-11"
        onsubmit={(e) => { e.preventDefault(); void saveToken() }}
      >
        {#if tokenFields.length}
          <div class="grid gap-3 sm:grid-cols-2">
            {#each tokenFields as f (f.key)}
              <div class="flex min-w-0 flex-col gap-1.5">
                <label for="tok-{provider}-{f.key}" class="text-ui-2xs font-medium text-muted-foreground">{f.label}</label>
                {#if f.type === 'password'}
                  <PasswordInput
                    id="tok-{provider}-{f.key}"
                    bind:value={tokenValues[f.key]}
                    placeholder={f.placeholder}
                    autocomplete={f.autocomplete}
                    class={cn('h-9', f.mono && 'font-mono text-ui-2xs')}
                  />
                {:else}
                  <Input
                    id="tok-{provider}-{f.key}"
                    type={f.type}
                    bind:value={tokenValues[f.key]}
                    placeholder={f.placeholder}
                    autocomplete={f.autocomplete}
                    spellcheck="false"
                    class={cn('h-9', f.mono && 'font-mono text-ui-2xs')}
                  />
                {/if}
              </div>
            {/each}
          </div>
        {:else}
          <Input
            bind:value={tokenInput}
            placeholder="postgres://…"
            aria-label="{meta?.name} connection string"
            spellcheck="false"
            class="h-9 font-mono text-ui-2xs"
          />
        {/if}
        <div>
          <Button type="submit" disabled={!tokenReady}>
            <KeyRound class="size-3.5 shrink-0" aria-hidden="true" /> Continue
          </Button>
        </div>
      </form>
    {:else}
      <ProviderAuthPanel
        title="Sign in with {meta?.name}"
        subtitle="Opens your browser to authorize. Your {meta?.name} password never passes through Stroke."
        hint={meta?.signIn ?? ''}
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
      subtitle={deviceCode
        ? `Confirm this code on the ${meta?.name} page in your browser, then come back here.`
        : 'Finish authorizing in the browser tab, then come back here.'}
      hint="Times out in 5 min"
    >
      {#snippet mark()}<DbIcon id={provider} class="size-4 shrink-0" />{/snippet}
      {#snippet action()}
        <Button variant="ghost" size="sm" onclick={cancelAuth}>
          <X class="size-3.5" /> Cancel
        </Button>
      {/snippet}
    </ProviderAuthPanel>
    {#if deviceCode}
      <!-- Sits on the panel's text edge (32px well + 12px gap), so the code
           reads as part of the step above rather than a new section. -->
      <div class="flex flex-wrap items-center gap-3 ps-11">
        <output
          aria-label="Confirmation code"
          class="rounded-lg border border-border/60 bg-muted/30 px-3.5 py-2 font-mono text-ui-lg font-semibold tracking-[0.2em] tabular-nums text-foreground select-all"
        >{deviceCode.userCode}</output>
        <Button variant="outline" size="sm" onclick={copyDeviceCode}>
          {#if codeCopied}<Check class="size-3.5 shrink-0 text-success" aria-hidden="true" /> Copied{:else}Copy code{/if}
        </Button>
        <Button variant="ghost" size="sm" class="text-muted-foreground" onclick={reopenDevicePage}>
          Open the page again
        </Button>
        <span class="sr-only" role="status">{codeCopied ? 'Code copied' : ''}</span>
      </div>
    {/if}

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
          {#if sessionExpired && meta?.mode === 'token'}
            <Button onclick={() => (phase = 'idle')}>Enter again</Button>
          {:else if sessionExpired}
            <Button onclick={startAuth}>Sign in again</Button>
          {:else}
            <Button variant="outline" class="group" onclick={() => loadDatabases()}>
              <RefreshCw class="size-3.5 transition-transform duration-500 ease-[var(--ease-out)] group-hover:rotate-180" />
              Try again
            </Button>
          {/if}
          <!-- Sign out, not "Start over": going back to idle kept the stored
               session, so a sign-in with the wrong scopes or the wrong org
               could only be replaced, never removed. -->
          <Button variant="ghost" class="text-muted-foreground" onclick={() => (confirmSignOut = true)}>
            <LogOut class="size-3.5 shrink-0" aria-hidden="true" />
            Sign out
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
      <Button variant="ghost" size="sm" class="shrink-0 text-muted-foreground hover:text-foreground" onclick={() => (confirmSignOut = true)}>
        <LogOut class="size-3.5 shrink-0" aria-hidden="true" />
        Sign out
      </Button>
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
      <!-- A field-width control, not a full-bleed bar: capped at 28rem so the
           name, the chevron and the list below sit where the eye reads, and the
           menu opens at the trigger's width (with a floor for long names). -->
      <div class="flex max-w-md flex-col gap-1.5">
        <span id="pc-db-label" class="flex items-baseline gap-1.5 text-ui-xs font-medium text-foreground/80">
          Database
          <span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">{databases.length}</span>
        </span>
        <SearchableMenu
          items={dbItems}
          placeholder="Search {databases.length} databases…"
          empty="No matching database"
          contentClass="w-[var(--bits-popover-anchor-width)] min-w-[280px]"
          align="start"
          onselect={(it) => pick(it.value)}
        >
          {#snippet trigger(props)}
            <button
              {...props}
              type="button"
              aria-labelledby="pc-db-label"
              disabled={phase === 'building'}
              class="field-surface flex h-9 w-full items-center gap-2 bg-muted/25 pl-3 pr-2.5 text-left text-ui-xs transition-[border-color,box-shadow] hover:border-border focus:outline-none disabled:opacity-60 data-[state=open]:border-ring"
            >
              <DbIcon id={provider} class={cn('size-4 shrink-0', selectedDbName ? 'text-foreground' : 'text-muted-foreground')} />
              <span class={cn('min-w-0 flex-1 truncate', selectedDbName ? 'font-medium text-foreground' : 'text-muted-foreground')}>
                {selectedDbName || 'Select a database'}
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
            <!-- Names in the UI face: they are project names ("Fantastic Jade
                 Beluga"), not identifiers, and mono made a list of them read as
                 code. Region and kind trail in muted text, the check last. -->
            <DbIcon id={provider} class={cn('size-4 shrink-0', it.value === selectedRef ? 'text-foreground' : 'text-muted-foreground')} />
            <span class="min-w-0 flex-1 truncate leading-snug">{it.label}</span>
            {#if it.region}<span class="shrink-0 font-mono text-ui-3xs text-muted-foreground">{it.region}</span>{/if}
            {#if it.value === selectedRef}<Check class="size-3.5 shrink-0 text-primary" />{/if}
          {/snippet}
        </SearchableMenu>
      </div>
    {:else}
      <!-- The same shape as the sidebar's empty tabs: a mark in a well, a title, one
     line on what to do, one button. Generous padding, because this block is
     the whole content of the panel when it shows. -->
      <div class="flex flex-col items-center gap-4 rounded-xl border border-dashed border-border/60 px-6 py-10 text-center">
        <div class="flex size-10 items-center justify-center rounded-lg border border-border/60 bg-muted/30">
          <DbIcon id={provider} class="size-5 text-muted-foreground" />
        </div>
        <div class="flex max-w-[36ch] flex-col gap-1">
          <p class="text-ui-sm font-medium text-foreground">No databases yet</p>
          <p class="text-pretty text-ui-xs leading-relaxed text-muted-foreground">Nothing on this {meta?.name ?? ''} account yet. Create one in the {meta?.name ?? 'provider'} dashboard, then refresh.</p>
        </div>
        <Button variant="outline" size="sm" class="group active:scale-[0.96]" onclick={() => loadDatabases()}>
          <RefreshCw class="size-3.5 shrink-0 transition-transform duration-500 ease-[var(--ease-out)] group-hover:rotate-180" aria-hidden="true" />
          Refresh
        </Button>
      </div>
    {/if}

  {/if}
</div>

<ConfirmDialog
  bind:open={confirmSignOut}
  icon="log-out"
  title="Sign out of {meta?.name ?? 'this provider'}?"
  description="Stroke forgets its {meta?.name ?? 'provider'} sign-in on this machine."
  note="Connections you already saved keep working. To browse your {meta?.name ?? ''} databases again, sign in again."
  confirmLabel="Sign out"
  confirmIcon="log-out"
  confirmOnEnter
  onconfirm={() => void handleLogout()}
/>
