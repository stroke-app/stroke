<script>
  // Prisma and Drizzle migrations for the open database: what ran (from the
  // tool's own table, or Wrangler's `d1_migrations` on Cloudflare D1), what
  // failed, and what's still pending (from the project's migrations folder,
  // picked once per connection). Read-only: applying stays with the tool's
  // CLI, whose command the page hands over. Logic: $lib/migrations.js.
  import { invoke } from '@tauri-apps/api/core'
  import EmptyState from './EmptyState.svelte'
  import { executeSql } from '$lib/api.js'
  import { mergePrisma, mergeDrizzle, mergeD1, counts, rowsOf, migrationQueries, isMissingTable } from '$lib/migrations.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import SegmentedControl from './SegmentedControl.svelte'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { cn, errorText } from '$lib/utils.js'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import FolderOpen from '@lucide/svelte/icons/folder-open'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import SquareTerminal from '@lucide/svelte/icons/square-terminal'
  import Copy from '@lucide/svelte/icons/copy'
  import Check from '@lucide/svelte/icons/check'
  import GitMerge from '@lucide/svelte/icons/git-merge'
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert'
  import Search from '@lucide/svelte/icons/search'

  let {
    /** @type {{ id: string, name?: string } | null} */
    connection = null,
    /** Engine family: postgres | mysql | sqlite | d1 | ... */
    family = 'postgres',
    /** The schema the sidebar is on (Prisma's table lives in the connection's schema). */
    schema = /** @type {string | null} */ (null),
    /** The tab is on screen: load then, not at mount. */
    active = true,
    /** Open SQL in a new editor tab. @type {((sql: string, title: string) => void) | null} */
    onopensql = null,
  } = $props()

  /** @typedef {import('$lib/migrations.js').Migration} Migration */
  /** @typedef {import('$lib/migrations.js').MigrationFile} MigrationFile */
  /** @typedef {{ tool: 'prisma' | 'drizzle', dir: string, files: MigrationFile[], sqlFiles: MigrationFile[], wranglerDb: string | null }} Project */

  const dirKey = $derived(connection ? `stroke:migrations-dir:${connection.id}` : '')
  const toolKey = $derived(connection ? `stroke:migrations-tool:${connection.id}` : '')

  /** @type {'prisma' | 'drizzle'} */
  let tool = $state('prisma')
  let projectDir = $state('')
  /** @type {Project | null} */
  let project = $state(null)
  /** Rows from each table; null = the table isn't in this database. @type {Record<'prisma' | 'drizzle' | 'd1', Record<string, any>[] | null>} */
  let tables = $state({ prisma: null, drizzle: null, d1: null })
  let loading = $state(false)
  let error = $state('')
  let projectError = $state('')
  /** @type {string | null} */
  let open = $state(null)
  /** @type {'all' | 'pending' | 'applied' | 'failed'} */
  let filter = $state('all')
  let copied = $state('')
  let loadedFor = ''

  /** Read the remembered folder and tool for this connection. */
  $effect(() => {
    if (!connection) return
    try {
      projectDir = localStorage.getItem(dirKey) ?? ''
      const t = localStorage.getItem(toolKey)
      if (t === 'prisma' || t === 'drizzle') tool = t
    } catch { /* not kept */ }
  })

  /** One table, or null when it isn't there. @param {'prisma' | 'drizzle' | 'd1'} t */
  async function readTable(t) {
    let last = null
    for (const sql of migrationQueries(t, family, schema)) {
      try {
        return rowsOf(await executeSql(sql))
      } catch (e) {
        last = e
        if (!isMissingTable(e)) throw e
      }
    }
    if (last && !isMissingTable(last)) throw last
    return null
  }

  async function load() {
    if (!connection || loading) return
    loading = true
    error = ''
    try {
      // One after the other: a second query on the connection cancels the first.
      const p = await readTable('prisma')
      const d = await readTable('drizzle')
      const w = await readTable('d1')
      tables = { prisma: p, drizzle: d, d1: w }
      // Show the tool this database actually uses, unless one was picked.
      let picked = null
      try { picked = localStorage.getItem(toolKey) } catch { /* ignore */ }
      const drizzleLike = d || w
      if (!picked) tool = p && !drizzleLike ? 'prisma' : drizzleLike && !p ? 'drizzle' : tool
      await loadProject()
    } catch (e) {
      error = String(/** @type {any} */ (e)?.message ?? e)
    } finally {
      loading = false
    }
  }

  async function loadProject() {
    projectError = ''
    project = null
    if (!projectDir) return
    try {
      project = await invoke('migrations_project', { dir: projectDir, tool })
    } catch (e) {
      projectError = String(e)
    }
  }

  $effect(() => {
    if (!active || !connection) return
    const key = `${connection.id}:${family}:${schema ?? ''}`
    if (key === loadedFor) return
    loadedFor = key
    void load()
  })

  async function chooseFolder() {
    try {
      const { open: pick } = await import('@tauri-apps/plugin-dialog')
      const dir = await pick({ directory: true, multiple: false, title: 'Pick the project folder' })
      if (typeof dir !== 'string') return
      projectDir = dir
      try { localStorage.setItem(dirKey, dir) } catch { /* not kept */ }
      await loadProject()
      if (project && project.tool !== tool) setTool(project.tool)
    } catch (e) {
      toast.error('Could not open the folder', { description: errorText(e) })
    }
  }

  /** @param {'prisma' | 'drizzle'} t */
  function setTool(t) {
    tool = t
    open = null
    try { localStorage.setItem(toolKey, t) } catch { /* not kept */ }
    void loadProject()
  }

  const files = $derived(project && project.tool === tool ? project.files : null)
  /**
   * Where "what ran" comes from. Drizzle on D1 is applied by Wrangler, which
   * records file names in d1_migrations and never writes __drizzle_migrations.
   * @type {'prisma' | 'drizzle' | 'd1'}
   */
  const source = $derived(tool === 'prisma' ? 'prisma' : tables.drizzle === null && tables.d1 !== null ? 'd1' : 'drizzle')
  const rows = $derived(tables[source])
  /** @type {Migration[]} */
  const list = $derived.by(() => {
    if (rows === null && !files) return []
    if (source === 'prisma') return mergePrisma(rows ?? [], files)
    if (source === 'd1') return mergeD1(rows ?? [], project && project.tool === 'drizzle' ? project.sqlFiles : null)
    return mergeDrizzle(rows ?? [], files)
  })
  const c = $derived(counts(list))
  const shown = $derived(
    filter === 'all' ? list
      : filter === 'failed' ? list.filter((m) => m.status === 'failed' || m.status === 'rolled-back' || m.changed)
      : list.filter((m) => m.status === filter),
  )
  const neither = $derived(!loading && !error && tables.prisma === null && tables.drizzle === null && tables.d1 === null && !files)
  const lastApplied = $derived(list.filter((m) => m.status === 'applied' && m.appliedAt).sort((a, b) => (b.appliedAt ?? 0) - (a.appliedAt ?? 0))[0] ?? null)
  const known = $derived(!!files || source === 'd1' && !!project)
  const pct = $derived(list.length ? Math.round((c.applied / list.length) * 100) : 0)

  const TABLE_NAME = { prisma: '_prisma_migrations', drizzle: '__drizzle_migrations', d1: 'd1_migrations' }

  /** The command that applies what's pending, for this tool. */
  const applyCommand = $derived(
    source === 'prisma' ? 'npx prisma migrate deploy'
      : source === 'd1' ? `npx wrangler d1 migrations apply ${project?.wranglerDb ?? '<database>'} --remote`
      : 'npx drizzle-kit migrate',
  )

  const STATUS = {
    applied: { label: 'Applied', dot: 'bg-success', text: 'text-muted-foreground', badge: 'bg-success/12 text-success' },
    pending: { label: 'Pending', dot: 'bg-warning', text: 'text-warning', badge: 'bg-warning/15 text-warning' },
    failed: { label: 'Failed', dot: 'bg-destructive', text: 'text-destructive', badge: 'bg-destructive/12 text-destructive' },
    'rolled-back': { label: 'Rolled back', dot: 'bg-warning', text: 'text-warning', badge: 'bg-warning/15 text-warning' },
  }

  /** "0013_sync" → ["0013", "sync"]; Prisma's "20240101120000_init" too. @param {string} name */
  function splitName(name) {
    const m = /^(\d+)[_-](.+)$/.exec(name)
    return m ? [m[1], m[2].replace(/_/g, ' ')] : ['', name]
  }

  /** @param {number | null} ms */
  const when = (ms) => (ms ? new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }) : '')
  /** @param {number | null} ms */
  function ago(ms) {
    if (!ms) return ''
    const s = Math.round((Date.now() - ms) / 1000)
    if (s < 3600) return `${Math.max(1, Math.round(s / 60))} min ago`
    if (s < 86400) return `${Math.round(s / 3600)} h ago`
    const d = Math.round(s / 86400)
    return d < 60 ? `${d} day${d === 1 ? '' : 's'} ago` : when(ms)
  }
  /** The editor's height for a migration's SQL: its lines, long ones counted as they wrap. @param {string} sql */
  const sqlHeight = (sql) => Math.min(360, sql.split('\n').reduce((n, l) => n + Math.max(1, Math.ceil(l.length / 80)), 1) * 22 + 12)
  /** "Jun 8, 12:07" this year, "Jun 8, 2025" before: one line, whatever the font. @param {number | null} ms */
  function shortDate(ms) {
    if (!ms) return ''
    const d = new Date(ms)
    return d.getFullYear() === new Date().getFullYear()
      ? d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false })
      : d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })
  }

  let query = $state('')
  const found = $derived.by(() => {
    const q = query.trim().toLowerCase()
    return q ? shown.filter((m) => m.name.toLowerCase().includes(q) || m.name.replace(/_/g, ' ').toLowerCase().includes(q)) : shown
  })
  /** Applied migrations by the month they ran, newest month last; pending at the end. */
  const groups = $derived.by(() => {
    /** @type {{ key: string, label: string, items: Migration[] }[]} */
    const out = []
    for (const m of found) {
      const at = m.appliedAt ?? m.startedAt
      const key = m.status === 'pending' ? 'pending' : at ? new Date(at).toISOString().slice(0, 7) : 'unknown'
      let g = out.find((x) => x.key === key)
      if (!g) {
        const label = key === 'pending' ? 'Not applied yet' : key === 'unknown' ? 'Applied' : new Date(/** @type {number} */ (at)).toLocaleDateString(undefined, { month: 'long', year: 'numeric' })
        g = { key, label, items: [] }
        out.push(g)
      }
      g.items.push(m)
    }
    return out.sort((a, b) => (a.key === 'pending' ? 1 : b.key === 'pending' ? -1 : a.key.localeCompare(b.key)))
  })
  /** @param {number | null} ms */
  const took = (ms) => (ms == null ? '' : ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`)
  /** The folder's last two parts, for the header. @param {string} p */
  const shortDir = (p) => p.split(/[\\/]/).filter(Boolean).slice(-2).join('/')

  /** @param {string} text @param {string} what */
  async function copy(text, what) {
    try {
      await navigator.clipboard.writeText(text)
      copied = what
      setTimeout(() => { if (copied === what) copied = '' }, 1500)
    } catch {
      toast.error('Could not copy')
    }
  }

  const FILTERS = $derived([
    { value: 'all', label: `All ${list.length}` },
    { value: 'pending', label: `Pending ${c.pending}` },
    { value: 'applied', label: `Applied ${c.applied}` },
    ...(c.failed || c.rolledBack || c.changed ? [{ value: 'failed', label: `Problems ${c.failed + c.rolledBack + c.changed}` }] : []),
  ])
</script>

<div class="flex min-h-0 flex-1 flex-col bg-panel">
  <!-- Header: the tool, the folder, refresh. -->
  <div class="studio-chrome flex h-12 shrink-0 items-center gap-3 border-b border-border/60 px-4" data-studio-chrome>
    <SegmentedControl
      options={[{ value: 'prisma', label: 'Prisma' }, { value: 'drizzle', label: 'Drizzle' }]}
      value={tool}
      ariaLabel="Migration tool"
      class="w-44 shrink-0"
      onchange={(v) => setTool(v)}
    />
    <span class="flex-1"></span>
    <Button variant="ghost" size="sm" class="max-w-72 gap-1.5 text-muted-foreground hover:text-foreground" title={projectDir || 'Pick the project folder to see pending migrations'} onclick={() => void chooseFolder()}>
      <FolderOpen class="size-3.5 shrink-0" />
      <span class="truncate">{projectDir ? shortDir(projectDir) : 'Project folder'}</span>
    </Button>
    <Button variant="ghost" size="icon-sm" class="text-muted-foreground hover:text-foreground" title="Refresh" aria-label="Refresh" onclick={() => { loadedFor = ''; void load() }}>
      <RefreshCw class={cn('size-3.5 shrink-0', loading && 'animate-pulse')} />
    </Button>
  </div>

  <div class="app-scroll min-h-0 flex-1 overflow-y-auto">
    <div class="mx-auto flex w-full max-w-4xl flex-col gap-5 px-6 py-6">
      {#if error}
        <div class="flex flex-col items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-4 py-3" role="alert">
          <p class="text-ui-sm font-medium text-destructive">Could not read the migrations table</p>
          <p class="text-ui-xs break-words text-muted-foreground">{error}</p>
        </div>
      {:else if neither}
        <EmptyState class="py-20" icon={GitMerge} title="No migrations in this database yet">
          {#snippet body()}There's no <code class="font-mono">_prisma_migrations</code>, <code class="font-mono">__drizzle_migrations</code> or <code class="font-mono">d1_migrations</code> table here. Pick the project folder to see what would run.{/snippet}
          <Button variant="secondary" size="sm" onclick={() => void chooseFolder()}><FolderOpen class="size-3.5 shrink-0" />Pick project folder</Button>
        </EmptyState>
      {:else if list.length}
        <!-- At a glance: how far along this database is. -->
        <section class="overflow-hidden rounded-lg border border-border/60" aria-label="Summary">
          <dl class="grid grid-cols-2 sm:grid-cols-4">
            <div class="flex flex-col gap-1 px-4 py-3.5">
              <dt class="text-ui-2xs text-muted-foreground">Applied</dt>
              <dd class="text-ui-2xl leading-none font-semibold tabular-nums">{c.applied}{#if known}<span class="ms-1.5 text-ui-sm font-normal text-muted-foreground">of {list.length}</span>{/if}</dd>
            </div>
            <div class="flex flex-col gap-1 border-s border-border/60 px-4 py-3.5">
              <dt class="text-ui-2xs text-muted-foreground">Pending</dt>
              <dd class={cn('text-ui-2xl leading-none font-semibold tabular-nums', known && c.pending ? 'text-warning' : !known && 'text-muted-foreground')}>{known ? c.pending : '?'}</dd>
            </div>
            <div class="flex flex-col gap-1 border-border/60 px-4 py-3.5 max-sm:border-t sm:border-s">
              <dt class="text-ui-2xs text-muted-foreground">Problems</dt>
              <dd class={cn('text-ui-2xl leading-none font-semibold tabular-nums', c.failed + c.rolledBack + c.changed ? 'text-destructive' : 'text-foreground')}>{c.failed + c.rolledBack + c.changed}</dd>
            </div>
            <div class="flex min-w-0 flex-col gap-1 border-s border-border/60 px-4 py-3.5 max-sm:border-t">
              <dt class="text-ui-2xs text-muted-foreground">Last applied</dt>
              <dd class="flex min-w-0 flex-col gap-0.5">
                <span class="truncate text-ui-sm font-medium" title={lastApplied?.name ?? ''}>{lastApplied ? splitName(lastApplied.name)[1] : 'Nothing yet'}</span>
                {#if lastApplied}<span class="text-ui-2xs whitespace-nowrap text-muted-foreground" title={when(lastApplied.appliedAt)}>{ago(lastApplied.appliedAt)}</span>{/if}
              </dd>
            </div>
          </dl>
          {#if known}
            <!-- One bar, three parts: applied, problems, pending. -->
            <div class="flex h-1 bg-muted" role="img" aria-label="{c.applied} applied, {c.pending} pending, {c.failed + c.rolledBack} failed">
              <span class="bg-success transition-[width] duration-300" style="width: {(c.applied / list.length) * 100}%"></span>
              <span class="bg-destructive" style="width: {((c.failed + c.rolledBack) / list.length) * 100}%"></span>
              <span class="bg-warning/70" style="width: {(c.pending / list.length) * 100}%"></span>
            </div>
          {/if}
          <div class="flex flex-col gap-2 border-t border-border/60 bg-muted/20 px-4 py-2.5 text-ui-xs text-muted-foreground">
            <span>Read from <code class="font-mono text-foreground/80">{TABLE_NAME[source]}</code>{source === 'd1' ? ', written by Wrangler' : ''}</span>
            {#if known && c.pending}
              <div class="flex items-center gap-2 rounded-md border border-border/60 bg-background py-1 ps-3 pe-1">
                <span class="shrink-0">Apply {c.pending === 1 ? 'it' : `the ${c.pending}`}</span>
                <code class="min-w-0 flex-1 truncate font-mono text-ui-2xs text-foreground [font-variant-ligatures:none]" title={applyCommand}>{applyCommand}</code>
                <Button variant="ghost" size="sm" class="h-7 shrink-0 text-muted-foreground hover:text-foreground" onclick={() => void copy(applyCommand, 'cmd')}>
                  {#if copied === 'cmd'}<Check class="size-3.5 shrink-0 text-success" />Copied{:else}<Copy class="size-3.5 shrink-0" />Copy{/if}
                </Button>
              </div>
            {/if}
          </div>
        </section>

        {#if !known}
          <div class="flex items-center gap-4 rounded-lg border border-dashed border-border px-4 py-3.5">
            <span class="flex size-9 shrink-0 items-center justify-center rounded-md bg-muted/60 text-muted-foreground"><FolderOpen class="size-4 shrink-0" /></span>
            <div class="min-w-0 flex-1">
              <p class="text-ui-sm font-medium">See what hasn't run yet</p>
              <p class="text-ui-xs text-muted-foreground">Pick the project folder (the one with {tool === 'prisma' ? 'prisma/migrations' : 'drizzle/ or migrations/'}). Stroke compares the files with what this database ran, and shows each migration's SQL.</p>
            </div>
            <Button variant="outline" size="sm" onclick={() => void chooseFolder()}>Pick folder</Button>
          </div>
        {/if}

        {#if projectError}
          <p class="flex items-start gap-2 rounded-lg border border-border/60 bg-muted/30 px-3 py-2 text-ui-xs text-muted-foreground" role="alert">
            <TriangleAlert class="mt-0.5 size-3.5 shrink-0 text-warning" />{projectError}
          </p>
        {/if}

        <!-- Find and filter. -->
        <div class="flex flex-wrap items-center gap-2">
          <div class="relative min-w-48 flex-1">
            <Search class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
            <input bind:value={query} placeholder="Find a migration" class="search-box no-field-frame h-8 w-full ps-8 pe-2.5 text-ui-xs" aria-label="Find a migration" />
          </div>
          {#if known}
            <SegmentedControl options={FILTERS} value={filter} ariaLabel="Show" class={FILTERS.length > 3 ? 'w-[26rem] max-w-full' : 'w-80 max-w-full'} onchange={(v) => { filter = v; open = null }} />
          {/if}
        </div>

        {#if !found.length}
          <p class="py-10 text-center text-ui-sm text-muted-foreground">{query.trim() ? `No migration matches "${query.trim()}".` : `Nothing ${filter === 'failed' ? 'needs a look' : filter}.`}</p>
        {:else}
          <div class="flex flex-col gap-5">
            {#each groups as g (g.key)}
              <section aria-label={g.label}>
                <h3 class="mb-1.5 flex items-baseline gap-2 px-1 text-ui-xs font-medium">
                  <span class={g.key === 'pending' ? 'text-warning' : 'text-foreground'}>{g.label}</span>
                  <span class="text-ui-2xs font-normal text-muted-foreground tabular-nums">{g.items.length}</span>
                </h3>
                <ol class="overflow-hidden rounded-lg border border-border/60">
                  {#each g.items as m, i (m.name)}
                    {@const st = STATUS[m.status]}
                    {@const expanded = open === m.name}
                    {@const [seq, label] = splitName(m.name)}
                    <li class="border-b border-border/40 last:border-0">
                      <button
                        type="button"
                        class={cn('group flex h-11 w-full items-center gap-3 pe-3 text-left transition-colors hover:bg-muted/40', expanded && 'bg-muted/30')}
                        aria-expanded={expanded}
                        onclick={() => (open = expanded ? null : m.name)}
                      >
                        <!-- The rail: one line through the group, a dot per migration. -->
                        <span class="relative flex w-8 shrink-0 self-stretch items-center justify-center" aria-hidden="true">
                          <span class={cn('absolute left-1/2 w-px -translate-x-1/2 bg-border', i === 0 ? 'top-1/2' : 'top-0', i === g.items.length - 1 ? 'bottom-1/2' : 'bottom-0')}></span>
                          <span class={cn('relative size-2 rounded-full ring-[3px] ring-panel', st.dot)}></span>
                        </span>
                        {#if seq}<span class="w-12 shrink-0 truncate font-mono text-ui-xs text-muted-foreground tabular-nums" title={seq}>{seq}</span>{/if}
                        <span class="flex min-w-0 flex-1 items-center gap-2">
                          <span class="truncate text-ui-sm" title={m.name}>{label}</span>
                          {#if m.changed}<span class="shrink-0 rounded bg-warning/15 px-1.5 py-px text-ui-2xs text-warning">Edited after it ran</span>{/if}
                          {#if m.missingFile && known}<span class="shrink-0 rounded bg-muted px-1.5 py-px text-ui-2xs text-muted-foreground">Not in folder</span>{/if}
                          {#if m.attempts > 1}<span class="shrink-0 rounded bg-muted px-1.5 py-px text-ui-2xs text-muted-foreground">{m.attempts} attempts</span>{/if}
                        </span>
                        <span class="hidden shrink-0 text-ui-xs whitespace-nowrap text-muted-foreground tabular-nums sm:block" title={when(m.appliedAt ?? m.startedAt)}>
                          {shortDate(m.appliedAt ?? m.startedAt)}
                        </span>
                        <span class={cn('inline-flex h-5 w-[5.5rem] shrink-0 items-center justify-center rounded-full text-ui-2xs font-medium', st.badge)}>{st.label}</span>
                        <ChevronRight class={cn('size-3.5 shrink-0 text-muted-foreground transition-transform duration-150', expanded && 'rotate-90')} />
                      </button>
                      {#if expanded}
                        <div class="flex flex-col gap-3 border-t border-border/40 bg-background/40 py-3 ps-11 pe-4">
                          <p class="flex flex-wrap gap-x-4 gap-y-1 text-ui-2xs text-muted-foreground">
                            <span class="font-mono text-foreground/80">{m.name}</span>
                            {#if m.appliedAt}<span>Applied {when(m.appliedAt)}</span>{/if}
                            {#if m.durationMs != null}<span>Took {took(m.durationMs)}</span>{/if}
                            {#if m.steps != null}<span>{m.steps} step{m.steps === 1 ? '' : 's'}</span>{/if}
                          </p>
                          {#if m.logs}
                            <pre class="max-h-48 overflow-auto rounded-md border border-destructive/30 bg-destructive/5 p-3 font-mono text-ui-2xs whitespace-pre-wrap text-destructive">{m.logs}</pre>
                          {/if}
                          {#if m.sql}
                            <div class="overflow-hidden rounded-md border border-border/60" style="height: {sqlHeight(m.sql)}px">
                              {#await import('./CodeEditor.svelte') then { default: CodeEditor }}
                                <CodeEditor value={m.sql} readOnly lang="sql" ariaLabel="SQL of {m.name}" />
                              {/await}
                            </div>
                            <div class="flex items-center gap-1">
                              {#if onopensql}
                                <Button variant="outline" size="sm" onclick={() => onopensql?.(m.sql ?? '', m.name)}>
                                  <SquareTerminal class="size-3.5 shrink-0" />Open in editor
                                </Button>
                              {/if}
                              <Button variant="ghost" size="sm" class="text-muted-foreground" onclick={() => void copy(m.sql ?? '', m.name)}>
                                {#if copied === m.name}<Check class="size-3.5 shrink-0 text-success" />Copied{:else}<Copy class="size-3.5 shrink-0" />Copy SQL{/if}
                              </Button>
                            </div>
                          {:else if !known}
                            <p class="text-ui-xs text-muted-foreground">Pick the project folder to see this migration's SQL.</p>
                          {:else}
                            <p class="text-ui-xs text-muted-foreground">This migration's file isn't in the project folder.</p>
                          {/if}
                        </div>
                      {/if}
                    </li>
                  {/each}
                </ol>
              </section>
            {/each}
          </div>
        {/if}
      {:else if !loading}
        <p class="py-16 text-center text-ui-sm text-muted-foreground">
          {tool === 'prisma' ? 'No Prisma migrations' : 'No Drizzle migrations'} {rows === null ? 'in this database' : 'yet'}.
        </p>
      {/if}
    </div>
  </div>
</div>
