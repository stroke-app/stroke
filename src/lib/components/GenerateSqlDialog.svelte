<script>
  import { appSqlEditor } from '$lib/stores/settings.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import Icon from './Icon.svelte'
  import ShikiBlock from './ShikiBlock.svelte'
  import { cn } from '$lib/utils.js'
  import { getTableRows, getTableDdl } from '$lib/api.js'
  import { readRowsResponse } from '$lib/table-query.js'
  import {
    genSelectStar,
    genSelectFields,
    genInsert,
    genUpdate,
    genDelete,
    genUpsert,
  } from '$lib/sql-generate.js'

  /**
   * "Generate SQL" - ready-to-paste statement skeletons for a table (create /
   * select / insert / update / delete / upsert), with `:name` placeholders and
   * the same per-engine identifier quoting the grid's own writes use.
   */
  let {
    open = $bindable(false),
    schema = 'public',
    /** @type {string | null} */
    table = null,
    /** @type {import('$lib/dml-preview.js').Dialect} */
    dialect = 'postgres',
    /** @type {(sql: string) => void} */
    onopeninsql = () => {},
  } = $props()

  let loading = $state(false)
  let error = $state('')
  /** @type {Array<{ name: string, dataType?: string }>} */
  let columns = $state([])
  /** @type {string[]} */
  let primaryKey = $state([])
  let ddl = $state('')
  let activeTab = $state('create')
  let requestSeq = 0

  $effect(() => {
    if (!open || !table) return
    const t = table
    const s = schema
    const req = ++requestSeq
    loading = true
    error = ''
    columns = []
    primaryKey = []
    ddl = ''
    Promise.all([
      getTableRows(s, t, 1, 0, { includeCount: false }),
      getTableDdl(s, t).catch(() => ''),
    ])
      .then(([rowsRes, ddlText]) => {
        if (req !== requestSeq) return
        const data = readRowsResponse(rowsRes)
        columns = data.columns
        primaryKey = data.primaryKey
        ddl = typeof ddlText === 'string' ? ddlText.trim() : ''
        activeTab = ddl ? 'create' : 'select'
      })
      .catch((e) => {
        if (req === requestSeq) error = String(e?.message ?? e)
      })
      .finally(() => {
        if (req === requestSeq) loading = false
      })
  })

  const tabs = $derived.by(() => {
    if (!table || columns.length === 0) return []
    // Names follow Settings → SQL editor → Quote object names and Qualify tables.
    const ctx = { dialect, schema, table, columns, primaryKey, quote: $appSqlEditor.quoteNames, qualify: $appSqlEditor.qualifySchema }
    /** @type {Array<{ id: string, label: string, sql: string }>} */
    const out = []
    if (ddl) out.push({ id: 'create', label: 'Create Table', sql: ddl })
    out.push({ id: 'select', label: 'Select *', sql: genSelectStar(ctx) })
    out.push({ id: 'fields', label: 'Select [fields]', sql: genSelectFields(ctx) })
    out.push({ id: 'insert', label: 'Insert', sql: genInsert(ctx) })
    out.push({ id: 'update', label: 'Update', sql: genUpdate(ctx) })
    out.push({ id: 'delete', label: 'Delete', sql: genDelete(ctx) })
    const upsert = genUpsert(ctx)
    if (upsert) out.push({ id: 'upsert', label: 'Upsert', sql: upsert })
    return out
  })

  const currentSql = $derived(
    tabs.find((t) => t.id === activeTab)?.sql ?? tabs[0]?.sql ?? '',
  )

  // Only the DML skeletons carry :name placeholders - the note would be
  // noise (and wrong) on the Create Table / Select tabs.
  const hasPlaceholders = $derived(
    activeTab === 'insert' || activeTab === 'update' || activeTab === 'delete' || activeTab === 'upsert',
  )

  let copied = $state(false)
  /** @type {ReturnType<typeof setTimeout> | null} */
  let copiedTimer = null

  function handleCopy() {
    navigator.clipboard.writeText(currentSql).then(() => {
      copied = true
      if (copiedTimer) clearTimeout(copiedTimer)
      copiedTimer = setTimeout(() => { copied = false }, 2000)
    })
  }

  function handleOpenInEditor() {
    if (!currentSql) return
    onopeninsql(currentSql)
    open = false
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    showCloseButton={false}
    class="w-[min(45rem,calc(100vw-2rem))] gap-0 overflow-hidden rounded-2xl p-0 sm:max-w-none"
  >
    <!-- Header -->
    <div class="flex h-12 items-center gap-2.5 border-b border-border/60 pl-4 pr-2.5">
      <span class="inline-flex size-6 shrink-0 items-center justify-center rounded-md bg-primary/12 text-primary">
        <Icon name="code-2" class="size-3.5" />
      </span>
      <Dialog.Title class="min-w-0 flex-1 truncate text-ui-sm font-medium text-foreground">
        Generate SQL
        <span class="text-muted-foreground">·</span>
        <span class="font-mono text-ui-sm">{table}</span>
      </Dialog.Title>
      <span class="shrink-0 select-none rounded-md bg-muted/60 px-1.5 py-0.5 font-mono text-ui-3xs uppercase tracking-wide text-muted-foreground">{dialect}</span>
      <Dialog.Close
        class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[background-color,color,scale] hover:bg-accent hover:text-foreground active:scale-[0.96]"
        aria-label="Close"
      >
        <Icon name="x" class="size-3.5" />
      </Dialog.Close>
    </div>

    <!-- Statement tabs -->
    <div class="flex items-center gap-1 overflow-x-auto border-b border-border/60 bg-panel px-2.5 py-1.5">
      {#each tabs as t (t.id)}
        <button
          type="button"
          class={cn(
            'shrink-0 rounded-md px-2 py-1 text-ui-xs transition-[background-color,color]',
            activeTab === t.id
              ? 'bg-accent font-medium text-foreground'
              : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground',
          )}
          aria-pressed={activeTab === t.id}
          onclick={() => (activeTab = t.id)}
        >
          {t.label}
        </button>
      {/each}
    </div>

    <!-- SQL body -->
    <div class="flex h-[min(320px,55vh)] min-h-0 flex-col bg-panel">
      {#if loading}
        <div class="flex flex-1 items-center justify-center">
          <Icon name="loader-2" class="size-4 animate-spin text-muted-foreground" />
        </div>
      {:else if error}
        <div class="flex flex-1 items-center justify-center px-6 text-center">
          <p class="font-mono text-ui-xs text-destructive">{error}</p>
        </div>
      {:else if currentSql}
        {#key activeTab}
          <ShikiBlock code={currentSql} lang="sql" nowrap />
        {/key}
      {/if}
    </div>

    <!-- Footer -->
    <div class="flex h-13 items-center gap-2 border-t border-border/60 bg-background px-4">
      <p class="min-w-0 flex-1 truncate text-ui-2xs text-muted-foreground">
        {#if hasPlaceholders}
          Values use <span class="rounded bg-muted/60 px-1 font-mono text-foreground/70">:name</span> placeholders, replace before running.
        {/if}
      </p>
      <button
        type="button"
        class= "field-surface inline-flex h-8 shrink-0 items-center gap-1.5 px-3 text-ui-xs text-muted-foreground transition-[background-color,color,scale] hover:bg-accent hover:text-foreground active:scale-[0.96] disabled:pointer-events-none disabled:opacity-40"
        disabled={!currentSql}
        onclick={handleCopy}
      >
        {#if copied}
          <Icon name="check" class="size-3.5 shrink-0 text-success" />
          Copied
        {:else}
          <Icon name="copy" class="size-3.5 shrink-0" />
          Copy
        {/if}
      </button>
      <button
        type="button"
        class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md bg-primary px-3 text-ui-xs font-medium text-primary-foreground transition-[opacity,scale] hover:opacity-90 active:scale-[0.96] disabled:pointer-events-none disabled:opacity-40"
        disabled={!currentSql}
        onclick={handleOpenInEditor}
      >
        <Icon name="terminal" class="size-3.5 shrink-0" />
        Open in SQL editor
      </button>
    </div>
  </Dialog.Content>
</Dialog.Root>
