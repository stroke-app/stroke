<script>
  import { onMount }     from 'svelte'
  import Download        from '@lucide/svelte/icons/download'
  import RefreshCw       from '@lucide/svelte/icons/refresh-cw'
  import X               from '@lucide/svelte/icons/x'
  import CheckCircle2    from '@lucide/svelte/icons/check-circle-2'
  import AlertCircle     from '@lucide/svelte/icons/alert-circle'
  import Loader2         from '@lucide/svelte/icons/loader-2'
  import Sparkles        from '@lucide/svelte/icons/sparkles'
  import { toast }       from '$lib/components/ui/sonner/toast.svelte.js'
  import ScrollText      from '@lucide/svelte/icons/scroll-text'
  import ExternalLink    from '@lucide/svelte/icons/external-link'
  import { cn }          from '$lib/utils.js'
  import { focusTrap } from '$lib/actions/focus-trap.js'
  import { updateCheck, updateDownload, updateStatus, updateRestart } from '$lib/api.js'
  import { loadSettings } from '$lib/stores/settings.js'
  import { isRevealed }  from '$lib/app-reveal.js'
  import { parseChangelog, releasesBetween } from '$lib/changelog.js'
  import WhatsNewDialog  from './WhatsNewDialog.svelte'

  let {
    /** An update was found ('available'), or is downloaded and waiting ('ready'). */
    onupdatefound = /** @type {(state: 'available' | 'ready') => void} */ ((_state) => {}),
  } = $props()

  /** @type {'idle'|'available'|'downloading'|'ready'|'error'|'up-to-date'} */
  let status = $state('idle')
  let updateVersion = $state('')
  let releaseNotes = $state('')
  /** The downloaded build installs by itself when Stroke quits (updates.rs). */
  let applyOnQuit = $state(false)
  let progress = $state(0)
  let downloadedBytes = $state(0)
  let totalBytes = $state(0)
  let errorMsg = $state('')
  let dismissed = $state(false)
  let checking = $state(false)
  let restarting = $state(false)
  /** Which operation produced `errorMsg`, so Retry repeats that one. */
  let failedAction = $state(/** @type {'check'|'install'|'restart'} */ ('check'))

  /** First background check, once startup has settled. */
  const FIRST_CHECK_MS = 5_000
  /** Stroke often stays open for days, so it looks again this often. */
  const RECHECK_MS = 6 * 60 * 60 * 1000

  /** Manually trigger an update check (e.g. from the command palette). */
  export async function checkNow() {
    dismissed = false
    // Already found, downloading or downloaded: show that instead of asking the
    // feed again. A background download shows its progress from here.
    if (checking || status === 'downloading' || status === 'available' || status === 'ready') return
    // Drop the previous outcome: re-checking after a failure showed "Update
    // failed" in the header while the new check was still running.
    status = 'idle'
    errorMsg = ''
    checking = true
    await checkForUpdate(true)
    if (status === 'idle') status = 'up-to-date'
    checking = false
  }

  const visible = $derived(
    !dismissed &&
    (status === 'available' ||
     status === 'downloading' ||
     status === 'ready' ||
     status === 'error' ||
     status === 'up-to-date' ||
     checking),
  )

  /** @param {number} bytes */
  function fmt(bytes) {
    if (bytes < 1024)      return `${bytes} B`
    if (bytes < 1024*1024) return `${(bytes/1024).toFixed(1)} KB`
    return `${(bytes/(1024*1024)).toFixed(1)} MB`
  }

  const changelog = $derived(parseChangelog(releaseNotes))

  // Actionable states are true modals (dark backdrop, blocks the app);
  // passive states (checking / up-to-date) float without blocking.
  const isModal = $derived(
    status === 'available' ||
    status === 'downloading' ||
    status === 'ready' ||
    status === 'error',
  )

  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let announceTimer

  onMount(() => {
    // Runs in dev too: it is a version comparison, not an update check, and a
    // dev build that has genuinely changed version should say so.
    void announceNewVersion()
    // A webview reload forgets the download; the Rust side still has it.
    updateStatus()
      .then((info) => { if (info) markReady(info, true) })
      .catch(() => { /* browser dev: no backend */ })
    if (import.meta.env.DEV) return () => clearTimeout(announceTimer)
    const first = setTimeout(() => void backgroundCheck(), FIRST_CHECK_MS)
    const again = setInterval(() => void backgroundCheck(), RECHECK_MS)
    return () => {
      clearTimeout(first)
      clearInterval(again)
      clearTimeout(announceTimer)
    }
  })

  /** The build this app last ran as, so a new one can be recognised. */
  const LAST_RUN_VERSION_KEY = 'stroke:last-run-version'

  let whatsNewOpen = $state(false)
  let whatsNew = $state({
    previous: '',
    current: '',
    /** @type {import('$lib/changelog.js').ChangelogRelease[]} */
    releases: [],
    omitted: 0,
  })

  /**
   * First launch on a new build: show what changed since the version this
   * install last ran as. The update itself happened out of sight (on quit, or
   * through the restart the dialog offered), so without this the release notes
   * are never seen at all.
   *
   * The notes come from the CHANGELOG.md bundled with this build, so they are
   * there offline and cover every version skipped on the way.
   *
   * Only ever announces a CHANGE. A fresh install has nothing stored, and
   * "updated to v1.24" on a first run is a lie about something the user did not
   * do; the version is recorded and the dialog waits for the next upgrade.
   */
  async function announceNewVersion() {
    let current = ''
    try {
      const { getVersion } = await import('@tauri-apps/api/app')
      current = await getVersion()
    } catch {
      return // not running under Tauri - there is no app version to compare
    }
    if (!current) return

    let previous = null
    try {
      previous = localStorage.getItem(LAST_RUN_VERSION_KEY)
      localStorage.setItem(LAST_RUN_VERSION_KEY, current)
    } catch {
      return // storage unavailable: without a record this would fire every launch
    }
    if (!previous || previous === current) return

    /** @type {{ releases: import('$lib/changelog.js').ChangelogRelease[], omitted: number }} */
    let notes = { releases: [], omitted: 0 }
    try {
      const { default: markdown } = await import('../../../CHANGELOG.md?raw')
      notes = releasesBetween(markdown, previous, current)
    } catch (e) {
      console.error('[updater] changelog unavailable:', e)
    }

    if (!notes.releases.length) {
      // A downgrade, or a build whose notes are missing: just say what happened.
      toast.success(`Updated to v${current}`, {
        description: `You were on v${previous}.`,
        // Longer than the 4.5s default: the toast carries an action, and an action
        // that times out before it is read is decoration.
        duration: 12000,
        action: { label: "What's new", onClick: () => void openChangelog('update-toast') },
      })
      return
    }
    whatsNew = { previous, current, ...notes }
    // After the window is on screen and the startup screen has settled, so the
    // dialog is not the first thing that flashes past.
    const wait = () => {
      announceTimer = isRevealed() ? setTimeout(() => (whatsNewOpen = true), 1500) : setTimeout(wait, 250)
    }
    wait()
  }

  /**
   * Record a failure and remember what caused it, so Retry repeats that step
   * rather than always falling back to a fresh check.
   * @param {'check'|'install'|'restart'} action
   * @param {unknown} e
   */
  function fail(action, e) {
    failedAction = action
    errorMsg = e instanceof Error ? e.message : String(e)
    status = 'error'
  }

  /** Re-run whatever failed: the download or install if we have an update, else the check. */
  async function retry() {
    if (failedAction === 'install') return install()
    if (failedAction === 'restart') return restart()
    await checkNow()
  }

  /** @param {import('$lib/api.js').UpdateInfo} info */
  function adopt(info) {
    updateVersion = info.version
    releaseNotes = info.notes ?? ''
    applyOnQuit = info.applyOnQuit
  }

  /**
   * @param {import('$lib/api.js').UpdateInfo} info
   * @param {boolean} quiet leave the dialog closed; the status bar says it
   */
  function markReady(info, quiet) {
    adopt(info)
    status = 'ready'
    if (quiet) dismissed = true
    onupdatefound('ready')
  }

  /** @param {boolean} manual a failed check only shows when someone asked */
  async function checkForUpdate(manual) {
    try {
      const info = await updateCheck()
      if (!info) {
        console.info('[updater] no update available')
        return
      }
      console.info('[updater] update available:', info.version)
      if (info.staged) {
        markReady(info, false)
        return
      }
      adopt(info)
      status = 'available'
      onupdatefound('available')
    } catch (e) {
      console.error('[updater] check failed:', e)
      if (manual) fail('check', e)
    }
  }

  /**
   * The check on a timer. With background updates on, and where this build can
   * install itself on quit, a new version downloads with no dialog at all and
   * the status bar offers the restart. Otherwise it offers the update the way
   * it always has.
   */
  async function backgroundCheck() {
    if (checking || status === 'available' || status === 'downloading' || status === 'ready') return
    if (status === 'error' && !dismissed) return
    let info
    try {
      info = await updateCheck()
    } catch (e) {
      console.error('[updater] check failed:', e)
      return
    }
    if (!info) return
    if (info.staged) {
      markReady(info, true)
      return
    }
    adopt(info)
    if (loadSettings().autoUpdate && info.applyOnQuit) {
      await install(true)
      return
    }
    status = 'available'
    dismissed = false
    onupdatefound('available')
  }

  /** @param {boolean} [quiet] a background download: no dialog unless opened */
  async function install(quiet = false) {
    dismissed = quiet
    status = 'downloading'
    progress = 0
    downloadedBytes = 0
    totalBytes = 0
    try {
      // Rust throttles progress to ten events a second, so each one can go
      // straight to state.
      const info = await updateDownload((event) => {
        if (event.event === 'started') {
          totalBytes = event.data.contentLength ?? 0
        } else if (event.event === 'progress') {
          downloadedBytes = event.data.downloaded
          if (totalBytes > 0) progress = Math.round((downloadedBytes / totalBytes) * 100)
        } else if (event.event === 'finished') {
          if (totalBytes > 0) downloadedBytes = totalBytes
          progress = 100
        }
      })
      // Opened from the status bar mid-download: show the result there.
      markReady(info, dismissed)
    } catch (e) {
      console.error('[updater] download failed:', e)
      if (quiet && dismissed) {
        // Nobody is watching. Offer it the ordinary way instead.
        status = 'available'
        onupdatefound('available')
        return
      }
      fail('install', e)
    }
  }

  async function restart() {
    if (restarting) return
    restarting = true
    try {
      await updateRestart()
    } catch (e) {
      console.error('[updater] install failed:', e)
      fail('restart', e)
    } finally {
      restarting = false
    }
  }

  // Tagged so web analytics can attribute changelog views to the desktop app,
  // and to the surface they came from - the dialog and the post-update toast are
  // different moments and read differently in the numbers.
  const changelogUrl = (/** @type {string} */ medium) =>
    `https://stroke.click/changelog?utm_source=stroke-app&utm_medium=${medium}&utm_campaign=changelog`

  /**
   * Open the online changelog in the user's browser (never the in-app tab).
   * @param {string} [medium] where the click came from
   */
  async function openChangelog(medium = 'update-dialog') {
    const url = changelogUrl(medium)
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl(url)
    } catch {
      window.open(url, '_blank', 'noopener,noreferrer')
    }
  }
</script>

<WhatsNewDialog
  bind:open={whatsNewOpen}
  current={whatsNew.current}
  previous={whatsNew.previous}
  releases={whatsNew.releases}
  omitted={whatsNew.omitted}
  onfullchangelog={() => void openChangelog('whats-new')}
/>

<!-- ── Centered update dialog (sits above the connection modal) ───── -->
{#if visible}
  <!-- Actionable states get a dark backdrop; passive states float without blocking. -->
  <div
    class={cn(
      'fixed inset-0 z-[200] flex items-center justify-center p-4',
      !isModal && 'pointer-events-none',
    )}
    role="dialog"
    aria-modal={isModal}
    aria-label="Application update"
    tabindex="-1"
    use:focusTrap={{ enabled: isModal }}
  >
    {#if isModal}
      <button
        type="button"
        tabindex="-1"
        aria-label="Dismiss"
        class="absolute inset-0 cursor-default bg-black/65"
        onclick={() => (dismissed = true)}
      ></button>
    {/if}

    <div
      class="pointer-events-auto relative w-[min(26.25rem,calc(100vw-2rem))] overflow-hidden rounded-2xl border border-border/60 bg-background elevate-3-rim"
    >
      <!-- header row -->
      <div class="flex items-center gap-3 border-b border-border/40 px-5 py-3.5">
        <div class="shrink-0 rounded-lg bg-muted/50 p-2">
          {#if status === 'ready' || status === 'up-to-date'}
            <CheckCircle2 class="size-4 text-success" />
          {:else if status === 'error'}
            <AlertCircle class="size-4 text-destructive" />
          {:else if checking}
            <Loader2 class="size-4 animate-spin text-primary" />
          {:else if status === 'available'}
            <Sparkles class="size-4 text-primary" />
          {:else}
            <Download class="size-4 text-primary" />
          {/if}
        </div>

        <span class="flex-1 whitespace-nowrap text-ui font-semibold text-foreground">
          {#if status === 'error'}
            {failedAction === 'install' ? 'Download failed'
              : failedAction === 'restart' ? "Couldn't install the update"
              : "Couldn't check for updates"}
          {:else if status === 'available'}
            Stroke {updateVersion} available
          {:else if status === 'downloading'}
            Downloading update…
          {:else if status === 'ready'}
            Ready to install
          {:else if status === 'up-to-date'}
            Up to date
          {:else if checking}
            Checking for updates…
          {/if}
        </span>

        <!-- Closable while downloading too: the download carries on, and the
             status bar picks it up when it is done. -->
        <button
          type="button"
          onclick={() => (dismissed = true)}
          class="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          aria-label="Dismiss"
        >
          <X class="size-4" />
        </button>
      </div>

      <!-- body -->
      <div class="px-5 py-4 text-ui-sm">

        {#if status === 'available'}
          <p class="mb-4 text-ui-sm text-muted-foreground">A new version is ready to install.</p>
          <div class="flex gap-2.5">
            <button
              type="button"
              onclick={() => void openChangelog()}
              class= "field-surface inline-flex h-9 flex-1 items-center justify-center gap-1.5 whitespace-nowrap px-3 text-ui-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            >
              <ScrollText class="size-3.5 shrink-0" />
              Release Notes
            </button>
            <button
              type="button"
              onclick={() => void install()}
              class="inline-flex h-9 flex-1 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg bg-primary px-3 text-ui-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
            >
              <Download class="size-3.5 shrink-0" />
              Install
            </button>
          </div>

        {:else if status === 'downloading'}
          <div class="flex flex-col gap-2.5">
            <div class="flex items-center justify-between text-ui-xs text-muted-foreground">
              <span>{totalBytes > 0 ? `${fmt(downloadedBytes)} / ${fmt(totalBytes)}` : 'Downloading…'}</span>
              <span class="font-mono tabular-nums">{progress}%</span>
            </div>
            <div class="h-1.5 w-full overflow-hidden rounded-full bg-muted">
              <div
                class="h-full rounded-full bg-primary transition-all duration-150"
                style="width:{progress}%"
              ></div>
            </div>
            <p class="text-ui-xs text-muted-foreground">
              Stroke <span class="font-mono font-medium text-foreground">{updateVersion}</span>.
              It keeps downloading if you close this.
            </p>
          </div>

        {:else if status === 'ready'}
          {#if applyOnQuit}
            <p class="mb-4 text-ui-sm text-muted-foreground">
              Stroke <span class="font-mono font-medium text-foreground">{updateVersion}</span> is ready. It installs when you quit.
            </p>
            <div class="flex gap-2.5">
              <button
                type="button"
                onclick={() => (dismissed = true)}
                class= "field-surface inline-flex h-9 flex-1 items-center justify-center gap-1.5 whitespace-nowrap px-3 text-ui-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              >
                Later
              </button>
              <button
                type="button"
                onclick={() => void restart()}
                disabled={restarting}
                class="inline-flex h-9 flex-1 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg bg-primary px-3 text-ui-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:pointer-events-none disabled:opacity-50"
              >
                <RefreshCw class={cn('size-3.5 shrink-0', restarting && 'animate-spin')} />
                Restart now
              </button>
            </div>
          {:else}
            <p class="mb-4 text-ui-sm text-muted-foreground">
              Version <span class="font-mono font-medium text-foreground">{updateVersion}</span> downloaded. Restart to apply.
            </p>
            <div class="flex gap-2.5">
              {#if changelog.length > 0}
                <button
                  type="button"
                  onclick={() => void openChangelog()}
                  class= "field-surface inline-flex h-9 flex-1 items-center justify-center gap-1.5 whitespace-nowrap px-3 text-ui-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                >
                  <ScrollText class="size-3.5 shrink-0" />
                  What's New
                </button>
              {/if}
              <button
                type="button"
                onclick={() => void restart()}
                disabled={restarting}
                class="inline-flex h-9 flex-1 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg bg-primary px-3 text-ui-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:pointer-events-none disabled:opacity-50"
              >
                <RefreshCw class={cn('size-3.5 shrink-0', restarting && 'animate-spin')} />
                Restart now
              </button>
            </div>
          {/if}

        {:else if status === 'error'}
          <p class="mb-2.5 text-ui-sm text-muted-foreground">
            {failedAction === 'install' ? 'The download did not finish.'
              : failedAction === 'restart' ? 'The update could not be installed. You can try again or keep working.'
              : 'Stroke could not reach the update server.'}
          </p>
          <!-- break-words: the endpoint URL is one long token and ran past the
               dialog edge without it. -->
          <p class="mb-4 font-mono text-ui-xs break-words text-destructive">{errorMsg}</p>
          <div class="flex gap-2.5">
            <button
              type="button"
              onclick={() => (dismissed = true)}
              class="field-surface inline-flex h-9 flex-1 items-center justify-center gap-1.5 whitespace-nowrap px-3 text-ui-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            >
              Not now
            </button>
            <button
              type="button"
              onclick={() => void retry()}
              class="inline-flex h-9 flex-1 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg bg-primary px-3 text-ui-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
            >
              <RefreshCw class="size-3.5 shrink-0" />
              Retry
            </button>
          </div>

        {:else if status === 'up-to-date' || checking}
          <p class="text-ui-sm leading-relaxed text-muted-foreground">
            {checking ? 'Checking GitHub for a newer release…' : "You're on the latest version."}
          </p>
          {#if !checking}
            <button
              type="button"
              onclick={() => void openChangelog()}
              class= "field-surface mt-4 inline-flex h-9 items-center gap-1.5 px-3 text-ui-sm text-muted-foreground transition-[color,background-color,transform] duration-150 ease-out hover:bg-muted hover:text-foreground active:scale-[0.97]"
            >
              <ScrollText class="size-3.5 shrink-0" />
              View changelog
              <ExternalLink class="ml-0.5 size-3.5 shrink-0 text-muted-foreground" />
            </button>
          {/if}
        {/if}

      </div>
    </div>
  </div>
{/if}
