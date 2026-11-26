<script>
  // Local database servers in a window of their own, DBngin-style: the
  // servers Stroke runs on this computer on the left, the one picked on the
  // right with how to reach it, its log and its settings. Below Stroke's own,
  // the database servers already running here that Stroke didn't make
  // (Postgres.app, Homebrew, docker compose), found by port, so they open in
  // one click too. Backend: src-tauri/src/local_servers.rs.
  import { onMount, onDestroy } from 'svelte'
  import EmptyState from './EmptyState.svelte'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import WindowControls from './WindowControls.svelte'
  import DbIcon from './DbIcon.svelte'
  import { revealApp } from '$lib/app-reveal.js'
  import { detectOs } from '$lib/platform.js'
  import { cn, errorText } from '$lib/utils.js'
  import {
    ENGINES, engineOf, nextPort, connectionFor, connectionUrl, envName, formatBytes, uptime,
    localEnv, listServers, createServer, startServer, stopServer, deleteServer, serverLogs, brewServices, brewAction,
    freePort, updateServer, discoverServers, revealServer, connectIn,
  } from '$lib/local-servers.js'
  import Plus from '@lucide/svelte/icons/plus'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import ArrowUpRight from '@lucide/svelte/icons/arrow-up-right'
  import MoreHorizontal from '@lucide/svelte/icons/more-horizontal'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Server from '@lucide/svelte/icons/server'
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert'
  import Hash from '@lucide/svelte/icons/hash'
  import Play from '@lucide/svelte/icons/play'
  import Square from '@lucide/svelte/icons/square'
  import Container from '@lucide/svelte/icons/container'
  import Cpu from '@lucide/svelte/icons/cpu'
  import Check from '@lucide/svelte/icons/check'
  import Copy from '@lucide/svelte/icons/copy'
  import FolderOpen from '@lucide/svelte/icons/folder-open'
  import Radar from '@lucide/svelte/icons/radar'
  import Beer from '@lucide/svelte/icons/beer'

  let {
    /** The window that opened this one: connections open there. */
    from = 'main',
  } = $props()

  const isMac = detectOs() === 'macos'

  /** @typedef {import('$lib/local-servers.js').LocalServer} LocalServer */
  /** @typedef {import('$lib/local-servers.js').FoundServer} FoundServer */

  /** @type {LocalServer[]} */
  let servers = $state([])
  /** @type {FoundServer[]} */
  let found = $state([])
  /** @type {import('$lib/local-servers.js').LocalEnv | null} */
  let env = $state(null)
  /** @type {import('$lib/local-servers.js').BrewService[]} */
  let brew = $state([])
  let loading = $state(false)
  let scanning = $state(false)
  let loaded = $state(false)
  /** Server or brew service with an action running. @type {string | null} */
  let busy = $state(null)
  /** What the right side shows: a server id, `found:<port>`, `new`, or nothing. @type {string | null} */
  let selected = $state(null)
  /** The last start or stop that failed, per server, kept until the next try. @type {Record<string, string>} */
  let failures = $state({})
  let now = $state(Date.now())

  onMount(() => {
    revealApp()
    document.title = 'Local Servers'
    void refresh().then(() => {
      if (!selected) selected = servers[0]?.id ?? (found.length ? null : 'new')
    })
    void scan()
    // Keep the list honest while the window stays open: a server stopped
    // from a terminal or Docker Desktop shows as stopped within seconds.
    const poll = setInterval(() => { if (document.visibilityState === 'visible' && !busy) void refresh(true) }, 8000)
    const tick = setInterval(() => (now = Date.now()), 30_000)
    const onFocus = () => { void refresh(true); void scan() }
    window.addEventListener('focus', onFocus)
    return () => {
      clearInterval(poll)
      clearInterval(tick)
      window.removeEventListener('focus', onFocus)
    }
  })

  /** @param {boolean} [quiet] */
  async function refresh(quiet = false) {
    if (!quiet) loading = true
    try {
      const [e, s] = await Promise.all([localEnv(), listServers()])
      env = e
      servers = s
      if (e.brew) brew = await brewServices().catch(() => [])
      if (selected && selected !== 'new' && !selected.startsWith('found:') && !s.some((x) => x.id === selected)) selected = s[0]?.id ?? null
    } catch (e) {
      if (!quiet) toast.error('Could not list local servers', { description: errorText(e) })
    } finally {
      loading = false
      loaded = true
    }
  }

  async function scan() {
    if (scanning) return
    scanning = true
    try {
      found = await discoverServers()
    } catch {
      found = []
    } finally {
      scanning = false
    }
  }

  const current = $derived(servers.find((s) => s.id === selected) ?? null)
  const currentFound = $derived(selected?.startsWith('found:') ? found.find((f) => `found:${f.port}` === selected) ?? null : null)

  /** @param {LocalServer} s */
  async function toggle(s) {
    if (busy) return
    busy = s.id
    const { [s.id]: _, ...rest } = failures
    failures = rest
    try {
      if (s.running) await stopServer(s.id)
      else await startServer(s.id)
    } catch (e) {
      failures = { ...failures, [s.id]: String(e) }
      void loadLogs(s)
    } finally {
      busy = null
      await refresh(true)
    }
  }

  /** @param {LocalServer} s */
  async function remove(s) {
    if (busy) return
    busy = s.id
    try {
      await deleteServer(s.id)
      toast.success(`Deleted ${s.name}`, { description: 'Its data was deleted with it.' })
      selected = null
      await refresh(true)
      selected = servers[0]?.id ?? null
    } catch (e) {
      toast.error(`Could not delete ${s.name}`, { description: errorText(e) })
    } finally {
      busy = null
    }
  }

  /** @param {any} conn */
  async function openIn(conn) {
    try {
      await connectIn(from, conn)
    } catch (e) {
      toast.error('Could not open the connection', { description: errorText(e) })
    }
  }

  /** @param {FoundServer} f */
  function foundConnection(f) {
    const label = engineOf(f.engine).label
    const name = f.source === 'docker' ? f.name : `${label} on ${f.port}`
    return { ...connectionFor(/** @type {any} */ ({ id: `found-${f.port}`, name, engine: f.engine, port: f.port })), id: `found-${f.engine}-${f.port}` }
  }

  // ── Copy ──────────────────────────────────────────────────────────────────
  let copied = $state('')
  /** @param {string} text @param {string} what */
  async function copy(text, what) {
    try {
      await navigator.clipboard.writeText(text)
      copied = what
      setTimeout(() => { if (copied === what) copied = '' }, 1400)
    } catch {
      toast.error('Could not copy')
    }
  }

  // ── Logs ──────────────────────────────────────────────────────────────────
  let logs = $state('')
  let logsFor = $state('')
  let logsLoading = $state(false)
  /** @type {HTMLPreElement | null} */
  let logEl = $state(null)
  // Open the log at its newest line, the one that says why it stopped.
  $effect(() => {
    void logs
    if (logEl) requestAnimationFrame(() => { if (logEl) logEl.scrollTop = logEl.scrollHeight })
  })
  /** @param {LocalServer} s */
  async function loadLogs(s) {
    logsLoading = true
    try {
      const text = (await serverLogs(s.id)).trim()
      if (selected === s.id) { logs = text; logsFor = s.id }
    } catch (e) {
      if (selected === s.id) { logs = String(e); logsFor = s.id }
    } finally {
      logsLoading = false
    }
  }
  $effect(() => {
    const s = current
    if (s && logsFor !== s.id) { logs = ''; void loadLogs(s) }
  })

  // ── Settings ──────────────────────────────────────────────────────────────
  /** @param {LocalServer} s @param {boolean} on */
  async function setAutostart(s, on) {
    try {
      await updateServer(s.id, { autostart: on })
      await refresh(true)
    } catch (e) {
      toast.error('Could not save that', { description: errorText(e) })
    }
  }

  let renaming = $state(false)
  let newName = $state('')
  async function saveName() {
    if (!current || !newName.trim()) { renaming = false; return }
    try {
      await updateServer(current.id, { name: newName.trim() })
      await refresh(true)
    } catch (e) {
      toast.error('Could not rename it', { description: errorText(e) })
    } finally {
      renaming = false
    }
  }

  let editingPort = $state(false)
  let newPort = $state('')
  let portError = $state('')
  let savingPort = $state(false)
  async function editPort() {
    if (!current) return
    portError = ''
    newPort = String(await freePort(current.port + 1).catch(() => current.port + 1))
    editingPort = true
  }
  async function savePort() {
    if (!current || savingPort) return
    savingPort = true
    portError = ''
    try {
      await updateServer(current.id, { port: Number(newPort) })
      await refresh(true)
      editingPort = false
    } catch (e) {
      portError = String(e)
    } finally {
      savingPort = false
    }
  }
  $effect(() => { void selected; editingPort = false; renaming = false })

  // ── New server ────────────────────────────────────────────────────────────
  let fEngine = $state('postgres')
  let fVersion = $state('17')
  /** @type {'docker' | 'native'} */
  let fRuntime = $state('native')
  let fName = $state('')
  let fPort = $state('5432')
  let fAutostart = $state(false)
  let nameTouched = false
  let creating = $state(false)
  let progress = $state(/** @type {{ message: string, fraction: number | null } | null} */ (null))
  let formError = $state('')

  const engine = $derived(engineOf(fEngine))
  const nativeOk = $derived(fEngine === 'postgres' || !!env?.native.includes(fEngine))
  const dockerOk = $derived(!!env?.docker)
  const portNum = $derived(Number(fPort))
  const portTaken = $derived(servers.find((s) => s.port === portNum) ?? null)
  const portFound = $derived(found.find((f) => f.port === portNum) ?? null)
  const canCreate = $derived(
    !creating && fName.trim().length > 0 && Number.isInteger(portNum) && portNum >= 1024 && portNum <= 65535 && !portTaken && !portFound &&
      (fRuntime === 'docker' ? dockerOk : nativeOk),
  )

  function startNew() {
    fRuntime = 'native'
    nameTouched = false
    pickEngine('postgres')
    fAutostart = false
    formError = ''
    progress = null
    selected = 'new'
  }

  /** @param {string} id */
  function pickEngine(id) {
    fEngine = id
    const e = engineOf(id)
    fVersion = e.versions[1] ?? e.versions[0]
    fPort = String(nextPort(id, [...servers, ...found]))
    const want = Number(fPort)
    void freePort(want).then((p) => { if (fEngine === id && Number(fPort) === want) fPort = String(p) }).catch(() => {})
    if (!nameTouched || !fName) { fName = `${e.label} ${fVersion}`; nameTouched = false }
    const canNative = id === 'postgres' || !!env?.native.includes(id)
    if (fRuntime === 'native' && !canNative && env?.docker) fRuntime = 'docker'
    if (fRuntime === 'docker' && !env?.docker && canNative) fRuntime = 'native'
  }

  /** @param {string} v */
  function pickVersion(v) {
    fVersion = v
    if (!nameTouched) fName = `${engine.label} ${v}`
  }

  /** @type {(() => void) | null} */
  let unlisten = null
  onDestroy(() => unlisten?.())

  async function create() {
    if (!canCreate) return
    creating = true
    formError = ''
    progress = { message: fRuntime === 'docker' ? 'Starting' : 'Preparing', fraction: null }
    try {
      const { listen } = await import('@tauri-apps/api/event')
      unlisten?.()
      unlisten = await listen('local-server-progress', (e) => {
        const p = /** @type {{ message: string, fraction: number | null }} */ (e.payload)
        progress = { message: p.message, fraction: p.fraction }
      })
      const s = /** @type {LocalServer} */ (await createServer({ name: fName.trim(), engine: fEngine, version: fVersion, port: portNum, runtime: fRuntime }))
      if (fAutostart) await updateServer(s.id, { autostart: true }).catch(() => {})
      progress = { message: `Starting ${s.name}`, fraction: null }
      // Created either way: a start that fails shows on the server's page,
      // where it now is, not in a form that would create it twice.
      try {
        await startServer(s.id)
      } catch (e) {
        failures = { ...failures, [s.id]: String(e) }
      }
      await refresh(true)
      selected = s.id
    } catch (e) {
      formError = String(e)
      await refresh(true)
    } finally {
      unlisten?.()
      unlisten = null
      creating = false
      progress = null
    }
  }

  /** @param {import('$lib/local-servers.js').BrewService} b */
  async function brewToggle(b) {
    if (busy) return
    busy = `brew:${b.name}`
    try {
      await brewAction(b.name, b.running ? 'stop' : 'start')
      brew = await brewServices()
      void scan()
    } catch (e) {
      toast.error(`Could not ${b.running ? 'stop' : 'start'} ${b.name}`, { description: errorText(e) })
    } finally {
      busy = null
    }
  }

  const runtimeLabel = (/** @type {string} */ r) => (r === 'docker' ? 'Docker' : 'Native')
  /** @param {string} engineId */
  const tile = (engineId) => {
    const c = engineOf(engineId).color
    return `background: color-mix(in oklch, ${c} 20%, transparent); color: color-mix(in oklch, ${c} 60%, var(--foreground))`
  }
  /** Connection fields shown on a server's page. @param {{ engine: string, port: number }} s */
  function fields(s) {
    /** @type {[string, string][]} */
    const out = [['Host', '127.0.0.1'], ['Port', String(s.port)]]
    if (s.engine === 'postgres') out.push(['User', 'postgres'], ['Database', 'postgres'])
    else if (s.engine === 'mysql' || s.engine === 'mariadb') out.push(['User', 'root'])
    out.push(['Password', 'None (local only)'])
    return out
  }
</script>

{#snippet engineTile(/** @type {string} */ id, /** @type {string} */ size, /** @type {string | null} */ dot)}
  <span class={cn('relative flex shrink-0 items-center justify-center rounded-lg', size)} style={tile(id)}>
    <DbIcon {id} class={size === 'size-12' ? 'size-6' : 'size-4'} />
    {#if dot}<span class={cn('absolute -right-0.5 -bottom-0.5 size-2.5 rounded-full ring-2 ring-sidebar', dot)}></span>{/if}
  </span>
{/snippet}

{#snippet copyButton(/** @type {string} */ text, /** @type {string} */ what, /** @type {string} */ label)}
  <button
    type="button"
    class="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
    title={label}
    aria-label={label}
    onclick={() => void copy(text, what)}
  >
    {#if copied === what}<Check class="size-3.5 shrink-0 text-success" />{:else}<Copy class="size-3.5 shrink-0" />{/if}
  </button>
{/snippet}

{#snippet connectionCard(/** @type {{ engine: string, port: number }} */ s, /** @type {string} */ key)}
  {@const url = connectionUrl(s)}
  <section class="flex flex-col rounded-lg border border-border/60" aria-label="Connection">
    <header class="flex h-10 items-center border-b border-border/60 px-4">
      <h2 class="text-ui-sm font-medium">Connection</h2>
    </header>
    <dl class="grid grid-cols-[7rem_1fr_auto] items-center gap-x-3 px-4 py-2">
      {#each fields(s) as [k, v] (k)}
        <dt class="py-1.5 text-ui-xs text-muted-foreground">{k}</dt>
        <dd class={cn('min-w-0 truncate py-1.5 text-ui-sm', k !== 'Password' && 'font-mono')}>{v}</dd>
        {#if k !== 'Password'}{@render copyButton(v, `${key}:${k}`, `Copy ${k.toLowerCase()}`)}{:else}<span></span>{/if}
      {/each}
    </dl>
    <div class="flex flex-col gap-2 border-t border-border/60 px-4 py-3">
      <div class="flex items-center gap-2 rounded-md bg-muted/40 py-1 ps-3 pe-1">
        <code class="min-w-0 flex-1 truncate font-mono text-ui-xs" title={url}>{url}</code>
        {@render copyButton(url, `${key}:url`, 'Copy URL')}
      </div>
      <div class="flex items-center gap-2 rounded-md bg-muted/40 py-1 ps-3 pe-1">
        <code class="min-w-0 flex-1 truncate font-mono text-ui-xs text-muted-foreground"><span class="text-foreground">{envName(s.engine)}</span>="{url}"</code>
        {@render copyButton(`${envName(s.engine)}="${url}"`, `${key}:env`, 'Copy as .env line')}
      </div>
    </div>
  </section>
{/snippet}

<div class="flex h-screen flex-col overflow-hidden bg-background text-foreground">
  <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border/60 px-3" data-tauri-drag-region>
    <div class={cn('flex items-center gap-2', isMac && 'ps-[72px]')} data-tauri-drag-region>
      <Server class="size-4 shrink-0 text-muted-foreground" />
      <h1 class="text-ui-sm font-medium" data-tauri-drag-region>Local Servers</h1>
    </div>
    <div class="flex-1 self-stretch" data-tauri-drag-region></div>
    <Button variant="ghost" size="icon-sm" class="text-muted-foreground" title="Refresh" aria-label="Refresh" onclick={() => { void refresh(); void scan() }}>
      <RefreshCw class={cn('size-3.5 shrink-0', (loading || scanning) && 'animate-pulse')} />
    </Button>
    <Button size="sm" onclick={startNew}><Plus class="size-3.5 shrink-0" />New server</Button>
    {#if !isMac}<WindowControls />{/if}
  </header>

  <div class="flex min-h-0 flex-1">
    <!-- The list: Stroke's servers, then the rest of this computer's. -->
    <nav class="app-scroll flex w-64 shrink-0 flex-col gap-4 overflow-y-auto border-r border-border/60 bg-sidebar px-2 py-3" aria-label="Servers">
      <div class="flex flex-col gap-0.5">
        <p class="px-2 pb-1 text-ui-2xs font-medium text-muted-foreground">Stroke servers</p>
        {#each servers as s (s.id)}
          {@const on = selected === s.id}
          <button
            type="button"
            class={cn('flex items-center gap-2.5 rounded-md px-2 py-1.5 text-left transition-colors', on ? 'bg-accent text-accent-foreground' : 'hover:bg-muted/60')}
            aria-current={on ? 'true' : undefined}
            onclick={() => (selected = s.id)}
          >
            {@render engineTile(s.engine, 'size-8', busy === s.id ? 'animate-pulse bg-warning' : failures[s.id] ? 'bg-destructive' : s.running ? 'bg-success' : 'bg-muted-foreground/40')}
            <span class="min-w-0 flex-1">
              <span class="block truncate text-ui-sm">{s.name}</span>
              <span class="block truncate text-ui-2xs text-muted-foreground">{s.running ? 'Running' : 'Stopped'} · <span class="font-mono">:{s.port}</span></span>
            </span>
          </button>
        {:else}
          {#if loaded}
            <button type="button" class={cn('flex items-center gap-2.5 rounded-md px-2 py-2 text-left text-ui-sm text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground', selected === 'new' && 'bg-accent text-accent-foreground')} onclick={startNew}>
              <span class="flex size-8 items-center justify-center rounded-lg border border-dashed border-border"><Plus class="size-3.5 shrink-0" /></span>
              New server
            </button>
          {/if}
        {/each}
      </div>

      <div class="flex flex-col gap-0.5">
        <p class="flex items-center gap-1.5 px-2 pb-1 text-ui-2xs font-medium text-muted-foreground">
          Also on this computer
          {#if scanning}<Radar class="size-3 shrink-0 animate-pulse" />{/if}
        </p>
        {#each found as f (f.port)}
          {@const on = selected === `found:${f.port}`}
          <button
            type="button"
            class={cn('flex items-center gap-2.5 rounded-md px-2 py-1.5 text-left transition-colors', on ? 'bg-accent text-accent-foreground' : 'hover:bg-muted/60')}
            aria-current={on ? 'true' : undefined}
            onclick={() => (selected = `found:${f.port}`)}
          >
            {@render engineTile(f.engine, 'size-8', 'bg-success')}
            <span class="min-w-0 flex-1">
              <span class="block truncate text-ui-sm">{f.source === 'docker' ? f.name : engineOf(f.engine).label}</span>
              <span class="block truncate text-ui-2xs text-muted-foreground">{f.source === 'docker' ? 'Docker' : 'Process'} · <span class="font-mono">:{f.port}</span></span>
            </span>
          </button>
        {:else}
          <p class="px-2 text-ui-2xs leading-relaxed text-muted-foreground">{scanning ? 'Looking for databases' : 'No other databases are listening on the usual ports.'}</p>
        {/each}
      </div>

      {#if env?.brew && brew.length}
        <div class="flex flex-col gap-0.5">
          <p class="flex items-center gap-1.5 px-2 pb-1 text-ui-2xs font-medium text-muted-foreground"><Beer class="size-3 shrink-0" />Homebrew</p>
          {#each brew as b (b.name)}
            <div class="flex items-center gap-2 rounded-md px-2 py-1">
              <span class={cn('size-1.5 shrink-0 rounded-full', b.running ? 'bg-success' : 'bg-muted-foreground/40')}></span>
              <span class="min-w-0 flex-1 truncate font-mono text-ui-xs">{b.name}</span>
              <button
                type="button"
                class="h-6 rounded px-2 text-ui-2xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
                disabled={busy === `brew:${b.name}`}
                onclick={() => void brewToggle(b)}
              >{busy === `brew:${b.name}` ? '…' : b.running ? 'Stop' : 'Start'}</button>
            </div>
          {/each}
        </div>
      {/if}

      <div class="mt-auto px-2 pt-2 text-ui-2xs leading-relaxed text-muted-foreground">
        {#if env}
          <p class="flex items-center gap-1.5">
            <span class={cn('size-1.5 shrink-0 rounded-full', env.docker ? 'bg-success' : 'bg-muted-foreground/40')}></span>
            {env.docker ? 'Docker is running' : env.dockerInstalled ? "Docker isn't running" : "Docker isn't installed"}
          </p>
        {/if}
      </div>
    </nav>

    <!-- The picked server. -->
    <main class="app-scroll min-w-0 flex-1 overflow-y-auto">
      {#if selected === 'new'}
        <form class="mx-auto flex max-w-2xl flex-col gap-6 px-8 py-8" onsubmit={(e) => { e.preventDefault(); void create() }}>
          <div class="flex flex-col gap-1">
            <h2 class="text-ui-xl font-semibold">New server</h2>
            <p class="text-ui-sm text-muted-foreground">Listens on 127.0.0.1 only, with no password. For development on this computer.</p>
          </div>
          <div class="flex flex-col gap-2" role="group" aria-labelledby="ls-engine">
            <p id="ls-engine" class="text-ui-xs text-muted-foreground">Engine</p>
            <div class="grid grid-cols-5 gap-2">
              {#each ENGINES as e (e.id)}
                {@const on = fEngine === e.id}
                <button
                  type="button"
                  aria-pressed={on}
                  disabled={creating}
                  class={cn(
                    'flex flex-col items-center gap-2 rounded-lg border px-1 py-3.5 text-ui-xs transition-colors',
                    on ? 'border-primary/70 bg-primary/10 text-foreground' : 'border-border/60 text-muted-foreground hover:border-border hover:bg-muted/40 hover:text-foreground',
                  )}
                  onclick={() => pickEngine(e.id)}
                >
                  <span style="color: color-mix(in oklab, {e.color} {on ? 70 : 45}%, var(--foreground))"><DbIcon id={e.id} class="size-6" /></span>
                  <span class="truncate">{e.label}</span>
                </button>
              {/each}
            </div>
          </div>
          <div class="flex flex-col gap-2" role="group" aria-labelledby="ls-version">
            <p id="ls-version" class="text-ui-xs text-muted-foreground">Version</p>
            <div class="flex flex-wrap gap-1.5">
              {#each engine.versions as v (v)}
                {@const on = fVersion === v}
                <button
                  type="button"
                  aria-pressed={on}
                  disabled={creating}
                  class={cn('h-7 min-w-11 rounded-md border px-2.5 font-mono text-ui-xs transition-colors', on ? 'border-primary/70 bg-primary/10 text-foreground' : 'border-transparent bg-muted/50 text-muted-foreground hover:bg-muted hover:text-foreground')}
                  onclick={() => pickVersion(v)}
                >{v}</button>
              {/each}
            </div>
          </div>
          <div class="flex flex-col gap-2" role="group" aria-labelledby="ls-runs-on">
            <p id="ls-runs-on" class="text-ui-xs text-muted-foreground">Runs on</p>
            <div class="grid grid-cols-2 gap-2">
              {#each [
                { id: /** @type {const} */ ('native'), Icon: Cpu, title: 'Native', ok: nativeOk,
                  body: fEngine === 'postgres' ? `Downloads Postgres ${fVersion} once, about 30 MB. No Docker needed.` : nativeOk ? `Uses the ${engine.label} installed on this computer.` : `${engine.label} isn't installed on this computer.` },
                { id: /** @type {const} */ ('docker'), Icon: Container, title: 'Docker', ok: dockerOk,
                  body: dockerOk ? 'A container, its data in a Docker volume.' : env?.dockerInstalled ? 'Start Docker Desktop to use this.' : 'Install Docker Desktop to use this.' },
              ] as r (r.id)}
                {@const on = fRuntime === r.id}
                <button
                  type="button"
                  aria-pressed={on}
                  disabled={!r.ok || creating}
                  class={cn(
                    'relative flex items-start gap-2.5 rounded-lg border p-3 text-left transition-colors disabled:cursor-not-allowed',
                    on ? 'border-primary/70 bg-primary/10' : 'border-border/60 hover:border-border hover:bg-muted/40',
                    !r.ok && 'opacity-60',
                  )}
                  onclick={() => (fRuntime = r.id)}
                >
                  <r.Icon class={cn('mt-0.5 size-4 shrink-0', on ? 'text-primary' : 'text-muted-foreground')} />
                  <span class="min-w-0 pe-4">
                    <span class="block text-ui-sm font-medium">{r.title}</span>
                    <span class="mt-0.5 block text-ui-xs leading-snug text-muted-foreground">{r.body}</span>
                  </span>
                  {#if on}<Check class="absolute top-2.5 right-2.5 size-3.5 text-primary" />{/if}
                </button>
              {/each}
            </div>
          </div>
          <div class="grid grid-cols-[1fr_8rem] gap-3">
            <label class="flex flex-col gap-1.5">
              <span class="text-ui-xs text-muted-foreground">Name</span>
              <Input bind:value={fName} disabled={creating} oninput={() => (nameTouched = true)} />
            </label>
            <label class="flex flex-col gap-1.5">
              <span class="text-ui-xs text-muted-foreground">Port</span>
              <Input bind:value={fPort} inputmode="numeric" class="font-mono" disabled={creating} aria-invalid={!!(portTaken || portFound)} />
            </label>
          </div>
          {#if portTaken}<p class="-mt-4 text-ui-xs text-destructive">{portTaken.name} already uses port {portNum}.</p>
          {:else if portFound}<p class="-mt-4 text-ui-xs text-destructive">{portFound.source === 'docker' ? portFound.name : engineOf(portFound.engine).label} is already listening on port {portNum}.</p>{/if}
          <label class="flex items-center justify-between gap-4 rounded-lg border border-border/60 px-4 py-3">
            <span class="flex flex-col gap-0.5">
              <span class="text-ui-sm">Start with Stroke</span>
              <span class="text-ui-xs text-muted-foreground">Starts this server whenever Stroke opens.</span>
            </span>
            <Switch bind:checked={fAutostart} disabled={creating} aria-label="Start with Stroke" />
          </label>
          {#if progress}
            <div class="flex flex-col gap-1.5" aria-live="polite">
              <p class="text-ui-xs text-muted-foreground">{progress.message}</p>
              <div class="h-1 overflow-hidden rounded-full bg-muted">
                <div class={cn('h-full bg-primary transition-[width] duration-300', progress.fraction == null && 'w-1/3 animate-pulse')} style={progress.fraction != null ? `width: ${Math.round(progress.fraction * 100)}%` : undefined}></div>
              </div>
            </div>
          {/if}
          {#if formError}<p class="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-ui-xs break-words whitespace-pre-wrap text-destructive" role="alert">{formError}</p>{/if}
          <div class="flex justify-end gap-2">
            {#if servers.length}<Button type="button" variant="ghost" disabled={creating} onclick={() => (selected = servers[0]?.id ?? null)}>Cancel</Button>{/if}
            <Button type="submit" disabled={!canCreate}>{creating ? 'Creating…' : 'Create and start'}</Button>
          </div>
        </form>
      {:else if current}
        {@const s = current}
        {@const e = engineOf(s.engine)}
        {@const isBusy = busy === s.id}
        <div class="mx-auto flex max-w-2xl flex-col gap-5 px-8 py-8">
          <div class="flex items-start gap-4">
            {@render engineTile(s.engine, 'size-12', null)}
            <div class="min-w-0 flex-1">
              {#if renaming}
                <form onsubmit={(ev) => { ev.preventDefault(); void saveName() }}>
                  <!-- svelte-ignore a11y_autofocus -->
                  <Input bind:value={newName} class="h-8 text-ui-lg font-semibold" autofocus onblur={() => void saveName()} onkeydown={(ev) => { if (ev.key === 'Escape') renaming = false }} aria-label="Server name" />
                </form>
              {:else}
                <button type="button" class="max-w-full truncate rounded text-left text-ui-xl font-semibold hover:text-muted-foreground" title="Rename" onclick={() => { newName = s.name; renaming = true }}>{s.name}</button>
              {/if}
              <p class="mt-0.5 text-ui-sm text-muted-foreground">{e.label} {s.version} · {runtimeLabel(s.runtime)}</p>
            </div>
            <div class="flex shrink-0 items-center gap-1.5">
              <Button variant={s.running ? 'outline' : 'default'} disabled={isBusy || (!!s.problem && !s.running)} onclick={() => void toggle(s)}>
                {#if s.running}<Square class="size-3 shrink-0 fill-current" />{:else}<Play class="size-3 shrink-0 fill-current" />{/if}
                {isBusy ? (s.running ? 'Stopping' : 'Starting') : s.running ? 'Stop' : 'Start'}
              </Button>
              <Button variant="outline" disabled={!s.running} title={s.running ? `Open ${s.name} in Stroke` : 'Start it to open it'} onclick={() => void openIn(connectionFor(s))}>
                Open<ArrowUpRight class="size-3.5 shrink-0" />
              </Button>
              <DropdownMenu.Root>
                <DropdownMenu.Trigger class="flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground data-[state=open]:bg-accent" aria-label="More for {s.name}">
                  <MoreHorizontal class="size-4 shrink-0" />
                </DropdownMenu.Trigger>
                <DropdownMenu.Content align="end" class="min-w-48">
                  <DropdownMenu.Item disabled={s.running} onclick={() => void editPort()}><Hash class="size-3.5 shrink-0" />Change port…</DropdownMenu.Item>
                  {#if s.runtime === 'native'}
                    <DropdownMenu.Item onclick={() => void revealServer(s.id).catch((err) => toast.error('Could not open the folder', { description: errorText(err) }))}><FolderOpen class="size-3.5 shrink-0" />Show data folder</DropdownMenu.Item>
                  {/if}
                  <DropdownMenu.Separator />
                  <DropdownMenu.Item variant="destructive" onclick={() => void remove(s)}><Trash2 class="size-3.5 shrink-0" />Delete with its data</DropdownMenu.Item>
                </DropdownMenu.Content>
              </DropdownMenu.Root>
            </div>
          </div>

          {#if failures[s.id] || s.problem}
            <div class="flex items-start gap-2.5 rounded-lg border border-destructive/30 bg-destructive/5 px-4 py-3" role="alert">
              <TriangleAlert class="mt-0.5 size-4 shrink-0 text-destructive" />
              <div class="min-w-0 flex-1">
                <p class="text-ui-sm font-medium text-destructive">{failures[s.id] ? (s.running ? "Didn't stop" : "Didn't start") : s.problem}</p>
                {#if failures[s.id]}<p class="mt-1 font-mono text-ui-2xs break-words whitespace-pre-wrap text-muted-foreground">{failures[s.id]}</p>{/if}
              </div>
            </div>
          {/if}

          {#if editingPort}
            <form class="flex items-end gap-2 rounded-lg border border-border/60 p-4" onsubmit={(ev) => { ev.preventDefault(); void savePort() }}>
              <label class="flex flex-col gap-1.5">
                <span class="text-ui-xs text-muted-foreground">New port (now {s.port})</span>
                <Input bind:value={newPort} inputmode="numeric" class="w-32 font-mono" disabled={savingPort} />
              </label>
              <Button type="submit" size="sm" disabled={savingPort || !/^\d+$/.test(newPort)}>{savingPort ? 'Saving…' : 'Save'}</Button>
              <Button type="button" variant="ghost" size="sm" disabled={savingPort} onclick={() => (editingPort = false)}>Cancel</Button>
              {#if portError}<p class="ms-2 self-center text-ui-xs text-destructive" role="alert">{portError}</p>{/if}
            </form>
          {/if}

          <!-- At a glance. -->
          <dl class="grid grid-cols-3 overflow-hidden rounded-lg border border-border/60">
            <div class="flex flex-col gap-1 px-4 py-3">
              <dt class="text-ui-2xs text-muted-foreground">Status</dt>
              <dd class="flex items-center gap-1.5 text-ui-sm">
                <span class={cn('size-2 shrink-0 rounded-full', isBusy ? 'animate-pulse bg-warning' : s.running ? 'bg-success' : 'bg-muted-foreground/40')}></span>
                {isBusy ? (s.running ? 'Stopping' : 'Starting') : s.running ? 'Running' : 'Stopped'}
              </dd>
            </div>
            <div class="flex flex-col gap-1 border-s border-border/60 px-4 py-3">
              <dt class="text-ui-2xs text-muted-foreground">{s.running ? 'Up for' : 'Port'}</dt>
              <dd class="text-ui-sm tabular-nums">{s.running ? uptime(s.startedAt, now) || 'Just started' : s.port}</dd>
            </div>
            <div class="flex flex-col gap-1 border-s border-border/60 px-4 py-3">
              <dt class="text-ui-2xs text-muted-foreground">Data</dt>
              <dd class="truncate text-ui-sm tabular-nums" title={s.dataDir ?? ''}>{s.runtime === 'docker' ? 'Docker volume' : formatBytes(s.diskBytes) || '0 B'}</dd>
            </div>
          </dl>

          {@render connectionCard(s, s.id)}

          <label class="flex items-center justify-between gap-4 rounded-lg border border-border/60 px-4 py-3">
            <span class="flex flex-col gap-0.5">
              <span class="text-ui-sm">Start with Stroke</span>
              <span class="text-ui-xs text-muted-foreground">Starts this server whenever Stroke opens.</span>
            </span>
            <Switch checked={s.autostart} onCheckedChange={(v) => void setAutostart(s, v)} aria-label="Start with Stroke" />
          </label>

          <section class="flex flex-col rounded-lg border border-border/60" aria-label="Log">
            <header class="flex h-10 items-center gap-2 border-b border-border/60 ps-4 pe-2">
              <h2 class="flex-1 text-ui-sm font-medium">Log</h2>
              <Button variant="ghost" size="icon-sm" class="text-muted-foreground" title="Reload the log" aria-label="Reload the log" onclick={() => void loadLogs(s)}>
                <RefreshCw class={cn('size-3.5 shrink-0', logsLoading && 'animate-pulse')} />
              </Button>
            </header>
            <pre bind:this={logEl} class="app-scroll max-h-64 min-h-24 overflow-auto px-4 py-3 font-mono text-ui-2xs leading-relaxed whitespace-pre-wrap text-muted-foreground">{logsFor === s.id ? logs || 'Nothing logged yet.' : ''}</pre>
          </section>
        </div>
      {:else if currentFound}
        {@const f = currentFound}
        {@const e = engineOf(f.engine)}
        <div class="mx-auto flex max-w-2xl flex-col gap-5 px-8 py-8">
          <div class="flex items-start gap-4">
            {@render engineTile(f.engine, 'size-12', null)}
            <div class="min-w-0 flex-1">
              <h2 class="truncate text-ui-xl font-semibold">{f.source === 'docker' ? f.name : `${e.label} on ${f.port}`}</h2>
              <p class="mt-0.5 text-ui-sm text-muted-foreground">
                {e.label} · {f.source === 'docker' ? `Docker container${f.image ? `, ${f.image}` : ''}` : 'Running on this computer'}
              </p>
            </div>
            <Button onclick={() => void openIn(foundConnection(f))}>Open in Stroke<ArrowUpRight class="size-3.5 shrink-0" /></Button>
          </div>
          <p class="rounded-lg bg-muted/40 px-4 py-3 text-ui-xs leading-relaxed text-muted-foreground">
            Stroke didn't start this server, so it can't stop it or read its log. It opens with the usual defaults below; if it asks for a password, set it in the connection.
          </p>
          {@render connectionCard(f, `found:${f.port}`)}
        </div>
      {:else if loaded}
        <EmptyState
          class="h-full justify-center"
          icon={Server}
          title="Pick a server"
          description="Or make one: Postgres, MySQL, MariaDB, Redis or MongoDB, any version, ready in a minute."
        >
          <Button size="sm" onclick={startNew}><Plus class="size-3.5 shrink-0" />New server</Button>
        </EmptyState>
      {/if}
    </main>
  </div>
</div>
