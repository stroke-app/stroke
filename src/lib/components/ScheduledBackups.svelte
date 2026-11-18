<script>
  // Scheduled backups for the open database (Pro): the jobs on this
  // connection with their last runs, and a form to add or edit one. The
  // schedule and history live on stroke.click; this device runs the jobs for
  // connections saved on it (see $lib/stores/scheduled-backups.js).
  import { onMount } from 'svelte'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import FieldSelect from './FieldSelect.svelte'
  import { isSignedIn, openManageAccount } from '$lib/stores/account.js'
  import { hasPro } from '$lib/stores/license.js'
  import {
    backupOverview, backupLoadError, runningJobId, refreshBackupJobs, saveBackupJob, patchBackupJob,
    deleteBackupJob, saveS3Keys, s3KeysRegion, describeCron, formatBytes, SCHEDULE_DRIVERS,
  } from '$lib/stores/scheduled-backups.js'
  import { cn } from '$lib/utils.js'
  import CalendarClock from '@lucide/svelte/icons/calendar-clock'
  import Cloud from '@lucide/svelte/icons/cloud'
  import HardDrive from '@lucide/svelte/icons/hard-drive'
  import Play from '@lucide/svelte/icons/play'
  import Pause from '@lucide/svelte/icons/pause'
  import Pencil from '@lucide/svelte/icons/pencil'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Plus from '@lucide/svelte/icons/plus'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import Check from '@lucide/svelte/icons/check'
  import X from '@lucide/svelte/icons/x'
  import ExternalLink from '@lucide/svelte/icons/external-link'

  /** @typedef {import('$lib/stores/scheduled-backups.js').BackupJob} BackupJob */

  let {
    /** The saved connection behind the open database, or null when it isn't saved. */
    connection = /** @type {{ id: string, name?: string, type?: string } | null} */ (null),
  } = $props()

  const sectionLabel = 'text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground'
  const driver = $derived(String(connection?.type ?? ''))
  const supported = $derived(SCHEDULE_DRIVERS.has(driver))
  const overview = $derived($backupOverview)
  const jobs = $derived((overview?.jobs ?? []).filter((j) => j.connection_id === connection?.id))
  const otherJobs = $derived((overview?.jobs ?? []).filter((j) => j.connection_id !== connection?.id))
  const atLimit = $derived(!!overview && overview.jobs.length >= overview.max_jobs)

  onMount(() => {
    if ($isSignedIn) void refreshBackupJobs()
  })

  // ── Form ───────────────────────────────────────────────────────────────────
  const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']
  let editing = $state(/** @type {string|null} */ (null))
  let formOpen = $state(false)
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

  /** @param {BackupJob|null} job */
  async function openForm(job = null) {
    formError = null
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
    if (destination === 's3') {
      if (!s3Bucket.trim()) { formError = 'Name the bucket.'; return }
      if (!s3HasKeys && (!s3AccessKey.trim() || !s3Secret.trim())) { formError = 'Add the access key ID and secret access key.'; return }
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
      toast.success(editing ? 'Schedule saved' : 'Backup scheduled', { description: `${describeCron(cron)}, while Stroke is open on this computer.` })
    } catch (err) {
      formError = String(err)
    } finally {
      saving = false
    }
  }

  /** @param {() => Promise<unknown>} fn @param {string} failMsg */
  async function act(fn, failMsg) {
    try { await fn() } catch (err) { toast.error(failMsg, { description: String(err) }) }
  }

  let confirmDelete = $state(/** @type {string|null} */ (null))

  /** @param {number} ms */
  const when = (ms) => new Date(ms).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })

  async function openWebsite() {
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl('https://stroke.click/app/backups')
    } catch {
      window.open('https://stroke.click/app/backups', '_blank', 'noopener,noreferrer')
    }
  }
</script>

{#snippet message(/** @type {string} */ title, /** @type {string} */ body)}
  <div class="flex flex-1 flex-col items-center justify-center gap-2 px-8 text-center">
    <CalendarClock class="size-5 shrink-0 text-muted-foreground" />
    <p class="text-ui-sm font-medium text-foreground">{title}</p>
    <p class="max-w-sm text-ui-xs leading-relaxed text-muted-foreground">{body}</p>
  </div>
{/snippet}

{#if !$isSignedIn}
  {@render message('Sign in for scheduled backups', 'Scheduled backups come with Stroke Pro on your account.')}
{:else if !$hasPro || overview?.pro === false}
  <div class="flex flex-1 flex-col items-center justify-center gap-3 px-8 text-center">
    <CalendarClock class="size-5 shrink-0 text-muted-foreground" />
    <p class="text-ui-sm font-medium text-foreground">Scheduled backups are part of Stroke Pro</p>
    <p class="max-w-sm text-ui-xs leading-relaxed text-muted-foreground">Back up up to 10 databases on a schedule, to Stroke Cloud or your own S3 bucket.</p>
    <Button variant="secondary" size="sm" onclick={() => void openManageAccount()}>Get Stroke Pro <ExternalLink class="size-3.5 shrink-0" /></Button>
  </div>
{:else if !connection}
  {@render message('Save this connection first', 'Scheduled backups run on saved connections, so Stroke can open them again later.')}
{:else if !supported}
  {@render message('Not available for this database', 'Scheduled backups support PostgreSQL, MySQL, SQLite, Cloudflare D1, Turso/libSQL, DuckDB and SQL Server.')}
{:else}
  <div class="app-scroll flex min-h-0 flex-1 flex-col overflow-y-auto">
    <div class="mx-auto flex w-full max-w-2xl flex-col gap-8 px-6 py-8">
      <header class="flex items-start justify-between gap-4">
        <div class="min-w-0">
          <h2 class="text-ui-lg font-semibold tracking-tight text-foreground">Scheduled backups</h2>
          <p class="mt-1 text-ui-xs leading-relaxed text-muted-foreground text-pretty">
            Stroke runs them while it's open on a computer with this connection. A run missed while it was closed happens the next time it opens.
          </p>
          {#if overview}
            <p class="mt-2 text-ui-xs tabular-nums text-muted-foreground">
              {overview.jobs.length} of {overview.max_jobs} schedules · {formatBytes(overview.used_bytes)} of {formatBytes(overview.quota_bytes)} on Stroke Cloud
            </p>
          {/if}
        </div>
        <Button variant="ghost" size="sm" class="shrink-0" onclick={() => void openWebsite()}>
          stroke.click <ExternalLink class="size-3.5 shrink-0" />
        </Button>
      </header>

      {#if $backupLoadError && !overview}
        <p class="text-ui-xs text-destructive">{$backupLoadError}</p>
      {/if}

      <section class="flex flex-col gap-2" aria-labelledby="sched-this">
        <h3 id="sched-this" class={sectionLabel}>This database</h3>
        {#if !overview}
          <div class="h-12 animate-pulse rounded-md bg-muted/50"></div>
        {:else if jobs.length === 0 && !formOpen}
          <p class="text-ui-sm text-muted-foreground">No schedule yet.</p>
        {/if}
        {#each jobs as job (job.id)}
          {@const last = job.runs[0]}
          <div class="flex items-center gap-3 border-b border-border/40 py-2.5 last:border-b-0">
            {#if job.destination === 'stroke'}<Cloud class="size-4 shrink-0 text-muted-foreground" />{:else}<HardDrive class="size-4 shrink-0 text-muted-foreground" />{/if}
            <div class="min-w-0 flex-1">
              <p class="flex items-center gap-2 text-ui-sm text-foreground">
                <span class="truncate">{describeCron(job.cron)}</span>
                {#if !job.enabled}<span class="shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-ui-3xs font-medium text-muted-foreground">Paused</span>{/if}
              </p>
              <p class="truncate text-ui-xs text-muted-foreground">
                {job.destination === 'stroke' ? `Stroke Cloud, keeps ${job.keep}` : `S3 bucket ${job.s3_bucket}`}
                {#if $runningJobId === job.id} · Running now
                {:else if last} · Last {last.status === 'failed' ? 'failed' : last.status === 'running' ? 'started' : 'ran'} {when(last.started_at)}
                {/if}
                {#if job.enabled && job.next_run_at} · Next {when(job.next_run_at)}{/if}
              </p>
              {#if last?.status === 'failed' && last.error}
                <p class="mt-0.5 truncate text-ui-xs text-destructive" title={last.error}>{last.error}</p>
              {/if}
            </div>
            <div class="flex shrink-0 items-center gap-0.5">
              {#if $runningJobId === job.id}
                <LoaderCircle class="mx-2 size-3.5 shrink-0 animate-spin text-muted-foreground" aria-label="Running" />
              {:else}
                <Button variant="ghost" size="sm" disabled={!!job.run_requested_at} onclick={() => act(() => patchBackupJob(job.id, { run_now: true }), "Couldn't start it")}>
                  <Play class="size-3.5 shrink-0" />Run now
                </Button>
              {/if}
              <Button variant="ghost" size="icon-sm" aria-label={job.enabled ? 'Pause' : 'Resume'} onclick={() => act(() => patchBackupJob(job.id, { enabled: !job.enabled }), "Couldn't change it")}>
                {#if job.enabled}<Pause class="size-3.5 shrink-0" />{:else}<Play class="size-3.5 shrink-0" />{/if}
              </Button>
              <Button variant="ghost" size="icon-sm" aria-label="Edit schedule" onclick={() => void openForm(job)}><Pencil class="size-3.5 shrink-0" /></Button>
              {#if confirmDelete === job.id}
                <Button variant="ghost" size="sm" class="text-destructive hover:text-destructive" onclick={() => act(() => deleteBackupJob(job.id), "Couldn't delete it")}>Delete with backups</Button>
                <Button variant="ghost" size="icon-sm" aria-label="Keep it" onclick={() => (confirmDelete = null)}><X class="size-3.5 shrink-0" /></Button>
              {:else}
                <Button variant="ghost" size="icon-sm" aria-label="Delete schedule" onclick={() => (confirmDelete = job.id)}><Trash2 class="size-3.5 shrink-0" /></Button>
              {/if}
            </div>
          </div>
        {/each}

        {#if overview && !formOpen}
          <div>
            <Button variant="secondary" size="sm" disabled={atLimit} onclick={() => void openForm()}><Plus class="size-3.5 shrink-0" />Add schedule</Button>
            {#if atLimit}<p class="mt-1.5 text-ui-xs text-muted-foreground">You have {overview.max_jobs} schedules, the most Pro allows. Delete one to add another.</p>{/if}
          </div>
        {/if}
      </section>

      {#if formOpen}
        <form class="flex flex-col gap-5 rounded-lg border border-border/60 p-4" onsubmit={submit} aria-label={editing ? 'Edit schedule' : 'New schedule'}>
          <div class="grid grid-cols-2 gap-3">
            <div class="flex flex-col gap-1.5">
              <label for="sched-preset" class={sectionLabel}>Repeat</label>
              <FieldSelect id="sched-preset" class="w-full" bind:value={preset} options={[
                { value: 'hourly', label: 'Every hour' },
                { value: '6h', label: 'Every 6 hours' },
                { value: 'daily', label: 'Every day' },
                { value: 'weekly', label: 'Every week' },
                { value: 'custom', label: 'Custom (cron)' },
              ]} />
            </div>
            {#if preset === 'daily' || preset === 'weekly'}
              <div class="flex flex-col gap-1.5">
                <label for="sched-time" class={sectionLabel}>At</label>
                <Input id="sched-time" type="time" bind:value={time} required />
              </div>
            {/if}
            {#if preset === 'weekly'}
              <div class="flex flex-col gap-1.5">
                <label for="sched-day" class={sectionLabel}>On</label>
                <FieldSelect id="sched-day" class="w-full" bind:value={weekday} options={DAYS.map((d, i) => ({ value: String(i), label: d }))} />
              </div>
            {/if}
            {#if preset === 'custom'}
              <div class="flex flex-col gap-1.5">
                <label for="sched-cron" class={sectionLabel}>Cron</label>
                <Input id="sched-cron" class="font-mono" bind:value={customCron} placeholder="0 3 * * *" required />
              </div>
            {/if}
          </div>
          <p class="-mt-3 text-ui-xs text-muted-foreground">{describeCron(cron)}, {timezone} time. At most once an hour.</p>

          <fieldset class="flex flex-col gap-2">
            <legend class={cn(sectionLabel, 'mb-2')}>Save to</legend>
            {#each [{ v: 'stroke', icon: Cloud, title: 'Stroke Cloud', body: 'Stored for you, 5 GB per account. Download from stroke.click.' }, { v: 's3', icon: HardDrive, title: 'My S3 bucket', body: 'AWS S3, Cloudflare R2, MinIO, Backblaze B2 or any S3-compatible storage.' }] as opt (opt.v)}
              <label class={cn('flex cursor-pointer items-start gap-3 rounded-md border px-3 py-2.5 transition-colors', destination === opt.v ? 'border-primary/60 bg-primary/5' : 'border-border/60 hover:border-border')}>
                <input type="radio" name="sched-dest" value={opt.v} bind:group={destination} class="sr-only" />
                <opt.icon class="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                <span class="min-w-0 flex-1">
                  <span class="block text-ui-sm font-medium text-foreground">{opt.title}</span>
                  <span class="block text-ui-xs text-muted-foreground">{opt.body}</span>
                </span>
                {#if destination === opt.v}<Check class="size-3.5 shrink-0 text-primary" />{/if}
              </label>
            {/each}
          </fieldset>

          {#if destination === 'stroke'}
            <div class="flex w-40 flex-col gap-1.5">
              <label for="sched-keep" class={sectionLabel}>Keep the last</label>
              <FieldSelect id="sched-keep" class="w-full" bind:value={keep} options={[3, 7, 14, 30].map((n) => ({ value: String(n), label: `${n} backups` }))} />
            </div>
          {:else}
            <div class="grid grid-cols-2 gap-3">
              <div class="flex flex-col gap-1.5">
                <label for="s3-bucket" class={sectionLabel}>Bucket</label>
                <Input id="s3-bucket" bind:value={s3Bucket} placeholder="my-backups" required />
              </div>
              <div class="flex flex-col gap-1.5">
                <label for="s3-region" class={sectionLabel}>Region</label>
                <Input id="s3-region" bind:value={s3Region} placeholder="us-east-1 (auto for R2)" />
              </div>
              <div class="col-span-2 flex flex-col gap-1.5">
                <label for="s3-endpoint" class={sectionLabel}>Endpoint <span class="normal-case tracking-normal font-normal">(leave empty for AWS)</span></label>
                <Input id="s3-endpoint" bind:value={s3Endpoint} placeholder="https://<account>.r2.cloudflarestorage.com" />
              </div>
              <div class="col-span-2 flex flex-col gap-1.5">
                <label for="s3-prefix" class={sectionLabel}>Folder <span class="normal-case tracking-normal font-normal">(optional)</span></label>
                <Input id="s3-prefix" bind:value={s3Prefix} placeholder="stroke/prod" />
              </div>
              <div class="flex flex-col gap-1.5">
                <label for="s3-key" class={sectionLabel}>Access key ID</label>
                <Input id="s3-key" bind:value={s3AccessKey} placeholder={s3HasKeys ? 'Saved on this computer' : ''} />
              </div>
              <div class="flex flex-col gap-1.5">
                <label for="s3-secret" class={sectionLabel}>Secret access key</label>
                <Input id="s3-secret" type="password" bind:value={s3Secret} placeholder={s3HasKeys ? 'Saved on this computer' : ''} />
              </div>
              <p class="col-span-2 text-ui-xs text-muted-foreground">The keys stay in this computer's keychain. stroke.click only sees the bucket name.</p>
            </div>
          {/if}

          {#if formError}<p class="text-ui-xs text-destructive" role="alert">{formError}</p>{/if}
          <div class="flex items-center gap-2">
            <Button type="submit" size="sm" disabled={saving}>{#if saving}<LoaderCircle class="size-3.5 shrink-0 animate-spin" />{/if}{editing ? 'Save schedule' : 'Schedule backup'}</Button>
            <Button type="button" variant="ghost" size="sm" onclick={() => (formOpen = false)}>Cancel</Button>
          </div>
        </form>
      {/if}

      {#if otherJobs.length}
        <section class="flex flex-col gap-2" aria-labelledby="sched-other">
          <h3 id="sched-other" class={sectionLabel}>Other databases</h3>
          <ul class="flex flex-col">
            {#each otherJobs as job (job.id)}
              <li class="flex h-9 items-center gap-3 border-b border-border/40 text-ui-sm last:border-b-0">
                <span class="min-w-0 flex-1 truncate text-foreground">{job.label}</span>
                <span class="shrink-0 text-ui-xs text-muted-foreground">{describeCron(job.cron)}{job.enabled ? '' : ' · Paused'}</span>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    </div>
  </div>
{/if}
