<script>
  // The way out of a broken build: the crash screen checks for a newer one by
  // itself and installs it in place. It leans on nothing but the updater
  // commands, so it still works when the rest of the interface can't render.
  import Download from '@lucide/svelte/icons/download'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import { onMount } from 'svelte'
  import { updateCheck, updateDownload, updateRestart } from '$lib/api.js'

  /** @type {'checking' | 'current' | 'available' | 'downloading' | 'installing' | 'failed'} */
  let phase = $state('checking')
  let version = $state('')
  let current = $state('')
  let progress = $state(/** @type {number | null} */ (null))
  let problem = $state('')

  async function check() {
    phase = 'checking'
    problem = ''
    try {
      const info = await updateCheck()
      if (info) {
        version = info.version
        current = info.currentVersion
        phase = 'available'
      } else {
        phase = 'current'
      }
    } catch (e) {
      problem = String(/** @type {any} */ (e)?.message ?? e)
      phase = 'failed'
    }
  }

  async function install() {
    phase = 'downloading'
    progress = null
    let total = 0
    let got = 0
    try {
      await updateDownload((ev) => {
        if (ev.event === 'started') total = ev.data.contentLength ?? 0
        else if (ev.event === 'progress') {
          got += ev.data.downloaded
          progress = total ? Math.min(100, Math.round((got / total) * 100)) : null
        }
      })
      phase = 'installing'
      await updateRestart()
    } catch (e) {
      problem = String(/** @type {any} */ (e)?.message ?? e)
      phase = 'failed'
    }
  }

  async function openDownloads() {
    const url = 'https://stroke.click/download'
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl(url)
    } catch {
      window.open(url, '_blank')
    }
  }

  onMount(() => {
    void check()
  })
</script>

<div class="flex w-full max-w-lg items-center gap-3 rounded-lg border border-border/60 bg-muted/20 px-3 py-2.5 text-left" aria-live="polite">
  <div class="min-w-0 flex-1 text-ui-xs">
    {#if phase === 'checking'}
      <span class="text-muted-foreground">Checking for a newer version</span>
    {:else if phase === 'current'}
      <span class="text-muted-foreground">No newer version yet. A fix may already be on the download page.</span>
    {:else if phase === 'available'}
      <span class="font-medium text-foreground">Stroke {version} is available</span>
      {#if current}<span class="text-muted-foreground"> (this is {current})</span>{/if}
    {:else if phase === 'downloading'}
      <span class="text-foreground">Downloading {version}{progress != null ? ` · ${progress}%` : ''}</span>
    {:else if phase === 'installing'}
      <span class="text-foreground">Installing and restarting</span>
    {:else}
      <span class="text-muted-foreground">Couldn't update from here{problem ? `: ${problem}` : ''}</span>
    {/if}
  </div>

  {#if phase === 'available'}
    <button
      type="button"
      onclick={install}
      class="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md bg-primary px-2.5 text-ui-xs font-medium text-primary-foreground transition-[opacity,scale] duration-[120ms] hover:opacity-90 active:scale-[0.97]"
    >
      <Download class="size-3.5 shrink-0" /> Update and restart
    </button>
  {:else if phase === 'checking' || phase === 'downloading' || phase === 'installing'}
    <LoaderCircle class="size-3.5 shrink-0 animate-spin text-muted-foreground" aria-hidden="true" />
  {:else}
    {#if phase === 'failed'}
      <button
        type="button"
        onclick={check}
        class="inline-flex h-7 shrink-0 items-center rounded-md px-2 text-ui-xs text-muted-foreground transition-[color,scale] duration-[120ms] hover:text-foreground active:scale-[0.97]"
      >Try again</button>
    {/if}
    <button
      type="button"
      onclick={openDownloads}
      class="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-[color,scale] duration-[120ms] hover:text-foreground active:scale-[0.97]"
    >
      Download page <ExternalLink class="size-3.5 shrink-0" />
    </button>
  {/if}
</div>
