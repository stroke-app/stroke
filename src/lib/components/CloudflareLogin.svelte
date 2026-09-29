<script>
  import { onMount } from 'svelte'
  import Check from '@lucide/svelte/icons/check'
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import LogOut from '@lucide/svelte/icons/log-out'
  import AlertTriangle from '@lucide/svelte/icons/alert-triangle'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import ArrowRight from '@lucide/svelte/icons/arrow-right'
  import DbIcon from './DbIcon.svelte'
  import SearchableMenu from './SearchableMenu.svelte'
  import ProviderAuthPanel from './ProviderAuthPanel.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import ConfirmDialog from './ConfirmDialog.svelte'
  import { cfStartOAuth, cfOAuthStatus, cfLogout, cfGetValidToken } from '$lib/cloudflare.js'
  import { readProviderList, writeProviderList, clearProviderLists } from '$lib/provider-list-cache.js'
  import { cloudflareListAccounts, cloudflareListD1Databases } from '$lib/api.js'
  import { cn } from '$lib/utils.js'

  let {
    /**
     * Called when user has selected an account + database.
     * @type {(info: {accountId: string, databaseId: string, databaseName: string, token: string}) => void}
     */
    onselect = () => {},
    /** Called when user logs out. */
    ondisconnect = () => {},
    // Seeds from a saved connection picked in the dialog's sidebar, so its
    // account and database show as already selected instead of the picker
    // restarting on the first account. Selection only - see `seedFromSaved`.
    initialAccountId = '',
    initialDatabaseId = '',
    initialDatabaseName = '',
  } = $props()

  /** @type {'idle' | 'authorizing' | 'fetching' | 'selecting' | 'error'} */
  let phase = $state('idle')
  let email = $state('')
  let errorMsg = $state('')

  /** @type {Array<{id: string, name: string}>} */
  let accounts = $state([])
  let selectedAccountId = $state('')
  const selectedAccountName = $derived(accounts.find((a) => a.id === selectedAccountId)?.name ?? '')

  /** @type {Array<{uuid: string, name: string, created_at?: string, num_tables?: number}>} */
  let databases = $state([])
  let selectedDbUuid = $state('')
  let loadingDbs = $state(false)

  // `value` is the uuid because that is what selection needs; SearchableMenu makes
  // the label searchable on its own.
  const dbItems = $derived(databases.map((d) => ({ value: d.uuid, label: d.name })))
  // Falls back to the saved connection's name so a sidebar pick reads as selected
  // straight away, before (or even without) the account's database list arriving.
  const selectedDbName = $derived(
    databases.find((d) => d.uuid === selectedDbUuid)?.name || initialDatabaseName || '',
  )

  // Show a saved connection's database as the current selection. Deliberately
  // does not call `onselect` - that is what connects, and picking a row in the
  // connections sidebar is not a request to dial it. Latched on the id so it
  // seeds once per saved connection and never fights a manual pick.
  let seededDbId = ''
  $effect(() => {
    const want = initialDatabaseId
    if (!want || seededDbId === want) return
    if (databases.some((d) => d.uuid === want)) {
      seededDbId = want
      selectedDbUuid = want
    }
  })

  /**
   * Nothing loads forever.
   *
   * The panel sat on "Loading your Cloudflare accounts…" indefinitely: the
   * shared Cloudflare HTTP client had no timeout (reqwest has no default), so a
   * stalled request left the Tauri command awaiting and this `await` never
   * returned - no error, no dropdown, no way back. The client is bounded now;
   * this is the backstop for everything else on that path, the keychain read
   * included.
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

  /** Turn a raw backend error into a calm title + one-line explanation. */
  function friendlyError(msg) {
    const m = String(msg ?? '')
    if (/not signed in|no.*token|unauthor/i.test(m))
      return { title: 'Session expired', detail: 'Your Cloudflare sign-in is no longer valid. Sign in again to continue.' }
    if (/timed out/i.test(m))
      return { title: 'Authorization timed out', detail: 'The browser sign-in took too long. Start again when you are ready.' }
    if (/cancel|denied/i.test(m))
      return { title: 'Authorization not completed', detail: 'The browser closed before authorizing. Try again to connect.' }
    if (/port .*in use|bind any callback/i.test(m))
      return { title: 'Callback port in use', detail: m }
    return { title: 'Something went wrong', detail: m || 'Please try again.' }
  }
  const shownError = $derived(friendlyError(errorMsg))

  const CACHE = 'cloudflare:'
  /** @param {string} accountId */
  const dbsKey = (accountId) => `${CACHE}dbs:${accountId}`

  /** The account to land on: the saved connection's, else the last one used, else the first. */
  function pickAccount(/** @type {Array<{id: string}>} */ list) {
    /** @type {string | undefined} */
    const last = readProviderList(`${CACHE}lastAccount`)
    for (const want of [selectedAccountId, initialAccountId, last]) {
      if (want && list.some((a) => a.id === want)) return want
    }
    return list[0]?.id ?? ''
  }

  onMount(async () => {
    // Open on the accounts and databases from last time, then refresh both
    // underneath. The token read fails with "not signed in" by itself when the
    // session is gone, so the status round-trip only runs with nothing to show.
    /** @type {{ email?: string, accounts?: typeof accounts } | undefined} */
    const cached = readProviderList(`${CACHE}accounts`)
    if (cached?.accounts?.length) {
      email = cached.email ?? ''
      accounts = cached.accounts
      selectedAccountId = pickAccount(accounts)
      databases = readProviderList(dbsKey(selectedAccountId)) ?? []
      phase = 'selecting'
      void loadAccounts({ quiet: true })
      return
    }
    try {
      const status = await cfOAuthStatus()
      if (status.connected) {
        email = status.email ?? ''
        phase = 'fetching'
        await loadAccounts()
      }
    } catch { /* stay idle */ }
  })

  /** The sign-in page, pushed by the backend so the panel can offer it again. */
  let authUrl = $state('')
  let linkCopied = $state(false)

  async function reopenAuthPage() {
    if (!authUrl) return
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl(authUrl)
    } catch { /* the link can still be copied */ }
  }

  async function copyAuthLink() {
    if (!authUrl) return
    try {
      await navigator.clipboard.writeText(authUrl)
      linkCopied = true
      setTimeout(() => (linkCopied = false), 1500)
    } catch { /* clipboard blocked */ }
  }

  async function startAuth() {
    phase = 'authorizing'
    errorMsg = ''
    authUrl = ''
    /** @type {(() => void) | undefined} */
    let unlisten
    try {
      const { listen } = await import('@tauri-apps/api/event')
      unlisten = await listen('provider-auth-url', (ev) => {
        authUrl = /** @type {{ url: string }} */ (ev.payload)?.url ?? ''
      })
    } catch { /* not in Tauri */ }
    try {
      const result = await cfStartOAuth()
      email = result.email ?? ''
      phase = 'fetching'
      await loadAccounts()
    } catch (e) {
      phase = 'error'
      errorMsg = String(e)
    } finally {
      unlisten?.()
      authUrl = ''
    }
  }

  /**
   * A failure while refreshing a cached view keeps that view: a network blip
   * shouldn't swap a usable list for an error card. An ended session still
   * surfaces, because nothing on the cached list would connect.
   * @param {unknown} e @param {boolean} quiet
   */
  function fail(e, quiet) {
    const msg = String(e)
    const ended = /not signed in|no.*token|unauthor/i.test(msg)
    if (ended) clearProviderLists(CACHE)
    if (quiet && !ended) return
    phase = 'error'
    errorMsg = msg
  }

  /** @param {{ quiet?: boolean }} [opts] */
  async function loadAccounts({ quiet = false } = {}) {
    if (!quiet) phase = 'fetching'
    errorMsg = ''
    try {
      const token = await withTimeout(cfGetValidToken(), 20_000, 'reading your Cloudflare session')
      // Start the D1 list for the account we expect to land on alongside the
      // account list, instead of waiting for one to finish before the other.
      /** @type {string} */
      const guess = accounts.length
        ? pickAccount(accounts)
        : initialAccountId || readProviderList(`${CACHE}lastAccount`) || ''
      const early = guess ? cloudflareListD1Databases(token, guess) : null
      early?.catch(() => {}) // settled below or abandoned; never an unhandled rejection
      accounts = await withTimeout(
        cloudflareListAccounts(token),
        20_000,
        'listing your Cloudflare accounts',
      )
      writeProviderList(`${CACHE}accounts`, { email, accounts: $state.snapshot(accounts) })
      phase = 'selecting'
      // Auto-select an account so the D1 database list loads immediately; the
      // user can still switch accounts via the dropdown when there are several.
      const id = pickAccount(accounts)
      if (id) await selectAccount(id, { token, pending: id === guess ? early : null, quiet })
    } catch (e) {
      fail(e, quiet)
    }
  }

  /**
   * @param {string} id
   * @param {{ token?: string, pending?: Promise<typeof databases> | null, quiet?: boolean }} [opts]
   */
  async function selectAccount(id, { token, pending = null, quiet = false } = {}) {
    if (id !== selectedAccountId) {
      selectedAccountId = id
      selectedDbUuid = ''
      // Another account's last-known list, if we have one, while it refreshes.
      databases = readProviderList(dbsKey(id)) ?? []
    }
    writeProviderList(`${CACHE}lastAccount`, id)
    loadingDbs = databases.length === 0
    try {
      const tok = token ?? (await withTimeout(cfGetValidToken(), 20_000, 'reading your Cloudflare session'))
      const list = await withTimeout(
        pending ?? cloudflareListD1Databases(tok, id),
        20_000,
        'listing D1 databases for this account',
      )
      // The user may have switched accounts while this was in flight.
      if (id !== selectedAccountId) return
      databases = list
      writeProviderList(dbsKey(id), $state.snapshot(list))
    } catch (e) {
      // Show the error card - staying in 'selecting' rendered a misleading
      // "No D1 databases in this account" empty state over a real failure.
      if (id === selectedAccountId) fail(e, quiet)
    } finally {
      if (id === selectedAccountId) loadingDbs = false
    }
  }

  async function selectDatabase(uuid) {
    selectedDbUuid = uuid
    const db = databases.find(d => d.uuid === uuid)
    if (!db) return
    try {
      const token = await cfGetValidToken()
      onselect({
        accountId: selectedAccountId,
        databaseId: uuid,
        databaseName: db.name,
        token,
      })
    } catch (e) {
      phase = 'error'
      errorMsg = String(e)
    }
  }

  /** Sign-out asks first: saved D1 connections depend on this sign-in. */
  let confirmSignOut = $state(false)

  async function handleLogout() {
    await cfLogout()
    clearProviderLists(CACHE)
    phase = 'idle'
    email = ''
    accounts = []
    selectedAccountId = ''
    databases = []
    selectedDbUuid = ''
    errorMsg = ''
    ondisconnect()
  }
</script>

<div class="flex flex-col gap-3">

  {#if phase === 'idle'}
    <ProviderAuthPanel
      title="Sign in with Cloudflare"
      subtitle="Opens your browser to authorize. Stroke gets a scoped token for D1 - your Cloudflare password never passes through it."
      hint="Same PKCE flow as Wrangler"
    >
      {#snippet mark()}<DbIcon id="d1" class="size-4 shrink-0" />{/snippet}
      {#snippet action()}
        <Button class="group" onclick={startAuth}>
          Sign in
          <ArrowRight class="size-3.5 transition-transform duration-150 ease-out group-hover:translate-x-0.5" />
        </Button>
      {/snippet}
    </ProviderAuthPanel>

  {:else if phase === 'authorizing'}
    <ProviderAuthPanel
      tone="busy"
      progress
      title="Waiting for Cloudflare…"
      subtitle="Finish authorizing in the browser tab, then come back here."
      hint="Times out in 5 min"
    >
      {#snippet mark()}<DbIcon id="d1" class="size-4 shrink-0" />{/snippet}
    </ProviderAuthPanel>
    {#if authUrl}
      <!-- Same recovery row as the other providers' waiting panel. -->
      <div class="flex flex-wrap items-center gap-x-1 gap-y-1 ps-11 text-ui-2xs text-muted-foreground">
        <span class="me-1">Browser didn't open?</span>
        <Button variant="ghost" size="xs" onclick={reopenAuthPage}>Open again</Button>
        <Button variant="ghost" size="xs" onclick={copyAuthLink}>
          {#if linkCopied}<Check class="size-3 shrink-0 text-success" aria-hidden="true" /> Copied{:else}Copy link{/if}
        </Button>
        <span class="sr-only" role="status">{linkCopied ? 'Link copied' : ''}</span>
      </div>
    {/if}

  {:else if phase === 'fetching'}
    <ProviderAuthPanel
      tone="busy"
      progress
      title="Signed in"
      subtitle="Loading your Cloudflare accounts…"
      hint={email}
    >
      {#snippet mark()}<Check class="size-4 shrink-0 text-success" />{/snippet}
    </ProviderAuthPanel>

  {:else if phase === 'error'}
    <!-- Retry the step that failed. When the sign-in is still good and only the
         account or database list fell over, sending the user back through the
         browser is a five-click answer to a one-click problem. -->
    {@const signedIn = !!email}
    <ProviderAuthPanel
      tone="error"
      title={shownError.title}
      subtitle={shownError.detail}
      hint={signedIn ? email : 'Nothing was saved'}
    >
      {#snippet mark()}<AlertTriangle class="size-4 shrink-0 text-destructive" />{/snippet}
      {#snippet action()}
        <div class="flex shrink-0 items-center gap-2">
          <Button variant="outline" class="group" onclick={() => (signedIn ? loadAccounts() : startAuth())}>
            <RefreshCw class="size-3.5 transition-transform duration-500 ease-[var(--ease-out)] group-hover:rotate-180" />
            Try again
          </Button>
          {#if signedIn}
            <Button variant="ghost" class="text-muted-foreground" onclick={() => (confirmSignOut = true)}>
              <LogOut class="size-3.5 shrink-0" aria-hidden="true" />
              Sign out
            </Button>
          {/if}
        </div>
      {/snippet}
    </ProviderAuthPanel>

  {:else if phase === 'selecting'}
    <!-- ── Connected header ── -->
    <div class="flex items-center gap-2.5 rounded-lg border border-border/40 bg-muted/[0.04] px-3 py-2.5">
      <div class="flex size-7 shrink-0 items-center justify-center rounded-lg border border-border/40 bg-background">
        <DbIcon id="d1" class="size-4 text-foreground" />
      </div>
      <div class="min-w-0 flex-1">
        <p class="flex items-center gap-1.5 text-ui-xs font-medium text-foreground">
          Cloudflare
          <span class="inline-flex items-center gap-1 text-ui-3xs font-normal text-success"><Check class="size-3" />Connected</span>
        </p>
        {#if email}
          <p class="truncate text-ui-3xs text-muted-foreground">{email}</p>
        {/if}
      </div>
      <Button variant="ghost" size="sm" class="shrink-0 text-muted-foreground hover:text-foreground" onclick={() => (confirmSignOut = true)}>
        <LogOut class="size-3.5 shrink-0" aria-hidden="true" />
        Sign out
      </Button>
    </div>

    <!-- Account selector -->
    {#if accounts.length > 1}
      <div class="flex max-w-md flex-col gap-1.5">
        <span class="text-ui-xs font-medium text-foreground/80">Account</span>
        <SearchableMenu
          items={accounts.map((a) => ({ value: a.id, label: a.name }))}
          placeholder="Search accounts…"
          empty="No matching account"
          contentClass="w-[var(--bits-popover-anchor-width)] min-w-[240px]"
          onselect={(it) => selectAccount(it.value)}
        >
          {#snippet trigger(props)}
            <button
              {...props}
              type="button"
              class="field-surface flex h-9 w-full items-center gap-2 bg-muted/25 pl-3 pr-2.5 text-left text-ui-xs transition-[border-color,box-shadow] hover:border-border focus:outline-none data-[state=open]:border-ring"
            >
              <span class={cn('min-w-0 flex-1 truncate', !selectedAccountId && 'text-muted-foreground')}>
                {selectedAccountName || 'Select an account'}
              </span>
              <ChevronDown class="size-3.5 shrink-0 text-muted-foreground" />
            </button>
          {/snippet}
          {#snippet item(it)}
            <DbIcon id="d1" class="size-3.5 shrink-0 text-muted-foreground" />
            <span class="min-w-0 flex-1 truncate">{it.label}</span>
            {#if it.value === selectedAccountId}<Check class="size-3.5 shrink-0 text-primary" />{/if}
          {/snippet}
        </SearchableMenu>
      </div>
    {:else if accounts.length === 1}
      <p class="text-ui-2xs text-muted-foreground">Account · <span class="text-foreground/70">{accounts[0].name}</span></p>
    {/if}

    <!-- Database selector -->
    {#if selectedAccountId}
      <div class="flex max-w-md flex-col gap-1.5">
        <span class="flex items-baseline gap-1.5 text-ui-xs font-medium text-foreground/80">D1 database{#if databases.length}<span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">{databases.length}</span>{/if}</span>

        {#if databases.length > 0}
          <SearchableMenu
            items={dbItems}
            placeholder="Search databases…"
            empty="No matching database"
            contentClass="w-[var(--bits-popover-anchor-width)] min-w-[240px]"
            align="start"
            onselect={(it) => selectDatabase(it.value)}
          >
            {#snippet trigger(props)}
              <button
                {...props}
                type="button"
                class="field-surface flex h-9 w-full items-center gap-2 bg-muted/25 pl-3 pr-2.5 text-left text-ui-xs transition-[border-color,box-shadow] hover:border-border focus:outline-none data-[state=open]:border-ring"
              >
                <DbIcon id="d1" class={cn('size-4 shrink-0', selectedDbName ? 'text-foreground' : 'text-muted-foreground')} />
                <span class={cn('min-w-0 flex-1 truncate font-mono', !selectedDbName && 'font-sans text-muted-foreground')}>
                  {selectedDbName || 'Select a database'}
                </span>
                <ChevronDown class="size-3.5 shrink-0 text-muted-foreground" />
              </button>
            {/snippet}
            {#snippet item(it)}
              <DbIcon id="d1" class={cn('size-4 shrink-0', it.value === selectedDbUuid ? 'text-foreground' : 'text-muted-foreground')} />
              <span class="min-w-0 flex-1 truncate font-mono leading-snug">{it.label}</span>
              {#if it.value === selectedDbUuid}<Check class="size-3.5 shrink-0 text-primary" />{/if}
            {/snippet}
          </SearchableMenu>
        {:else if loadingDbs}
          <!-- Hold the control's footprint while the list loads so the form does
               not jump once the databases arrive. -->
                 <div class= "field-surface flex h-9 w-full items-center gap-2 bg-muted/15 pl-3 pr-2.5 text-ui-xs text-muted-foreground">
            <Loader2 class="size-3.5 shrink-0 animate-spin" />
            <span class="min-w-0 flex-1 truncate">Loading databases…</span>
          </div>
        {:else}
          <!-- The same shape as the sidebar's empty tabs: a mark in a well, a title, one
         line on what to do, one button. Generous padding, because this block is
         the whole content of the panel when it shows. -->
          <div class="flex flex-col items-center gap-4 rounded-xl border border-dashed border-border/60 px-6 py-10 text-center">
            <div class="flex size-10 items-center justify-center rounded-lg border border-border/60 bg-muted/30">
              <DbIcon id={"d1"} class="size-5 text-muted-foreground" />
            </div>
            <div class="flex max-w-[36ch] flex-col gap-1">
              <p class="text-ui-sm font-medium text-foreground">No databases yet</p>
              <p class="text-pretty text-ui-xs leading-relaxed text-muted-foreground">This account has no D1 databases. Create one with <code class="font-mono text-ui-2xs">wrangler d1 create</code> or in the Cloudflare dashboard, then refresh.</p>
            </div>
            <Button variant="outline" size="sm" class="group active:scale-[0.96]" onclick={() => selectAccount(selectedAccountId)}>
              <RefreshCw class="size-3.5 shrink-0 transition-transform duration-500 ease-[var(--ease-out)] group-hover:rotate-180" aria-hidden="true" />
              Refresh
            </Button>
          </div>
        {/if}
      </div>
    {/if}

  {/if}

</div>

<!-- Unlike the other providers, a saved D1 connection reconnects by minting a
     fresh token from this sign-in (see d1Call in api.js), so say so. -->
<ConfirmDialog
  bind:open={confirmSignOut}
  icon="log-out"
  title="Sign out of Cloudflare?"
  description={email ? `Stroke forgets the Cloudflare sign-in for ${email} on this machine.` : 'Stroke forgets its Cloudflare sign-in on this machine.'}
  note="Saved D1 connections use this sign-in, so they stop connecting until you sign in again."
  confirmLabel="Sign out"
  confirmIcon="log-out"
  variant="destructive"
  onconfirm={() => void handleLogout()}
/>
