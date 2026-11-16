<script>
  // Shared links: every link this account made, how often each was opened, how
  // long it lasts, and a revoke that works at once (the content is deleted, so
  // even a copied link stops working). Titles and full links (with their keys)
  // come from this machine; the server only ever knew ids and counts.
  //
  // Opens as a tab; SharesOverlay shows the same page when there is no
  // connection and so no tab bar.
  import { untrack } from 'svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import Icon from './Icon.svelte'
  import Kbd from './Kbd.svelte'
  import SearchField from './SearchField.svelte'
  import { cn } from '$lib/utils.js'
  import { shareList, shareRevoke } from '$lib/api.js'
  import { localShares, forgetShare } from '$lib/share-flow.js'
  import { isSignedIn, signInWithToast } from '$lib/stores/account.js'

  let {
    /** Reload each time this turns true (the tab was shown again). */
    active = true,
    /** Close control in the header; none in a tab (the tab has its own). @type {(() => void) | null} */
    onclose = null,
  } = $props()

  /** @type {import('$lib/api.js').ShareRow[]} */
  let rows = $state([])
  /** @type {{ plan: 'free' | 'pro', limits: import('$lib/api.js').ShareLimits } | null} */
  let plan = $state(null)
  let loading = $state(false)
  let loaded = $state(false)
  let error = $state('')
  let copiedId = $state('')
  let revoking = $state('')
  let confirmId = $state('')
  let local = $state(localShares())
  let query = $state('')
  let filter = $state(/** @type {'active' | 'all'} */ ('active'))

  async function load() {
    if (loading) return
    loading = true
    error = ''
    try {
      const r = await shareList()
      rows = r.shares
      plan = { plan: r.plan, limits: r.limits }
      local = localShares()
    } catch (e) {
      error = String(/** @type {any} */ (e)?.message ?? e)
    } finally {
      loading = false
      loaded = true
    }
  }
  // Reload when shown, and when the account signs in.
  $effect(() => {
    if (!active) return
    void $isSignedIn
    untrack(() => void load())
  })

  // Each kind has its own tint, so a list of links reads at a glance.
  const KIND = /** @type {Record<string, { label: string, icon: string, tint: string }>} */ ({
    result: { label: 'Results', icon: 'table-2', tint: 'bg-success/12 text-success' },
    table: { label: 'Table rows', icon: 'table-2', tint: 'bg-success/12 text-success' },
    query: { label: 'Query', icon: 'code-2', tint: 'bg-warning/14 text-warning' },
    erd: { label: 'Schema diagram', icon: 'network', tint: 'bg-info/12 text-info' },
    mermaid: { label: 'Diagram', icon: 'workflow', tint: 'bg-info/12 text-info' },
    chart: { label: 'Chart', icon: 'bar-chart-3', tint: 'bg-primary/12 text-primary' },
  })
  /** @param {string} k */
  const kindOf = (k) => KIND[k] ?? { label: k, icon: 'share-2', tint: 'bg-muted text-muted-foreground' }

  /** @param {import('$lib/api.js').ShareRow} r */
  function status(r) {
    if (r.revoked) return { label: 'Revoked', live: false }
    if (r.expiresAt && r.expiresAt <= Date.now()) return { label: 'Expired', live: false }
    if (r.maxViews !== null && r.views >= r.maxViews) return { label: r.maxViews === 1 ? 'Opened' : 'Used up', live: false }
    return { label: 'Active', live: true }
  }

  /** @param {number} ms */
  function when(ms) {
    const d = ms - Date.now()
    const abs = Math.abs(d)
    const [unit, size] = abs < 3_600_000 ? ['minute', 60_000] : abs < 86_400_000 ? ['hour', 3_600_000] : ['day', 86_400_000]
    return new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' }).format(Math.round(d / /** @type {number} */ (size)), /** @type {Intl.RelativeTimeFormatUnit} */ (unit))
  }

  /** @param {number} n */
  function bytes(n) {
    return n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`
  }

  const activeCount = $derived(rows.filter((r) => r.active).length)
  const liveShown = $derived(shown.filter((r) => r.active))
  const endedShown = $derived(shown.filter((r) => !r.active))

  // ── Keyboard ──────────────────────────────────────────────────────────────
  // Up/Down move through the links (from the search box too), Enter copies,
  // Mod+Enter opens, Delete asks to revoke, Esc backs out of that.
  /** @type {HTMLDivElement | null} */
  let rootEl = $state(null)
  // Shown: take the keys, unless something inside already has focus.
  $effect(() => {
    if (!active || !rootEl) return
    const el = rootEl
    requestAnimationFrame(() => { if (!el.contains(document.activeElement)) el.focus({ preventScroll: true }) })
  })

  /** Rows in the order they're drawn: active first, then ended. */
  const order = $derived([...liveShown, ...endedShown])
  let selId = $state('')

  /** @param {KeyboardEvent} e */
  function onListKey(e) {
    if (!active || !order.length) return
    const t = /** @type {HTMLElement | null} */ (e.target instanceof HTMLElement ? e.target : null)
    // Typing in the search box keeps its keys, except the arrows and Enter.
    const inField = !!t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)
    const i = order.findIndex((r) => r.id === selId)
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const next = e.key === 'ArrowDown' ? Math.min(order.length - 1, i + 1) : Math.max(0, i - 1)
      selId = order[next].id
      document.querySelector(`[data-share="${selId}"]`)?.scrollIntoView({ block: 'nearest' })
      return
    }
    const r = order[i]
    if (!r) return
    if (e.key === 'Enter') {
      e.preventDefault()
      if (confirmId === r.id) { void revoke(r.id); return }
      const url = local[r.id]?.url
      if (!url || !r.active) return
      if (e.metaKey || e.ctrlKey) window.open(url, '_blank', 'noopener')
      else void copy(r.id)
      return
    }
    if (inField) return
    if ((e.key === 'Delete' || e.key === 'Backspace') && r.active) { e.preventDefault(); confirmId = r.id; return }
    if (e.key === 'Escape' && confirmId) { e.preventDefault(); e.stopPropagation(); confirmId = '' }
  }
  const totalViews = $derived(rows.reduce((n, r) => n + r.views, 0))
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase()
    return rows.filter((r) => {
      if (filter === 'active' && !r.active) return false
      if (!q) return true
      const t = `${local[r.id]?.title ?? ''} ${kindOf(r.kind).label}`.toLowerCase()
      return t.includes(q)
    })
  })

  /** @param {string} id */
  async function copy(id) {
    const url = local[id]?.url
    if (!url) return
    await navigator.clipboard.writeText(url).catch(() => {})
    copiedId = id
    setTimeout(() => { if (copiedId === id) copiedId = '' }, 1500)
  }

  /** @param {string} id */
  async function revoke(id) {
    confirmId = ''
    revoking = id
    try {
      await shareRevoke(id)
      forgetShare(id)
      rows = rows.map((r) => (r.id === id ? { ...r, revoked: true, active: false } : r))
      local = localShares()
    } catch (e) {
      error = String(/** @type {any} */ (e)?.message ?? e)
    } finally {
      revoking = ''
    }
  }

  const signedOut = $derived(!$isSignedIn || /sign in/i.test(error))
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_no_noninteractive_tabindex -->
<div bind:this={rootEl} tabindex="-1" class="flex min-h-0 flex-1 flex-col bg-background outline-none" onkeydown={onListKey}>
  <!-- Header, the same shape as Keyboard shortcuts -->
  <div class="flex h-12 shrink-0 items-center gap-2 border-b border-border/40 px-4">
    <h1 class="min-w-0 flex-1 truncate text-ui-sm font-medium text-foreground">Shared links</h1>
    {#if rows.length}
      <SearchField bind:value={query} placeholder="Search links" label="Search shared links" class="w-56 shrink-0" />
      <div role="radiogroup" aria-label="Show" class="flex shrink-0 gap-0.5 rounded-lg bg-muted/55 p-0.5">
        {#each [{ v: 'active', l: 'Active' }, { v: 'all', l: 'All' }] as f (f.v)}
          <button
            type="button"
            role="radio"
            aria-checked={filter === f.v}
            onclick={() => (filter = /** @type {'active' | 'all'} */ (f.v))}
            class={cn('h-7 rounded-md px-2.5 text-ui-xs font-medium transition-colors', filter === f.v ? 'bg-background text-foreground shadow-xs' : 'text-muted-foreground hover:text-foreground')}
          >{f.l}</button>
        {/each}
      </div>
    {/if}
    <Button variant="ghost" size="icon" class="size-8 shrink-0" aria-label="Refresh" title="Refresh" disabled={loading} onclick={() => void load()}>
      <Icon name="refresh-cw" class={cn('size-3.5 shrink-0', loading && 'animate-spin')} />
    </Button>
    {#if onclose}
      <Button variant="ghost" size="icon" class="size-8 shrink-0" aria-label="Close" onclick={onclose}>
        <Icon name="x" class="size-4 shrink-0" />
      </Button>
    {/if}
  </div>

  <div class="app-scroll min-h-0 flex-1 overflow-y-auto">
    <div class="mx-auto flex w-full max-w-3xl flex-col px-4 pt-3 pb-8">
      {#if signedOut && loaded}
        <div class="flex flex-col items-center gap-3 py-24 text-center">
          <span class="flex size-12 items-center justify-center rounded-lg bg-muted/60"><Icon name="lock" class="size-5 shrink-0 text-muted-foreground" /></span>
          <p class="mt-1 text-ui-sm font-medium text-foreground">Sign in to share</p>
          <p class="max-w-80 text-ui-xs leading-relaxed text-muted-foreground">Share links live on your Stroke account, so you can see who opened them and revoke them from any device.</p>
          <Button size="sm" class="mt-2" onclick={() => void signInWithToast()}>Sign in</Button>
        </div>
      {:else if !loaded}
        <div class="flex items-center justify-center gap-2 py-24 text-ui-xs text-muted-foreground">
          <Icon name="loader-2" class="size-3.5 shrink-0 animate-spin" />Loading
        </div>
      {:else}
        <!-- One quiet line of totals instead of a row of boxes. -->
        {#if plan}
          {@const cap = plan.limits.maxActive}
          <div class="mb-1 flex flex-wrap items-center gap-x-4 gap-y-2 px-2 text-ui-2xs text-muted-foreground">
            <span class="inline-flex items-center gap-1.5"><span class="size-1.5 rounded-full bg-success"></span><span class="font-medium text-foreground tabular-nums">{activeCount}</span> active</span>
            <span class="inline-flex items-center gap-1.5"><Icon name="eye" class="size-3.5 shrink-0" /><span class="font-medium text-foreground tabular-nums">{totalViews.toLocaleString()}</span> views</span>
            <span class="flex-1"></span>
            {#if cap !== null}
              <span class="inline-flex items-center gap-2">
                <span class="tabular-nums">{activeCount} of {cap} links on Free</span>
                <span class="h-1 w-16 overflow-hidden rounded-full bg-muted">
                  <span class={cn('block h-full rounded-full', activeCount >= cap ? 'bg-warning' : 'bg-primary')} style="width: {Math.min(100, (activeCount / cap) * 100)}%"></span>
                </span>
              </span>
            {:else}
              <span class="chip-gloss h-[1.125rem] px-1.5 text-ui-3xs font-medium">Pro</span>
            {/if}
          </div>
        {/if}

        {#if error && !signedOut}
          <p class="mb-4 flex items-start gap-1.5 rounded-lg bg-destructive/10 px-3 py-2 text-ui-xs text-destructive">
            <Icon name="alert-circle" class="mt-0.5 size-3.5 shrink-0" />{error}
          </p>
        {/if}

        {#if !rows.length}
          <div class="flex flex-col items-center gap-3 py-20 text-center">
            <span class="flex size-12 items-center justify-center rounded-lg bg-muted/60"><Icon name="share-2" class="size-5 shrink-0 text-muted-foreground" /></span>
            <p class="mt-1 text-ui-sm font-medium text-foreground">No shared links yet</p>
            <p class="max-w-96 text-ui-xs leading-relaxed text-muted-foreground">
              Press <span class="font-medium text-foreground">Share</span> above query results, a table, a chart or the schema diagram, or ask the AI to "share this chart". Everything is encrypted on this computer first.
            </p>
          </div>
        {:else if !shown.length}
          <p class="py-16 text-center text-ui-xs text-muted-foreground">
            {query ? 'No links match your search.' : 'No active links.'}
            {#if filter === 'active'}<button type="button" class="ms-1 font-medium text-foreground underline-offset-2 hover:underline" onclick={() => (filter = 'all')}>Show all</button>{/if}
          </p>
        {:else}
          {#if liveShown.length}
            {@render section('Active', liveShown)}
          {/if}
          {#if endedShown.length}
            {@render section('Ended', endedShown)}
          {/if}
          <p class="mt-4 flex items-start gap-1.5 px-2 text-ui-3xs leading-relaxed text-muted-foreground">
            <Icon name="lock" class="mt-0.5 size-3 shrink-0" />
            A link's key stays on the computer that made it, so links made elsewhere can't be copied here. Revoking deletes the content everywhere at once.
            <span class="ms-auto hidden shrink-0 items-center gap-1 ps-4 sm:inline-flex"><Kbd combo="ArrowUp" /><Kbd combo="ArrowDown" /> move <Kbd combo="Enter" /> copy <Kbd combo="Backspace" /> revoke</span>
          </p>
        {/if}
      {/if}
    </div>
  </div>
</div>

{#snippet section(/** @type {string} */ label, /** @type {import('$lib/api.js').ShareRow[]} */ list)}
  <div class="flex items-center gap-2 px-2 pt-3 pb-1">
    <span class="text-ui-2xs font-medium text-muted-foreground">{label}</span>
    <span class="text-ui-3xs text-muted-foreground/70 tabular-nums">{list.length}</span>
  </div>
  <ul class="flex flex-col" role="listbox" aria-label="{label} links">
    {#each list as r (r.id)}
      {@const st = status(r)}
      {@const mine = local[r.id]}
      {@const k = kindOf(r.kind)}
      <li
        role="option"
        data-share={r.id}
        aria-selected={selId === r.id}
        class={cn('group/row relative flex h-11 items-center gap-2.5 rounded-md px-2 transition-colors hover:bg-muted/45', selId === r.id && 'bg-muted/55')}
        onpointerenter={() => (selId = r.id)}
      >
        <span class={cn('flex size-7 shrink-0 items-center justify-center rounded-md', st.live ? k.tint : 'bg-muted/60 text-muted-foreground')}>
          <Icon name={k.icon} class="size-3.5 shrink-0" />
        </span>
        <div class="min-w-0 flex-1">
          <div class="flex min-w-0 items-center gap-1.5">
            <span class={cn('truncate text-ui-xs', st.live ? 'font-medium text-foreground' : 'text-muted-foreground')}>{mine?.title ?? k.label}</span>
            {#if r.hasPassword}<Icon name="lock" class="size-3 shrink-0 text-muted-foreground" aria-label="Password protected" />{/if}
            {#if r.maxViews === 1 && st.live}<Icon name="flame" class="size-3 shrink-0 text-warning" aria-label="Burns after one view" />{/if}
          </div>
          <div class="truncate text-ui-3xs text-muted-foreground">
            {k.label} · {when(r.createdAt)} · {bytes(r.size)}
          </div>
        </div>

        <!-- Accessories, Raycast style: quiet icon + value pairs. Hidden while
             the row's actions show. -->
        <div class={cn('flex shrink-0 items-center gap-4 text-ui-2xs text-muted-foreground tabular-nums group-hover/row:hidden group-focus-within/row:hidden', confirmId === r.id && 'hidden')}>
          <span class="inline-flex items-center gap-1" title="{r.views} view{r.views === 1 ? '' : 's'}{r.lastViewedAt ? `, last ${when(r.lastViewedAt)}` : ''}">
            <Icon name="eye" class="size-3.5 shrink-0" /><span>{r.views.toLocaleString()}{#if r.maxViews !== null}<span class="text-muted-foreground/60">/{r.maxViews}</span>{/if}</span>
          </span>
          <span class={cn('inline-flex min-w-24 items-center justify-end gap-1', !st.live && 'text-muted-foreground/70')}>
            <Icon name="clock" class="size-3.5 shrink-0" />
            {#if st.live}{r.expiresAt ? when(r.expiresAt) : 'No expiry'}{:else}{st.label}{/if}
          </span>
        </div>

        <!-- Actions on hover or focus -->
        <div class={cn('hidden shrink-0 items-center gap-0.5 group-hover/row:flex group-focus-within/row:flex', confirmId === r.id && 'flex')}>
          {#if confirmId === r.id}
            <span class="me-1 text-ui-2xs text-muted-foreground">Revoke this link?</span>
            <Button variant="ghost" size="sm" class="h-7 px-2" onclick={() => (confirmId = '')}>Keep</Button>
            <Button variant="destructive" size="sm" class="h-7 px-2.5" onclick={() => void revoke(r.id)}>Revoke</Button>
          {:else}
            {#if mine && r.active}
              <Button variant="ghost" size="sm" class="h-7 gap-1.5 px-2" onclick={() => void copy(r.id)}>
                <Icon name={copiedId === r.id ? 'check' : 'copy'} class={cn('size-3.5 shrink-0', copiedId === r.id && 'text-success')} />{copiedId === r.id ? 'Copied' : 'Copy'}
              </Button>
              <Button variant="ghost" size="icon" class="size-7" title="Open in browser" aria-label="Open in browser" onclick={() => window.open(mine.url, '_blank', 'noopener')}>
                <Icon name="external-link" class="size-3.5 shrink-0" />
              </Button>
            {/if}
            {#if r.active}
              <Button
                variant="ghost"
                size="icon"
                class="size-7 text-muted-foreground hover:text-destructive"
                title="Revoke"
                aria-label="Revoke link"
                disabled={revoking === r.id}
                onclick={() => (confirmId = r.id)}
              >
                <Icon name={revoking === r.id ? 'loader-2' : 'trash-2'} class={cn('size-3.5 shrink-0', revoking === r.id && 'animate-spin')} />
              </Button>
            {:else}
              <span class="px-2 text-ui-2xs text-muted-foreground">{st.label}</span>
            {/if}
          {/if}
        </div>
      </li>
    {/each}
  </ul>
{/snippet}
