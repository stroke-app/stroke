<script>
  // "Create database" for a signed-in provider: an inline form that asks only
  // what the provider needs (name; organization and region when there is a
  // choice; a generated password for Supabase), then waits while it
  // provisions. Shared by ProviderConnect and the Cloudflare D1 flow, which
  // pass in their own API calls.
  import { onMount, onDestroy, untrack } from 'svelte'
  import Check from '@lucide/svelte/icons/check'
  import Copy from '@lucide/svelte/icons/copy'
  import AlertTriangle from '@lucide/svelte/icons/alert-triangle'
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import DbIcon from './DbIcon.svelte'
  import ProviderAuthPanel from './ProviderAuthPanel.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import SearchableMenu from './SearchableMenu.svelte'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import { cn } from '$lib/utils.js'
  import { nameProblem, suggestName, generatePassword, waitUntilReady } from '$lib/provider-create.js'

  let {
    /** Display name, e.g. "TiDB Cloud". */
    providerName,
    /** DbIcon id for the busy panel. */
    iconId,
    /** @type {import('$lib/provider-create.js').CreateSpec} */
    spec,
    /** Names already on the account, so the suggested one is free. @type {string[]} */
    taken = [],
    /** @type {(scope: string | null) => Promise<import('$lib/provider-create.js').CreateOptions>} */
    loadOptions,
    /** @type {(req: { name: string, scope: string | null, region: string | null, password: string | null }) => Promise<import('$lib/provider-create.js').CreatedDatabase>} */
    create,
    /** Provisioning status; omitted when a new database is ready at once. @type {((ref: string) => Promise<import('$lib/provider-create.js').DatabaseStatus>) | undefined} */
    checkStatus = undefined,
    /** The database is ready to connect. @type {(db: import('$lib/provider-create.js').CreatedDatabase, extra: { password: string | null }) => void} */
    oncreated,
    /**
     * The form closed without a ready database. `pending` is the one still
     * provisioning when the user stopped waiting.
     * @type {(pending: import('$lib/provider-create.js').CreatedDatabase | null) => void}
     */
    onclose,
  } = $props()

  const uid = $props.id()

  /** @type {'form' | 'creating' | 'provisioning'} */
  let step = $state('form')
  // Seeded once: a list refresh behind the open form must not rewrite the name.
  let name = $state(untrack(() => suggestName(spec, taken)))
  let touched = $state(false)
  const problem = $derived(nameProblem(name, spec))
  const showProblem = $derived(touched && !!problem)

  /** @type {import('$lib/provider-create.js').CreateOptions | null} */
  let options = $state(null)
  let loadingOptions = $state(true)
  let optionsError = $state('')
  let scope = $state(/** @type {string | null} */ (null))
  let region = $state(/** @type {string | null} */ (null))

  const scopeName = $derived(options?.scopes.find((s) => s.id === scope)?.name ?? '')
  const regionName = $derived(options?.regions.find((r) => r.id === region)?.name ?? '')
  const needsRegion = $derived((options?.regions.length ?? 0) > 0)

  /** Generated once per form; Supabase never returns it, so it is shown here. */
  const password = untrack(() => spec.password) ? generatePassword() : null
  let copied = $state(false)

  let error = $state('')
  /** @type {import('$lib/provider-create.js').CreatedDatabase | null} */
  let pending = $state(null)
  let providerState = $state('')
  let elapsed = $state(0)
  /** @type {ReturnType<typeof setInterval> | undefined} */
  let ticker
  /** @type {AbortController | null} */
  let waiting = null
  let nameInput = $state(/** @type {HTMLInputElement | null} */ (null))

  /** @param {string | null} wanted */
  async function fetchOptions(wanted) {
    loadingOptions = true
    optionsError = ''
    try {
      const o = await loadOptions(wanted)
      options = o
      scope = o.scope ?? o.scopes[0]?.id ?? null
      // Keep the region the user chose when it is still offered for this scope.
      if (!region || !o.regions.some((r) => r.id === region)) region = o.default_region ?? o.regions[0]?.id ?? null
    } catch (e) {
      optionsError = String(e).replace(/^Error:\s*/, '')
    } finally {
      loadingOptions = false
    }
  }

  onMount(() => {
    void fetchOptions(null)
    nameInput?.select()
  })

  onDestroy(() => {
    waiting?.abort()
    clearInterval(ticker)
  })

  /** @param {string | undefined} next */
  function changeScope(next) {
    if (!next || next === scope) return
    scope = next
    // Regions (and Turso's groups) belong to the organization.
    void fetchOptions(next)
  }

  async function copyPassword() {
    if (!password) return
    try {
      await navigator.clipboard.writeText(password)
      copied = true
      setTimeout(() => (copied = false), 1500)
    } catch { /* clipboard blocked: the password is on screen */ }
  }

  const canSubmit = $derived(!problem && !loadingOptions && !optionsError && (!needsRegion || !!region))

  async function submit() {
    touched = true
    if (problem) {
      nameInput?.focus()
      return
    }
    if (!canSubmit) return
    error = ''
    step = 'creating'
    try {
      const created = await create({ name: name.trim(), scope, region, password })
      if (created.ready || !checkStatus) {
        oncreated(created, { password })
        return
      }
      pending = created
      await provision(created)
    } catch (e) {
      step = 'form'
      error = friendly(String(e).replace(/^Error:\s*/, ''))
    }
  }

  /** @param {import('$lib/provider-create.js').CreatedDatabase} created */
  async function provision(created) {
    step = 'provisioning'
    elapsed = 0
    providerState = ''
    ticker = setInterval(() => (elapsed += 1), 1000)
    waiting = new AbortController()
    const check = /** @type {NonNullable<typeof checkStatus>} */ (checkStatus)
    try {
      await waitUntilReady({
        check: () => check(created.db_ref),
        onstate: (s) => (providerState = s.state),
        signal: waiting.signal,
      })
      oncreated(created, { password })
    } catch (e) {
      if (/** @type {any} */ (e)?.name === 'AbortError') return
      // The database exists; only the wait went wrong. Say so rather than
      // reopening a form that would make a second one.
      step = 'form'
      error = `${created.name} was created, but it isn't ready yet: ${String(e).replace(/^Error:\s*/, '')}`
    } finally {
      clearInterval(ticker)
      waiting = null
    }
  }

  function stopWaiting() {
    waiting?.abort()
    onclose(pending)
  }

  /** Backend errors, minus the transport prefix, with the common ones worded. @param {string} m */
  function friendly(m) {
    if (/already exists|\b409\b/i.test(m)) return `A ${spec.noun} with that name already exists. Pick another name.`
    if (/not signed in/i.test(m)) return m
    if (/\b402\b|payment|billing|quota|limit/i.test(m)) return `${providerName} declined: ${m.replace(/^.*?:\s*/, '')} Check your plan's limits in the ${providerName} dashboard.`
    return m
  }

  /** @param {number} s */
  const clock = (s) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
</script>

<!-- Organization and region: the database list's searchable menu, so a long
     region list (TiDB, Neon) is typed into instead of scrolled. -->
{#snippet picker(/** @type {{ items: any[], current: string | null | undefined, currentName: string, noun: string, labelledby: string, onpick: (v: string) => void }} */ p)}
  <SearchableMenu
    items={p.items}
    placeholder="Search {p.items.length} {p.noun}s…"
    empty="No matching {p.noun}"
    searchThreshold={6}
    contentClass="z-[120] w-[var(--bits-popover-anchor-width)] min-w-[260px]"
    align="start"
    onselect={(it) => p.onpick(it.value)}
  >
    {#snippet trigger(props)}
      <button
        {...props}
        type="button"
        aria-labelledby={p.labelledby}
        disabled={loadingOptions}
        class="field-surface flex h-9 w-full items-center gap-2 pl-3 pr-2.5 text-left text-ui-xs transition-[border-color,box-shadow] hover:border-border focus:outline-none disabled:opacity-60 data-[state=open]:border-ring"
      >
        {#if loadingOptions}
          <Loader2 class="size-3.5 shrink-0 animate-spin text-muted-foreground" aria-hidden="true" />
          <span class="min-w-0 flex-1 truncate text-muted-foreground">Loading…</span>
        {:else}
          <span class={cn('min-w-0 flex-1 truncate', p.currentName ? 'text-foreground' : 'text-muted-foreground')}>{p.currentName || 'Select…'}</span>
        {/if}
        <ChevronDown class="size-3.5 shrink-0 text-muted-foreground" />
      </button>
    {/snippet}
    {#snippet item(it)}
      <span class="min-w-0 flex-1 truncate">{it.label}</span>
      {#if it.value === p.current}<Check class="size-3.5 shrink-0 text-primary" />{/if}
    {/snippet}
  </SearchableMenu>
{/snippet}

{#if step === 'form'}
  <form
    class="flex flex-col gap-4 rounded-lg border border-border/60 p-4"
    aria-labelledby="{uid}-title"
    onsubmit={(e) => { e.preventDefault(); void submit() }}
  >
    <div class="flex flex-col gap-1">
      <p id="{uid}-title" class="text-ui-sm font-medium text-foreground">New {spec.noun}</p>
      <p class="text-pretty text-ui-xs leading-relaxed text-muted-foreground">
        Created in your {providerName} account{#if spec.slow}. Setting it up can take a minute or two{/if}.
      </p>
    </div>

    <div class="grid gap-3 sm:grid-cols-2">
      <div class="flex min-w-0 flex-col gap-1.5 sm:col-span-2">
        <label for="{uid}-name" class="text-ui-xs font-medium text-muted-foreground">Name</label>
        <Input
          id="{uid}-name"
          bind:ref={nameInput}
          bind:value={name}
          autocomplete="off"
          spellcheck="false"
          maxlength={spec.maxLength}
          aria-invalid={showProblem || undefined}
          aria-describedby={showProblem ? `${uid}-name-error` : spec.rule ? `${uid}-name-rule` : undefined}
          onblur={() => (touched = true)}
          class="h-9"
        />
        {#if showProblem}
          <p id="{uid}-name-error" class="text-ui-2xs text-destructive">{problem}</p>
        {:else if spec.rule}
          <p id="{uid}-name-rule" class="text-ui-2xs text-muted-foreground">{spec.rule}</p>
        {/if}
      </div>

      {#if (options?.scopes.length ?? 0) > 1}
        <div class="flex min-w-0 flex-col gap-1.5">
          <span id="{uid}-scope" class="text-ui-xs font-medium text-muted-foreground">{options?.scope_label || 'Organization'}</span>
          {@render picker({
            items: (options?.scopes ?? []).map((s) => ({ value: s.id, label: s.name, keywords: [s.name, s.id] })),
            current: scope,
            currentName: scopeName,
            noun: (options?.scope_label || 'organization').toLowerCase(),
            labelledby: `${uid}-scope`,
            onpick: changeScope,
          })}
        </div>
      {/if}

      {#if needsRegion || loadingOptions}
        <div class="flex min-w-0 flex-col gap-1.5">
          <span id="{uid}-region" class="text-ui-xs font-medium text-muted-foreground">{options?.region_label || 'Region'}</span>
          {@render picker({
            items: (options?.regions ?? []).map((r) => ({ value: r.id, label: r.name, keywords: [r.name, r.id] })),
            current: region,
            currentName: regionName,
            noun: (options?.region_label || 'region').toLowerCase(),
            labelledby: `${uid}-region`,
            onpick: (/** @type {string} */ v) => v && (region = v),
          })}
        </div>
      {/if}
    </div>

    {#if password}
      <div class="flex flex-col gap-1.5">
        <span id="{uid}-pw" class="text-ui-xs font-medium text-muted-foreground">Database password</span>
        <!-- One field: the value, and its copy button inside it at the end. -->
        <div class="field-surface flex h-9 min-w-0 items-center gap-1 bg-muted/25 pl-3 pr-1">
          <output
            aria-labelledby="{uid}-pw"
            class="min-w-0 flex-1 truncate font-mono text-ui-xs tabular-nums text-foreground select-all"
          >{password}</output>
          <button
            type="button"
            class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            title={copied ? 'Copied' : 'Copy password'}
            aria-label="Copy password"
            onclick={copyPassword}
          >
            {#if copied}<Check class="size-3.5 shrink-0 text-success" aria-hidden="true" />{:else}<Copy class="size-3.5 shrink-0" aria-hidden="true" />{/if}
          </button>
          <span class="sr-only" role="status">{copied ? 'Password copied' : ''}</span>
        </div>
        <p class="text-pretty text-ui-2xs leading-relaxed text-muted-foreground">
          {providerName} won't show it again. Stroke saves it with the connection.
        </p>
      </div>
    {/if}

    {#if optionsError}
      <div role="alert" class="flex items-start gap-2 text-ui-xs text-destructive">
        <AlertTriangle class="mt-px size-3.5 shrink-0" aria-hidden="true" />
        <p class="min-w-0 flex-1 text-pretty">Couldn't load the options: {optionsError}</p>
        <Button variant="ghost" size="xs" onclick={() => fetchOptions(scope)}>
          <RefreshCw class="size-3 shrink-0" aria-hidden="true" />Try again
        </Button>
      </div>
    {/if}
    {#if error}
      <div role="alert" class="flex items-start gap-2 text-ui-xs text-destructive">
        <AlertTriangle class="mt-px size-3.5 shrink-0" aria-hidden="true" />
        <p class="min-w-0 flex-1 text-pretty">{error}</p>
      </div>
    {/if}

    <div class="flex items-center gap-2">
      <Button type="submit" disabled={!canSubmit && !problem}>Create {spec.noun}</Button>
      <Button variant="ghost" class="text-muted-foreground" onclick={() => onclose(null)}>Cancel</Button>
    </div>
  </form>

{:else}
  <ProviderAuthPanel
    tone="busy"
    progress
    title="Creating {pending?.name ?? name.trim()}…"
    subtitle={step === 'creating'
      ? `Asking ${providerName} to create the ${spec.noun}.`
      : `${providerName} is setting up the ${spec.noun}${providerState ? ` (${providerState.toLowerCase()})` : ''}. Stroke connects as soon as it's ready.`}
    hint={step === 'provisioning' ? `${clock(elapsed)} elapsed${spec.slow ? ' · usually 1 to 2 min' : ''}` : ''}
  >
    {#snippet mark()}<DbIcon id={iconId} class="size-4 shrink-0" />{/snippet}
    {#snippet action()}
      {#if step === 'provisioning'}
        <Button variant="ghost" size="sm" onclick={stopWaiting}>Stop waiting</Button>
      {/if}
    {/snippet}
  </ProviderAuthPanel>
  <span class="sr-only" role="status">{step === 'provisioning' && providerState ? `Status: ${providerState}` : ''}</span>
{/if}
