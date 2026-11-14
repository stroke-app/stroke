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
  import * as Select from '$lib/components/ui/select/index.js'
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
          <Select.Root type="single" value={scope ?? undefined} onValueChange={changeScope}>
            <Select.Trigger class="w-full data-[size=default]:h-9" aria-labelledby="{uid}-scope" disabled={loadingOptions}>
              <span data-slot="select-value" class="truncate">{scopeName || 'Select…'}</span>
            </Select.Trigger>
            <Select.Content class="z-[120] max-h-[20rem] p-1" sideOffset={6}>
              {#each options?.scopes ?? [] as s (s.id)}
                <Select.Item value={s.id} label={s.name}>{s.name}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
      {/if}

      {#if needsRegion || loadingOptions}
        <div class="flex min-w-0 flex-col gap-1.5">
          <span id="{uid}-region" class="text-ui-xs font-medium text-muted-foreground">{options?.region_label || 'Region'}</span>
          <Select.Root type="single" value={region ?? undefined} onValueChange={(v) => v && (region = v)}>
            <Select.Trigger class="w-full data-[size=default]:h-9" aria-labelledby="{uid}-region" disabled={loadingOptions}>
              {#if loadingOptions}
                <span data-slot="select-value" class="flex items-center gap-1.5 text-muted-foreground">
                  <Loader2 class="size-3.5 shrink-0 animate-spin" aria-hidden="true" />Loading…
                </span>
              {:else}
                <span data-slot="select-value" class="truncate">{regionName || 'Select…'}</span>
              {/if}
            </Select.Trigger>
            <Select.Content class="z-[120] max-h-[20rem] p-1" sideOffset={6}>
              {#each options?.regions ?? [] as r (r.id)}
                <Select.Item value={r.id} label={r.name}>{r.name}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
      {/if}
    </div>

    {#if password}
      <div class="flex flex-col gap-1.5">
        <span id="{uid}-pw" class="text-ui-xs font-medium text-muted-foreground">Database password</span>
        <div class="flex items-center gap-2">
          <output
            aria-labelledby="{uid}-pw"
            class="field-surface flex h-9 min-w-0 flex-1 items-center bg-muted/25 px-3 font-mono text-ui-xs tabular-nums text-foreground select-all"
          ><span class="truncate">{password}</span></output>
          <Button variant="outline" size="lg" onclick={copyPassword}>
            {#if copied}<Check class="size-3.5 shrink-0 text-success" aria-hidden="true" />Copied{:else}<Copy class="size-3.5 shrink-0" aria-hidden="true" />Copy{/if}
          </Button>
          <span class="sr-only" role="status">{copied ? 'Password copied' : ''}</span>
        </div>
        <p class="text-pretty text-ui-2xs leading-relaxed text-muted-foreground">
          Generated for you. {providerName} won't show it again; Stroke saves it with this connection. Copy it if you'll connect from somewhere else.
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
