<script>
  // Local database servers, DBngin-style: Postgres, MySQL, MariaDB, Redis and
  // MongoDB on this computer, any version side by side, each on its own port.
  // Docker or native; Homebrew services listed beside them. Backend:
  // src-tauri/src/local_servers.rs. Opens from Tools → Local Servers and the
  // command palette.
  import { onDestroy } from 'svelte'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { cn } from '$lib/utils.js'
  import {
    ENGINES, engineOf, nextPort, connectionFor,
    localEnv, listServers, createServer, startServer, stopServer, deleteServer, serverLogs, brewServices, brewAction, freePort, updateServer,
  } from '$lib/local-servers.js'
  import Plus from '@lucide/svelte/icons/plus'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import ArrowRight from '@lucide/svelte/icons/arrow-right'
  import MoreHorizontal from '@lucide/svelte/icons/more-horizontal'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import ScrollText from '@lucide/svelte/icons/scroll-text'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Server from '@lucide/svelte/icons/server'
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert'
  import Hash from '@lucide/svelte/icons/hash'
  import Play from '@lucide/svelte/icons/play'
  import Square from '@lucide/svelte/icons/square'
  import Container from '@lucide/svelte/icons/container'
  import Cpu from '@lucide/svelte/icons/cpu'
  import Check from '@lucide/svelte/icons/check'
  import DbIcon from './DbIcon.svelte'

  let {
    open = $bindable(false),
    /** Connect to a running server. @type {(conn: any) => void} */
    onopen = () => {},
  } = $props()

  /** @typedef {import('$lib/local-servers.js').LocalServer} LocalServer */

  /** @type {'list' | 'new' | 'logs' | 'port'} */
  let view = $state('list')
  /** @type {LocalServer[]} */
  let servers = $state([])
  /** @type {import('$lib/local-servers.js').LocalEnv | null} */
  let env = $state(null)
  /** @type {import('$lib/local-servers.js').BrewService[]} */
  let brew = $state([])
  let brewOpen = $state(false)
  let loading = $state(false)
  /** Server or brew service with an action running. @type {string | null} */
  let busy = $state(null)

  async function refresh() {
    loading = true
    try {
      const [e, s] = await Promise.all([localEnv(), listServers()])
      env = e
      servers = s
      if (e.brew) brew = await brewServices().catch(() => [])
    } catch (e) {
      toast.error('Could not list local servers', { description: String(e) })
    } finally {
      loading = false
    }
  }

  $effect(() => {
    if (open) {
      view = 'list'
      void refresh()
    }
  })

  /** @param {LocalServer} s */
  async function toggle(s) {
    if (busy) return
    busy = s.id
    try {
      if (s.running) await stopServer(s.id)
      else await startServer(s.id)
      await refresh()
    } catch (e) {
      toast.error(s.running ? `Could not stop ${s.name}` : `Could not start ${s.name}`, { description: String(e) })
      await refresh()
    } finally {
      busy = null
    }
  }

  /** @param {LocalServer} s */
  async function remove(s) {
    if (busy) return
    busy = s.id
    try {
      await deleteServer(s.id)
      toast.success(`Deleted ${s.name}`, { description: 'Its data was deleted with it.' })
      await refresh()
    } catch (e) {
      toast.error(`Could not delete ${s.name}`, { description: String(e) })
    } finally {
      busy = null
    }
  }

  /** @param {LocalServer} s */
  function connect(s) {
    onopen(connectionFor(s))
    open = false
  }

  // ── Logs ──────────────────────────────────────────────────────────────────
  let logs = $state('')
  let logsFor = $state('')
  /** @param {LocalServer} s */
  async function showLogs(s) {
    logsFor = s.name
    logs = 'Loading…'
    view = 'logs'
    try {
      logs = (await serverLogs(s.id)).trim() || 'Nothing logged yet.'
    } catch (e) {
      logs = String(e)
    }
  }

  // ── Change port ───────────────────────────────────────────────────────────
  /** @type {LocalServer | null} */
  let portFor = $state(null)
  let newPort = $state('')
  let portError = $state('')
  let savingPort = $state(false)
  /** @param {LocalServer} s */
  async function editPort(s) {
    portFor = s
    portError = ''
    newPort = String(await freePort(s.port + 1).catch(() => s.port + 1))
    view = 'port'
  }
  async function savePort() {
    if (!portFor || savingPort) return
    savingPort = true
    portError = ''
    try {
      await updateServer(portFor.id, { port: Number(newPort) })
      await refresh()
      view = 'list'
    } catch (e) {
      portError = String(e)
    } finally {
      savingPort = false
    }
  }

  // ── New server ────────────────────────────────────────────────────────────
  let fEngine = $state('postgres')
  let fVersion = $state('17')
  /** @type {'docker' | 'native'} */
  let fRuntime = $state('docker')
  let fName = $state('')
  let fPort = $state('5432')
  let nameTouched = false
  let creating = $state(false)
  let progress = $state(/** @type {{ message: string, fraction: number | null } | null} */ (null))
  let formError = $state('')

  const engine = $derived(engineOf(fEngine))
  const nativeOk = $derived(fEngine === 'postgres' || !!env?.native.includes(fEngine))
  const dockerOk = $derived(!!env?.docker)
  const portNum = $derived(Number(fPort))
  const portTaken = $derived(servers.find((s) => s.port === portNum) ?? null)
  const canCreate = $derived(
    !creating && fName.trim().length > 0 && Number.isInteger(portNum) && portNum >= 1024 && portNum <= 65535 && !portTaken &&
      (fRuntime === 'docker' ? dockerOk : nativeOk),
  )

  function startNew() {
    fEngine = 'postgres'
    fRuntime = env?.docker ? 'docker' : 'native'
    pickEngine('postgres')
    formError = ''
    progress = null
    view = 'new'
  }

  /** @param {string} id */
  function pickEngine(id) {
    fEngine = id
    const e = engineOf(id)
    fVersion = e.versions[1] ?? e.versions[0]
    fPort = String(nextPort(id, servers))
    // Skip ports something else on this computer already listens on.
    const want = Number(fPort)
    void freePort(want).then((p) => { if (fEngine === id && Number(fPort) === want) fPort = String(p) }).catch(() => {})
    if (!nameTouched || !fName) { fName = `${e.label} ${fVersion}`; nameTouched = false }
    if (fRuntime === 'docker' && !env?.docker && (id === 'postgres' || env?.native.includes(id))) fRuntime = 'native'
    if (fRuntime === 'native' && !(id === 'postgres' || env?.native.includes(id))) fRuntime = 'docker'
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
      progress = { message: `Starting ${s.name}`, fraction: null }
      // Created either way: a start that fails is reported on the list, where
      // the server now is, not in a form that would create it twice.
      try {
        await startServer(s.id)
        toast.success(`${s.name} is running`, { description: `127.0.0.1:${s.port}` })
      } catch (e) {
        toast.error(`${s.name} was created but didn't start`, { description: String(e) })
      }
      await refresh()
      view = 'list'
    } catch (e) {
      formError = String(e)
      await refresh()
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
    } catch (e) {
      toast.error(`Could not ${b.running ? 'stop' : 'start'} ${b.name}`, { description: String(e) })
    } finally {
      busy = null
    }
  }

  const runtimeLabel = (/** @type {string} */ r) => (r === 'docker' ? 'Docker' : 'Native')
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="flex max-h-[min(40rem,85vh)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[34rem]">
    {#if view === 'list'}
      <Dialog.Header class="flex-row items-center gap-2 space-y-0 border-b border-border/60 py-3.5 ps-5 pe-12">
        <Dialog.Title class="flex-1 text-ui-lg">Local servers</Dialog.Title>
        <Dialog.Description class="sr-only">Database servers running on this computer.</Dialog.Description>
        <Button variant="ghost" size="icon-sm" class="text-muted-foreground" title="Refresh" aria-label="Refresh" onclick={() => void refresh()}>
          <RefreshCw class={cn('size-3.5 shrink-0', loading && 'animate-pulse')} />
        </Button>
        <Button size="sm" onclick={startNew}><Plus class="size-3.5 shrink-0" />New</Button>
      </Dialog.Header>

      <div class="app-scroll min-h-0 flex-1 overflow-y-auto py-2">
        {#if env && !env.docker}
          <p class="mx-5 mb-2 flex items-start gap-2 text-ui-xs text-muted-foreground">
            <TriangleAlert class="mt-0.5 size-3.5 shrink-0 text-warning" />
            {env.dockerInstalled ? "Docker isn't running. Start Docker Desktop for Docker servers; native Postgres works without it." : "Docker isn't installed. Native Postgres works without it."}
          </p>
        {/if}

        {#if servers.length === 0 && !loading}
          <div class="flex flex-col items-center gap-3 px-6 py-10 text-center">
            <span class="flex size-10 items-center justify-center rounded-lg bg-muted/50 text-muted-foreground"><Server class="size-5 shrink-0" /></span>
            <div class="flex max-w-xs flex-col gap-1">
              <p class="text-ui-sm font-medium text-foreground">No local servers yet</p>
              <p class="text-ui-xs text-muted-foreground">Postgres, MySQL, MariaDB, Redis or MongoDB, any version, ready in a minute.</p>
            </div>
            <Button size="sm" onclick={startNew}><Plus class="size-3.5 shrink-0" />New server</Button>
          </div>
        {:else}
          <ul class="flex flex-col">
            {#each servers as s (s.id)}
              {@const e = engineOf(s.engine)}
              <li class="group flex items-center gap-3 px-5 py-3 transition-colors hover:bg-muted/30">
                <span class="relative flex size-9 shrink-0 items-center justify-center rounded-lg" style="background: color-mix(in oklch, {e.color} 22%, transparent); color: color-mix(in oklch, {e.color} 55%, white)">
                  <DbIcon id={s.engine} class="size-[18px]" />
                  <span class={cn('absolute -right-0.5 -bottom-0.5 size-2.5 rounded-full ring-2 ring-background', busy === s.id ? 'animate-pulse bg-warning' : s.running ? 'bg-success' : 'bg-muted-foreground/50')}></span>
                </span>
                <div class="min-w-0 flex-1">
                  <p class="truncate text-ui-sm font-medium text-foreground">{s.name}</p>
                  <p class="truncate text-ui-xs text-muted-foreground">
                    {#if s.problem}<span class="text-warning" title={s.problem}>{s.problem}</span>
                    {:else}{e.label} {s.version} · <span class="font-mono">{s.running ? `127.0.0.1:${s.port}` : `port ${s.port}`}</span> · {runtimeLabel(s.runtime)}{/if}
                  </p>
                </div>
                <Button
                  variant={s.running ? 'outline' : 'secondary'}
                  size="sm"
                  class="w-20"
                  disabled={busy === s.id || (!!s.problem && !s.running)}
                  onclick={() => void toggle(s)}
                >
                  {#if s.running}<Square class="size-3 shrink-0 fill-current" />{:else}<Play class="size-3 shrink-0 fill-current" />{/if}
                  {busy === s.id ? (s.running ? 'Stopping' : 'Starting') : s.running ? 'Stop' : 'Start'}
                </Button>
                <Button size="sm" variant="ghost" class="text-muted-foreground" disabled={!s.running} title={s.running ? `Connect to ${s.name}` : 'Start it to connect'} onclick={() => connect(s)}>
                  Open<ArrowRight class="size-3.5 shrink-0" />
                </Button>
                <DropdownMenu.Root>
                  <DropdownMenu.Trigger class="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground data-[state=open]:bg-accent" aria-label="More for {s.name}">
                    <MoreHorizontal class="size-4 shrink-0" />
                  </DropdownMenu.Trigger>
                  <DropdownMenu.Content align="end" class="min-w-44">
                    <DropdownMenu.Item onclick={() => void showLogs(s)}><ScrollText class="size-3.5 shrink-0" />Logs</DropdownMenu.Item>
                    <DropdownMenu.Item disabled={s.running} onclick={() => void editPort(s)}><Hash class="size-3.5 shrink-0" />Change port…</DropdownMenu.Item>
                    <DropdownMenu.Separator />
                    <DropdownMenu.Item variant="destructive" onclick={() => void remove(s)}><Trash2 class="size-3.5 shrink-0" />Delete with its data</DropdownMenu.Item>
                  </DropdownMenu.Content>
                </DropdownMenu.Root>
              </li>
            {/each}
          </ul>
        {/if}

        {#if env?.brew && brew.length}
          <div class="mt-2 border-t border-border/40 pt-2">
            <button type="button" class="flex w-full items-center gap-2 px-5 py-1.5 text-ui-sm text-foreground hover:bg-muted/30" aria-expanded={brewOpen} onclick={() => (brewOpen = !brewOpen)}>
              <ChevronRight class={cn('size-3.5 shrink-0 text-muted-foreground transition-transform duration-150', brewOpen && 'rotate-90')} />Homebrew
              <span class="text-ui-xs text-muted-foreground">{brew.filter((b) => b.running).length} of {brew.length} running</span>
            </button>
            {#if brewOpen}
              <ul class="flex flex-col">
                {#each brew as b (b.name)}
                  <li class="flex items-center gap-3 py-2 ps-11 pe-5">
                    <span class="min-w-0 flex-1 truncate font-mono text-ui-xs text-foreground">{b.name}</span>
                    <Button variant="outline" size="sm" class="w-[5.5rem] justify-between" disabled={busy === `brew:${b.name}`} onclick={() => void brewToggle(b)}>
                      {b.running ? 'Stop' : 'Start'}
                      <span class={cn('size-2 shrink-0 rounded-full', b.running ? 'bg-success' : 'bg-destructive')}></span>
                    </Button>
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        {/if}
      </div>
    {:else if view === 'port' && portFor}
      <Dialog.Header class="space-y-0 border-b border-border/60 py-3.5 ps-5 pe-12">
        <Dialog.Title class="text-ui-lg">Change the port of {portFor.name}</Dialog.Title>
        <Dialog.Description class="sr-only">Pick a new port for this server.</Dialog.Description>
      </Dialog.Header>
      <form class="flex flex-col gap-3 px-5 py-4" onsubmit={(e) => { e.preventDefault(); void savePort() }}>
        <label class="flex flex-col gap-1.5">
          <span class="text-ui-xs text-muted-foreground">Port (now {portFor.port})</span>
          <Input bind:value={newPort} inputmode="numeric" class="w-32 font-mono" disabled={savingPort} />
        </label>
        {#if portError}<p class="text-ui-xs text-destructive" role="alert">{portError}</p>{/if}
        <div class="flex justify-end gap-2">
          <Button type="button" variant="ghost" size="sm" disabled={savingPort} onclick={() => (view = 'list')}>Cancel</Button>
          <Button type="submit" size="sm" disabled={savingPort || !/^\d+$/.test(newPort)}>{savingPort ? 'Saving…' : 'Save'}</Button>
        </div>
      </form>
    {:else if view === 'logs'}
      <Dialog.Header class="flex-row items-center gap-2 space-y-0 border-b border-border/60 py-3.5 ps-5 pe-12">
        <Dialog.Title class="flex-1 truncate text-ui-lg">{logsFor} logs</Dialog.Title>
        <Dialog.Description class="sr-only">The last lines the server wrote.</Dialog.Description>
        <Button variant="ghost" size="sm" onclick={() => (view = 'list')}>Back</Button>
      </Dialog.Header>
      <pre class="app-scroll min-h-0 flex-1 overflow-auto bg-background/40 p-4 font-mono text-ui-2xs whitespace-pre-wrap text-muted-foreground">{logs}</pre>
    {:else}
      <Dialog.Header class="space-y-0 border-b border-border/60 px-5 py-3.5">
        <Dialog.Title class="text-ui-lg">New local server</Dialog.Title>
        <Dialog.Description class="sr-only">Pick an engine, a version and where it runs.</Dialog.Description>
      </Dialog.Header>
      <form class="app-scroll flex min-h-0 flex-col gap-4 overflow-y-auto px-5 py-4" onsubmit={(e) => { e.preventDefault(); void create() }}>
        <fieldset class="flex flex-col gap-2" disabled={creating}>
          <legend class="mb-2 text-ui-xs text-muted-foreground">Engine</legend>
          <div class="grid grid-cols-5 gap-2">
            {#each ENGINES as e (e.id)}
              {@const on = fEngine === e.id}
              <button
                type="button"
                aria-pressed={on}
                class={cn(
                  'flex flex-col items-center gap-2 rounded-lg border px-1 py-3 text-ui-xs transition-colors',
                  on ? 'border-primary/70 bg-primary/10 text-foreground' : 'border-border/60 text-muted-foreground hover:border-border hover:bg-muted/40 hover:text-foreground',
                )}
                onclick={() => pickEngine(e.id)}
              >
                <span style="color: color-mix(in oklch, {e.color} {on ? 70 : 45}%, white)"><DbIcon id={e.id} class="size-6" /></span>
                <span class="truncate">{e.label}</span>
              </button>
            {/each}
          </div>
        </fieldset>
        <fieldset class="flex flex-col" disabled={creating}>
          <legend class="mb-2 text-ui-xs text-muted-foreground">Version</legend>
          <div class="flex flex-wrap gap-1.5">
            {#each engine.versions as v (v)}
              {@const on = fVersion === v}
              <button
                type="button"
                aria-pressed={on}
                class={cn('h-7 min-w-11 rounded-md px-2.5 font-mono text-ui-xs transition-colors', on ? 'bg-foreground text-background' : 'bg-muted/50 text-muted-foreground hover:bg-muted hover:text-foreground')}
                onclick={() => pickVersion(v)}
              >{v}</button>
            {/each}
          </div>
        </fieldset>
        <fieldset class="flex flex-col" disabled={creating}>
          <legend class="mb-2 text-ui-xs text-muted-foreground">Runs on</legend>
          <div class="grid grid-cols-2 gap-2">
            {#each [
              { id: /** @type {const} */ ('docker'), Icon: Container, title: 'Docker', ok: dockerOk,
                body: dockerOk ? 'A container on 127.0.0.1, its data in a Docker volume.' : env?.dockerInstalled ? 'Start Docker Desktop to use this.' : 'Install Docker Desktop to use this.' },
              { id: /** @type {const} */ ('native'), Icon: Cpu, title: 'Native', ok: nativeOk,
                body: fEngine === 'postgres' ? `Downloads Postgres ${fVersion} once, about 30 MB.` : nativeOk ? `Uses the ${engine.label} installed here.` : `${engine.label} isn't installed on this computer.` },
            ] as r (r.id)}
              {@const on = fRuntime === r.id}
              <button
                type="button"
                aria-pressed={on}
                disabled={!r.ok}
                class={cn(
                  'relative flex items-start gap-2.5 rounded-lg border p-3 text-left transition-colors disabled:cursor-not-allowed',
                  on ? 'border-primary/70 bg-primary/10' : 'border-border/60 hover:border-border hover:bg-muted/40',
                  !r.ok && 'opacity-60',
                )}
                onclick={() => (fRuntime = r.id)}
              >
                <r.Icon class={cn('mt-0.5 size-4 shrink-0', on ? 'text-primary' : 'text-muted-foreground')} />
                <span class="min-w-0">
                  <span class="block text-ui-sm font-medium text-foreground">{r.title}</span>
                  <span class="mt-0.5 block text-ui-2xs leading-snug text-muted-foreground">{r.body}</span>
                </span>
                {#if on}<Check class="absolute top-2.5 right-2.5 size-3.5 text-primary" />{/if}
              </button>
            {/each}
          </div>
        </fieldset>
        <div class="grid grid-cols-[1fr_7rem] gap-3">
          <label class="flex flex-col gap-1.5">
            <span class="text-ui-xs text-muted-foreground">Name</span>
            <Input bind:value={fName} disabled={creating} oninput={() => (nameTouched = true)} />
          </label>
          <label class="flex flex-col gap-1.5">
            <span class="text-ui-xs text-muted-foreground">Port</span>
            <Input bind:value={fPort} inputmode="numeric" class="font-mono" disabled={creating} aria-invalid={!!portTaken} />
          </label>
        </div>
        {#if portTaken}<p class="-mt-2 text-ui-xs text-destructive">{portTaken.name} already uses port {portNum}.</p>{/if}
        {#if progress}
          <div class="flex flex-col gap-1.5" aria-live="polite">
            <p class="text-ui-xs text-muted-foreground">{progress.message}</p>
            <div class="h-1 overflow-hidden rounded-full bg-muted">
              <div class={cn('h-full bg-primary transition-[width] duration-300', progress.fraction == null && 'w-1/3 animate-pulse')} style={progress.fraction != null ? `width: ${Math.round(progress.fraction * 100)}%` : undefined}></div>
            </div>
          </div>
        {/if}
        {#if formError}<p class="text-ui-xs break-words whitespace-pre-wrap text-destructive" role="alert">{formError}</p>{/if}
        <div class="flex justify-end gap-2 pt-1">
          <Button type="button" variant="ghost" size="sm" disabled={creating} onclick={() => (view = 'list')}>Cancel</Button>
          <Button type="submit" size="sm" disabled={!canCreate}>{creating ? 'Creating…' : 'Create server'}</Button>
        </div>
      </form>
    {/if}
  </Dialog.Content>
</Dialog.Root>
