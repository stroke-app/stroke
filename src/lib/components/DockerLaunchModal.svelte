<script>
  import { Button } from '$lib/components/ui/button/index.js'
  import { onDestroy, tick } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import Database from '@lucide/svelte/icons/database'
  import HardDrive from '@lucide/svelte/icons/hard-drive'
  import Package from '@lucide/svelte/icons/package'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import CheckCircle from '@lucide/svelte/icons/check-circle'
  import XCircle from '@lucide/svelte/icons/x-circle'
  import { dockerCheck, dockerRunDb } from '$lib/api.js'
  import { cn } from '$lib/utils.js'

  let {
    open = $bindable(false),
    /** Accepted for call-site compatibility; the pick list renders no pre-selection. */
    initialDbType = /** @type {string | null} */ (null),
    /** Called with connection info when user clicks Connect */
    onconnect = /** @type {(info: { db_type: string, host: string, port: number, user: string, password: string, database: string, name: string }) => void} */ (() => {}),
  } = $props()

  /** @type {'pick' | 'running' | 'done' | 'error'} */
  let phase = $state('pick')
  let selectedDb = $state(/** @type {string | null} */ (null))
  let logs = $state(/** @type {string[]} */ ([]))
  let errorMsg = $state('')
  let connInfo = $state(/** @type {any} */ (null))
  let logEl = $state(/** @type {HTMLDivElement | null} */ (null))

  let _unlisten = /** @type {(() => void) | null} */ (null)

  $effect(() => {
    if (!open) reset()
  })

  function reset() {
    phase = 'pick'
    selectedDb = null
    logs = []
    errorMsg = ''
    connInfo = null
    if (_unlisten) { _unlisten(); _unlisten = null }
  }

  async function launch(/** @type {string} */ dbType) {
    selectedDb = dbType
    phase = 'running'
    logs = []
    errorMsg = ''
    connInfo = null

    const eventId = crypto.randomUUID()
    const evtName = `docker-log:${eventId}`

    try {
      _unlisten = await listen(evtName, (event) => {
        const payload = /** @type {{ line: string, kind: string }} */ (event.payload)
        logs = [...logs, payload.line]
        tick().then(() => {
          if (logEl) logEl.scrollTop = logEl.scrollHeight
        })
      })

      await dockerCheck()
      const result = await dockerRunDb(dbType, eventId)
      connInfo = result
      phase = 'done'
    } catch (err) {
      errorMsg = String(err)
      phase = 'error'
    } finally {
      if (_unlisten) { _unlisten(); _unlisten = null }
    }
  }

  function handleConnect() {
    if (!connInfo) return
    open = false
    onconnect(connInfo)
  }

  onDestroy(() => {
    if (_unlisten) _unlisten()
  })

  const DB_OPTIONS = [
    {
      type: 'postgres',
      label: 'PostgreSQL',
      desc: 'postgres:17-alpine  ·  host port 5433',
      Icon: Database,
    },
    {
      type: 'mysql',
      label: 'MySQL',
      desc: 'mysql:8.4  ·  host port 3307',
      Icon: HardDrive,
    },
  ]
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title class="flex items-center gap-2">
        <Package class="size-4 shrink-0 text-muted-foreground" />
        Docker Database
      </Dialog.Title>
      <Dialog.Description>
        Pull and run a database container, then connect instantly.
      </Dialog.Description>
    </Dialog.Header>

    {#if phase === 'pick'}
      <div class="flex flex-col gap-2 py-1">
        {#each DB_OPTIONS as opt (opt.type)}
          <button
            type="button"
            class="flex items-center gap-3 rounded-lg border border-border bg-card px-4 py-3 text-left transition-colors hover:bg-accent hover:text-foreground"
            onclick={() => launch(opt.type)}
          >
            <opt.Icon class="size-5 shrink-0 text-muted-foreground" />
            <div class="min-w-0">
              <div class="text-ui-sm font-medium">{opt.label}</div>
              <div class="font-mono text-ui-xs text-muted-foreground">{opt.desc}</div>
            </div>
          </button>
        {/each}
        <p class="pt-1 text-center text-ui-xs text-muted-foreground">
          Requires Docker Desktop to be installed and running
        </p>
      </div>

    {:else if phase === 'running'}
      <div class="flex flex-col gap-3 py-1">
        <div class="flex items-center gap-2 text-ui-sm text-muted-foreground">
          <RefreshCw class="size-3.5 shrink-0 animate-spin" />
          <span>Pulling and starting {selectedDb === 'postgres' ? 'PostgreSQL' : 'MySQL'}…</span>
        </div>
        <div
          bind:this={logEl}
          class="h-52 overflow-y-auto rounded-md border border-border bg-black/5 dark:bg-white/5 p-3 font-mono text-ui-2xs leading-relaxed text-muted-foreground"
        >
          {#each logs as line, i (i)}
            <div class="whitespace-pre-wrap break-all">{line}</div>
          {/each}
          {#if logs.length === 0}
            <span class="opacity-40">Waiting for output…</span>
          {/if}
        </div>
      </div>

    {:else if phase === 'done' && connInfo}
      <div class="flex flex-col gap-3 py-1">
        <div class="flex items-center gap-2 text-ui-sm text-success">
          <CheckCircle class="size-4 shrink-0" />
          Container is running and ready
        </div>
        <div class="rounded-md border border-border bg-muted/40 px-3 py-2.5 font-mono text-ui-xs leading-loose">
          <div class="grid grid-cols-[3rem_1fr] gap-x-2">
            <span class="text-muted-foreground">host</span><span>{connInfo.host}:{connInfo.port}</span>
            <span class="text-muted-foreground">user</span><span>{connInfo.user}</span>
            <span class="text-muted-foreground">pass</span><span>{connInfo.password}</span>
            <span class="text-muted-foreground">db</span><span>{connInfo.database}</span>
          </div>
        </div>
        <Button
          class="w-full" size="lg"
          onclick={handleConnect}
>
          Connect now
        </Button>
      </div>

    {:else if phase === 'error'}
      <div class="flex flex-col gap-3 py-1">
        <div class="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2.5 text-ui-sm text-destructive">
          <XCircle class="mt-0.5 size-4 shrink-0" />
          <span class="leading-snug">{errorMsg}</span>
        </div>
        <button
          type="button"
          class="text-left text-ui-sm text-muted-foreground transition-colors hover:text-foreground"
          onclick={() => { phase = 'pick' }}
        >
          ← Try again
        </button>
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
