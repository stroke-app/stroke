<script>
  // Prisma and Drizzle migrations for the open database: what ran (from the
  // tool's own table), what failed, and what's still pending (from the
  // project's migrations folder, picked once per connection). Read-only:
  // applying stays with the tool's CLI. Logic: $lib/migrations.js.
  import { invoke } from '@tauri-apps/api/core'
  import { executeSql } from '$lib/api.js'
  import { mergePrisma, mergeDrizzle, counts, rowsOf, migrationQueries, isMissingTable } from '$lib/migrations.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import SegmentedControl from './SegmentedControl.svelte'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { cn } from '$lib/utils.js'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import FolderOpen from '@lucide/svelte/icons/folder-open'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import SquareTerminal from '@lucide/svelte/icons/square-terminal'
  import Copy from '@lucide/svelte/icons/copy'
  import GitMerge from '@lucide/svelte/icons/git-merge'
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert'

  let {
    /** @type {{ id: string, name?: string } | null} */
    connection = null,
    /** Engine family: postgres | mysql | sqlite | ... */
    family = 'postgres',
    /** The schema the sidebar is on (Prisma's table lives in the connection's schema). */
    schema = /** @type {string | null} */ (null),
    /** The tab is on screen: load then, not at mount. */
    active = true,
    /** Open SQL in a new editor tab. @type {((sql: string, title: string) => void) | null} */
    onopensql = null,
  } = $props()

  /** @typedef {import('$lib/migrations.js').Migration} Migration */
  /** @typedef {{ tool: 'prisma' | 'drizzle', dir: string, files: import('$lib/migrations.js').MigrationFile[] }} Project */

  const dirKey = $derived(connection ? `stroke:migrations-dir:${connection.id}` : '')
  const toolKey = $derived(connection ? `stroke:migrations-tool:${connection.id}` : '')

  /** @type {'prisma' | 'drizzle'} */
  let tool = $state('prisma')
  let projectDir = $state('')
  /** @type {Project | null} */
  let project = $state(null)
  /** Rows from each tool's table; null = the table isn't in this database. @type {Record<string, Record<string, any>[] | null>} */
  let tables = $state({ prisma: null, drizzle: null })
  let loading = $state(false)
  let error = $state('')
  let projectError = $state('')
  /** @type {string | null} */
  let open = $state(null)
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

  /** One tool's table, or null when it isn't there. @param {'prisma' | 'drizzle'} t */
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
      tables = { prisma: p, drizzle: d }
      // Show the tool this database actually uses, unless one was picked.
      let picked = null
      try { picked = localStorage.getItem(toolKey) } catch { /* ignore */ }
      if (!picked) tool = p && !d ? 'prisma' : d && !p ? 'drizzle' : tool
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
      toast.error('Could not open the folder', { description: String(e) })
    }
  }

  /** @param {'prisma' | 'drizzle'} t */
  function setTool(t) {
    tool = t
    try { localStorage.setItem(toolKey, t) } catch { /* not kept */ }
    void loadProject()
  }

  const files = $derived(project && project.tool === tool ? project.files : null)
  const rows = $derived(tables[tool])
  /** @type {Migration[]} */
  const list = $derived(rows === null && !files ? [] : tool === 'prisma' ? mergePrisma(rows ?? [], files) : mergeDrizzle(rows ?? [], files))
  const c = $derived(counts(list))
  const neither = $derived(!loading && !error && tables.prisma === null && tables.drizzle === null && !files)

  const STATUS = {
    applied: { label: 'Applied', dot: 'bg-success' },
    pending: { label: 'Pending', dot: 'bg-muted-foreground/40 ring-1 ring-inset ring-muted-foreground/60' },
    failed: { label: 'Failed', dot: 'bg-destructive' },
    'rolled-back': { label: 'Rolled back', dot: 'bg-warning' },
  }

  /** @param {number | null} ms */
  const when = (ms) => (ms ? new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }) : '')
  /** @param {number | null} ms */
  const took = (ms) => (ms == null ? '' : ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`)
  /** The folder's last two parts, for the header. @param {string} p */
  const shortDir = (p) => p.split(/[\\/]/).filter(Boolean).slice(-2).join('/')

  /** @param {string} text */
  async function copy(text) {
    try {
      await navigator.clipboard.writeText(text)
      toast.success('Copied')
    } catch {
      toast.error('Could not copy')
    }
  }
</script>

<div class="flex min-h-0 flex-1 flex-col bg-panel">
  <!-- Header: the tool, the counts, the folder, refresh. -->
  <div class="studio-chrome flex h-12 shrink-0 items-center gap-3 border-b border-border/60 px-4" data-studio-chrome>
    <SegmentedControl
      options={[{ value: 'prisma', label: 'Prisma' }, { value: 'drizzle', label: 'Drizzle' }]}
      value={tool}
      ariaLabel="Migration tool"
      class="w-44 shrink-0"
      onchange={(v) => setTool(v)}
    />
    {#if list.length}
      <p class="flex items-center gap-3 text-ui-xs text-muted-foreground tabular-nums">
        <span><span class="text-foreground">{c.applied}</span> applied</span>
        {#if files}<span><span class={cn(c.pending ? 'text-foreground' : '')}>{c.pending}</span> pending</span>{/if}
        {#if c.failed}<span class="text-destructive">{c.failed} failed</span>{/if}
        {#if c.rolledBack}<span class="text-warning">{c.rolledBack} rolled back</span>{/if}
      </p>
    {/if}
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
    <div class="mx-auto flex w-full max-w-4xl flex-col gap-4 px-6 py-6">
      {#if projectError}
        <p class="flex items-start gap-2 rounded-lg border border-border/60 bg-muted/30 px-3 py-2 text-ui-xs text-muted-foreground" role="alert">
          <TriangleAlert class="mt-0.5 size-3.5 shrink-0 text-warning" />{projectError}
        </p>
      {:else if !projectDir && list.length}
        <p class="text-ui-xs text-muted-foreground">
          Showing what ran. <button type="button" class="text-foreground underline-offset-2 hover:underline" onclick={() => void chooseFolder()}>Pick the project folder</button> to see pending migrations too.
        </p>
      {/if}

      {#if error}
        <div class="flex flex-col items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-4 py-3" role="alert">
          <p class="text-ui-sm font-medium text-destructive">Could not read the migrations table</p>
          <p class="text-ui-xs break-words text-muted-foreground">{error}</p>
        </div>
      {:else if neither}
        <div class="flex flex-col items-center gap-3 py-20 text-center">
          <span class="flex size-10 items-center justify-center rounded-lg bg-muted/50 text-muted-foreground"><GitMerge class="size-5 shrink-0" /></span>
          <div class="flex max-w-sm flex-col gap-1">
            <p class="text-ui-sm font-medium text-foreground">No migrations in this database yet</p>
            <p class="text-ui-xs text-muted-foreground">Neither <code class="font-mono">_prisma_migrations</code> nor <code class="font-mono">__drizzle_migrations</code> is here. Pick the project folder to see what would run.</p>
          </div>
          <Button variant="secondary" size="sm" onclick={() => void chooseFolder()}><FolderOpen class="size-3.5 shrink-0" />Pick project folder</Button>
        </div>
      {:else if !loading && !list.length}
        <p class="py-16 text-center text-ui-sm text-muted-foreground">
          {tool === 'prisma' ? 'No Prisma migrations' : 'No Drizzle migrations'} {rows === null ? 'in this database' : 'yet'}.
        </p>
      {:else}
        <ol class="flex flex-col overflow-hidden rounded-lg border border-border/60">
          {#each list as m (m.name)}
            {@const st = STATUS[m.status]}
            {@const expanded = open === m.name}
            <li class="border-b border-border/40 last:border-0">
              <button
                type="button"
                class="flex w-full items-center gap-3 px-4 py-2.5 text-left transition-colors hover:bg-muted/40"
                aria-expanded={expanded}
                onclick={() => (open = expanded ? null : m.name)}
              >
                <span class={cn('size-2 shrink-0 rounded-full', st.dot)} aria-hidden="true"></span>
                <span class="min-w-0 flex-1">
                  <span class="block truncate font-mono text-ui-sm text-foreground">{m.name}</span>
                  {#if m.changed || m.missingFile || m.attempts > 1}
                    <span class="mt-0.5 flex flex-wrap gap-x-3 text-ui-2xs">
                      {#if m.changed}<span class="text-warning">The file changed after it ran</span>{/if}
                      {#if m.missingFile}<span class="text-muted-foreground">Not in the project folder</span>{/if}
                      {#if m.attempts > 1}<span class="text-muted-foreground">{m.attempts} attempts</span>{/if}
                    </span>
                  {/if}
                </span>
                <span class={cn('shrink-0 text-ui-xs', m.status === 'failed' ? 'text-destructive' : m.status === 'pending' ? 'text-foreground' : 'text-muted-foreground')}>{st.label}</span>
                <span class="hidden w-44 shrink-0 text-right text-ui-xs text-muted-foreground tabular-nums sm:block">
                  {m.appliedAt ? when(m.appliedAt) : m.startedAt ? when(m.startedAt) : ''}
                </span>
                <ChevronRight class={cn('size-3.5 shrink-0 text-muted-foreground transition-transform duration-150', expanded && 'rotate-90')} />
              </button>
              {#if expanded}
                <div class="flex flex-col gap-3 bg-background/40 px-4 pt-1 pb-4">
                  {#if m.durationMs != null || m.steps != null}
                    <p class="text-ui-xs text-muted-foreground tabular-nums">
                      {[m.durationMs != null ? `Took ${took(m.durationMs)}` : '', m.steps != null ? `${m.steps} step${m.steps === 1 ? '' : 's'} applied` : ''].filter(Boolean).join(' · ')}
                    </p>
                  {/if}
                  {#if m.logs}
                    <pre class="max-h-48 overflow-auto rounded-md border border-destructive/30 bg-destructive/5 p-3 font-mono text-ui-2xs whitespace-pre-wrap text-destructive">{m.logs}</pre>
                  {/if}
                  {#if m.sql}
                    <div class="overflow-hidden rounded-md border border-border/60" style="height: {Math.min(256, (m.sql.split('\n').length + 1) * 22 + 8)}px">
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
                      <Button variant="ghost" size="sm" class="text-muted-foreground" onclick={() => void copy(m.sql ?? '')}>
                        <Copy class="size-3.5 shrink-0" />Copy SQL
                      </Button>
                    </div>
                  {:else if !files}
                    <p class="text-ui-xs text-muted-foreground">Pick the project folder to see this migration's SQL.</p>
                  {/if}
                </div>
              {/if}
            </li>
          {/each}
        </ol>
      {/if}
    </div>
  </div>
</div>
