<script>
  // Scheduled backups for the open database (Pro). One line of header, one
  // row per schedule, and a compact dialog to add or edit one. The schedule
  // and history live on stroke.click; this device runs the jobs for
  // connections saved on it (see $lib/stores/scheduled-backups.js).
  import { onMount } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import LockKeyhole from '@lucide/svelte/icons/lock-keyhole'
  import { Cron } from 'croner'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import SegmentedControl from './SegmentedControl.svelte'
  import { isSignedIn, openManageAccount, signInWithToast } from '$lib/stores/account.js'
  import EmptyState from './EmptyState.svelte'
  import { hasPaidPro, isTrialActive } from '$lib/stores/license.js'
  import {
    backupOverview, backupLoadError, runningJobId, refreshBackupJobs, saveBackupJob, patchBackupJob,
    deleteBackupJob, saveS3Keys, s3KeysRegion, describeCron, formatBytes, SCHEDULE_DRIVERS,
  } from '$lib/stores/scheduled-backups.js'
  import { cn, errorText } from '$lib/utils.js'
  import CalendarClock from '@lucide/svelte/icons/calendar-clock'
  import Cloud from '@lucide/svelte/icons/cloud'
  import HardDrive from '@lucide/svelte/icons/hard-drive'
  import Play from '@lucide/svelte/icons/play'
  import Pause from '@lucide/svelte/icons/pause'
  import Pencil from '@lucide/svelte/icons/pencil'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Plus from '@lucide/svelte/icons/plus'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import Info from '@lucide/svelte/icons/info'

  /** @typedef {import('$lib/stores/scheduled-backups.js').BackupJob} BackupJob */

  let {
    /** The saved connection behind the open database, or null when it isn't saved. */
    connection = /** @type {{ id: string, name?: string, type?: string } | null} */ (null),
  } = $props()

  const label = 'text-ui-xs font-medium text-muted-foreground'
  const driver = $derived(String(connection?.type ?? ''))
  const supported = $derived(SCHEDULE_DRIVERS.has(driver))
  const overview = $derived($backupOverview)
  const jobs = $derived((overview?.jobs ?? []).filter((j) => j.connection_id === connection?.id))
  const otherJobs = $derived((overview?.jobs ?? []).filter((j) => j.connection_id !== connection?.id))
  const atLimit = $derived(!!overview && overview.jobs.length >= overview.max_jobs)
  const usedPct = $derived(overview ? Math.min(100, (overview.used_bytes / overview.quota_bytes) * 100) : 0)

  /** The sync key is on this computer: backups can be encrypted before upload. */
  let syncUnlocked = $state(/** @type {boolean|null} */ (null))
  async function checkSync() {
    try {
      const st = /** @type {{ unlocked: boolean }} */ (await invoke('sync_state'))
      syncUnlocked = !!st.unlocked
    } catch {
      syncUnlocked = false
    }
  }

  onMount(() => {
    if ($isSignedIn) void refreshBackupJobs()
    void checkSync()
  })

  // ── Form ───────────────────────────────────────────────────────────────────
  const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']
  const FREQ = [
    { value: 'hourly', label: 'Hourly' },
    { value: '6h', label: 'Every 6h' },
    { value: 'daily', label: 'Daily' },
    { value: 'weekly', label: 'Weekly' },
    { value: 'custom', label: 'Cron' },
  ]
  let formOpen = $state(false)
  let editing = $state(/** @type {string|null} */ (null))
  let saving = $state(false)
  let formError = $state(/** @type {string|null} */ (null))
  let preset = $state('daily')
  let time = $state('03:00')
  let weekday = $state('1')
  let customCron = $state('0 3 * * *')
  let destination = $state(/** @type {'stroke'|'s3'} */ ('stroke'))
  let keep = $state('7')
  let s3Bucket = $state('')
  let s3Endpoint = $state('')
  let s3Prefix = $state('')
  let s3Region = $state('')
  let s3AccessKey = $state('')
  let s3Secret = $state('')
  /** S3 keys already on this device for the job being edited (left blank = keep them). */
  let s3HasKeys = $state(false)

  const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'

  const cron = $derived.by(() => {
    const [h, m] = (time || '03:00').split(':').map((x) => String(Number(x) || 0))
    if (preset === 'hourly') return '0 * * * *'
    if (preset === '6h') return '0 */6 * * *'
    if (preset === 'daily') return `${m} ${h} * * *`
    if (preset === 'weekly') return `${m} ${h} * * ${weekday}`
    return customCron.trim()
  })

  /** The next time the schedule fires, or an error for a bad cron. */
  const nextRun = $derived.by(() => {
    try {
      const next = new Cron(cron, { timezone, paused: true }).nextRun()
      return next ? { at: next, error: null } : { at: null, error: 'Never runs' }
    } catch {
      return { at: null, error: 'Not a valid cron' }
    }
  })

  /** @param {BackupJob|null} job */
  async function openForm(job = null) {
    formError = null
    void checkSync()
    editing = job?.id ?? null
    destination = job?.destination ?? 'stroke'
    keep = String(job?.keep ?? 7)
    s3Bucket = job?.s3_bucket ?? ''
    s3Endpoint = job?.s3_endpoint ?? ''
    s3Prefix = job?.s3_prefix ?? ''
    s3AccessKey = ''
    s3Secret = ''
    s3Region = ''
    s3HasKeys = false
    preset = 'daily'
    time = '03:00'
    if (job) {
      const [min, hour, dom, mon, dow] = job.cron.split(/\s+/)
      const num = /^\d+$/
      const hhmm = () => `${hour.padStart(2, '0')}:${min.padStart(2, '0')}`
      if (job.cron === '0 * * * *') preset = 'hourly'
      else if (job.cron === '0 */6 * * *') preset = '6h'
      else if (num.test(min) && num.test(hour) && dom === '*' && mon === '*' && dow === '*') { preset = 'daily'; time = hhmm() }
      else if (num.test(min) && num.test(hour) && dom === '*' && mon === '*' && num.test(dow)) { preset = 'weekly'; time = hhmm(); weekday = dow }
      else { preset = 'custom'; customCron = job.cron }
      if (job.destination === 's3') {
        const region = await s3KeysRegion(job.id)
        s3HasKeys = region !== null
        s3Region = region ?? ''
      }
    }
    formOpen = true
  }

  async function submit(/** @type {SubmitEvent} */ e) {
    e.preventDefault()
    if (!connection || saving) return
    formError = null
    if (nextRun.error) { formError = nextRun.error; return }
    if (destination === 'stroke' && !syncUnlocked) { formError = 'Set up or unlock sync (Settings → Sync) first: Stroke Cloud backups are encrypted with your sync key.'; return }
    if (destination === 's3') {
      if (!s3Bucket.trim()) { formError = 'Name the bucket.'; return }
      if (!s3HasKeys && (!s3AccessKey.trim() || !s3Secret.trim())) { formError = 'Add the access key and secret.'; return }
    }
    saving = true
    const id = editing ?? crypto.randomUUID()
    try {
      // Keys first: a job the server knows about but this device can't upload
      // for would fail on its first run.
      if (destination === 's3' && s3AccessKey.trim() && s3Secret.trim()) {
        await saveS3Keys(id, { accessKeyId: s3AccessKey, secretAccessKey: s3Secret, region: s3Region || 'us-east-1' })
      }
      await saveBackupJob({
        id,
        connection_id: connection.id,
        label: connection.name || driver,
        driver,
        cron,
        timezone,
        destination,
        keep: Number(keep),
        s3_bucket: destination === 's3' ? s3Bucket.trim() : null,
        s3_endpoint: destination === 's3' ? s3Endpoint.trim() || null : null,
        s3_prefix: destination === 's3' ? s3Prefix.trim() || null : null,
      })
      formOpen = false
      toast.success(editing ? 'Schedule saved' : 'Backup scheduled', { description: describeCron(cron) })
    } catch (err) {
      formError = String(err)
    } finally {
      saving = false
    }
  }

  /** @param {() => Promise<unknown>} fn @param {string} failMsg */
  async function act(fn, failMsg) {
    try { await fn() } catch (err) { toast.error(failMsg, { description: errorText(err) }) }
  }

  let confirmDelete = $state(/** @type {string|null} */ (null))

  /** @param {number|Date} t */
  const when = (t) => new Date(t).toLocaleString([], { weekday: 'short', hour: '2-digit', minute: '2-digit' })
  /** @param {number} t */
  function ago(t) {
    const d = Math.max(0, Date.now() - t) / 1000
    if (d < 90) return 'just now'
    if (d < 3600) return `${Math.round(d / 60)}m ago`
    if (d < 86400) return `${Math.round(d / 3600)}h ago`
    return `${Math.round(d / 86400)}d ago`
  }

  async function openWebsite() {
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl('https://stroke.click/app/backups')
    } catch {
      window.open('https://stroke.click/app/backups', '_blank', 'noopener,noreferrer')
    }
  }
</script>

{#snippet message(/** @type {string} */ title, /** @type {string} */ body, /** @type {import('svelte').Snippet | undefined} */ action = undefined)}
  <EmptyState class="flex-1 justify-center" icon={CalendarClock} {title} description={body}>
    {#if action}{@render action()}{/if}
  </EmptyState>
{/snippet}

{#snippet signInButton()}
  <Button size="sm" onclick={() => void signInWithToast()}>Sign in</Button>
{/snippet}

{#snippet getPro()}
  <Button variant="secondary" size="sm" onclick={() => void openManageAccount()}>Get Stroke Pro <ExternalLink class="size-3.5 shrink-0" /></Button>
{/snippet}

{#snippet addButton()}
  <Button size="sm" disabled={atLimit} onclick={() => void openForm()}><Plus class="size-3.5 shrink-0" />Add schedule</Button>
{/snippet}

{#if !$isSignedIn}
  {@render message('Sign in for scheduled backups', 'They come with Stroke Pro on your account.', signInButton)}
{:else if !$hasPaidPro || overview?.pro === false}
  {@render message('Scheduled backups need Stroke Pro', $isTrialActive ? "The free trial doesn't include them. Up to 10 databases, to Stroke Cloud or your own S3." : 'Up to 10 databases, to Stroke Cloud or your own S3.', getPro)}
{:else if !connection}
  {@render message('Save this connection first', 'Schedules run on saved connections.')}
{:else if !supported}
  {@render message('Not available for this database', 'PostgreSQL, MySQL, SQLite, D1, Turso, DuckDB and SQL Server.')}
{:else}
  <div class="app-scroll flex min-h-0 flex-1 flex-col overflow-y-auto">
    <div class="mx-auto flex w-full max-w-3xl flex-col gap-6 px-6 py-6">
      <!-- One line: title, usage, the one action. -->
      <header class="flex items-center gap-3">
        <h2 class="text-ui-lg font-semibold text-foreground">Scheduled backups</h2>
        <span
          class="text-muted-foreground"
          title="Runs while Stroke is open on a computer with this connection. A run missed while it was closed happens when it opens."
        >
          <Info class="size-3.5 shrink-0" />
        </span>
        <span class="min-w-0 flex-1"></span>
        {#if overview}
          <span class="hidden items-center gap-2 text-ui-xs text-muted-foreground tabular-nums sm:flex" title="Stroke Cloud storage">
            <span class="h-1 w-16 overflow-hidden rounded-full bg-muted">
              <span class="block h-full rounded-full bg-primary" style:width="{usedPct}%"></span>
            </span>
            {formatBytes(overview.used_bytes)} / {formatBytes(overview.quota_bytes)}
          </span>
          <span class="text-ui-xs text-muted-foreground tabular-nums">{overview.jobs.length}/{overview.max_jobs}</span>
        {/if}
        <Button variant="ghost" size="icon-sm" aria-label="Open on stroke.click" title="History and downloads on stroke.click" onclick={() => void openWebsite()}>
          <ExternalLink class="size-3.5 shrink-0" />
        </Button>
        {@render addButton()}
      </header>

      {#if $backupLoadError && !overview}
        <p class="text-ui-xs text-destructive">{$backupLoadError}</p>
      {/if}

      {#if !overview}
        <div class="h-24 animate-pulse rounded-lg bg-muted/40"></div>
      {:else if jobs.length === 0}
        <div class="flex flex-col items-center gap-3 rounded-lg border border-dashed border-border/70 px-6 py-12 text-center">
          <CalendarClock class="size-5 shrink-0 text-muted-foreground" />
          <p class="text-ui-sm text-muted-foreground">No schedule for this database yet.</p>
          {@render addButton()}
        </div>
      {:else}
        <ul class="divide-y divide-border/50 overflow-hidden rounded-lg border border-border/60">
          {#each jobs as job (job.id)}
            {@const last = job.runs[0]}
            {@const running = $runningJobId === job.id}
            <li class="group flex items-center gap-3 px-3 py-2.5">
              <span class="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted/60">
                {#if job.destination === 'stroke'}<Cloud class="size-4 shrink-0 text-muted-foreground" />{:else}<HardDrive class="size-4 shrink-0 text-muted-foreground" />{/if}
              </span>
              <div class="min-w-0 flex-1">
                <p class="flex items-center gap-2 text-ui-sm text-foreground">
                  <span class="truncate">{describeCron(job.cron)}</span>
                  {#if !job.enabled}<span class="rounded bg-muted px-1.5 text-ui-3xs text-muted-foreground">Paused</span>{/if}
                </p>
                <p class="truncate text-ui-xs text-muted-foreground" title={last?.error ?? undefined}>
                  {job.destination === 'stroke' ? `Stroke Cloud · keeps ${job.keep}` : job.s3_bucket}
                </p>
              </div>
              <div class="hidden w-28 shrink-0 text-right text-ui-xs sm:block">
                {#if running}
                  <span class="inline-flex items-center gap-1.5 text-foreground"><LoaderCircle class="size-3 shrink-0 animate-spin" />Running</span>
                {:else if last}
                  <span class={cn('inline-flex items-center gap-1.5', last.status === 'failed' ? 'text-destructive' : 'text-muted-foreground')} title={last.error ?? undefined}>
                    <span class={cn('size-1.5 shrink-0 rounded-full', last.status === 'failed' ? 'bg-destructive' : last.status === 'success' ? 'bg-success' : 'bg-muted-foreground')}></span>
                    {last.status === 'failed' ? 'Failed' : ago(last.started_at)}
                  </span>
                {:else}
                  <span class="text-muted-foreground">Never run</span>
                {/if}
                {#if job.enabled && job.next_run_at}
                  <span class="block text-muted-foreground tabular-nums">Next {when(job.next_run_at)}</span>
                {/if}
              </div>
              <div class="flex shrink-0 items-center">
                {#if confirmDelete === job.id}
                  <Button variant="ghost" size="sm" class="text-destructive hover:text-destructive" onclick={() => act(() => deleteBackupJob(job.id), "Couldn't delete it")}>Delete</Button>
                  <Button variant="ghost" size="sm" onclick={() => (confirmDelete = null)}>Keep</Button>
                {:else}
                  <Button variant="ghost" size="icon-sm" aria-label="Run now" title="Run now" disabled={running || !!job.run_requested_at} onclick={() => act(() => patchBackupJob(job.id, { run_now: true }), "Couldn't start it")}>
                    <Play class="size-3.5 shrink-0" />
                  </Button>
                  <Button variant="ghost" size="icon-sm" aria-label={job.enabled ? 'Pause' : 'Resume'} title={job.enabled ? 'Pause' : 'Resume'} onclick={() => act(() => patchBackupJob(job.id, { enabled: !job.enabled }), "Couldn't change it")}>
                    {#if job.enabled}<Pause class="size-3.5 shrink-0" />{:else}<CalendarClock class="size-3.5 shrink-0" />{/if}
                  </Button>
                  <Button variant="ghost" size="icon-sm" aria-label="Edit" title="Edit" onclick={() => void openForm(job)}><Pencil class="size-3.5 shrink-0" /></Button>
                  <Button variant="ghost" size="icon-sm" aria-label="Delete" title="Delete with its backups" onclick={() => (confirmDelete = job.id)}><Trash2 class="size-3.5 shrink-0" /></Button>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}

      {#if otherJobs.length}
        <section class="flex flex-col gap-2" aria-labelledby="sched-other">
          <h3 id="sched-other" class={label}>Other databases</h3>
          <ul class="flex flex-col">
            {#each otherJobs as job (job.id)}
              <li class="flex h-8 items-center gap-3 text-ui-xs">
                <span class="min-w-0 flex-1 truncate text-foreground">{job.label}</span>
                <span class="shrink-0 text-muted-foreground">{describeCron(job.cron)}{job.enabled ? '' : ' · Paused'}</span>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    </div>
  </div>
{/if}

<Dialog.Root bind:open={formOpen}>
  <Dialog.Content class="sm:max-w-[30rem]">
    <Dialog.Header>
      <Dialog.Title>{editing ? 'Edit schedule' : 'New schedule'}</Dialog.Title>
      <Dialog.Description class="truncate">{connection?.name || driver}</Dialog.Description>
    </Dialog.Header>

    <form class="flex flex-col gap-5" onsubmit={submit}>
      <div class="flex flex-col gap-2">
        <span class={label}>Repeat</span>
        <SegmentedControl options={FREQ} bind:value={preset} ariaLabel="Repeat" class="w-full" />
        {#if preset === 'daily' || preset === 'weekly'}
          <div class="flex items-center gap-2">
            {#if preset === 'weekly'}
              <SegmentedControl options={DAYS.map((d, i) => ({ value: String(i), label: d }))} bind:value={weekday} ariaLabel="Day" class="min-w-0 flex-1" />
            {/if}
            <Input type="time" bind:value={time} required aria-label="Time" class="h-8 w-28" />
          </div>
        {:else if preset === 'custom'}
          <Input bind:value={customCron} placeholder="0 3 * * *" aria-label="Cron" class="h-8 font-mono" required />
        {/if}
        <p class={cn('text-ui-xs tabular-nums', nextRun.error ? 'text-destructive' : 'text-muted-foreground')}>
          {nextRun.error ?? `Next ${when(nextRun.at ?? 0)}`}
        </p>
      </div>

      <div class="flex flex-col gap-2">
        <span class={label}>Save to</span>
        <div class="grid grid-cols-2 gap-2" role="radiogroup" aria-label="Save to">
          {#each [{ v: /** @type {const} */ ('stroke'), icon: Cloud, title: 'Stroke Cloud', sub: '5 GB included' }, { v: /** @type {const} */ ('s3'), icon: HardDrive, title: 'S3 bucket', sub: 'R2, MinIO, B2, AWS' }] as opt (opt.v)}
            <button
              type="button"
              role="radio"
              aria-checked={destination === opt.v}
              onclick={() => (destination = opt.v)}
              class={cn(
                'flex items-center gap-2.5 rounded-md border px-3 py-2 text-left transition-colors',
                destination === opt.v ? 'border-primary/60 bg-primary/5' : 'border-border/60 hover:border-border',
              )}
            >
              <opt.icon class="size-4 shrink-0 text-muted-foreground" />
              <span class="min-w-0">
                <span class="block text-ui-sm text-foreground">{opt.title}</span>
                <span class="block text-ui-2xs text-muted-foreground">{opt.sub}</span>
              </span>
            </button>
          {/each}
        </div>
      </div>

      {#if destination === 'stroke'}
        <div class="flex items-center justify-between gap-3">
          <span class={label}>Keep</span>
          <SegmentedControl options={['3', '7', '14', '30'].map((n) => ({ value: n, label: n }))} bind:value={keep} ariaLabel="Backups to keep" />
        </div>
      {:else}
        <div class="grid grid-cols-2 gap-2">
          <Input bind:value={s3Bucket} placeholder="Bucket" aria-label="Bucket" class="h-8" required />
          <Input bind:value={s3Region} placeholder="Region (us-east-1)" aria-label="Region" class="h-8" />
          <Input bind:value={s3Endpoint} placeholder="Endpoint (empty for AWS)" aria-label="Endpoint" class="col-span-2 h-8" />
          <Input bind:value={s3Prefix} placeholder="Folder (optional)" aria-label="Folder" class="col-span-2 h-8" />
          <Input bind:value={s3AccessKey} placeholder={s3HasKeys ? 'Access key (saved)' : 'Access key ID'} aria-label="Access key ID" class="h-8" />
          <Input type="password" bind:value={s3Secret} placeholder={s3HasKeys ? 'Secret (saved)' : 'Secret access key'} aria-label="Secret access key" class="h-8" />
          <p class="col-span-2 text-ui-2xs text-muted-foreground">Keys stay in this computer's keychain.</p>
        </div>
      {/if}

      <p class="flex items-start gap-1.5 text-ui-2xs leading-relaxed text-muted-foreground">
        <LockKeyhole class="mt-px size-3 shrink-0" />
        {#if destination === 'stroke' || syncUnlocked}
          Encrypted on this computer with your sync key. Stroke can't read it, and without your sync passphrase nobody can, you included.
        {:else}
          Set up sync to encrypt these too; until then the bucket gets the plain .sql.gz.
        {/if}
      </p>
      {#if destination === 'stroke' && syncUnlocked === false}
        <p class="text-ui-xs text-warning" role="status">Stroke Cloud needs sync set up and unlocked on this computer (Settings → Sync).</p>
      {/if}
      {#if formError}<p class="text-ui-xs text-destructive" role="alert">{formError}</p>{/if}

      <Dialog.Footer>
        <Button type="button" variant="ghost" size="sm" onclick={() => (formOpen = false)}>Cancel</Button>
        <Button type="submit" size="sm" disabled={saving}>
          {#if saving}<LoaderCircle class="size-3.5 shrink-0 animate-spin" />{/if}
          {editing ? 'Save' : 'Schedule'}
        </Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>
