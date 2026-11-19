<script>
  // Redis insight views beside the key browser: Overview (INFO + memory
  // analysis), Slow log, Clients and Pub/Sub. Everything goes through
  // redisCommand, which returns replies as JSON with nesting intact.
  import { redisCommand, redisSubscribe } from '$lib/api.js'
  import { readOnlyMode, READ_ONLY_HINT } from '$lib/stores/read-only.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { cn } from '$lib/utils.js'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import X from '@lucide/svelte/icons/x'
  import Send from '@lucide/svelte/icons/send'
  import Radio from '@lucide/svelte/icons/radio'

  /**
   * @type {{ view: 'insights' | 'slowlog' | 'clients' | 'pubsub', active?: boolean,
   *   onfilter?: (pattern: string) => void }}
   */
  let { view, active = false, onfilter = () => {} } = $props()

  const readOnly = $derived($readOnlyMode)
  let loading = $state(false)
  let error = $state('')

  /** @param {unknown} e */
  const msg = (e) => String(e instanceof Error ? e.message : e).replace(/^Error:\s*/, '')

  /** @param {number} n */
  function bytes(n) {
    if (!Number.isFinite(n)) return '-'
    const u = ['B', 'KB', 'MB', 'GB', 'TB']
    let i = 0
    while (n >= 1024 && i < u.length - 1) { n /= 1024; i++ }
    return `${n < 10 && i ? n.toFixed(1) : Math.round(n)} ${u[i]}`
  }
  /** @param {number} s */
  function duration(s) {
    if (!Number.isFinite(s)) return '-'
    if (s < 60) return `${s}s`
    if (s < 3600) return `${Math.floor(s / 60)}m`
    if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`
    return `${Math.floor(s / 86400)}d ${Math.floor((s % 86400) / 3600)}h`
  }

  // ── Overview ────────────────────────────────────────────────────────────
  /** @type {Record<string, string>} */
  let info = $state({})
  /** ops/sec history for the sparkline. */
  let opsHistory = $state(/** @type {number[]} */ ([]))

  async function loadInfo() {
    const raw = String((await redisCommand(['INFO'])) ?? '')
    /** @type {Record<string, string>} */
    const map = {}
    for (const line of raw.split(/\r?\n/)) {
      const i = line.indexOf(':')
      if (line && line[0] !== '#' && i > 0) map[line.slice(0, i)] = line.slice(i + 1).trim()
    }
    info = map
    opsHistory = [...opsHistory.slice(-59), Number(map.instantaneous_ops_per_sec) || 0]
  }

  const num = (/** @type {string} */ k) => Number(info[k] ?? NaN)
  const hits = $derived(num('keyspace_hits'))
  const misses = $derived(num('keyspace_misses'))
  const hitRatio = $derived(hits + misses > 0 ? (hits / (hits + misses)) * 100 : null)
  const dbs = $derived(
    Object.entries(info)
      .filter(([k]) => /^db\d+$/.test(k))
      .map(([k, v]) => {
        const f = Object.fromEntries(v.split(',').map((p) => p.split('=')))
        return { db: k, keys: Number(f.keys) || 0, expires: Number(f.expires) || 0 }
      }),
  )
  const totalKeys = $derived(dbs.reduce((a, d) => a + d.keys, 0))
  const totalExpiring = $derived(dbs.reduce((a, d) => a + d.expires, 0))

  const tiles = $derived([
    { label: 'Memory', value: info.used_memory_human ?? '-', sub: info.maxmemory && info.maxmemory !== '0' ? `of ${info.maxmemory_human}` : `peak ${info.used_memory_peak_human ?? '-'}` },
    { label: 'Ops / sec', value: (num('instantaneous_ops_per_sec') || 0).toLocaleString(), sub: `${(num('total_commands_processed') || 0).toLocaleString()} total`, spark: true },
    { label: 'Hit ratio', value: hitRatio == null ? '-' : `${hitRatio.toFixed(1)}%`, sub: `${(hits || 0).toLocaleString()} hits · ${(misses || 0).toLocaleString()} misses`, tone: hitRatio != null && hitRatio < 80 ? 'warn' : '' },
    { label: 'Clients', value: info.connected_clients ?? '-', sub: `${info.blocked_clients ?? 0} blocked` },
    { label: 'Keys', value: totalKeys.toLocaleString(), sub: `${totalExpiring.toLocaleString()} with a TTL` },
    { label: 'Evicted · expired', value: `${(num('evicted_keys') || 0).toLocaleString()} · ${(num('expired_keys') || 0).toLocaleString()}`, sub: `policy ${info.maxmemory_policy ?? '-'}`, tone: num('evicted_keys') > 0 ? 'warn' : '' },
  ])
  const facts = $derived([
    ['Version', `${info.redis_version ?? info.valkey_version ?? '-'}${info.redis_mode ? ` · ${info.redis_mode}` : ''}`],
    ['Uptime', duration(num('uptime_in_seconds'))],
    ['Role', info.role ?? '-'],
    ['Fragmentation', info.mem_fragmentation_ratio ?? '-'],
    ['Persistence', `${info.aof_enabled === '1' ? 'AOF' : 'no AOF'} · last save ${info.rdb_last_save_time ? new Date(Number(info.rdb_last_save_time) * 1000).toLocaleString() : '-'}`],
    ['Network', `${info.total_net_input_bytes ? bytes(num('total_net_input_bytes')) : '-'} in · ${info.total_net_output_bytes ? bytes(num('total_net_output_bytes')) : '-'} out`],
  ])
  const sparkPath = $derived.by(() => {
    const h = opsHistory
    if (h.length < 2) return ''
    const max = Math.max(1, ...h)
    return h.map((v, i) => `${i === 0 ? 'M' : 'L'}${(i / (h.length - 1)) * 100},${24 - (v / max) * 22}`).join(' ')
  })

  // Memory analysis: one Lua script walks SCAN server-side and reports each
  // key's MEMORY USAGE and TYPE, so a sample of thousands is a single round trip.
  const SAMPLE = 5000
  const ANALYZE_LUA = `local cur='0' local out={} local n=0 local cap=tonumber(ARGV[1])
repeat local r=redis.call('SCAN',cur,'COUNT',1000) cur=r[1]
for _,k in ipairs(r[2]) do n=n+1 local m=redis.call('MEMORY','USAGE',k) or 0 local t=redis.call('TYPE',k) out[#out+1]={k,m,t.ok or t} if n>=cap then break end end
until cur=='0' or n>=cap
return out`
  let analysis = $state(/** @type {{ keys: { key: string, bytes: number, type: string }[], byType: [string, { n: number, bytes: number }][], byPrefix: [string, { n: number, bytes: number }][], total: number } | null} */ (null))
  let analyzing = $state(false)

  async function analyze() {
    analyzing = true
    try {
      const out = /** @type {[string, number, string][]} */ (await redisCommand(['EVAL', ANALYZE_LUA, '0', String(SAMPLE)])) ?? []
      const keys = out.map(([key, b, type]) => ({ key: String(key), bytes: Number(b) || 0, type: String(type) }))
      /** @type {Map<string, { n: number, bytes: number }>} */
      const byType = new Map()
      /** @type {Map<string, { n: number, bytes: number }>} */
      const byPrefix = new Map()
      for (const k of keys) {
        const t = byType.get(k.type) ?? { n: 0, bytes: 0 }
        t.n++; t.bytes += k.bytes; byType.set(k.type, t)
        const cut = k.key.search(/[:/.]/)
        const p = cut > 0 ? k.key.slice(0, cut + 1) : '(no prefix)'
        const pp = byPrefix.get(p) ?? { n: 0, bytes: 0 }
        pp.n++; pp.bytes += k.bytes; byPrefix.set(p, pp)
      }
      analysis = {
        keys: keys.sort((a, b) => b.bytes - a.bytes).slice(0, 25),
        byType: [...byType].sort((a, b) => b[1].bytes - a[1].bytes),
        byPrefix: [...byPrefix].sort((a, b) => b[1].bytes - a[1].bytes).slice(0, 12),
        total: keys.reduce((a, k) => a + k.bytes, 0),
      }
    } catch (e) {
      toast.error('Memory analysis failed', { description: msg(e) })
    } finally {
      analyzing = false
    }
  }

  // ── Slow log ────────────────────────────────────────────────────────────
  let slow = $state(/** @type {{ id: number, at: number, us: number, args: string[], client: string }[]} */ ([]))
  async function loadSlowlog() {
    const out = /** @type {any[]} */ (await redisCommand(['SLOWLOG', 'GET', '128'])) ?? []
    slow = out.map((e) => ({ id: Number(e[0]), at: Number(e[1]), us: Number(e[2]), args: (e[3] ?? []).map(String), client: String(e[4] ?? '') }))
  }
  async function resetSlowlog() {
    try { await redisCommand(['SLOWLOG', 'RESET']); slow = []; toast.success('Slow log cleared') } catch (e) { toast.error('Could not clear', { description: msg(e) }) }
  }

  // ── Clients ─────────────────────────────────────────────────────────────
  let clients = $state(/** @type {Record<string, string>[]} */ ([]))
  async function loadClients() {
    const raw = String((await redisCommand(['CLIENT', 'LIST'])) ?? '')
    clients = raw.split(/\r?\n/).filter(Boolean).map((line) => Object.fromEntries(line.split(' ').map((p) => { const i = p.indexOf('='); return [p.slice(0, i), p.slice(i + 1)] })))
  }
  /** @param {string} id */
  async function killClient(id) {
    try { await redisCommand(['CLIENT', 'KILL', 'ID', id]); toast.success(`Disconnected client ${id}`); await loadClients() } catch (e) { toast.error('Could not disconnect', { description: msg(e) }) }
  }

  // ── Pub/Sub ─────────────────────────────────────────────────────────────
  let channels = $state(/** @type {{ name: string, subs: number }[]} */ ([]))
  let patterns = $state(0)
  let pubChannel = $state('')
  let pubMessage = $state('')
  async function loadPubsub() {
    const names = /** @type {string[]} */ ((await redisCommand(['PUBSUB', 'CHANNELS'])) ?? []).map(String)
    const counts = names.length ? /** @type {any[]} */ (await redisCommand(['PUBSUB', 'NUMSUB', ...names])) ?? [] : []
    /** @type {Record<string, number>} */
    const m = {}
    for (let i = 0; i < counts.length; i += 2) m[String(counts[i])] = Number(counts[i + 1]) || 0
    channels = names.map((name) => ({ name, subs: m[name] ?? 0 })).sort((a, b) => b.subs - a.subs)
    patterns = Number(await redisCommand(['PUBSUB', 'NUMPAT'])) || 0
  }
  // Subscribe: a live tail of messages. Names with * ? or [ are patterns.
  let subInput = $state('')
  /** @type {(() => Promise<void>) | null} */
  let stopSub = $state(null)
  let subscribing = $state(false)
  let subTargets = $state(/** @type {string[]} */ ([]))
  let messages = $state.raw(/** @type {{ id: number, channel: string, pattern: string | null, payload: string, at: number }[]} */ ([]))
  let paused = $state(false)
  let msgSeq = 0
  const MSG_CAP = 500

  async function startSub() {
    const names = subInput.split(/[\s,]+/).map((x) => x.trim()).filter(Boolean)
    if (!names.length) return
    await stopSubscription()
    subscribing = true
    try {
      const isPat = (/** @type {string} */ n) => /[*?[]/.test(n)
      stopSub = await redisSubscribe(names.filter((n) => !isPat(n)), names.filter(isPat), (m) => {
        if (paused) return
        messages = [{ id: ++msgSeq, ...m }, ...messages].slice(0, MSG_CAP)
      })
      subTargets = names
    } catch (e) {
      toast.error('Subscribe failed', { description: msg(e) })
    } finally {
      subscribing = false
    }
  }
  async function stopSubscription() {
    const s = stopSub
    stopSub = null
    subTargets = []
    if (s) await s()
  }
  // Leaving Pub/Sub (or the page) ends the subscription.
  $effect(() => {
    if (view === 'pubsub' && active) return
    if (stopSub) void stopSubscription()
  })
  $effect(() => () => { if (stopSub) void stopSub() })

  async function publish() {
    if (!pubChannel.trim()) return
    try {
      const n = Number(await redisCommand(['PUBLISH', pubChannel.trim(), pubMessage]))
      toast.success(`Published to ${pubChannel.trim()}`, { description: `${n} ${n === 1 ? 'subscriber' : 'subscribers'} received it` })
      pubMessage = ''
    } catch (e) {
      toast.error('Publish failed', { description: msg(e) })
    }
  }

  async function refresh() {
    loading = true
    error = ''
    try {
      if (view === 'insights') await loadInfo()
      else if (view === 'slowlog') await loadSlowlog()
      else if (view === 'clients') await loadClients()
      else await loadPubsub()
    } catch (e) {
      error = msg(e)
    } finally {
      loading = false
    }
  }

  // Load on open; the overview polls every 3 s while it is on screen.
  $effect(() => {
    if (!active) return
    void view
    void refresh()
    if (view !== 'insights') return
    const t = setInterval(() => { if (!document.hidden) void loadInfo().catch(() => {}) }, 3000)
    return () => clearInterval(t)
  })

  const TITLES = { insights: 'Overview', slowlog: 'Slow log', clients: 'Clients', pubsub: 'Pub/Sub' }
  const iconBtn = 'inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[color,background-color,scale] duration-[120ms] ease-[var(--ease-out)] active:scale-[0.97] hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40'
  const th = 'sticky top-0 z-10 bg-panel px-3 py-2 text-left text-ui-2xs font-medium text-muted-foreground'
  const td = 'px-3 py-1.5 align-top'
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  <header class="flex h-11 shrink-0 items-center gap-2 border-b border-border/30 pl-4 pr-2">
    <h2 class="text-ui-sm font-medium text-foreground">{TITLES[view]}</h2>
    {#if view === 'insights'}<span class="text-ui-2xs text-muted-foreground">live, every 3 s</span>{/if}
    <span class="flex-1"></span>
    {#if view === 'slowlog' && slow.length}
      <Button variant="ghost" size="sm" class="rounded-md text-muted-foreground" disabled={readOnly} title={readOnly ? READ_ONLY_HINT : 'SLOWLOG RESET'} onclick={() => void resetSlowlog()}>Clear</Button>
    {/if}
    <button type="button" class={iconBtn} onclick={() => void refresh()} title="Refresh" aria-label="Refresh"><RefreshCw class={cn('size-3.5', loading && 'animate-spin')} /></button>
  </header>

  <div class={cn('min-h-0 flex-1', view === 'pubsub' ? 'overflow-hidden' : 'app-scroll overflow-auto')}>
    {#if error}
      <div class="mx-auto flex max-w-md flex-col gap-2 px-6 py-16">
        <p class="text-ui-sm font-medium text-foreground">Couldn't load {TITLES[view].toLowerCase()}</p>
        <pre class="whitespace-pre-wrap break-words font-mono text-ui-2xs text-muted-foreground">{error}</pre>
      </div>
    {:else if view === 'insights'}
      <div class="flex flex-col gap-6 px-5 py-5">
        <div class="grid grid-cols-2 gap-3 lg:grid-cols-3">
          {#each tiles as t (t.label)}
            <div class="flex flex-col gap-1 rounded-xl bg-muted/30 p-4">
              <span class="text-ui-2xs text-muted-foreground">{t.label}</span>
              <span class={cn('text-ui-xl font-semibold tabular-nums tracking-tight', t.tone === 'warn' ? 'text-warning' : 'text-foreground')}>{t.value}</span>
              {#if t.spark && sparkPath}
                <svg viewBox="0 0 100 24" preserveAspectRatio="none" class="h-6 w-full text-primary" aria-hidden="true"><path d={sparkPath} fill="none" stroke="currentColor" stroke-width="1.5" vector-effect="non-scaling-stroke" /></svg>
              {/if}
              <span class="truncate text-ui-2xs text-muted-foreground">{t.sub}</span>
            </div>
          {/each}
        </div>

        <dl class="grid grid-cols-1 gap-x-8 gap-y-2 sm:grid-cols-2">
          {#each facts as [k, v] (k)}
            <div class="flex min-w-0 items-baseline gap-3 text-ui-xs">
              <dt class="w-28 shrink-0 text-muted-foreground">{k}</dt>
              <dd class="min-w-0 truncate tabular-nums text-foreground/85">{v}</dd>
            </div>
          {/each}
        </dl>

        {#if dbs.length}
          <section>
            <h3 class="mb-2 text-ui-xs font-medium text-foreground/80">Keyspace</h3>
            <div class="flex flex-wrap gap-2">
              {#each dbs as d (d.db)}
                <span class="inline-flex h-7 items-center gap-2 rounded-md bg-muted/40 px-3 text-ui-xs">
                  <span class="text-foreground">{d.db}</span>
                  <span class="tabular-nums text-muted-foreground">{d.keys.toLocaleString()} keys · {d.expires.toLocaleString()} TTL</span>
                </span>
              {/each}
            </div>
          </section>
        {/if}

        <section>
          <div class="mb-2 flex items-center gap-3">
            <h3 class="text-ui-xs font-medium text-foreground/80">Memory analysis</h3>
            <span class="text-ui-2xs text-muted-foreground">Samples up to {SAMPLE.toLocaleString()} keys of this database with MEMORY USAGE</span>
            <span class="flex-1"></span>
            <Button size="sm" variant="secondary" class="rounded-md" disabled={analyzing} onclick={() => void analyze()}>
              {#if analyzing}<LoaderCircle class="animate-spin" />{/if}{analysis ? 'Analyze again' : 'Analyze'}
            </Button>
          </div>
          {#if analysis}
            <div class="grid grid-cols-1 gap-5 lg:grid-cols-2">
              <div class="flex flex-col gap-1.5">
                <span class="text-ui-2xs text-muted-foreground">By type · {bytes(analysis.total)} sampled</span>
                {#each analysis.byType as [t, v] (t)}
                  <div class="flex items-center gap-3 text-ui-xs">
                    <span class="w-14 shrink-0 text-foreground/85">{t}</span>
                    <div class="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-muted/60"><div class="h-full rounded-full bg-primary/70" style="width: {(v.bytes / Math.max(1, analysis.total)) * 100}%"></div></div>
                    <span class="w-28 shrink-0 text-right tabular-nums text-muted-foreground">{bytes(v.bytes)} · {v.n.toLocaleString()}</span>
                  </div>
                {/each}
                <span class="mt-3 text-ui-2xs text-muted-foreground">By prefix (click to filter keys)</span>
                {#each analysis.byPrefix as [p, v] (p)}
                  <button type="button" class="flex items-center gap-3 rounded-lg px-1 py-0.5 text-left text-ui-xs transition-colors hover:bg-accent/50" disabled={p === '(no prefix)'} onclick={() => onfilter(`${p}*`)}>
                    <span class="w-28 shrink-0 truncate font-mono text-foreground/85">{p}</span>
                    <div class="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-muted/60"><div class="h-full rounded-full bg-primary/50" style="width: {(v.bytes / Math.max(1, analysis.total)) * 100}%"></div></div>
                    <span class="w-28 shrink-0 text-right tabular-nums text-muted-foreground">{bytes(v.bytes)} · {v.n.toLocaleString()}</span>
                  </button>
                {/each}
              </div>
              <div class="flex flex-col">
                <span class="mb-1.5 text-ui-2xs text-muted-foreground">Biggest keys</span>
                {#each analysis.keys as k (k.key)}
                  <button type="button" class="flex items-center gap-3 rounded-lg px-1 py-1 text-left text-ui-xs transition-colors hover:bg-accent/50" onclick={() => onfilter(k.key)}>
                    <span class="min-w-0 flex-1 truncate font-mono text-foreground/85">{k.key}</span>
                    <span class="shrink-0 text-ui-2xs text-muted-foreground">{k.type}</span>
                    <span class="w-16 shrink-0 text-right tabular-nums text-foreground/85">{bytes(k.bytes)}</span>
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        </section>
      </div>
    {:else if view === 'slowlog'}
      {#if !slow.length}
        <p class="px-6 py-16 text-center text-ui-sm text-muted-foreground">{loading ? 'Loading…' : 'No slow commands logged'}</p>
      {:else}
        <table class="w-full text-ui-xs">
          <thead><tr><th class={th}>When</th><th class={cn(th, 'text-right')}>Took</th><th class={th}>Command</th><th class={th}>Client</th></tr></thead>
          <tbody>
            {#each slow as e (e.id)}
              <tr class="hover:bg-muted/30">
                <td class={cn(td, 'whitespace-nowrap text-muted-foreground')}>{new Date(e.at * 1000).toLocaleString()}</td>
                <td class={cn(td, 'whitespace-nowrap text-right tabular-nums', e.us > 100000 ? 'text-warning' : 'text-foreground/85')}>{e.us >= 1000 ? `${(e.us / 1000).toFixed(1)} ms` : `${e.us} µs`}</td>
                <td class={cn(td, 'max-w-0 w-full truncate font-mono text-foreground')} title={e.args.join(' ')}>{e.args.join(' ')}</td>
                <td class={cn(td, 'whitespace-nowrap font-mono text-muted-foreground')}>{e.client}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    {:else if view === 'clients'}
      <table class="w-full text-ui-xs">
        <thead><tr><th class={th}>Address</th><th class={th}>Name</th><th class={th}>DB</th><th class={cn(th, 'text-right')}>Age</th><th class={cn(th, 'text-right')}>Idle</th><th class={th}>Last command</th><th class={th}></th></tr></thead>
        <tbody>
          {#each clients as c (c.id)}
            <tr class="group hover:bg-muted/30">
              <td class={cn(td, 'whitespace-nowrap font-mono text-foreground')}>{c.addr}</td>
              <td class={cn(td, 'text-muted-foreground')}>{c.name || '-'}</td>
              <td class={cn(td, 'tabular-nums text-muted-foreground')}>{c.db}</td>
              <td class={cn(td, 'whitespace-nowrap text-right tabular-nums text-muted-foreground')}>{duration(Number(c.age))}</td>
              <td class={cn(td, 'whitespace-nowrap text-right tabular-nums text-muted-foreground')}>{duration(Number(c.idle))}</td>
              <td class={cn(td, 'w-full font-mono text-foreground/85')}>{c.cmd}</td>
              <td class={cn(td, 'pr-2')}>
                <button type="button" class={cn(iconBtn, 'size-6 opacity-0 group-hover:opacity-100 hover:text-destructive')} disabled={readOnly} title={readOnly ? READ_ONLY_HINT : 'Disconnect (CLIENT KILL)'} aria-label="Disconnect client" onclick={() => void killClient(c.id)}><X class="size-3.5 shrink-0" /></button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <!-- Pub/Sub reads like a chat: listen bar on top, the stream in the middle,
           a composer at the bottom, and the live channels down the side. -->
      <div class="flex h-full min-h-0">
        <div class="flex min-w-0 flex-1 flex-col">
          <form class="flex shrink-0 items-center gap-2 px-4 py-3" onsubmit={(e) => { e.preventDefault(); void startSub() }}>
            {#if stopSub}
              <span class="relative flex size-2 shrink-0"><span class="absolute inline-flex size-full animate-ping rounded-full bg-success opacity-60"></span><span class="relative inline-flex size-2 rounded-full bg-success"></span></span>
              <span class="min-w-0 flex-1 truncate text-ui-xs text-muted-foreground">Listening to <span class="font-mono text-foreground">{subTargets.join(' ')}</span>{paused ? ' · paused' : ''}</span>
              <Button type="button" variant="ghost" size="sm" class="rounded-md" onclick={() => (paused = !paused)}>{paused ? 'Resume' : 'Pause'}</Button>
              <Button type="button" variant="secondary" size="sm" class="rounded-md" onclick={() => void stopSubscription()}>Stop</Button>
            {:else}
              <input bind:value={subInput} placeholder="Channels or patterns to listen to, e.g. orders news.*" aria-label="Channels to subscribe to" spellcheck="false" autocomplete="off" class="field-surface h-8 min-w-0 flex-1 px-3 font-mono text-ui-xs text-foreground shadow-none outline-none placeholder:font-sans placeholder:text-muted-foreground" />
              <button type="submit" class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md bg-foreground px-3.5 text-ui-xs font-medium text-background transition-opacity hover:opacity-90 disabled:opacity-40" disabled={!subInput.trim() || subscribing}>
                {#if subscribing}<LoaderCircle class="size-3.5 shrink-0 animate-spin" />{/if}Listen
              </button>
            {/if}
          </form>

          <div class="app-scroll flex min-h-0 flex-1 flex-col overflow-auto border-y border-border/30">
            {#if messages.length}
              <div class="sticky top-0 z-10 flex items-center gap-2 bg-panel px-4 py-1.5 text-ui-2xs text-muted-foreground">
                <span class="tabular-nums">{messages.length.toLocaleString()} {messages.length === 1 ? 'message' : 'messages'}{messages.length === MSG_CAP ? ', latest kept' : ''}</span>
                <span class="flex-1"></span>
                <button type="button" class="hover:text-foreground" onclick={() => (messages = [])}>Clear</button>
              </div>
              {#each messages as m (m.id)}
                <div class="flex items-baseline gap-3 px-4 py-1.5 text-ui-xs hover:bg-muted/20">
                  <span class="w-20 shrink-0 tabular-nums text-muted-foreground">{new Date(m.at).toLocaleTimeString()}</span>
                  <span class="w-40 shrink-0 truncate font-mono text-primary/90" title={m.pattern ? `${m.channel} (via ${m.pattern})` : m.channel}>{m.channel}</span>
                  <span class="min-w-0 flex-1 whitespace-pre-wrap break-all font-mono text-foreground/85">{m.payload}</span>
                </div>
              {/each}
            {:else}
              <div class="m-auto flex max-w-xs flex-col items-center gap-1.5 px-6 text-center">
                <Radio class="size-5 shrink-0 text-muted-foreground" />
                <p class="text-ui-sm text-foreground">{stopSub ? 'Waiting for messages…' : 'Nothing to show yet'}</p>
                <p class="text-ui-xs text-muted-foreground">{stopSub ? 'Messages published to these channels appear here as they arrive.' : 'Listen to a channel above, then publish below or from your app.'}</p>
              </div>
            {/if}
          </div>

          <form class="shrink-0 p-3" onsubmit={(e) => { e.preventDefault(); void publish() }}>
            <div class="field-surface flex items-center gap-2 p-1 pl-1 focus-within:border-ring">
              <input bind:value={pubChannel} placeholder="channel" aria-label="Channel" spellcheck="false" autocomplete="off" class="no-field-frame h-7 w-40 shrink-0 rounded-sm border-0 bg-muted/60 px-2.5 font-mono text-ui-xs text-foreground shadow-none outline-none placeholder:text-muted-foreground" />
              <input bind:value={pubMessage} placeholder="Message to publish" aria-label="Message" spellcheck="false" autocomplete="off" class="no-field-frame h-7 min-w-0 flex-1 border-0 bg-transparent px-1 font-mono text-ui-xs text-foreground shadow-none outline-none placeholder:font-sans placeholder:text-muted-foreground" />
              <button type="submit" class="inline-flex size-7 shrink-0 items-center justify-center rounded-sm bg-foreground text-background transition-opacity hover:opacity-90 disabled:opacity-30" disabled={!pubChannel.trim() || readOnly} title={readOnly ? READ_ONLY_HINT : 'Publish (Enter)'} aria-label="Publish"><Send class="size-3.5 shrink-0" /></button>
            </div>
          </form>
        </div>

        <aside class="flex w-56 shrink-0 flex-col border-l border-border/30">
          <div class="flex h-10 shrink-0 items-center px-4 text-ui-xs text-muted-foreground">
            <span class="flex-1">Active channels</span>
            <span class="tabular-nums">{channels.length}</span>
          </div>
          <div class="app-scroll min-h-0 flex-1 overflow-auto px-2 pb-2">
            {#each channels as c (c.name)}
              <button type="button" class="flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-ui-xs transition-colors hover:bg-accent/50" title="Use for listening and publishing" onclick={() => { pubChannel = c.name; if (!stopSub) subInput = c.name }}>
                <span class="min-w-0 flex-1 truncate font-mono text-foreground">{c.name}</span>
                <span class="shrink-0 font-mono text-ui-2xs tabular-nums text-muted-foreground">{c.subs}</span>
              </button>
            {:else}
              <p class="px-2 py-2 text-ui-xs text-muted-foreground">No channel has a subscriber right now.</p>
            {/each}
            {#if patterns > 0}<p class="px-2 pt-2 text-ui-2xs text-muted-foreground">{patterns} pattern {patterns === 1 ? 'subscription' : 'subscriptions'}</p>{/if}
          </div>
        </aside>
      </div>
    {/if}
  </div>
</div>
