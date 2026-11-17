<script>
  /**
   * "Check for updates" for the screens that stand in front of the app (sign
   * in, trial over). Those screens replace the whole shell, update dialog
   * included, so a build with a sign-in bug had no way to fetch its own fix.
   *
   * One quiet line in the footer: the version, and a button whose label is
   * the state. A found update installs and restarts from here. Every state
   * change is read out through one polite live region.
   */
  import { onMount } from 'svelte'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'
  import { updateCheck, updateDownload, updateRestart, updateStatus } from '$lib/api.js'

  /** @type {'idle'|'checking'|'current'|'available'|'downloading'|'ready'|'error'} */
  let state = $state('idle')
  let version = $state('')
  let next = $state('')
  let progress = $state(0)
  let error = $state('')

  onMount(() => {
    import('@tauri-apps/api/app').then((m) => m.getVersion()).then((v) => (version = v)).catch(() => {})
    // A download from an earlier run is still waiting: offer the restart.
    updateStatus().then((info) => { if (info) { next = info.version; state = 'ready' } }).catch(() => {})
  })

  /** "2.17.3" from "2.17.3-nightly.202611170418". @param {string} v */
  const short = (v) => v.replace(/-nightly\.\d+$/, '')

  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let settle
  async function check() {
    if (state === 'checking' || state === 'downloading') return
    clearTimeout(settle)
    state = 'checking'
    error = ''
    try {
      const info = await updateCheck()
      if (info?.staged) { next = info.version; state = 'ready' }
      else if (info) { next = info.version; state = 'available' }
      else {
        state = 'current'
        settle = setTimeout(() => { if (state === 'current') state = 'idle' }, 6000)
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
      state = 'error'
    }
  }

  async function install() {
    state = 'downloading'
    progress = 0
    let total = 0
    try {
      await updateDownload((event) => {
        if (event.event === 'started') total = event.data.contentLength ?? 0
        else if (event.event === 'progress' && total > 0) progress = Math.round((event.data.downloaded / total) * 100)
        else if (event.event === 'finished') progress = 100
      })
      state = 'ready'
      await restart()
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
      state = 'error'
    }
  }

  async function restart() {
    try {
      await updateRestart()
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
      state = 'error'
    }
  }

  /** What the live region reads out. */
  const announce = $derived(
    state === 'checking' ? 'Checking for updates'
    : state === 'current' ? `Stroke is up to date${version ? `, version ${short(version)}` : ''}`
    : state === 'available' ? `Stroke ${short(next)} is available`
    : state === 'downloading' ? `Downloading Stroke ${short(next)}`
    : state === 'ready' ? `Stroke ${short(next)} is ready to install`
    : state === 'error' ? `Couldn't update: ${error}`
    : '',
  )

  const link = 'inline-flex h-7 items-center gap-1.5 rounded-md px-1.5 transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:pointer-events-none'
</script>

<div class="flex items-center gap-1">
  {#if version && (state === 'idle' || state === 'checking' || state === 'current')}
    <span class="tabular-nums">v{short(version)}</span>
    <span aria-hidden="true" class="px-1">·</span>
  {/if}

  {#if state === 'idle'}
    <button type="button" class={link} onclick={check}>Check for updates</button>
  {:else if state === 'checking'}
    <button type="button" class={link} disabled aria-disabled="true">
      <Icon name="loader-2" class="size-3 shrink-0 animate-spin" />Checking
    </button>
  {:else if state === 'current'}
    <button type="button" class={link} onclick={check}>
      <Icon name="check" class="size-3 shrink-0 text-success" />Up to date
    </button>
  {:else if state === 'available'}
    <span class="text-foreground">Stroke {short(next)} is available</span>
    <button type="button" class={cn(link, 'font-medium text-foreground underline-offset-4 hover:underline')} onclick={install}>Update and restart</button>
  {:else if state === 'downloading'}
    <span class="inline-flex h-7 items-center gap-2">
      Downloading {short(next)}
      <span
        role="progressbar"
        aria-label="Download progress"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={progress}
        class="relative h-1 w-20 overflow-hidden rounded-full bg-muted"
      >
        <span class="absolute inset-y-0 left-0 rounded-full bg-foreground/70 transition-[width] duration-200" style:width="{progress}%"></span>
      </span>
      <span class="tabular-nums">{progress}%</span>
    </span>
  {:else if state === 'ready'}
    <span class="text-foreground">Stroke {short(next)} is ready</span>
    <button type="button" class={cn(link, 'font-medium text-foreground underline-offset-4 hover:underline')} onclick={restart}>Restart to install</button>
  {:else if state === 'error'}
    <span class="inline-flex items-center gap-1.5" title={error}>
      <Icon name="alert-circle" class="size-3 shrink-0 text-destructive" />Couldn't update
    </span>
    <button type="button" class={link} onclick={check}>Retry</button>
  {/if}

  <span class="sr-only" role="status" aria-live="polite">{announce}</span>
</div>
