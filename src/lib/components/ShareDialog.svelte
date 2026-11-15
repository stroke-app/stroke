<script>
  // One dialog for every share: a result or table, a query, a chart, a schema
  // diagram or a Mermaid diagram. It opens on `shareRequest` (share-flow.js),
  // so any page shares by calling requestShare(source).
  import { untrack } from 'svelte'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'
  import { shareList } from '$lib/api.js'
  import { detectSensitiveColumns } from '$lib/share-crypto.js'
  import { shareRequest, buildPayload, publishShare, SHARE_EXPIRIES, loadShareDefaults, openSharesPage } from '$lib/share-flow.js'

  const src = $derived($shareRequest)
  let open = $state(false)
  $effect(() => { open = !!src })
  function close() { open = false; shareRequest.set(null) }

  const KIND = /** @type {Record<string, { label: string, icon: string }>} */ ({
    result: { label: 'query results', icon: 'table-2' },
    table: { label: 'table rows', icon: 'table-2' },
    query: { label: 'query', icon: 'code-2' },
    erd: { label: 'schema diagram', icon: 'network' },
    mermaid: { label: 'diagram', icon: 'workflow' },
    chart: { label: 'chart', icon: 'bar-chart-3' },
  })
  const kind = $derived(KIND[src?.kind ?? ''] ?? { label: '', icon: 'share-2' })
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

  // A fresh form for each share, from the defaults in Settings. Only `src` is
  // a dependency: everything else is written untracked. (Reading `sensitive`
  // after writing it here made the effect depend on itself and loop until
  // Svelte gave up with effect_update_depth_exceeded.)
  $effect(() => {
    const s = src
    if (!s) return
    untrack(() => reset(s))
  })

  /** @param {import('$lib/share-flow.js').ShareSource} s */
  function reset(s) {
    const d = loadShareDefaults()
    title = s.title
    expiry = d.expiry
    access = d.access
    viewLimit = d.viewLimit
    password = ''
    usePassword = false
    error = ''
    link = ''
    copied = false
    if (s.kind === 'result' || s.kind === 'table' || s.kind === 'chart') {
      const hits = detectSensitiveColumns(s.columns, s.rows)
      keep = new Set(s.columns.map((_, i) => i))
      sensitive = hits
      mask = new Set(d.maskPii ? hits.map((h) => h.index) : [])
    } else {
      keep = new Set(); mask = new Set(); sensitive = []
    }
    plan = null
    planError = ''
    shareList()
      .then((r) => {
        plan = { plan: r.plan, limits: r.limits }
        // A Pro-only default on the free plan falls back to the longest it allows.
        if (r.plan !== 'pro' && expiry === null) expiry = 7 * 86400
      })
      .catch((e) => { planError = String(e?.message ?? e) })
  }

  const pro = $derived(plan?.plan === 'pro')
  const rowCap = $derived(plan?.limits.rows ?? 1000)
  const rowCount = $derived(src && 'rows' in src ? src.rows.length : 0)
  const sensitiveByIndex = $derived(new Map(sensitive.map((s) => [s.index, s])))
  const colCount = $derived(src && 'columns' in src ? src.columns.length : 0)

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

  function openMyShares() { close(); openSharesPage() }

  const ACCESS = /** @type {const} */ ([
    { v: 'link', l: 'Anyone with the link', icon: 'link-2' },
    { v: 'once', l: 'One view, then gone', icon: 'flame' },
    { v: 'limit', l: 'Limited views', icon: 'eye' },
  ])

  const expiryLabel = $derived(SHARE_EXPIRIES.find((e) => e.seconds === expiry)?.label ?? '')
  const summary = $derived([
    expiry ? `Expires in ${expiryLabel}` : 'Never expires',
    access === 'once' ? 'One view' : access === 'limit' ? `${viewLimit} views` : 'Unlimited views',
    usePassword && password ? 'Password' : null,
  ].filter(Boolean))

  const lbl = 'text-ui-xs font-medium text-foreground'
  const hint = 'text-ui-2xs leading-relaxed text-muted-foreground'
  const tinp = 'field-surface h-9 w-full bg-transparent px-2.5 text-ui-sm text-foreground placeholder:text-muted-foreground outline-none transition-colors'
  const seg = 'h-7 min-w-0 truncate rounded-md px-2 text-ui-xs font-medium transition-colors duration-150 disabled:cursor-not-allowed disabled:opacity-45'
  const segOn = 'bg-background text-foreground shadow-xs'
  const segOff = 'text-muted-foreground hover:text-foreground'
</script>

<Dialog.Root bind:open onOpenChange={(v) => { if (!v) close() }}>
  <Dialog.Content class="w-[min(32rem,calc(100vw-2rem))] gap-0 overflow-hidden p-0">
    <Dialog.Header class="flex-row items-center gap-3 px-5 pt-5 pb-4 pe-12 text-start">
      <span class="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted/60 text-foreground">
        <Icon name={link ? 'check' : kind.icon} class={cn('size-4 shrink-0', link && 'text-success')} />
      </span>
      <div class="flex min-w-0 flex-col gap-0.5">
        <Dialog.Title class="truncate text-ui-sm font-semibold">{link ? 'Link ready' : `Share ${kind.label}`}</Dialog.Title>
        <Dialog.Description class="flex items-center gap-1 text-ui-2xs text-muted-foreground">
          <Icon name="lock" class="size-3 shrink-0" />
          End-to-end encrypted. Only people with the link can read it.
        </Dialog.Description>
      </div>
    </Dialog.Header>

    {#if link}
      <!-- Done: the link, already on the clipboard. -->
      <div class="flex flex-col gap-3 px-5 pb-5">
        <div class="field-surface flex h-10 items-center gap-1 ps-3 pe-1">
          <Icon name="link-2" class="size-3.5 shrink-0 text-muted-foreground" />
          <input readonly value={link} class="min-w-0 flex-1 truncate bg-transparent font-mono text-ui-xs text-foreground outline-none" onfocus={(e) => e.currentTarget.select()} aria-label="Share link" />
          <Button size="sm" class="h-8 shrink-0 gap-1.5" onclick={copy}>
            <Icon name={copied ? 'check' : 'copy'} class="size-3.5 shrink-0" />{copied ? 'Copied' : 'Copy'}
          </Button>
        </div>
        <div class="flex flex-wrap items-center gap-1.5">
          {#each summary as s (s)}
            <span class="inline-flex h-6 items-center rounded-full bg-muted/60 px-2.5 text-ui-2xs text-muted-foreground">{s}</span>
          {/each}
        </div>
        <p class={hint}>
          It's on your clipboard. The key is the part after <span class="font-mono">#</span>, so send the whole link{usePassword && password ? ', and the password separately' : ''}. Revoke it any time from Shared links.
        </p>
      </div>
      <div class="flex items-center gap-2 border-t border-border/40 bg-muted/20 px-5 py-3">
        <Button variant="ghost" size="sm" class="gap-1.5" onclick={openMyShares}>
          <Icon name="layout-list" class="size-3.5 shrink-0" />Shared links
        </Button>
        <span class="flex-1"></span>
        <Button variant="outline" size="sm" class="gap-1.5" onclick={() => window.open(link, '_blank', 'noopener')}>
          <Icon name="external-link" class="size-3.5 shrink-0" />Open
        </Button>
        <Button size="sm" onclick={close}>Done</Button>
      </div>
    {:else}
      <div class="app-scroll flex max-h-[min(68vh,38rem)] flex-col gap-5 overflow-y-auto px-5 pb-5">
        <div class="flex flex-col gap-1.5">
          <label for="sh-title" class={lbl}>Title</label>
          <input id="sh-title" bind:value={title} class={tinp} placeholder="What this is" />
        </div>

        {#if tabular && src && 'columns' in src}
          <div class="flex flex-col gap-1.5">
            <div class="flex items-baseline justify-between gap-3">
              <span class={lbl}>Columns</span>
              <span class="text-ui-2xs text-muted-foreground tabular-nums">
                {keep.size} of {colCount} shared{#if mask.size} · {mask.size} masked{/if}
              </span>
            </div>
            <ul class="app-scroll max-h-52 overflow-y-auto rounded-lg border border-border/60">
              {#each src.columns as col, i (i)}
                {@const hit = sensitiveByIndex.get(i)}
                {@const kept = keep.has(i)}
                <li class="flex h-9 items-center gap-2.5 border-b border-border/40 px-3 last:border-0">
                  <input type="checkbox" checked={kept} onchange={() => toggleKeep(i)} class="size-3.5 shrink-0 accent-primary" aria-label="Include {col.name}" />
                  <span class={cn('min-w-0 flex-1 truncate font-mono text-ui-xs', kept ? 'text-foreground' : 'text-muted-foreground line-through')}>{col.name}</span>
                  {#if hit}
                    <span class="inline-flex h-5 shrink-0 items-center gap-1 rounded-full bg-warning/12 px-2 text-ui-3xs font-medium text-warning" title={hit.why === 'name' ? 'The name suggests personal data or a secret' : `Most values look like ${hit.why}`}>
                      <Icon name="shield-alert" class="size-3 shrink-0" />{hit.why === 'name' ? 'Sensitive' : hit.why}
                    </span>
                  {/if}
                  <label class={cn('flex shrink-0 items-center gap-1.5 text-ui-2xs', kept ? 'text-muted-foreground' : 'text-muted-foreground/50')}>
                    Mask
                    <Switch checked={mask.has(i)} onCheckedChange={() => toggleMask(i)} disabled={!kept} aria-label="Mask {col.name}" />
                  </label>
                </li>
              {/each}
            </ul>
            <p class={hint}>
              {#if rowCount > rowCap}
                Shares the first {rowCap.toLocaleString()} of {rowCount.toLocaleString()} rows{pro ? '' : '. Pro shares up to 100,000'}.
              {:else}
                {rowCount.toLocaleString()} row{rowCount === 1 ? '' : 's'}. Masked values keep their shape, like a••@e••••.com.
              {/if}
            </p>
          </div>
        {/if}

        <div class="flex flex-col gap-1.5">
          <span class={lbl} id="sh-exp">Expires</span>
          <div role="radiogroup" aria-labelledby="sh-exp" class="grid grid-cols-5 gap-0.5 rounded-lg bg-muted/50 p-0.5">
            {#each SHARE_EXPIRIES as e (e.label)}
              {@const locked = e.seconds === null && !pro}
              <button
                type="button"
                role="radio"
                aria-checked={expiry === e.seconds}
                disabled={locked}
                title={locked ? 'Links that never expire are part of Stroke Pro' : undefined}
                onclick={() => (expiry = e.seconds)}
                class={cn(seg, 'inline-flex items-center justify-center gap-1', expiry === e.seconds ? segOn : segOff)}
              >{#if locked}<Icon name="lock" class="size-3 shrink-0" />{/if}{e.label}</button>
            {/each}
          </div>
        </div>

        <div class="flex flex-col gap-1.5">
          <span class={lbl} id="sh-access">Views</span>
          <div role="radiogroup" aria-labelledby="sh-access" class="grid grid-cols-3 gap-0.5 rounded-lg bg-muted/50 p-0.5">
            {#each ACCESS as a (a.v)}
              <button
                type="button"
                role="radio"
                aria-checked={access === a.v}
                onclick={() => (access = a.v)}
                class={cn(seg, 'inline-flex items-center justify-center gap-1.5', access === a.v ? segOn : segOff)}
              ><Icon name={a.icon} class="size-3.5 shrink-0" /><span class="truncate">{a.l}</span></button>
            {/each}
          </div>
          {#if access === 'limit'}
            <label class="mt-1 flex items-center gap-2 text-ui-xs text-muted-foreground">
              Stops working after
              <input type="number" min="1" max="10000" bind:value={viewLimit} class={cn(tinp, 'h-8 w-20 text-center tabular-nums')} />
              views
            </label>
          {:else if access === 'once'}
            <p class={hint}>The first person to open it gets it, then it's deleted. Link previews in chat apps don't count as a view.</p>
          {/if}
        </div>

        <div class="flex flex-col gap-2 rounded-lg border border-border/60 p-3">
          <div class="flex items-center justify-between gap-3">
            <div class="flex min-w-0 flex-col gap-0.5">
              <span class={cn(lbl, 'flex items-center gap-1.5')}>
                Password
                {#if plan && !pro}<span class="inline-flex h-4 items-center rounded-full bg-primary/12 px-1.5 text-ui-3xs font-semibold text-primary">PRO</span>{/if}
              </span>
              <span class={hint}>Part of the key, so the link alone won't open it.</span>
            </div>
            <Switch checked={usePassword} disabled={!pro} onCheckedChange={(v) => (usePassword = v === true)} aria-label="Protect with a password" />
          </div>
          {#if usePassword}
            <input type="password" bind:value={password} placeholder="Password" class={tinp} autocomplete="new-password" aria-label="Password" />
          {/if}
        </div>

        {#if planError || error}
          <p class="flex items-start gap-1.5 rounded-lg bg-destructive/10 px-3 py-2 text-ui-xs text-destructive">
            <Icon name="alert-circle" class="mt-0.5 size-3.5 shrink-0" />{planError || error}
          </p>
        {/if}
      </div>

      <div class="flex items-center gap-2 border-t border-border/40 bg-muted/20 px-5 py-3">
        <Button variant="ghost" size="sm" class="gap-1.5 text-muted-foreground" onclick={openMyShares}>
          <Icon name="layout-list" class="size-3.5 shrink-0" />Shared links
        </Button>
        <span class="flex-1"></span>
        <Button variant="ghost" size="sm" onclick={close}>Cancel</Button>
        <Button size="sm" class="gap-1.5" disabled={busy || !!planError || (usePassword && !password)} onclick={() => void create()}>
          {#if busy}<Icon name="loader-2" class="size-3.5 shrink-0 animate-spin" />Encrypting{:else}<Icon name="link-2" class="size-3.5 shrink-0" />Create link{/if}
        </Button>
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
