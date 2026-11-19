<script>
  /**
   * Starting a database in Docker: the image chosen in the ⌘K Docker page
   * starts as this opens, its pull and start-up stream in, and when it takes
   * connections the app connects to it. Opened without a choice it lists the
   * catalog (docker_templates).
   */
  import { Button } from '$lib/components/ui/button/index.js'
  import { tick, untrack } from 'svelte'
  import { get } from 'svelte/store'
  import { dockerLaunches, launchDocker, backgroundLaunch } from '$lib/docker-launch.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import Icon from './Icon.svelte'
  import DbIcon from './DbIcon.svelte'
  import { dockerTemplates } from '$lib/api.js'
  import { cn } from '$lib/utils.js'

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
  let errorMsg = $state('')
  let logEl = $state(/** @type {HTMLDivElement | null} */ (null))
  let showLog = $state(false)
  /** The launch this dialog is showing (docker-launch.js runs it). */
  let launchKey = $state('')
  const launchState = $derived($dockerLaunches.find((l) => l.key === launchKey) ?? null)

  // Once per opening: "Other images" after a failure must not start it again.
  let autoStarted = false
  $effect(() => {
    if (!open) {
      // Closed while it runs: it carries on, and a toast takes over.
      untrack(() => {
        if (launchKey && phase === 'running') backgroundLaunch(launchKey, onconnect)
        reset()
      })
      return
    }
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
    errorMsg = ''
    launchKey = ''
    showLog = false
  }

  /** @param {string} id */
  async function launch(id) {
    const t = templates.find((x) => x.id === id) ?? { id, label: id, engine: id, icon: id, group: '', image: '', note: '', port: 0 }
    current = t
    phase = 'running'
    errorMsg = ''
    const run = launchDocker(t, {
      inDialog: () => open,
      onconnect: (conn) => { phase = 'pick'; open = false; onconnect(conn) },
    })
    launchKey = get(dockerLaunches)[0]?.key ?? ''
    try {
      await run
    } catch (err) {
      if (open) {
        errorMsg = String(/** @type {any} */ (err)?.message ?? err)
        phase = 'error'
      }
    }
  }

  // The log follows its newest line while it is open.
  $effect(() => {
    void launchState?.logs.length
    if (showLog) tick().then(() => { if (logEl) logEl.scrollTop = logEl.scrollHeight })
  })

  const STEPS = [
    { id: 'pull', label: 'Pull the image', on: ['checking', 'pulling'] },
    { id: 'start', label: 'Start the container', on: ['starting'] },
    { id: 'ready', label: 'Accept connections', on: ['waiting'] },
  ]
  /** 0, 1, 2 for the step in progress; 3 when all are done. */
  const stepAt = $derived(
    !launchState ? 0
    : launchState.phase === 'ready' ? 3
    : STEPS.findIndex((st) => st.on.includes(launchState.phase)),
  )

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
      {@const pct = launchState?.progress ?? -1}
      <div class="flex flex-col gap-4 pt-1">
        <!-- The whole run as one bar: the pull is most of it, then start-up. -->
        <div class="flex flex-col gap-1.5">
          <div class="flex items-baseline justify-between gap-3 text-ui-xs">
            <span class="min-w-0 truncate text-foreground">{launchState?.line ?? 'Checking Docker'}</span>
            <span class="shrink-0 tabular-nums text-muted-foreground">
              {#if launchState?.phase === 'pulling' && launchState.layers.total}{launchState.layers.done}/{launchState.layers.total} layers{:else if pct >= 0}{Math.round(pct * 100)}%{/if}
            </span>
          </div>
          <div class="h-1.5 w-full overflow-hidden rounded-full bg-muted" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={pct >= 0 ? Math.round(pct * 100) : undefined}>
            {#if pct >= 0}
              <div class="h-full rounded-full bg-primary transition-[width] duration-300 ease-out" style="width: {Math.max(3, pct * 100)}%"></div>
            {:else}
              <div class="toast-indeterminate h-full w-1/3 rounded-full bg-primary"></div>
            {/if}
          </div>
        </div>

        <ol class="flex flex-col gap-2">
          {#each STEPS as st, i (st.id)}
            <li class={cn('flex items-center gap-2.5 text-ui-sm', i < stepAt ? 'text-foreground' : i === stepAt ? 'text-foreground' : 'text-muted-foreground')}>
              <span class="flex size-4 shrink-0 items-center justify-center">
                {#if i < stepAt}
                  <Icon name="check" class="size-3.5 shrink-0 text-success" />
                {:else if i === stepAt}
                  <Icon name="loader-2" class="size-3.5 shrink-0 animate-spin text-primary" />
                {:else}
                  <span class="size-1.5 rounded-full bg-muted-foreground/40"></span>
                {/if}
              </span>
              {st.label}
            </li>
          {/each}
        </ol>

        <div>
          <button type="button" class="inline-flex items-center gap-1 text-ui-xs text-muted-foreground hover:text-foreground" onclick={() => (showLog = !showLog)} aria-expanded={showLog}>
            <Icon name="chevron-right" class={cn('size-3 shrink-0 transition-transform duration-150', showLog && 'rotate-90')} />
            {showLog ? 'Hide' : 'Show'} Docker's output
          </button>
          {#if showLog}
            <div bind:this={logEl} class="app-scroll mt-2 h-40 overflow-y-auto rounded-lg bg-muted/40 p-3 font-mono text-ui-2xs leading-relaxed text-muted-foreground">
              {#each launchState?.logs ?? [] as line, i (i)}
                <div class="whitespace-pre-wrap break-all">{line}</div>
              {/each}
            </div>
          {/if}
        </div>

        <div class="flex items-center justify-between gap-3 border-t border-border/40 pt-3">
          <p class="text-ui-2xs text-muted-foreground">The first start downloads the image; later starts take seconds.</p>
          <Button variant="outline" size="sm" class="shrink-0" onclick={() => (open = false)}>Run in background</Button>
        </div>
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
