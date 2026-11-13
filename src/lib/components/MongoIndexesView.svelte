<script>
  // A collection's indexes: keys as pills, flags, usage and size, a two-click
  // drop, and a form for a new one.
  import { cn } from '$lib/utils.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import { humanBytes } from '$lib/doc-adapters.js'
  import { guardWrite, READ_ONLY_HINT } from '$lib/stores/read-only.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import Plus from '@lucide/svelte/icons/plus'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import X from '@lucide/svelte/icons/x'
  import Check from '@lucide/svelte/icons/check'

  /**
   * @type {{
   *   adapter: import('$lib/doc-adapters.js').DocAdapter,
   *   coll: string,
   *   fields?: string[],
   *   readOnly?: boolean,
   *   onchanged?: () => void,
   * }}
   */
  let { adapter, coll, fields = [], readOnly = false, onchanged = () => {} } = $props()

  let list = $state.raw(/** @type {import('$lib/doc-adapters.js').MongoIndex[]} */ ([]))
  let loaded = $state(false)
  let loading = $state(false)
  let error = $state('')
  let confirming = $state('')
  let dropping = $state('')
  let token = 0

  /** @param {unknown} e */
  const msg = (e) => (e instanceof Error ? e.message : String(e))

  async function load() {
    if (!adapter.indexes) return
    const t = ++token
    loading = true
    error = ''
    try {
      const l = await adapter.indexes.list(coll)
      if (t === token) {
        list = l
        loaded = true
      }
    } catch (e) {
      if (t === token) error = msg(e)
    } finally {
      if (t === token) loading = false
    }
  }
  $effect(() => {
    void coll
    loaded = false
    formOpen = false
    void load()
  })

  /** @param {string} name */
  async function drop(name) {
    if (!adapter.indexes || !guardWrite('drop indexes')) return
    if (confirming !== name) {
      confirming = name
      return
    }
    dropping = name
    try {
      await adapter.indexes.drop(coll, name)
      toast.success(`Dropped ${name}`)
      onchanged()
      await load()
    } catch (e) {
      toast.error('Could not drop the index', { description: msg(e) })
    } finally {
      dropping = ''
      confirming = ''
    }
  }

  // ── New index ──────────────────────────────────────────────────────────────
  const DIRS = [
    { v: '1', l: '↑ asc' },
    { v: '-1', l: '↓ desc' },
    { v: 'text', l: 'text' },
    { v: '2dsphere', l: '2dsphere' },
    { v: 'hashed', l: 'hashed' },
  ]
  let formOpen = $state(false)
  let keyRows = $state(/** @type {{ field: string, dir: string }[]} */ ([{ field: '', dir: '1' }]))
  let name = $state('')
  let unique = $state(false)
  let sparse = $state(false)
  let hidden = $state(false)
  let ttl = $state('')
  let partial = $state('')
  let creating = $state(false)
  let formError = $state('')

  function openForm() {
    if (!guardWrite('create indexes')) return
    keyRows = [{ field: '', dir: '1' }]
    name = partial = ttl = ''
    unique = sparse = hidden = false
    formError = ''
    formOpen = true
  }

  async function create() {
    if (!adapter.indexes || !guardWrite('create indexes')) return
    const rows = keyRows.filter((r) => r.field.trim())
    if (!rows.length) {
      formError = 'Add at least one field'
      return
    }
    const keys = `{${rows.map((r) => `${JSON.stringify(r.field.trim())}: ${/^-?1$/.test(r.dir) ? r.dir : JSON.stringify(r.dir)}`).join(', ')}}`
    const ttlN = ttl.trim() ? Number(ttl) : null
    if (ttlN != null && (!Number.isInteger(ttlN) || ttlN < 0)) {
      formError = 'TTL is a whole number of seconds'
      return
    }
    creating = true
    formError = ''
    try {
      const n = await adapter.indexes.create(coll, keys, { name: name.trim() || null, unique, sparse, hidden, ttlSeconds: ttlN, partialFilter: partial.trim() || null })
      toast.success(`Created ${n}`)
      formOpen = false
      onchanged()
      await load()
    } catch (e) {
      formError = msg(e)
    } finally {
      creating = false
    }
  }

  /** @param {any} d */
  const arrow = (d) => (d === 1 ? '↑' : d === -1 ? '↓' : String(d))
  const listId = $derived(`idx-fields-${coll.replace(/\W/g, '_')}`)
  const field = 'field-surface h-7 min-w-0 px-2.5 font-mono text-ui-xs text-foreground shadow-none outline-none placeholder:text-muted-foreground/60'
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex h-10 shrink-0 items-center gap-2 pl-4 pr-2 text-ui-xs text-muted-foreground">
    {#if loaded}<span class="tabular-nums">{list.length} {list.length === 1 ? 'index' : 'indexes'}</span>{/if}
    <span class="flex-1"></span>
    <button type="button" class="inline-flex size-7 shrink-0 items-center justify-center rounded-md transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40" onclick={() => void load()} disabled={loading} title="Refresh" aria-label="Refresh indexes">
      <RefreshCw class={cn('size-3.5 shrink-0', loading && 'animate-spin')} />
    </button>
    {#if !formOpen}
      <Button size="sm" variant="secondary" class="rounded-md" onclick={openForm} disabled={readOnly} title={readOnly ? READ_ONLY_HINT : 'Create an index'}><Plus /> New index</Button>
    {/if}
  </div>

  <div class="app-scroll min-h-0 flex-1 overflow-auto px-2 pb-4">
    {#if formOpen}
      <form class="mx-2 mb-3 flex flex-col gap-3 rounded-xl bg-muted/30 p-3" onsubmit={(e) => { e.preventDefault(); void create() }}>
        <div class="flex items-center gap-2">
          <span class="flex-1 text-ui-xs font-medium text-foreground">New index</span>
          <button type="button" class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" onclick={() => (formOpen = false)} aria-label="Cancel"><X class="size-3.5 shrink-0" /></button>
        </div>
        <datalist id={listId}>{#each fields as f (f)}<option value={f}></option>{/each}</datalist>
        <div class="flex flex-col gap-1.5">
          <span class="text-ui-2xs text-muted-foreground">Keys</span>
          {#each keyRows as row, i (i)}
            <div class="flex items-center gap-1.5">
              <input bind:value={row.field} list={listId} placeholder="field.path" aria-label="Key field {i + 1}" spellcheck="false" autocomplete="off" class={cn(field, 'flex-1')} />
              <Select.Root type="single" value={row.dir} onValueChange={(v) => { if (v) row.dir = v }}>
                <Select.Trigger size="sm" class="h-7 w-28 text-ui-xs shadow-none" aria-label="Direction">{DIRS.find((d) => d.v === row.dir)?.l}</Select.Trigger>
                <Select.Content>
                  {#each DIRS as d (d.v)}<Select.Item value={d.v} label={d.l} />{/each}
                </Select.Content>
              </Select.Root>
              <button type="button" class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-30" disabled={keyRows.length === 1} onclick={() => (keyRows = keyRows.filter((_, j) => j !== i))} aria-label="Remove key"><X class="size-3.5 shrink-0" /></button>
            </div>
          {/each}
          <button type="button" class="inline-flex h-7 w-fit items-center gap-1.5 rounded-md px-2.5 text-ui-xs text-muted-foreground hover:bg-accent/50 hover:text-foreground" onclick={() => (keyRows = [...keyRows, { field: '', dir: '1' }])}><Plus class="size-3.5 shrink-0" /> Add key</button>
        </div>
        <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
          {#each [{ l: 'Unique', k: 'unique' }, { l: 'Sparse', k: 'sparse' }, { l: 'Hidden', k: 'hidden' }] as o (o.k)}
            <label class="inline-flex items-center gap-2 text-ui-xs text-foreground">
              {#if o.k === 'unique'}<Switch bind:checked={unique} />{:else if o.k === 'sparse'}<Switch bind:checked={sparse} />{:else}<Switch bind:checked={hidden} />{/if}
              {o.l}
            </label>
          {/each}
        </div>
        <div class="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-3 gap-y-2">
          <span class="text-ui-2xs text-muted-foreground">Name</span>
          <input bind:value={name} placeholder="generated from the keys" spellcheck="false" autocomplete="off" class={field} />
          <span class="text-ui-2xs text-muted-foreground">TTL seconds</span>
          <input bind:value={ttl} inputmode="numeric" placeholder="expire documents after… (date field)" class={field} />
          <span class="text-ui-2xs text-muted-foreground">Partial filter</span>
          <input bind:value={partial} placeholder={'{status: "active"}'} spellcheck="false" autocomplete="off" class={field} />
        </div>
        {#if formError}<p class="rounded-lg bg-destructive/5 px-3 py-1.5 text-ui-2xs text-destructive">{formError}</p>{/if}
        <div class="flex justify-end gap-1.5">
          <Button type="button" size="sm" variant="ghost" class="rounded-md" onclick={() => (formOpen = false)}>Cancel</Button>
          <Button type="submit" size="sm" class="rounded-md" disabled={creating || readOnly}>
            {#if creating}<LoaderCircle class="animate-spin" />{:else}<Check />{/if} Create
          </Button>
        </div>
      </form>
    {/if}

    {#if error}
      <p class="mx-2 rounded-lg bg-destructive/5 px-3 py-2 text-ui-2xs text-destructive">{error}</p>
    {:else if !loaded}
      <div class="loading-reveal flex items-center gap-2 px-2 py-6 text-ui-xs text-muted-foreground"><LoaderCircle class="size-3.5 shrink-0 animate-spin" /> Loading indexes…</div>
    {:else}
      <ul class="flex flex-col gap-px">
        {#each list as ix (ix.name)}
          <li class="group flex min-w-0 items-center gap-3 rounded-lg py-1.5 pl-3 pr-1 transition-colors hover:bg-muted/40">
            <div class="flex min-w-0 flex-1 flex-col gap-1">
              <div class="flex min-w-0 items-center gap-2">
                <span class="truncate font-mono text-ui-xs text-foreground" title={ix.name}>{ix.name}</span>
                {#each [ix.unique && 'unique', ix.sparse && 'sparse', ix.hidden && 'hidden', ix.ttlSeconds != null && `TTL ${ix.ttlSeconds.toLocaleString()}s`, ix.partialFilter && 'partial'].filter(Boolean) as flag (flag)}
                  <span class="shrink-0 rounded-full bg-muted/60 px-2 text-ui-2xs text-muted-foreground" title={flag === 'partial' ? JSON.stringify(ix.partialFilter) : undefined}>{flag}</span>
                {/each}
              </div>
              <div class="flex min-w-0 flex-wrap items-center gap-1">
                {#each ix.keys as [k, d] (k)}
                  <span class="rounded-full bg-muted/40 px-2 font-mono text-ui-2xs text-foreground/80">{k} {arrow(d)}</span>
                {/each}
              </div>
            </div>
            <div class="flex shrink-0 flex-col items-end gap-0.5 font-mono text-ui-2xs tabular-nums text-muted-foreground">
              {#if ix.ops != null}<span title="Operations that used this index since the server started">{ix.ops.toLocaleString()} ops</span>{/if}
              {#if ix.size != null}<span>{humanBytes(ix.size)}</span>{/if}
            </div>
            {#if ix.name !== '_id_'}
              <Button
                size="sm"
                variant={confirming === ix.name ? 'destructive' : 'ghost'}
                class={cn('rounded-md', confirming !== ix.name && 'text-muted-foreground opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:text-destructive')}
                onclick={() => void drop(ix.name)}
                onblur={() => { if (confirming === ix.name) confirming = '' }}
                disabled={readOnly || dropping === ix.name}
                title={readOnly ? READ_ONLY_HINT : `Drop ${ix.name}`}
                aria-label={`Drop ${ix.name}`}
              >
                {#if dropping === ix.name}<LoaderCircle class="animate-spin" />{:else}<Trash2 />{/if}
                {#if confirming === ix.name}Click again to drop{/if}
              </Button>
            {:else}
              <span class="w-8 shrink-0"></span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
