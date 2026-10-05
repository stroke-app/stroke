<script>
  // The live database, read as ORM code.
  //
  // The point of this tab is the shape of the schema, not its DDL: model names,
  // the field types you would actually type, and relations spelled out on both
  // sides. It introspects once (three schema-wide calls, no per-table fan-out)
  // and re-renders locally when you switch between Prisma and Drizzle, so the
  // toggle is instant even on a hundred-table schema.
  import { onDestroy } from 'svelte'
  import CodeTextView from './CodeTextView.svelte'
  import Icon from './Icon.svelte'
  import DbIcon from './DbIcon.svelte'
  import TabLoading from './TabLoading.svelte'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { getSchemaColumnStructure, listIndexes, listEnums, listTables, listSchemas, saveExportAs } from '$lib/api.js'
  import {
    buildOrmSchemaModel, renderPrismaSchema, renderDrizzleSchema,
    renderSqlSchema, renderSqlDatabase,
    PRISMA_ENGINES, DRIZZLE_ENGINES, SQL_ENGINES,
  } from '$lib/orm-schema.js'
  import { cn } from '$lib/utils.js'

  let {
    /** Schema to describe. */
    schema = 'public',
    /** Saved-connection type: `postgres`, `mysql`, `sqlite`, … */
    dbType = 'postgres',
    /** Identity of the live connection. Two databases can both be `public` on
     *  Postgres, so this is what tells one session's schema from another's. */
    connectionId = '',
  } = $props()

  const TARGETS = [
    { id: 'prisma',  label: 'Prisma',  engines: PRISMA_ENGINES,  ext: 'prisma', file: 'schema.prisma' },
    { id: 'drizzle', label: 'Drizzle', engines: DRIZZLE_ENGINES, ext: 'ts',     file: 'schema.ts' },
    // SQL describes every engine, because it is the engine's own language.
    { id: 'sql',     label: 'SQL',     engines: SQL_ENGINES,     ext: 'sql',    file: 'schema.sql' },
  ]

  let target = $state(/** @type {'prisma'|'drizzle'|'sql'} */ ('prisma'))
  /** 'schema' = the schema in the sidebar; 'database' = every schema, one script.
   *  Only offered for the SQL target: DDL names are schema-qualified, so two
   *  schemas owning a `users` table produce two statements rather than a clash,
   *  which is not true of a Prisma model or a Drizzle export. */
  const SCOPES = /** @type {const} */ ([
    { id: 'schema', label: 'This schema' },
    { id: 'database', label: 'Whole database' },
  ])
  let scope = $state(/** @type {'schema'|'database'} */ ('schema'))
  /** Models for every schema, loaded only when the database scope is picked. */
  let dbModels = $state.raw(/** @type {import('$lib/orm-schema.js').OrmSchemaModel[]} */ ([]))
  let dbLoading = $state(false)
  // $state.raw: the model is a whole introspected schema - every table, every
  // column, every index - and it is replaced wholesale by `load` and never
  // mutated. Deep $state proxied every one of those objects on a hundred-table
  // database for a value only ever read by the two renderers.
  let model = $state.raw(/** @type {import('$lib/orm-schema.js').OrmSchemaModel | null} */ (null))
  let loading = $state(true)
  let error = $state('')
  let copied = $state(false)
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let copyTimer
  onDestroy(() => clearTimeout(copyTimer))

  /**
   * Arrow-key movement for a roving tablist. Shared by both switches: a
   * `role="tablist"` that cannot be walked with the arrow keys is worse than
   * one that never claimed the role, because it tells assistive tech to expect
   * behaviour that is not there.
   * @param {KeyboardEvent} e
   * @param {readonly {id: string}[]} items
   * @param {string} current
   * @param {(id: string) => void} pick
   */
  function rovingKeydown(e, items, current, pick) {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return
    e.preventDefault()
    const i = items.findIndex((t) => t.id === current)
    const next = items[(i + (e.key === 'ArrowRight' ? 1 : -1) + items.length) % items.length]
    pick(next.id)
    const list = /** @type {HTMLElement} */ (e.currentTarget)
    requestAnimationFrame(() =>
      /** @type {HTMLElement | null} */ (list.querySelector(`[data-tab="${next.id}"]`))?.focus(),
    )
  }

  const active = $derived(TARGETS.find((t) => t.id === target) ?? TARGETS[0])
  const supported = $derived(active.engines.includes(dbType))

  /** The database scope is SQL-only, so anything else falls back to the schema. */
  const effectiveScope = $derived(target === 'sql' ? scope : 'schema')

  const code = $derived.by(() => {
    if (!supported) return ''
    try {
      if (target === 'sql' && effectiveScope === 'database') {
        if (dbLoading) return '-- Reading every schema…\n'
        return renderSqlDatabase(dbModels)
      }
      if (!model) return ''
      if (target === 'sql') return renderSqlSchema(model)
      return target === 'prisma' ? renderPrismaSchema(model) : renderDrizzleSchema(model)
    } catch (e) {
      // A generator failure must not blank the tab - say so in the editor.
      const c = target === 'sql' ? '--' : '//'
      return `${c} Could not render this schema as ${active.label}: ${String(e)}\n`
    }
  })

  const tableCount = $derived(
    effectiveScope === 'database'
      ? dbModels.reduce((n, m) => n + m.tables.length, 0)
      : (model?.tables.length ?? 0),
  )

  const language = $derived(
    target === 'sql' ? 'sql' : target === 'prisma' ? 'prisma' : 'typescript',
  )

  // Every schema, loaded only once the database scope is actually picked -
  // it is N times the introspection of a single schema, and most sessions never
  // ask for it.
  let dbToken = ''
  $effect(() => {
    if (effectiveScope !== 'database') return
    void loadDatabase(`${connectionId} ${dbType}`)
  })

  async function loadDatabase(key) {
    if (key === dbToken) return
    dbToken = key
    dbLoading = true
    error = ''
    try {
      const names = await listSchemas()
      const list = (Array.isArray(names) ? names : [])
        .map((n) => (typeof n === 'string' ? n : n?.name))
        .filter(Boolean)
      const wanted = list.length ? list : [schema]
      const built = []
      // Sequential on purpose: four calls per schema fired at once across
      // twenty schemas is eighty concurrent round trips at the pool, which
      // starves the rest of the app for the duration.
      for (const name of wanted) {
        const [structures, indexes, enums, tables] = await Promise.all([
          getSchemaColumnStructure(name),
          listIndexes(name).catch(() => []),
          listEnums(name).catch(() => []),
          listTables(name).catch(() => []),
        ])
        if (key !== dbToken) return // a newer load started
        built.push(buildOrmSchemaModel({ schema: name, dbType, structures, indexes, enums, tables }))
      }
      if (key !== dbToken) return
      dbModels = built
    } catch (e) {
      if (key !== dbToken) return
      error = String(e)
      dbModels = []
    } finally {
      if (key === dbToken) dbLoading = false
    }
  }

  $effect(() => {
    // Follows the connection, not just the schema name: switching databases
    // usually lands on `public` again, so keying on schema alone left the
    // previous database's models on screen. A schema altered in place is picked
    // up with the Refresh button.
    void load(`${connectionId} ${schema} ${dbType}`)
  })

  let loadToken = ''
  async function load(key) {
    if (key === loadToken) return
    // Drop the previous database's models immediately - showing them under a new
    // connection's name is worse than showing a spinner.
    loadToken = key
    model = null
    loading = true
    error = ''
    try {
      // Enums and the table list are best-effort: engines without them (SQLite,
      // MySQL) must still produce a schema.
      const [structures, indexes, enums, tables] = await Promise.all([
        getSchemaColumnStructure(schema),
        listIndexes(schema).catch(() => []),
        listEnums(schema).catch(() => []),
        listTables(schema).catch(() => []),
      ])
      if (key !== loadToken) return // a newer load started
      model = buildOrmSchemaModel({ schema, dbType, structures, indexes, enums, tables })
    } catch (e) {
      if (key !== loadToken) return
      error = String(e)
      model = null
    } finally {
      if (key === loadToken) loading = false
    }
  }

  async function copyCode() {
    try {
      await navigator.clipboard.writeText(code)
      copied = true
      clearTimeout(copyTimer)
      copyTimer = setTimeout(() => (copied = false), 1600)
    } catch (e) {
      toast.error('Could not copy', { description: String(e) })
    }
  }

  async function saveCode() {
    try {
      const path = await saveExportAs(code, active.file, {
        name: active.label,
        extensions: [active.ext],
      })
      if (!path) return // dialog cancelled
      toast.success(`Exported ${active.label} schema`, { description: `Saved to ${path}` })
    } catch (e) {
      toast.error('Export failed', { description: String(e) })
    }
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border/50 px-3">
    <!-- Target switch: the same schema, in either ORM's dialect.
         rounded-lg (8px) outer against p-0.5 (2px) puts the pills at 6px -
         outer minus padding, so the active pill's corners follow the track's
         curve instead of cutting inside it. h-7 is the design system's compact
         control height; the pills were 24px, under any sane target size and
         visibly shorter than the Refresh/Save/Copy buttons beside them.
         A real tablist, not two buttons: this is the page's primary switch and
         it was invisible to assistive tech and unreachable by arrow key. -->
    <div
      role="tablist"
      aria-label="Code target"
      tabindex="-1"
      class="flex shrink-0 items-center gap-0.5 rounded-lg bg-muted/40 p-0.5"
      onkeydown={(e) => rovingKeydown(e, TARGETS, target, (id) => (target = /** @type {'prisma'|'drizzle'|'sql'} */ (id)))}
    >
      {#each TARGETS as t (t.id)}
        {@const on = target === t.id}
        <button
          type="button"
          role="tab"
          aria-selected={on}
          tabindex={on ? 0 : -1}
          data-tab={t.id}
          onclick={() => (target = /** @type {'prisma'|'drizzle'} */ (t.id))}
          class={cn(
            'inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-ui-xs transition-[color,background-color,box-shadow,scale] duration-150 ease-out active:scale-[0.96] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40',
            on
              ? 'bg-background font-medium text-foreground elevate-1-rim'
              : 'text-muted-foreground hover:text-foreground',
          )}
        >
          <!-- DbIcon carries engine marks; SQL is not an engine, so it takes a
               glyph from the icon set rather than rendering an empty box. -->
          {#if t.id === 'sql'}
            <Icon name="terminal" class="size-3.5 shrink-0" />
          {:else}
            <DbIcon id={t.id} class="size-3.5 shrink-0" />
          {/if}
          {t.label}
        </button>
      {/each}
    </div>

    {#if target === 'sql'}
      <!-- Scope, SQL only. A Prisma model or a Drizzle export named `users`
           can exist once per file; a CREATE TABLE is schema-qualified, so the
           whole database concatenates without colliding. -->
      <div
        role="tablist"
        aria-label="Scope"
        tabindex="-1"
        class="flex shrink-0 items-center gap-0.5 rounded-lg bg-muted/40 p-0.5"
        onkeydown={(e) => rovingKeydown(e, SCOPES, scope, (id) => (scope = /** @type {'schema'|'database'} */ (id)))}
      >
        {#each SCOPES.map((sc) => ({ ...sc, label: sc.id === 'schema' ? schema : sc.label })) as sc (sc.id)}
          {@const on = scope === sc.id}
          <button
            type="button"
            role="tab"
            aria-selected={on}
            tabindex={on ? 0 : -1}
            data-tab={sc.id}
            onclick={() => (scope = /** @type {'schema'|'database'} */ (sc.id))}
            class={cn(
              'inline-flex h-7 max-w-[12rem] items-center rounded-md px-2.5 text-ui-xs transition-[color,background-color,box-shadow,scale] duration-150 ease-out active:scale-[0.96] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40',
              on ? 'bg-background font-medium text-foreground elevate-1-rim' : 'text-muted-foreground hover:text-foreground',
            )}
          ><span class="truncate font-mono">{sc.label}</span></button>
        {/each}
      </div>
    {/if}

    <span class="min-w-0 truncate font-mono text-ui-xs text-muted-foreground">
      {#if effectiveScope === 'database'}
        {dbModels.length} {dbModels.length === 1 ? 'schema' : 'schemas'}
      {:else}
        {schema}
      {/if}
      {#if tableCount}<span class="text-muted-foreground"> · {tableCount} {tableCount === 1 ? 'table' : 'tables'}</span>{/if}
    </span>

    <span class="ml-auto flex shrink-0 items-center gap-1">
      <button
        type="button"
        onclick={() => {
          const stamp = Date.now()
          loadToken = ''
          void load(`${connectionId} ${schema} ${dbType} ${stamp}`)
          if (effectiveScope === 'database') { dbToken = ''; void loadDatabase(`${connectionId} ${dbType} ${stamp}`) }
        }}
        title="Read the schema again"
        class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      >
        <Icon name="refresh-cw" class={cn('size-3.5 shrink-0', (loading || dbLoading) && 'animate-spin')} />
        Refresh
      </button>
      <button
        type="button"
        onclick={saveCode}
        disabled={!code}
        title={`Save as ${active.file}`}
        class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40"
      >
        <Icon name="download" class="size-3.5 shrink-0" />
        Save
      </button>
      <button
        type="button"
        onclick={copyCode}
        disabled={!code}
        title={`Copy ${active.label} schema`}
        class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40"
      >
        <Icon name={copied ? 'check' : 'copy'} class="size-3.5 shrink-0" />
        {copied ? 'Copied' : 'Copy'}
      </button>
    </span>
  </div>

  {#if error}
    <div class="flex min-h-0 flex-1 items-center justify-center p-8">
      <div class="max-w-md text-center">
        <Icon name="alert-circle" class="mx-auto size-5 text-destructive" />
        <p class="mt-2 text-ui-sm text-foreground">Couldn't read this schema</p>
        <p class="mt-1 text-ui-xs text-muted-foreground">{error}</p>
      </div>
    </div>
  {:else if !supported}
    <div class="flex min-h-0 flex-1 items-center justify-center p-8">
      <div class="max-w-md text-center">
        {#if active.id === 'sql'}
          <Icon name="terminal" class="mx-auto size-6 text-muted-foreground" />
        {:else}
          <DbIcon id={active.id} class="mx-auto size-6 text-muted-foreground" />
        {/if}
        <p class="mt-2 text-ui-sm text-foreground">{active.label} doesn't support this engine</p>
        <p class="mt-1 text-ui-xs text-muted-foreground">
          {active.label} has no {dbType} driver, so there's no schema to write.
          {#if TARGETS.some((t) => t.id !== active.id && t.engines.includes(dbType))}
            Switch to {TARGETS.find((t) => t.id !== active.id)?.label} above.
          {/if}
        </p>
      </div>
    </div>
  {:else if effectiveScope === 'database' ? (dbLoading && !dbModels.length) : (loading && !model)}
    <TabLoading />
  {:else if tableCount === 0}
    <div class="flex min-h-0 flex-1 items-center justify-center p-8">
      <p class="text-ui-sm text-muted-foreground">No tables in <span class="font-mono">{schema}</span>.</p>
    </div>
  {:else}
    <CodeTextView text={code} {language} wordWrap="off" />
  {/if}
</div>
