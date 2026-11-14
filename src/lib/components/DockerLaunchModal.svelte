<script>
  /**
   * Starting a database in Docker: the image chosen in the ⌘K Docker page
   * starts as this opens, its pull and start-up stream in, and when it takes
   * connections the app connects to it. Opened without a choice it lists the
   * catalog (docker_templates).
   */
  import { Button } from '$lib/components/ui/button/index.js'
  import { onDestroy, tick, untrack } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import Icon from './Icon.svelte'
  import DbIcon from './DbIcon.svelte'
  import { dockerCheck, dockerRunDb, dockerTemplates } from '$lib/api.js'

  let {
    open = $bindable(false),
    /** A catalog id to start at once (from the palette), or null to pick. */
    initialDbType = /** @type {string | null} */ (null),
    /** The connection to open, once the container takes connections. */
    onconnect = /** @type {(conn: Record<string, any>) => void} */ (() => {}),
  } = $props()

  /** @type {'pick' | 'running' | 'error'} */
  let phase = $state('pick')
  /** @type {import('$lib/api.js').DockerTemplate[]} */
  let templates = $state([])
  /** @type {import('$lib/api.js').DockerTemplate | null} */
  let current = $state(null)
  let logs = $state(/** @type {string[]} */ ([]))
  let errorMsg = $state('')
  let logEl = $state(/** @type {HTMLDivElement | null} */ (null))
  let _unlisten = /** @type {(() => void) | null} */ (null)

  // Once per opening: "Other images" after a failure must not start it again.
  let autoStarted = false
  $effect(() => {
    if (!open) { reset(); return }
    const id = initialDbType
    untrack(() => {
      void (async () => {
        if (!templates.length) {
          try { templates = await dockerTemplates() } catch { templates = [] }
        }
        if (id && !autoStarted) { autoStarted = true; void launch(id) }
      })()
    })
  })

  function reset() {
    autoStarted = false
    phase = 'pick'
    current = null
    logs = []
    errorMsg = ''
    if (_unlisten) { _unlisten(); _unlisten = null }
  }

  /** @param {string} id */
  async function launch(id) {
    current = templates.find((t) => t.id === id) ?? { id, label: id, engine: id, icon: id, group: '', image: '', note: '', port: 0 }
    phase = 'running'
    logs = []
    errorMsg = ''
    const eventId = crypto.randomUUID()
    try {
      _unlisten = await listen(`docker-log:${eventId}`, (event) => {
        const payload = /** @type {{ line: string }} */ (event.payload)
        logs = [...logs.slice(-400), payload.line]
        tick().then(() => { if (logEl) logEl.scrollTop = logEl.scrollHeight })
      })
      await dockerCheck()
      const conn = await dockerRunDb(id, eventId)
      open = false
      onconnect(conn)
    } catch (err) {
      errorMsg = String(err).replace(/^Error:\s*/, '')
      phase = 'error'
    } finally {
      if (_unlisten) { _unlisten(); _unlisten = null }
    }
  }

  onDestroy(() => { if (_unlisten) _unlisten() })

  const groups = $derived.by(() => {
    /** @type {Map<string, import('$lib/api.js').DockerTemplate[]>} */
    const m = new Map()
    for (const t of templates) m.set(t.group, [...(m.get(t.group) ?? []), t])
    return [...m]
  })
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title class="flex items-center gap-2">
        {#if current}
          <DbIcon id={current.icon} class="size-4 shrink-0" />{current.label}
        {:else}
          <DbIcon id="docker" class="size-4 shrink-0" />Start a database in Docker
        {/if}
      </Dialog.Title>
      <Dialog.Description>
        {#if current?.image}<span class="font-mono">{current.image}</span>{:else}Pulls the image, starts it, and connects when it is ready.{/if}
      </Dialog.Description>
    </Dialog.Header>

    {#if phase === 'pick'}
      <div class="flex max-h-[26rem] flex-col gap-3 overflow-y-auto py-1">
        {#each groups as [group, items] (group)}
          <section class="flex flex-col gap-0.5">
            <h3 class="px-2 pb-1 text-ui-2xs font-medium text-muted-foreground">{group}</h3>
            {#each items as t (t.id)}
              <button
                type="button"
                class="flex h-9 w-full min-w-0 items-center gap-2.5 rounded-md px-2 text-left text-ui-sm hover:bg-accent"
                onclick={() => launch(t.id)}
              >
                <DbIcon id={t.icon} class="size-4 shrink-0 text-muted-foreground" />
                <span class="min-w-0 truncate">{t.label}</span>
                {#if t.note}<span class="min-w-0 truncate text-ui-2xs text-muted-foreground">{t.note}</span>{/if}
                <span class="ml-auto shrink-0 font-mono text-ui-2xs text-muted-foreground">{t.image}</span>
              </button>
            {/each}
          </section>
        {/each}
        {#if !templates.length}
          <p class="px-2 py-6 text-center text-ui-xs text-muted-foreground">Loading images…</p>
        {/if}
      </div>

    {:else if phase === 'running'}
      <div class="flex flex-col gap-3 py-1">
        <div class="flex items-center gap-2 text-ui-sm text-muted-foreground">
          <Icon name="loader-2" class="size-3.5 shrink-0 animate-spin" />
          <span class="min-w-0 truncate">{logs.at(-1) ?? 'Checking Docker…'}</span>
        </div>
        <div
          bind:this={logEl}
          class="h-52 overflow-y-auto rounded-md border border-border bg-muted/30 p-3 font-mono text-ui-2xs leading-relaxed text-muted-foreground"
        >
          {#each logs as line, i (i)}
            <div class="whitespace-pre-wrap break-all">{line}</div>
          {/each}
        </div>
        <p class="text-ui-2xs text-muted-foreground">The first start downloads the image; later starts take seconds.</p>
      </div>

    {:else if phase === 'error'}
      <div class="flex flex-col gap-3 py-1">
        <div class="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2.5 text-ui-sm text-destructive">
          <Icon name="alert-circle" class="mt-0.5 size-4 shrink-0" />
          <span class="leading-snug break-words">{errorMsg}</span>
        </div>
        <div class="flex justify-end gap-2">
          <Button variant="ghost" onclick={() => { current = null; phase = 'pick' }}>Other images</Button>
          {#if current}<Button onclick={() => current && launch(current.id)}>Try again</Button>{/if}
        </div>
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
