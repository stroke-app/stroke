<script>
  // Every link this account made: who has opened them, how long they last,
  // and a revoke that takes effect at once (the content is deleted, so even a
  // copy of the link stops working). Titles and full links (with their keys)
  // come from this machine; the server only ever knew ids and counts.
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'
  import { shareList, shareRevoke } from '$lib/api.js'
  import { localShares, forgetShare } from '$lib/share-flow.js'

  let { open = $bindable(false) } = $props()

  /** @type {import('$lib/api.js').ShareRow[]} */
  let rows = $state([])
  let plan = $state(/** @type {'free' | 'pro' | null} */ (null))
  let loading = $state(false)
  let error = $state('')
  let copiedId = $state('')
  let revoking = $state('')
  let local = $state(localShares())

  async function load() {
    loading = true
    error = ''
    try {
      const r = await shareList()
      rows = r.shares
      plan = r.plan
      local = localShares()
    } catch (e) {
      error = String(/** @type {any} */ (e)?.message ?? e)
    } finally {
      loading = false
    }
  }
  $effect(() => { if (open) void load() })

  const KIND = /** @type {Record<string, string>} */ ({
    result: 'Results', table: 'Table', query: 'Query', erd: 'Schema diagram', mermaid: 'Diagram', chart: 'Chart',
  })

  /** @param {import('$lib/api.js').ShareRow} r */
  function status(r) {
    if (r.revoked) return { label: 'Revoked', tone: 'muted' }
    if (r.expiresAt && r.expiresAt <= Date.now()) return { label: 'Expired', tone: 'muted' }
    if (r.maxViews !== null && r.views >= r.maxViews) return { label: r.maxViews === 1 ? 'Opened' : 'Used up', tone: 'muted' }
    return { label: 'Active', tone: 'success' }
  }

  /** @param {number} ms */
  function when(ms) {
    const d = ms - Date.now()
    const abs = Math.abs(d)
    const [unit, size] = abs < 3_600_000 ? ['minute', 60_000] : abs < 86_400_000 ? ['hour', 3_600_000] : ['day', 86_400_000]
    return new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' }).format(Math.round(d / /** @type {number} */ (size)), /** @type {Intl.RelativeTimeFormatUnit} */ (unit))
  }

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
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="w-[min(40rem,calc(100vw-2rem))] gap-0 p-0">
    <Dialog.Header class="gap-1 px-6 pt-6 pe-12">
      <Dialog.Title class="text-ui-lg">My shares</Dialog.Title>
      <Dialog.Description class="text-ui-xs">
        Links you've made{plan ? ` on the ${plan === 'pro' ? 'Pro' : 'free'} plan` : ''}. Revoking deletes the content, so the link stops working everywhere at once.
      </Dialog.Description>
    </Dialog.Header>

    <div class="max-h-[min(60vh,32rem)] min-h-40 overflow-y-auto px-3 pt-4 pb-3">
      {#if loading && !rows.length}
        <div class="flex items-center justify-center gap-2 py-12 text-ui-xs text-muted-foreground">
          <Icon name="loader-2" class="size-3.5 shrink-0 animate-spin" />Loading
        </div>
      {:else if error && !rows.length}
        <p class="px-3 py-10 text-center text-ui-xs text-destructive">{error}</p>
      {:else if !rows.length}
        <div class="flex flex-col items-center gap-2 py-12 text-center">
          <Icon name="share-2" class="size-5 shrink-0 text-muted-foreground" />
          <p class="text-ui-sm text-foreground">No shares yet</p>
          <p class="max-w-72 text-ui-2xs text-muted-foreground">Share a result, a chart or a diagram from its toolbar, or ask the AI to share one.</p>
        </div>
      {:else}
        <ul class="flex flex-col">
          {#each rows as r (r.id)}
            {@const st = status(r)}
            {@const mine = local[r.id]}
            <li class="group flex items-center gap-3 rounded-lg px-3 py-2.5 hover:bg-muted/40">
              <span class={cn('size-1.5 shrink-0 rounded-full', st.tone === 'success' ? 'bg-success' : 'bg-muted-foreground/40')} aria-hidden="true"></span>
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <span class={cn('truncate text-ui-sm', r.active ? 'text-foreground' : 'text-muted-foreground')}>{mine?.title ?? KIND[r.kind] ?? r.kind}</span>
                  {#if r.hasPassword}<Icon name="lock" class="size-3 shrink-0 text-muted-foreground" aria-label="Password protected" />{/if}
                </div>
                <div class="mt-0.5 flex flex-wrap items-center gap-x-2 text-ui-2xs text-muted-foreground tabular-nums">
                  <span>{KIND[r.kind] ?? r.kind}</span>
                  <span aria-hidden="true">·</span>
                  <span>{r.views} view{r.views === 1 ? '' : 's'}{r.maxViews !== null ? ` of ${r.maxViews}` : ''}</span>
                  <span aria-hidden="true">·</span>
                  <span>{st.label === 'Active' ? (r.expiresAt ? `Expires ${when(r.expiresAt)}` : 'Never expires') : st.label}</span>
                  {#if r.lastViewedAt}<span aria-hidden="true">·</span><span>Last opened {when(r.lastViewedAt)}</span>{/if}
                </div>
              </div>
              {#if r.active}
                {#if mine}
                  <Button variant="ghost" size="sm" class="h-7 gap-1.5" onclick={() => void copy(r.id)}>
                    <Icon name={copiedId === r.id ? 'check' : 'copy'} class="size-3.5 shrink-0" />{copiedId === r.id ? 'Copied' : 'Copy link'}
                  </Button>
                {/if}
                <Button variant="ghost" size="sm" class="h-7 gap-1.5 text-muted-foreground hover:text-destructive" disabled={revoking === r.id} onclick={() => void revoke(r.id)}>
                  <Icon name={revoking === r.id ? 'loader-2' : 'trash-2'} class={cn('size-3.5 shrink-0', revoking === r.id && 'animate-spin')} />Revoke
                </Button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
    {#if error && rows.length}<p class="border-t border-border/40 px-6 py-2 text-ui-2xs text-destructive">{error}</p>{/if}
  </Dialog.Content>
</Dialog.Root>
