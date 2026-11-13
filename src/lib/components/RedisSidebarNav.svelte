<script>
  // Redis navigation in the app sidebar: databases with key counts, the views,
  // and the namespaces of the loaded keys. State lives in the Redis page; this
  // reads it from the redis-nav store and calls back into the page.
  import { redisNav, redisNavCall } from '$lib/stores/redis-nav.js'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'

  import { redisCommand } from '$lib/api.js'

  let showEmpty = $state(false)
  const nav = $derived($redisNav)

  // Its own INFO, so counts and the server line are right before (and without)
  // the Redis page being open. Refreshed every 10 s.
  /** @type {Record<string, string>} */
  let info = $state({})
  $effect(() => {
    let alive = true
    const load = async () => {
      try {
        const raw = String((await redisCommand(['INFO'])) ?? '')
        /** @type {Record<string, string>} */
        const m = {}
        for (const line of raw.split(/\r?\n/)) {
          const i = line.indexOf(':')
          if (line && line[0] !== '#' && i > 0) m[line.slice(0, i)] = line.slice(i + 1).trim()
        }
        if (alive) info = m
      } catch { /* not connected yet */ }
    }
    void load()
    const t = setInterval(() => { if (!document.hidden) void load() }, 10_000)
    return () => { alive = false; clearInterval(t) }
  })
  const ownKeys = $derived.by(() => {
    /** @type {Record<number, number>} */
    const m = {}
    for (const [k, v] of Object.entries(info)) {
      const mm = /^db(\d+)$/.exec(k)
      if (mm) m[Number(mm[1])] = Number(/keys=(\d+)/.exec(v)?.[1] ?? 0)
    }
    return m
  })
  const keysFor = (/** @type {number} */ n) => ownKeys[n] ?? nav.dbKeys[n] ?? 0
  const server = $derived(nav.server ?? (info.redis_version || info.valkey_version ? {
    version: info.valkey_version ? `Valkey ${info.valkey_version}` : `Redis ${info.redis_version}`,
    memory: info.used_memory_human ?? '', clients: info.connected_clients ?? '', ops: info.instantaneous_ops_per_sec ?? '',
  } : null))
  const dbs = $derived(
    Array.from({ length: 16 }, (_, n) => ({ n, keys: keysFor(n) })).filter((d) => showEmpty || d.keys > 0 || d.n === nav.currentDb),
  )
  const TOOLS = [
    { id: 'insights', label: 'Overview', icon: 'bar-chart-3' },
    { id: 'slowlog', label: 'Slow log', icon: 'clock' },
    { id: 'clients', label: 'Clients', icon: 'zap' },
    { id: 'pubsub', label: 'Pub/Sub', icon: 'radio' },
  ]
  let dbOpen = $state(true)
  let toolsOpen = $state(true)
  // Same rhythm as the SQL tree: 28px rows, a chevron column, small mono counts.
  const row = 'group flex h-7 w-full items-center gap-1.5 rounded-md pr-2 text-left text-ui-sm transition-colors disabled:opacity-50'
  const on = 'bg-sidebar-accent text-foreground'
  const off = 'text-sidebar-foreground hover:bg-sidebar-accent/60 hover:text-foreground'
  const count = 'shrink-0 font-mono text-ui-2xs tabular-nums text-muted-foreground'
</script>

<div class="flex min-h-0 flex-1 flex-col">
<div class="app-scroll flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-2 pb-2" role="tree">
  <!-- Databases: each expands to the namespaces of its loaded keys. -->
  <section>
    <div class="flex h-7 items-center gap-1.5 pr-2">
      <button type="button" class="flex min-w-0 flex-1 items-center gap-1.5 text-left text-ui-xs text-muted-foreground hover:text-foreground" onclick={() => (dbOpen = !dbOpen)} aria-expanded={dbOpen}>
        <span class="flex w-4 shrink-0 justify-center"><Icon name="chevron-right" class={cn('size-3.5 shrink-0 transition-transform duration-100', dbOpen && 'rotate-90')} /></span>
        Databases
      </button>
      <button type="button" class="text-ui-2xs text-muted-foreground hover:text-foreground" onclick={() => (showEmpty = !showEmpty)}>{showEmpty ? 'Hide empty' : 'All 16'}</button>
    </div>
    {#if dbOpen}
      {#each dbs as d (d.n)}
        {@const current = d.n === nav.currentDb}
        {@const selected = current && nav.view === 'keys' && !nav.filter}
        <button
          type="button"
          role="treeitem"
          aria-selected={selected}
          aria-expanded={current && nav.namespaces.length ? true : undefined}
          class={cn(row, selected ? on : off)}
          disabled={nav.switching || (nav.locked && !current)}
          title={nav.locked && !current ? 'Apply or discard staged changes before switching database' : `Keys in db${d.n}`}
          onclick={() => (current ? (redisNavCall('filterTo', ''), redisNavCall('setView', 'keys')) : redisNavCall('switchDb', d.n))}
        >
          <span class="flex w-4 shrink-0 justify-center">{#if current && nav.namespaces.length}<Icon name="chevron-right" class="size-3.5 shrink-0 rotate-90 text-muted-foreground" />{/if}</span>
          <Icon name="database" class={cn('size-3.5 shrink-0', current ? 'text-primary' : 'text-muted-foreground')} />
          <span class={cn('min-w-0 flex-1 truncate', current && 'font-medium')}>db{d.n}</span>
          <span class={count}>{d.keys.toLocaleString()}</span>
        </button>
        {#if current}
          <div class="relative">
            <span class="absolute bottom-1 left-[1.05rem] top-0 w-px bg-border/50" aria-hidden="true"></span>
            {#each nav.namespaces as [p, n] (p)}
              {@const active = nav.view === 'keys' && nav.filter === `${p}*`}
              <button type="button" role="treeitem" aria-selected={active} class={cn(row, 'pl-6', active ? on : off)} onclick={() => redisNavCall('filterTo', active ? '' : `${p}*`)} title={`Show ${p}* keys`}>
                <span class="w-4 shrink-0"></span>
                <Icon name="folder" class="size-3.5 shrink-0 text-muted-foreground" />
                <span class="min-w-0 flex-1 truncate font-mono text-ui-xs">{p}</span>
                <span class={count}>{n.toLocaleString()}</span>
              </button>
            {/each}
          </div>
        {/if}
      {/each}
    {/if}
  </section>

  <!-- Server tools. -->
  <section>
    <button type="button" class="flex h-7 w-full items-center gap-1.5 pr-2 text-left text-ui-xs text-muted-foreground hover:text-foreground" onclick={() => (toolsOpen = !toolsOpen)} aria-expanded={toolsOpen}>
      <span class="flex w-4 shrink-0 justify-center"><Icon name="chevron-right" class={cn('size-3.5 shrink-0 transition-transform duration-100', toolsOpen && 'rotate-90')} /></span>
      Server
    </button>
    {#if toolsOpen}
      {#each TOOLS as v (v.id)}
        <button type="button" role="treeitem" aria-selected={nav.view === v.id} class={cn(row, nav.view === v.id ? on : off)} onclick={() => redisNavCall('setView', v.id)}>
          <span class="w-4 shrink-0"></span>
          <Icon name={v.icon} class="size-3.5 shrink-0 text-muted-foreground" />
          <span class="min-w-0 flex-1 truncate">{v.label}</span>
        </button>
      {/each}
    {/if}
  </section>
</div>
  {#if server}
    <!-- The server at a glance; Overview has the rest. -->
    <button type="button" class="mx-2 mb-1 flex flex-col gap-0.5 rounded-lg px-2 py-1.5 text-left text-ui-xs text-muted-foreground transition-colors hover:bg-sidebar-accent/60" onclick={() => redisNavCall('setView', 'insights')} title="Open Overview">
      <span class="text-foreground/85">{server.version}</span>
      <span class="tabular-nums">{[server.memory && `${server.memory} used`, server.clients && `${server.clients} clients`, server.ops !== '' && `${server.ops} ops/s`].filter(Boolean).join(' · ')}</span>
    </button>
  {/if}
</div>
