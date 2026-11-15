<script>
  // One dialog for every share: a result or table, a query, a chart, a schema
  // diagram or a Mermaid diagram. It opens on `shareRequest` (share-flow.js),
  // so any page shares by calling requestShare(source).
  import { untrack } from 'svelte'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import Icon from './Icon.svelte'
  import SelectMenu from './SelectMenu.svelte'
  import { cn } from '$lib/utils.js'
  import { shareList } from '$lib/api.js'
  import { detectSensitiveColumns } from '$lib/share-crypto.js'
  import { shareRequest, buildPayload, publishShare, settleShare, SHARE_EXPIRIES, loadShareDefaults, openSharesPage } from '$lib/share-flow.js'

  const src = $derived($shareRequest)
  let open = $state(false)
  $effect(() => { open = !!src })
  function close() { open = false; settleShare(link || null); shareRequest.set(null) }

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
  /** Which step a Create link is on, for the button and the bar. */
  let stage = $state(/** @type {'encrypt' | 'upload'} */ ('encrypt'))
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
      stage = 'encrypt'
      const res = await publishShare(payload, {
        expiresIn: expiry,
        maxViews: access === 'once' ? 1 : access === 'limit' ? Math.max(1, Math.min(10000, Math.round(viewLimit) || 1)) : null,
        password: usePassword && password ? password : null,
      }, (st) => (stage = st))
      link = res.url
      settleShare(link)
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

  /** In the default browser. window.open does nothing inside the app's webview. */
  async function openLink() {
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl(link)
    } catch {
      window.open(link, '_blank', 'noopener,noreferrer')
    }
  }

  const ACCESS = /** @type {const} */ ([
    { v: 'link', l: 'Anyone with the link', icon: 'link-2' },
    { v: 'once', l: 'One view, then gone', icon: 'flame' },
    { v: 'limit', l: 'Limited views', icon: 'eye' },
  ])

  const expiryLabel = $derived(SHARE_EXPIRIES.find((e) => e.seconds === expiry)?.label ?? '')
  /** What the link was made with, as label and value rows. */
  const summary = $derived([
    { k: 'Expires', v: expiry ? `In ${expiryLabel.toLowerCase()}` : 'Never' },
    { k: 'Views', v: access === 'once' ? 'One, then deleted' : access === 'limit' ? `Up to ${viewLimit}` : 'Unlimited' },
    { k: 'Password', v: usePassword && password ? 'Required' : 'None' },
  ])

  const lbl = 'text-ui-xs font-medium text-foreground'
  const hint = 'text-ui-2xs leading-relaxed text-muted-foreground'
  const tinp = 'field-surface h-9 w-full bg-transparent px-2.5 text-ui-sm text-foreground placeholder:text-muted-foreground outline-none transition-colors'
  const row = 'flex items-center gap-x-4 gap-y-2.5 border-b border-border/40 px-3.5 py-3 last:border-0'

  /** "Never" is Pro: picking it without Pro says so instead of switching. */
  let lockedNote = $state('')
  /** @param {string} v */
  function pickExpiry(v) {
    if (v === 'never' && !pro) {
      lockedNote = 'Links that never expire are part of Stroke Pro.'
      return
    }
    lockedNote = ''
    expiry = v === 'never' ? null : Number(v)
  }
</script>

<Dialog.Root bind:open onOpenChange={(v) => { if (!v) close() }}>
  <Dialog.Content class="w-[min(32rem,calc(100vw-2rem))] gap-0 overflow-hidden p-0">
    <Dialog.Header class="flex-row items-center gap-3 px-5 pt-5 pb-4 pe-12 text-start">
      <span class={cn('flex size-9 shrink-0 items-center justify-center rounded-lg', link ? 'bg-success/12 text-success' : 'bg-muted/60 text-foreground')}>
        <Icon name={link ? 'check' : kind.icon} class="size-4 shrink-0" />
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
      <!-- Done: the link (already on the clipboard), then what it was made with. -->
      <div class="flex flex-col gap-3 px-5 pb-5">
        <div class="flex h-10 items-center gap-2 rounded-lg border border-border/60 bg-muted/30 ps-3 pe-1">
          <Icon name="link-2" class="size-3.5 shrink-0 text-muted-foreground" />
          <button type="button" class="min-w-0 flex-1 truncate text-start font-mono text-ui-xs text-foreground select-all" title={link} onclick={copy}>{link}</button>
          <Button size="sm" variant={copied ? 'outline' : 'default'} class="h-8 w-[5.5rem] shrink-0 gap-1.5" onclick={copy}>
            <Icon name={copied ? 'check' : 'copy'} class="size-3.5 shrink-0" />{copied ? 'Copied' : 'Copy'}
          </Button>
        </div>
        <dl class="flex flex-col rounded-lg border border-border/60">
          {#each summary as r (r.k)}
            <div class="flex h-9 items-center justify-between gap-3 border-b border-border/40 px-3.5 text-ui-xs last:border-0">
              <dt class="text-muted-foreground">{r.k}</dt>
              <dd class="font-medium text-foreground">{r.v}</dd>
            </div>
          {/each}
        </dl>
        <p class={hint}>
          It's on your clipboard. The key is the part after <span class="font-mono">#</span>, so send the whole link{usePassword && password ? ', and the password separately' : ''}. Revoke it any time from Shared links.
        </p>
      </div>
      <div class="flex items-center gap-2 border-t border-border/40 px-5 py-3">
        <Button variant="ghost" size="sm" class="gap-1.5 text-muted-foreground" onclick={openMyShares}>
          <Icon name="layout-list" class="size-3.5 shrink-0" />Shared links
        </Button>
        <span class="flex-1"></span>
        <Button variant="outline" size="sm" class="gap-1.5" onclick={() => void openLink()}>
          <Icon name="external-link" class="size-3.5 shrink-0" />Open
        </Button>
        <Button size="sm" onclick={close}>Done</Button>
      </div>
    {:else}
      <div class={cn('app-scroll flex max-h-[min(68vh,38rem)] flex-col gap-5 overflow-y-auto px-5 pb-5 transition-opacity duration-150', busy && 'pointer-events-none opacity-60')} inert={busy || undefined}>
        <div class="flex flex-col gap-1.5">
          <label for="sh-title" class={lbl}>Title</label>
          <input id="sh-title" bind:value={title} class={tinp} placeholder="What this is" />
        </div>

        {#if tabular && src && 'columns' in src}
          <section class="flex flex-col gap-1.5" aria-labelledby="sh-cols">
            <div class="flex items-baseline justify-between gap-3">
              <span id="sh-cols" class={lbl}>Columns</span>
              <span class="text-ui-2xs text-muted-foreground tabular-nums">
                {keep.size} of {colCount} shared{#if mask.size} · {mask.size} masked{/if}
              </span>
            </div>
            <!-- One row per column: include it, and mask it. Masked rows say so
                 in words; the rest keep a quiet Mask button. -->
            <ul class="app-scroll max-h-56 overflow-y-auto rounded-lg border border-border/60">
              {#each src.columns as col, i (i)}
                {@const hit = sensitiveByIndex.get(i)}
                {@const kept = keep.has(i)}
                {@const masked = mask.has(i)}
                <li class="group flex h-10 items-center gap-3 border-b border-border/40 ps-3 pe-2 last:border-0">
                  <input type="checkbox" checked={kept} onchange={() => toggleKeep(i)} class="size-3.5 shrink-0 accent-primary" aria-label="Include {col.name}" />
                  <span class="flex min-w-0 flex-1 items-baseline gap-2">
                    <span class={cn('truncate text-ui-sm', kept ? 'text-foreground' : 'text-muted-foreground line-through')}>{col.name}</span>
                    {#if col.type}<span class="shrink-0 truncate font-mono text-ui-3xs text-muted-foreground/80">{col.type}</span>{/if}
                  </span>
                  {#if hit && !masked}
                    <span class="inline-flex shrink-0 items-center gap-1 text-ui-2xs text-warning" title={hit.why === 'name' ? 'The name suggests personal data or a secret' : `Most values look like ${hit.why}`}>
                      <Icon name="shield-alert" class="size-3 shrink-0" />{hit.why === 'name' ? 'Sensitive' : hit.why}
                    </span>
                  {/if}
                  <button
                    type="button"
                    disabled={!kept}
                    aria-pressed={masked}
                    aria-label="Mask {col.name}"
                    onclick={() => toggleMask(i)}
                    class={cn(
                      'inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-ui-xs font-medium transition-colors disabled:pointer-events-none disabled:opacity-40',
                      masked ? 'bg-primary/10 text-primary hover:bg-primary/15' : 'text-muted-foreground hover:bg-muted hover:text-foreground',
                    )}
                  >
                    <Icon name={masked ? 'eye-off' : 'eye'} class="size-3.5 shrink-0" />{masked ? 'Masked' : 'Mask'}
                  </button>
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
          </section>
        {/if}

        <!-- The link's rules as settings rows: what it is on the left, the
             choice on the right. -->
        <section class="flex flex-col rounded-lg border border-border/60" aria-label="Link settings">
          <div class={row}>
            <div class="min-w-0 flex-1">
              <p class={lbl}>Expires</p>
              <p class={hint}>{lockedNote || (expiry ? `The link stops working after ${expiryLabel.toLowerCase()}.` : 'The link keeps working until you revoke it.')}</p>
            </div>
            <SelectMenu
              ariaLabel="Expires"
              triggerClass="h-8 w-32"
              contentClass="w-[var(--bits-popover-anchor-width)] min-w-[9rem]"
              align="end"
              value={expiry === null ? 'never' : String(expiry)}
              onValueChange={(v) => pickExpiry(v)}
              items={SHARE_EXPIRIES.map((e) => ({ value: e.seconds === null ? 'never' : String(e.seconds), label: e.label, hint: e.seconds === null && !pro ? 'Pro' : undefined }))}
            />
          </div>
          <div class={row}>
            <div class="min-w-0 flex-1">
              <p class={lbl}>Views</p>
              {#if access === 'limit'}
                <label class="mt-1 flex items-center gap-1.5 text-ui-2xs text-muted-foreground">
                  Stops working after
                  <input type="number" min="1" max="10000" bind:value={viewLimit} class={cn(tinp, 'h-7 w-16 px-1.5 text-center text-ui-xs tabular-nums')} aria-label="View limit" />
                  views
                </label>
              {:else}
                <p class={hint}>{access === 'once' ? "The first person to open it gets it, then it's deleted. Link previews don't count." : 'Anyone with the link can open it, as often as they like.'}</p>
              {/if}
            </div>
            <SelectMenu
              ariaLabel="Views"
              triggerClass="h-8 w-44"
              contentClass="w-[var(--bits-popover-anchor-width)] min-w-[12rem]"
              align="end"
              value={access}
              onValueChange={(v) => (access = v)}
              items={ACCESS.map((a) => ({ value: a.v, label: a.l, icon: a.icon }))}
            />
          </div>
          <div class={cn(row, 'flex-wrap')}>
            <div class="min-w-0 flex-1">
              <p class={cn(lbl, 'flex items-center gap-1.5')}>
                Password
                {#if plan && !pro}<span class="inline-flex h-4 items-center rounded-full bg-primary/12 px-1.5 text-ui-3xs font-semibold text-primary">PRO</span>{/if}
              </p>
              <p class={hint}>Part of the key, so the link alone won't open it.</p>
            </div>
            <Switch checked={usePassword} disabled={!pro} onCheckedChange={(v) => (usePassword = v === true)} aria-label="Protect with a password" />
            {#if usePassword}
              <input type="password" bind:value={password} placeholder="Password" class={cn(tinp, 'basis-full')} autocomplete="new-password" aria-label="Password" />
            {/if}
          </div>
        </section>

        {#if planError || error}
          <p class="flex items-start gap-1.5 rounded-lg bg-destructive/10 px-3 py-2 text-ui-xs text-destructive">
            <Icon name="alert-circle" class="mt-0.5 size-3.5 shrink-0" />{planError || error}
          </p>
        {/if}
      </div>

      <div class="relative flex items-center gap-2 border-t border-border/40 px-5 py-3">
        {#if busy}
          <!-- Encrypting is quick; uploading waits on the network. The bar
               fills to a third for the first and runs on for the second. -->
          <div class="absolute inset-x-0 -top-px h-0.5 overflow-hidden" role="progressbar" aria-label={stage === 'encrypt' ? 'Encrypting' : 'Uploading'}>
            <div class="h-full bg-primary transition-[width] duration-700 ease-out" style:width={stage === 'encrypt' ? '35%' : '85%'}></div>
          </div>
        {/if}
        <Button variant="ghost" size="sm" class="gap-1.5 text-muted-foreground" disabled={busy} onclick={openMyShares}>
          <Icon name="layout-list" class="size-3.5 shrink-0" />Shared links
        </Button>
        <span class="flex-1"></span>
        <Button variant="ghost" size="sm" disabled={busy} onclick={close}>Cancel</Button>
        <Button size="sm" class="gap-1.5" disabled={busy || !!planError || (usePassword && !password)} onclick={() => void create()}>
          {#if busy}<Icon name="loader-2" class="size-3.5 shrink-0 animate-spin" />{stage === 'encrypt' ? 'Encrypting' : 'Uploading'}{:else}<Icon name="link-2" class="size-3.5 shrink-0" />Create link{/if}
        </Button>
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
