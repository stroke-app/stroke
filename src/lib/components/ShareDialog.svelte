<script>
  // One dialog for every share: a result or table, a query, a chart, a schema
  // diagram or a Mermaid diagram. It opens on `shareRequest` (share-flow.js),
  // so any page shares by calling requestShare(source).
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'
  import { shareList } from '$lib/api.js'
  import { detectSensitiveColumns } from '$lib/share-crypto.js'
  import { shareRequest, buildPayload, publishShare, SHARE_EXPIRIES } from '$lib/share-flow.js'

  let { onopenmyshares = () => {} } = $props()

  const src = $derived($shareRequest)
  let open = $state(false)
  $effect(() => { open = !!src })
  function close() { open = false; shareRequest.set(null) }

  const KIND_LABEL = /** @type {Record<string, string>} */ ({
    result: 'query results', table: 'table rows', query: 'query', erd: 'schema diagram', mermaid: 'diagram', chart: 'chart',
  })
  const tabular = $derived(src?.kind === 'result' || src?.kind === 'table' || src?.kind === 'chart')

  /** @type {{ plan: 'free' | 'pro', limits: import('$lib/api.js').ShareLimits } | null} */
  let plan = $state(null)
  let planError = $state('')

  let title = $state('')
  let expiry = $state(/** @type {number | null} */ (86400))
  let access = $state(/** @type {'link' | 'once' | 'limit'} */ ('link'))
  let viewLimit = $state(10)
  let password = $state('')
  let usePassword = $state(false)
  /** Columns kept, and masked, by index. */
  let keep = $state(/** @type {Set<number>} */ (new Set()))
  let mask = $state(/** @type {Set<number>} */ (new Set()))
  /** @type {{ index: number, name: string, why: string }[]} */
  let sensitive = $state([])

  let busy = $state(false)
  let error = $state('')
  let link = $state('')
  let copied = $state(false)

  // A fresh form for each share; likely personal data is masked by default.
  $effect(() => {
    if (!src) return
    title = src.title
    expiry = 86400
    access = 'link'
    viewLimit = 10
    password = ''
    usePassword = false
    error = ''
    link = ''
    if (src.kind === 'result' || src.kind === 'table' || src.kind === 'chart') {
      keep = new Set(src.columns.map((_, i) => i))
      sensitive = detectSensitiveColumns(src.columns, src.rows)
      mask = new Set(sensitive.map((s) => s.index))
    } else {
      keep = new Set(); mask = new Set(); sensitive = []
    }
    plan = null
    planError = ''
    shareList()
      .then((r) => { plan = { plan: r.plan, limits: r.limits } })
      .catch((e) => { planError = String(e?.message ?? e) })
  })

  const pro = $derived(plan?.plan === 'pro')
  const rowCap = $derived(plan?.limits.rows ?? 1000)
  const rowCount = $derived(src && 'rows' in src ? src.rows.length : 0)
  const sensitiveByIndex = $derived(new Map(sensitive.map((s) => [s.index, s])))

  /** @param {number} i */
  function toggleKeep(i) {
    const n = new Set(keep)
    if (n.has(i)) n.delete(i); else n.add(i)
    keep = n
  }
  /** @param {number} i */
  function toggleMask(i) {
    const n = new Set(mask)
    if (n.has(i)) n.delete(i); else n.add(i)
    mask = n
  }

  async function create() {
    if (!src || busy) return
    if (tabular && keep.size === 0) { error = 'Keep at least one column.'; return }
    busy = true
    error = ''
    try {
      const payload = buildPayload(src, { title, keep, mask, rowCap })
      const res = await publishShare(payload, {
        expiresIn: expiry,
        maxViews: access === 'once' ? 1 : access === 'limit' ? Math.max(1, Math.min(10000, Math.round(viewLimit) || 1)) : null,
        password: usePassword && password ? password : null,
      })
      link = res.url
      await copy()
    } catch (e) {
      error = String(/** @type {any} */ (e)?.message ?? e)
    } finally {
      busy = false
    }
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(link)
      copied = true
      setTimeout(() => (copied = false), 1600)
    } catch {}
  }

  const lbl = 'text-ui-xs font-medium text-foreground/90'
  const tinp = 'field-surface h-9 w-full bg-transparent px-2.5 text-ui-sm text-foreground placeholder:text-muted-foreground outline-none transition-colors'
</script>

<Dialog.Root bind:open onOpenChange={(v) => { if (!v) close() }}>
  <Dialog.Content class="w-[min(34rem,calc(100vw-2rem))] gap-0 p-0">
    <Dialog.Header class="gap-1 px-6 pt-6 pe-12">
      <Dialog.Title class="text-ui-lg">Share {KIND_LABEL[src?.kind ?? ''] ?? ''}</Dialog.Title>
      <Dialog.Description class="flex items-center gap-1.5 text-ui-xs">
        <Icon name="shield-check" class="size-3.5 shrink-0" />
        Encrypted on this computer. The link holds the key; stroke.click can't read it.
      </Dialog.Description>
    </Dialog.Header>

    {#if link}
      <!-- Done: the link, already on the clipboard. -->
      <div class="flex flex-col gap-3 px-6 pt-5 pb-6">
        <div class="flex items-center gap-2">
          <input readonly value={link} class={cn(tinp, 'font-mono text-ui-xs')} onfocus={(e) => e.currentTarget.select()} aria-label="Share link" />
          <Button variant="secondary" class="h-9 shrink-0 gap-1.5" onclick={copy}>
            <Icon name={copied ? 'check' : 'copy'} class="size-3.5 shrink-0" />{copied ? 'Copied' : 'Copy'}
          </Button>
        </div>
        <p class="text-ui-2xs leading-relaxed text-muted-foreground">
          Copied. Anyone with this link{usePassword && password ? ' and the password' : ''} can view it{access === 'once' ? ' once' : access === 'limit' ? ` up to ${viewLimit} times` : ''}{expiry ? `, for ${SHARE_EXPIRIES.find((e) => e.seconds === expiry)?.label}` : ''}. Revoke it any time in My shares.
        </p>
      </div>
      <div class="flex items-center gap-2 border-t border-border/40 px-6 py-4">
        <Button variant="ghost" class="gap-1.5" onclick={() => { close(); onopenmyshares() }}>
          <Icon name="layout-list" class="size-3.5 shrink-0" />My shares
        </Button>
        <span class="flex-1"></span>
        <Button onclick={close}>Done</Button>
      </div>
    {:else}
      <div class="flex max-h-[min(70vh,40rem)] flex-col gap-5 overflow-y-auto px-6 pt-5 pb-6">
        <div class="flex flex-col gap-1.5">
          <label for="sh-title" class={lbl}>Title</label>
          <input id="sh-title" bind:value={title} class={tinp} placeholder="What this is" />
        </div>

        {#if tabular && src && 'columns' in src}
          <div class="flex flex-col gap-1.5">
            <div class="flex items-baseline justify-between">
              <span class={lbl}>Columns</span>
              <span class="text-ui-2xs text-muted-foreground">{keep.size} of {src.columns.length}</span>
            </div>
            <ul class="max-h-48 overflow-y-auto rounded-lg border border-border/60">
              {#each src.columns as col, i (i)}
                {@const hit = sensitiveByIndex.get(i)}
                <li class="flex h-9 items-center gap-2.5 border-b border-border/40 px-3 last:border-0">
                  <input type="checkbox" checked={keep.has(i)} onchange={() => toggleKeep(i)} class="size-3.5 shrink-0 accent-primary" aria-label="Include {col.name}" />
                  <span class={cn('min-w-0 flex-1 truncate font-mono text-ui-xs', keep.has(i) ? 'text-foreground' : 'text-muted-foreground line-through')}>{col.name}</span>
                  {#if hit}
                    <span class="shrink-0 text-ui-3xs text-warning">{hit.why === 'name' ? 'Personal data?' : `Looks like ${hit.why}`}</span>
                  {/if}
                  <label class="flex shrink-0 items-center gap-1.5 text-ui-2xs text-muted-foreground">
                    <Switch checked={mask.has(i)} onCheckedChange={() => toggleMask(i)} disabled={!keep.has(i)} aria-label="Mask {col.name}" />
                    Mask
                  </label>
                </li>
              {/each}
            </ul>
            <p class="text-ui-2xs text-muted-foreground">
              {#if rowCount > rowCap}
                Shares the first {rowCap.toLocaleString()} of {rowCount.toLocaleString()} rows{pro ? '' : ' on the free plan (Pro shares up to 100,000)'}.
              {:else}
                {rowCount.toLocaleString()} row{rowCount === 1 ? '' : 's'}. Masked values keep their shape (a••@e••••.com).
              {/if}
            </p>
          </div>
        {/if}

        <div class="flex flex-col gap-1.5">
          <span class={lbl} id="sh-exp">Expires after</span>
          <div role="radiogroup" aria-labelledby="sh-exp" class="grid grid-cols-5 gap-0.5 rounded-lg bg-muted/40 p-0.5">
            {#each SHARE_EXPIRIES as e (e.label)}
              {@const locked = e.seconds === null && !pro}
              <button
                type="button"
                role="radio"
                aria-checked={expiry === e.seconds}
                disabled={locked}
                title={locked ? 'Links that never expire are part of Stroke Pro' : undefined}
                onclick={() => (expiry = e.seconds)}
                class={cn(
                  'h-7 min-w-0 truncate rounded-md px-1.5 text-ui-xs font-medium transition-colors duration-150 disabled:opacity-45',
                  expiry === e.seconds ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground',
                )}
              >{e.label}{#if locked}<span class="ms-1 text-ui-3xs">Pro</span>{/if}</button>
            {/each}
          </div>
        </div>

        <div class="flex flex-col gap-1.5">
          <span class={lbl} id="sh-access">Who can open it</span>
          <div role="radiogroup" aria-labelledby="sh-access" class="grid grid-cols-3 gap-0.5 rounded-lg bg-muted/40 p-0.5">
            {#each [{ v: 'link', l: 'Anyone, any time' }, { v: 'once', l: 'Once, then gone' }, { v: 'limit', l: 'A few views' }] as a (a.v)}
              <button
                type="button"
                role="radio"
                aria-checked={access === a.v}
                onclick={() => (access = /** @type {'link'|'once'|'limit'} */ (a.v))}
                class={cn(
                  'h-7 min-w-0 truncate rounded-md px-1.5 text-ui-xs font-medium transition-colors duration-150',
                  access === a.v ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground',
                )}
              >{a.l}</button>
            {/each}
          </div>
          {#if access === 'limit'}
            <label class="mt-1 flex items-center gap-2 text-ui-xs text-muted-foreground">
              Up to
              <input type="number" min="1" max="10000" bind:value={viewLimit} class={cn(tinp, 'h-8 w-20 text-center tabular-nums')} />
              views
            </label>
          {/if}
        </div>

        <div class="flex flex-col gap-2">
          <label class="flex items-center justify-between gap-3">
            <span class="flex flex-col">
              <span class={lbl}>Password{#if !pro}<span class="ms-1.5 text-ui-3xs font-normal text-muted-foreground">Pro</span>{/if}</span>
              <span class="text-ui-2xs text-muted-foreground">Mixed into the key: the link alone won't open it.</span>
            </span>
            <Switch checked={usePassword} disabled={!pro} onCheckedChange={(v) => (usePassword = v === true)} aria-label="Protect with a password" />
          </label>
          {#if usePassword}
            <input type="password" bind:value={password} placeholder="Password" class={tinp} autocomplete="new-password" aria-label="Password" />
          {/if}
        </div>

        {#if planError}
          <p class="text-ui-xs text-destructive">{planError}</p>
        {:else if error}
          <p class="text-ui-xs text-destructive">{error}</p>
        {/if}
      </div>

      <div class="flex items-center gap-2 border-t border-border/40 px-6 py-4">
        <Button variant="ghost" class="gap-1.5" onclick={() => { close(); onopenmyshares() }}>
          <Icon name="layout-list" class="size-3.5 shrink-0" />My shares
        </Button>
        <span class="flex-1"></span>
        <Button variant="ghost" onclick={close}>Cancel</Button>
        <Button class="gap-1.5" disabled={busy || !!planError || (usePassword && !password)} onclick={() => void create()}>
          {#if busy}<Icon name="loader-2" class="size-3.5 shrink-0 animate-spin" />Encrypting{:else}<Icon name="link-2" class="size-3.5 shrink-0" />Create link{/if}
        </Button>
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
