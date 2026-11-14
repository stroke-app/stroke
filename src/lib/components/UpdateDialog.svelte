<script>
  import { Button } from '$lib/components/ui/button/index.js'
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
  import { loadSettings, updateSettings } from '$lib/stores/settings.js'
  import ReleaseNotes from './ReleaseNotes.svelte'
  import Logo from './Logo.svelte'
  import { isRevealed }  from '$lib/app-reveal.js'
  import { parseChangelog, parseReleases, releasesBetween } from '$lib/changelog.js'
  import WhatsNewDialog  from './WhatsNewDialog.svelte'

  let {
    /** An update was found ('available'), or is downloaded and waiting ('ready'). */
    onupdatefound = /** @type {(state: 'available' | 'ready') => void} */ ((_state) => {}),
    /** A manual check is running: the sidebar's update button spins (T3: no dialog for checking). */
    onchecking = /** @type {(checking: boolean) => void} */ ((_c) => {}),
    /** A What's new highlight's button. @type {(id: import('$lib/whats-new.js').HighlightAction) => void} */
    onwhatsnewaction = () => {},
  } = $props()

  /** @type {'idle'|'available'|'downloading'|'ready'|'error'|'up-to-date'} */
  let status = $state('idle')
  let updateVersion = $state('')
  let releaseNotes = $state('')
  /** The downloaded build installs by itself when Stroke quits (updates.rs). */
  let applyOnQuit = $state(false)
  /** The running build, for "You have v…". */
  let appVersion = $state('')
  /** Settings → Download and install updates automatically, from this window too. */
  let autoUpdate = $state(loadSettings().autoUpdate)
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
    onchecking(true)
    // T3's pattern: checking is a spinner on the button, not a dialog; the
    // answer is a toast. Only a found update opens anything.
    let failure = /** @type {unknown} */ (null)
    try {
      const info = await updateCheck()
      if (info?.staged) markReady(info, false)
      else if (info) {
        adopt(info)
        status = 'available'
        onupdatefound('available')
      }
    } catch (e) {
      failure = e
    } finally {
      checking = false
      onchecking(false)
    }
    if (failure) {
      toast.error("Couldn't check for updates", {
        description: failure instanceof Error ? failure.message : String(failure),
        action: { label: 'Retry', onClick: () => void checkNow() },
      })
    } else if (status === 'idle') {
      let v = ''
      try { v = await (await import('@tauri-apps/api/app')).getVersion() } catch { /* dev */ }
      toast.success('Stroke is up to date', { description: v ? `You're on v${v}.` : undefined })
    }
  }

  const visible = $derived(
    !dismissed &&
    (status === 'available' ||
     status === 'downloading' ||
     status === 'ready' ||
     status === 'error'),
  )

  /** @param {number} bytes */
  function fmt(bytes) {
    if (bytes < 1024)      return `${bytes} B`
    if (bytes < 1024*1024) return `${(bytes/1024).toFixed(1)} KB`
    return `${(bytes/(1024*1024)).toFixed(1)} MB`
  }

  /**
   * The update's notes as releases: Nightly's feed carries CHANGELOG.md
   * sections for several versions (## [x.y.z]); a stable release carries one
   * version's sections without the heading.
   * @type {import('$lib/changelog.js').ChangelogRelease[]}
   */
  const changelog = $derived.by(() => {
    const many = parseReleases(releaseNotes)
    if (many.length) return many
    const sections = parseChangelog(releaseNotes)
    return sections.length ? [{ version: updateVersion, date: '', sections }] : []
  })

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
    import('@tauri-apps/api/app').then((m) => m.getVersion()).then((v) => (appVersion = v)).catch(() => {})
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
    /** @type {import('$lib/whats-new.js').Highlight[]} */
    highlights: [],
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
  /**
   * What's new on demand (the palette): the newest highlights and the notes
   * since the previous minor version, whenever it is asked for.
   */
  export async function showWhatsNew() {
    let current = ''
    try {
      const { getVersion } = await import('@tauri-apps/api/app')
      current = await getVersion()
    } catch {
      return
    }
    const base = current.replace(/-.*$/, '')
    const [maj, min] = base.split('.').map(Number)
    const previous = min > 0 ? `${maj}.${min - 1}.99` : `${Math.max(0, maj - 1)}.99.99`
    /** @type {{ releases: import('$lib/changelog.js').ChangelogRelease[], omitted: number }} */
    let notes = { releases: [], omitted: 0 }
    try {
      const { default: markdown } = await import('../../../CHANGELOG.md?raw')
      notes = releasesBetween(markdown, previous, base)
    } catch { /* the highlights still show */ }
    const { latestHighlights } = await import('$lib/whats-new.js')
    whatsNew = { previous: "", current, ...notes, highlights: latestHighlights(base) }
    whatsNewOpen = true
  }

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

    const { highlightsFor } = await import('$lib/whats-new.js')
    const highlights = highlightsFor(previous, current)

    if (!notes.releases.length && !highlights.length) {
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
    whatsNew = { previous, current, ...notes, highlights }
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
    // Automatic updates (the default) always download in the background, with
    // no window: where it can, the build installs as Stroke quits; elsewhere the
    // sidebar's pill offers Restart. With them off, the pill offers Update.
    // Neither interrupts what is open.
    if (loadSettings().autoUpdate) {
      await install(true)
      return
    }
    status = 'available'
    dismissed = true
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
  highlights={whatsNew.highlights}
  onaction={onwhatsnewaction}
  onfullchangelog={() => void openChangelog('whats-new')}
/>

<!-- ── Centered update dialog (sits above the connection modal) ───── -->
{#if visible}
  <!-- Sparkle's update window, in the app's own style: the mark and what is
       on offer, the notes right there, one choice, and the automatic setting
       beside it. -->
  <div
    class="fixed inset-0 z-[200] flex items-center justify-center p-4"
    role="dialog"
    aria-modal="true"
    aria-labelledby="update-title"
    tabindex="-1"
    use:focusTrap={{ enabled: true }}
  >
    <button
      type="button"
      tabindex="-1"
      aria-label="Dismiss"
      class="absolute inset-0 cursor-default bg-black/60"
      onclick={() => (dismissed = true)}
    ></button>

    <div class="pointer-events-auto relative flex max-h-[min(40rem,calc(100vh-2rem))] w-[min(34rem,calc(100vw-2rem))] flex-col overflow-hidden rounded-lg border border-border/60 bg-background elevate-3-rim">
      <div class="flex items-start gap-3.5 px-6 pt-6 pe-12">
        <Logo class="size-11 shrink-0" />
        <div class="min-w-0">
          <h2 id="update-title" class="text-ui-lg font-semibold text-balance text-foreground">
            {#if status === 'error'}
              {failedAction === 'install' ? 'The download did not finish' : failedAction === 'restart' ? "Couldn't install the update" : "Couldn't check for updates"}
            {:else if status === 'ready'}
              Stroke {updateVersion} is ready
            {:else if status === 'downloading'}
              Downloading Stroke {updateVersion}
            {:else}
              Stroke {updateVersion} is available
            {/if}
          </h2>
          <p class="mt-1 text-ui-xs text-pretty text-muted-foreground">
            {#if status === 'error'}
              <span class="break-words font-mono text-destructive">{errorMsg}</span>
            {:else if status === 'ready'}
              {applyOnQuit ? 'It installs when you quit Stroke. Restart now to use it straight away.' : 'Restart Stroke to finish installing it.'}
              {#if appVersion}<span class="tabular-nums"> You have v{appVersion}.</span>{/if}
            {:else if status === 'downloading'}
              {totalBytes > 0 ? `${fmt(downloadedBytes)} of ${fmt(totalBytes)}` : 'Starting…'} · it keeps going if you close this.
            {:else}
              {appVersion ? `You have v${appVersion}.` : 'A newer version of Stroke is out.'}
            {/if}
          </p>
        </div>
        <button
          type="button"
          onclick={() => (dismissed = true)}
          class="absolute top-4 right-4 inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          aria-label="Close"
        >
          <X class="size-4 shrink-0" />
        </button>
      </div>

      {#if status === 'downloading'}
        <div class="px-6 pt-4">
          <div class="h-1.5 w-full overflow-hidden rounded-full bg-muted" role="progressbar" aria-valuenow={progress} aria-valuemin="0" aria-valuemax="100" aria-label="Download progress">
            <div class="h-full rounded-full bg-primary transition-[width] duration-150" style="width:{progress}%"></div>
          </div>
        </div>
      {/if}

      {#if status !== 'error'}
        <div class="mt-5 min-h-0 flex-1 overflow-y-auto border-t border-border/40 px-6 py-4 [scrollbar-gutter:stable]">
          {#if changelog.length}
            <ReleaseNotes releases={changelog} compact />
          {:else}
            <p class="text-ui-sm text-muted-foreground">
              The notes for this version are on the release page.
              <button type="button" class="ms-1 inline-flex items-center gap-1 text-foreground underline-offset-4 hover:underline" onclick={() => void openChangelog()}>
                Read them<ExternalLink class="size-3 shrink-0" />
              </button>
            </p>
          {/if}
        </div>
      {/if}

      <div class={cn('flex flex-wrap items-center gap-3 border-t border-border/40 px-6 py-4', status === 'error' && 'mt-5')}>
        <label class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 text-ui-xs text-muted-foreground">
          <input
            type="checkbox"
            class="size-3.5 shrink-0 accent-[var(--primary)]"
            checked={autoUpdate}
            onchange={(e) => { autoUpdate = e.currentTarget.checked; updateSettings({ autoUpdate }) }}
          />
          <span class="min-w-0">Download and install updates automatically</span>
        </label>
        <div class="flex shrink-0 items-center gap-2">
          {#if status === 'error'}
            <Button variant="ghost" onclick={() => (dismissed = true)}>Not now</Button>
            <Button onclick={() => void retry()}><RefreshCw class="size-3.5 shrink-0" />Try again</Button>
          {:else if status === 'ready'}
            <Button variant="ghost" onclick={() => (dismissed = true)}>{applyOnQuit ? 'On quit' : 'Later'}</Button>
            <Button onclick={() => void restart()} disabled={restarting}>
              <RefreshCw class={cn('size-3.5 shrink-0', restarting && 'animate-spin')} />Restart now
            </Button>
          {:else if status === 'downloading'}
            <Button variant="ghost" onclick={() => (dismissed = true)}>Hide</Button>
          {:else}
            <Button variant="ghost" onclick={() => (dismissed = true)}>Later</Button>
            <Button onclick={() => void install()}><Download class="size-3.5 shrink-0" />Install</Button>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}
