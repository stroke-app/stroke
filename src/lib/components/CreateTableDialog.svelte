<script>
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { getTableColumnStructure } from '$lib/api.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import FieldSelect from './FieldSelect.svelte'
  import PickerField from './PickerField.svelte'
  import DdlDialogShell from './DdlDialogShell.svelte'
  import {
    buildTableSql, validateTable, tableFamily, typeDef, indexName, fkActions,
    COLUMN_TYPES, DEFAULT_SUGGESTIONS, hasPlaceholder, placeholderText, toSnippet,
  } from '$lib/ddl-builders.js'
  import { cn } from '$lib/utils.js'
  import Plus from '@lucide/svelte/icons/plus'
  import X from '@lucide/svelte/icons/x'
  import GripVertical from '@lucide/svelte/icons/grip-vertical'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import MessageSquare from '@lucide/svelte/icons/message-square'
  import ArrowRight from '@lucide/svelte/icons/arrow-right'

  let {
    open = $bindable(false),
    activeSchema = 'public',
    /** 'postgres' | 'sqlite' | 'mysql' | 'd1' */
    dbType = 'postgres',
    saving = false,
    /** Existing tables, for foreign keys. @type {{ name: string, kind?: string }[]} */
    tables = [],
    /** @param {string} sql */
    onexecute = /** @type {(sql: string) => Promise<void>} */ (async () => {}),
    /** @param {string} tableName */
    oncreated = /** @type {(name: string) => void} */ (() => {}),
    /** Opens SQL in a new editor tab. @type {((spec: { text: string, title: string, snippet?: string }) => void) | null} */
    onopensql = null,
  } = $props()

  /**
   * @typedef {import('$lib/ddl-builders.js').ColDef & { showComment?: boolean }} ColRow
   * @typedef {import('$lib/ddl-builders.js').FkDef} FkRow
   * @typedef {import('$lib/ddl-builders.js').IndexDef} IndexRow
   */

  const family = $derived(tableFamily(dbType))
  const typeItems = $derived(COLUMN_TYPES[family].map((t) => ({
    value: t.name, group: t.group, hint: t.params === 2 ? '(p, s)' : t.params === 1 ? '(n)' : '',
  })))
  const canComment = $derived(family !== 'sqlite')
  const actions = $derived(fkActions(family).map((a) => ({ value: a, label: a === 'NO ACTION' ? 'No action' : a[0] + a.slice(1).toLowerCase() })))

  const uid = () => crypto.randomUUID()

  /** @param {string} name @param {string} type @param {Partial<ColRow>} [rest] @returns {ColRow} */
  function col(name, type, rest = {}) {
    const def = typeDef(family, type)
    return { id: uid(), name, type, length: def?.len ?? '', scale: def?.scale ?? '', pk: false, notNull: false, unique: false, defaultVal: '', comment: '', ...rest }
  }

  function makeDefaultCols() {
    /** @type {ColRow[]} */
    const cols = [
      family === 'sqlite' ? col('id', 'INTEGER', { pk: true, notNull: true })
        : family === 'mysql' ? col('id', 'INT', { pk: true, notNull: true, defaultVal: 'AUTO_INCREMENT' })
        : col('id', 'serial', { pk: true, notNull: true }),
    ]
    if (family === 'postgres') cols.push(col('created_at', 'timestamptz', { notNull: true, defaultVal: 'now()' }))
    return cols
  }

  let tableName = $state('')
  let tableComment = $state('')
  let submitting = $state(false)
  let attempted = $state(false)
  let /** @type {ColRow[]} */ cols = $state(makeDefaultCols())
  let /** @type {FkRow[]} */ fks = $state([])
  let /** @type {IndexRow[]} */ indexes = $state([])
  /** @type {Record<string, string[]>} */
  let refColumns = $state({})

  $effect(() => {
    if (!open) {
      tableName = ''; tableComment = ''
      cols = makeDefaultCols(); fks = []; indexes = []
      submitting = false; attempted = false
    }
  })

  const namedCols = $derived(cols.map((c) => c.name.trim()).filter(Boolean))

  // Foreign keys and indexes name columns; drop names a rename or removal took away.
  const liveFks = $derived(fks.map((f) => (f.column && !namedCols.includes(f.column) ? { ...f, column: '' } : f)))
  const liveIndexes = $derived(indexes.map((ix) => ({ ...ix, columns: ix.columns.filter((c) => namedCols.includes(c)) })))

  const spec = $derived({
    dbType, schema: activeSchema, name: tableName, columns: cols,
    foreignKeys: liveFks, indexes: liveIndexes, comment: canComment ? tableComment : '',
  })
  const statements = $derived(buildTableSql(spec))
  const sql = $derived(statements.join('\n\n'))
  const errors = $derived(validateTable(spec))
  const canSubmit = $derived(
    Object.keys(errors).length === 0 && !hasPlaceholder(sql) && !submitting && !saving,
  )
  /** @param {string} k */
  const err = (k) => (attempted || k !== 'name' ? errors[k] ?? '' : '')

  // Columns of each referenced table, fetched once per table.
  $effect(() => {
    if (!open) return
    for (const f of fks) {
      const t = f.refTable
      if (!t || t === tableName.trim() || refColumns[t]) continue
      refColumns = { ...refColumns, [t]: [] }
      getTableColumnStructure(activeSchema, t)
        .then((/** @type {any[]} */ rows) => { refColumns = { ...refColumns, [t]: (rows ?? []).map((c) => c.name) } })
        .catch(() => {})
    }
  })

  /** @param {string} t */
  const refColsFor = (t) => (t && t === tableName.trim() ? namedCols : refColumns[t] ?? [])

  // ── Columns ──
  function addCol() {
    const type = family === 'sqlite' ? 'TEXT' : family === 'mysql' ? 'VARCHAR' : 'text'
    cols = [...cols, col('', type)]
    requestAnimationFrame(() => {
      const inputs = document.querySelectorAll('[data-col-name]')
      ;/** @type {HTMLInputElement | undefined} */ (inputs[inputs.length - 1])?.focus()
    })
  }
  /** @param {string} id */
  const removeCol = (id) => { cols = cols.filter((c) => c.id !== id) }
  /** @param {string} id @param {Partial<ColRow>} patch */
  const patchCol = (id, patch) => { cols = cols.map((c) => (c.id === id ? { ...c, ...patch } : c)) }
  /** @param {string} id @param {string} type */
  function setType(id, type) {
    const def = typeDef(family, type)
    patchCol(id, { type, length: def?.len ?? '', scale: def?.scale ?? '' })
  }
  /** @param {number} from @param {number} to */
  function moveCol(from, to) {
    if (to < 0 || to >= cols.length || from === to) return
    const next = [...cols]
    const [c] = next.splice(from, 1)
    next.splice(to, 0, c)
    cols = next
  }

  let dragIndex = $state(-1)
  let dropIndex = $state(-1)

  // ── Foreign keys / indexes ──
  function addFk() {
    fks = [...fks, { id: uid(), column: '', refTable: '', refColumn: '', onDelete: 'NO ACTION', onUpdate: 'NO ACTION' }]
  }
  /** @param {string} id @param {Partial<FkRow>} patch */
  const patchFk = (id, patch) => { fks = fks.map((f) => (f.id === id ? { ...f, ...patch } : f)) }
  function addIndex() { indexes = [...indexes, { id: uid(), columns: [], unique: false }] }
  /** @param {string} id @param {string} c */
  function toggleIndexCol(id, c) {
    indexes = indexes.map((ix) => ix.id !== id ? ix
      : { ...ix, columns: ix.columns.includes(c) ? ix.columns.filter((x) => x !== c) : [...ix.columns, c] })
  }

  async function handleCreate() {
    attempted = true
    if (!canSubmit) return
    const name = tableName.trim()
    const [create, ...rest] = statements
    submitting = true
    try {
      try {
        await onexecute(create)
      } catch (e) {
        toast.error('Could not create table', { description: String(e) })
        return
      }
      // Comments and indexes run after the table exists, one at a time: not
      // every engine takes several statements in one call.
      const failed = []
      for (const stmt of rest) {
        try { await onexecute(stmt) } catch (e) { failed.push(String(e)) }
      }
      if (failed.length) toast.warning(`Table "${name}" created, ${failed.length} follow-up statement${failed.length > 1 ? 's' : ''} failed`, { description: failed[0] })
      else toast.success(`Table "${name}" created`)
      oncreated(name)
      open = false
    } finally {
      submitting = false
    }
  }

  function openInEditor() {
    onopensql?.({
      text: placeholderText(sql),
      title: tableName.trim() ? `Create · ${tableName.trim()}` : 'New table',
      snippet: hasPlaceholder(sql) ? toSnippet(sql) : undefined,
    })
    open = false
  }

  const lbl = 'text-ui-xs text-muted-foreground'
  const errCls = 'text-ui-2xs text-destructive'
  const cell = 'no-field-frame h-7 w-full min-w-0 rounded-md bg-transparent px-2 font-mono text-ui-sm text-foreground outline-none placeholder:text-muted-foreground hover:bg-accent/60 focus:bg-accent'
  const flag = (/** @type {boolean} */ on) => cn(
    'inline-flex h-6 min-w-7 items-center justify-center rounded-md px-1 font-mono text-ui-2xs transition-colors',
    on ? 'bg-primary/15 font-medium text-foreground ring-1 ring-primary/30' : 'text-muted-foreground hover:bg-accent hover:text-foreground',
  )
  const iconBtn = 'hit-area inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[color,background-color,scale] duration-[120ms] ease-[var(--ease-out)] active:scale-[0.97] hover:bg-accent hover:text-foreground'
  const sectionHead = 'flex items-center gap-2'
  const grid = 'grid grid-cols-[1.25rem_minmax(0,1fr)_minmax(0,1.15fr)_auto_minmax(0,0.9fr)_3.25rem] items-center gap-1'
</script>

<DdlDialogShell
  bind:open
  title="New table"
  subtitle={family === 'sqlite' ? '' : `in ${activeSchema}`}
  {sql}
  {canSubmit}
  busy={submitting || saving}
  submitLabel="Create table"
  size="sm:max-w-3xl"
  onsubmit={handleCreate}
  onopensql={onopensql ? openInEditor : null}
>
  <div class={cn('grid gap-3', canComment && 'grid-cols-2')}>
    <div class="flex min-w-0 flex-col gap-1.5">
      <label class={lbl} for="create-table-name">Name</label>
      <!-- svelte-ignore a11y_autofocus -->
      <Input id="create-table-name" class="h-8 font-mono" placeholder="users" bind:value={tableName} autofocus
        aria-invalid={!!err('name') || undefined} />
      {#if err('name')}<p class={errCls}>{err('name')}</p>{/if}
    </div>
    {#if canComment}
      <div class="flex min-w-0 flex-col gap-1.5">
        <label class={lbl} for="create-table-comment">Comment <span class="text-muted-foreground">(optional)</span></label>
        <Input id="create-table-comment" class="h-8" placeholder="What this table holds" bind:value={tableComment} />
      </div>
    {/if}
  </div>

  <!-- Columns -->
  <section class="flex flex-col gap-1" aria-labelledby="create-table-cols">
    <div class={sectionHead}>
      <span class="text-ui-sm font-medium text-foreground" id="create-table-cols">Columns</span>
      <span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">{cols.length}</span>
      <Button variant="ghost" size="sm" class="ml-auto text-muted-foreground" onclick={addCol}>
        <Plus class="size-3.5 shrink-0" />Add column
      </Button>
    </div>
    {#if err('columns')}<p class={errCls}>{err('columns')}</p>{/if}
    <div class="{grid} px-0.5 text-ui-2xs text-muted-foreground" aria-hidden="true">
      <span></span><span class="px-2">Name</span><span class="px-2">Type</span>
      <span class="flex gap-0.5"><span class="w-7 text-center">PK</span><span class="w-7 text-center">NN</span><span class="w-7 text-center">UQ</span></span>
      <span class="px-2">Default</span><span></span>
    </div>
    <div class="flex flex-col" role="list">
      {#each cols as c, i (c.id)}
        {@const def = typeDef(family, c.type)}
        <div
          role="listitem"
          class={cn('group flex flex-col rounded-md px-0.5 py-0.5 transition-colors hover:bg-muted/30',
            dragIndex === i && 'opacity-50',
            dropIndex === i && dragIndex !== -1 && dragIndex !== i && (dropIndex < dragIndex ? 'shadow-[inset_0_2px_0_var(--ring)]' : 'shadow-[inset_0_-2px_0_var(--ring)]'))}
          ondragover={(e) => { if (dragIndex < 0) return; e.preventDefault(); dropIndex = i }}
          ondrop={(e) => { e.preventDefault(); moveCol(dragIndex, i); dragIndex = -1; dropIndex = -1 }}
        >
          <div class={grid}>
            <button type="button" draggable="true"
              class="flex h-7 w-5 cursor-grab items-center justify-center rounded-md text-muted-foreground opacity-60 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 active:cursor-grabbing"
              aria-label="Move column {c.name || i + 1} (Alt+Up / Alt+Down)"
              title="Drag to reorder · Alt+↑/↓"
              ondragstart={(e) => { dragIndex = i; e.dataTransfer?.setData('text/plain', c.id) }}
              ondragend={() => { dragIndex = -1; dropIndex = -1 }}
              onkeydown={(e) => {
                if (!e.altKey || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return
                e.preventDefault()
                const to = e.key === 'ArrowUp' ? i - 1 : i + 1
                moveCol(i, to)
                requestAnimationFrame(() => /** @type {HTMLElement | null} */ (document.querySelector(`[data-grip="${c.id}"]`))?.focus())
              }}
              data-grip={c.id}
            ><GripVertical class="size-3.5 shrink-0" /></button>

            <input class={cell} data-col-name placeholder="column_name" aria-label="Column {i + 1} name"
              bind:value={c.name} autocomplete="off" spellcheck="false" aria-invalid={!!(errors[c.id] && (attempted || c.name.trim())) || undefined} />

            <div class="flex min-w-0 items-center gap-0.5">
              <div class="min-w-0 flex-1">
                <PickerField size="sm" mono value={c.type} items={typeItems} search="Search types…" empty="No matching type"
                  aria-label="Column {i + 1} type" onchange={(v) => setType(c.id, v)} />
              </div>
              {#if def?.params}
                <span class="text-muted-foreground">(</span>
                <input class="{cell} w-10 px-1 text-center tabular-nums" inputmode="numeric" aria-label="Column {i + 1} {def.params === 2 ? 'precision' : 'length'}"
                  placeholder={def.params === 2 ? 'p' : 'n'} bind:value={c.length} />
                {#if def.params === 2}
                  <span class="text-muted-foreground">,</span>
                  <input class="{cell} w-8 px-1 text-center tabular-nums" inputmode="numeric" aria-label="Column {i + 1} scale" placeholder="s" bind:value={c.scale} />
                {/if}
                <span class="text-muted-foreground">)</span>
              {/if}
            </div>

            <div class="flex gap-0.5">
              <button type="button" class={flag(c.pk)} aria-pressed={c.pk} title="Primary key" aria-label="Column {i + 1} primary key"
                onclick={() => patchCol(c.id, { pk: !c.pk, notNull: !c.pk ? true : c.notNull })}>PK</button>
              <button type="button" class={flag(c.notNull)} aria-pressed={c.notNull} title="Not null" aria-label="Column {i + 1} not null"
                onclick={() => patchCol(c.id, { notNull: !c.notNull })}>NN</button>
              <button type="button" class={flag(!!c.unique)} aria-pressed={!!c.unique} title="Unique" aria-label="Column {i + 1} unique"
                onclick={() => patchCol(c.id, { unique: !c.unique })}>UQ</button>
            </div>

            <div class="flex min-w-0 items-center">
              <input class={cell} placeholder="none" aria-label="Column {i + 1} default" bind:value={c.defaultVal} autocomplete="off" spellcheck="false" />
              <DropdownMenu.Root>
                <DropdownMenu.Trigger>
                  {#snippet child({ props })}
                    <button {...props} type="button" class={iconBtn} aria-label="Default suggestions"><ChevronDown class="size-3.5 shrink-0" /></button>
                  {/snippet}
                </DropdownMenu.Trigger>
                <DropdownMenu.Content align="end" class="z-[130] min-w-40">
                  {#each DEFAULT_SUGGESTIONS[family] as s (s)}
                    <DropdownMenu.Item class="font-mono" onSelect={() => patchCol(c.id, { defaultVal: s })}>{s}</DropdownMenu.Item>
                  {/each}
                  {#if c.defaultVal}
                    <DropdownMenu.Separator />
                    <DropdownMenu.Item onSelect={() => patchCol(c.id, { defaultVal: '' })}>No default</DropdownMenu.Item>
                  {/if}
                </DropdownMenu.Content>
              </DropdownMenu.Root>
            </div>

            <div class="flex items-center justify-end gap-0.5">
              {#if canComment}
                <button type="button" class={cn(iconBtn, (c.showComment || c.comment) && 'text-foreground')}
                  aria-label="Column {i + 1} comment" aria-pressed={!!c.showComment} title="Comment"
                  onclick={() => patchCol(c.id, { showComment: !c.showComment })}><MessageSquare class="size-3.5 shrink-0" /></button>
              {/if}
              <button type="button" class="{iconBtn} hover:text-destructive" aria-label="Remove column {c.name || i + 1}" title="Remove column"
                onclick={() => removeCol(c.id)}><X class="size-3.5 shrink-0" /></button>
            </div>
          </div>
          {#if canComment && (c.showComment || c.comment)}
            <div class="ml-6 pr-14">
              <input class="{cell} font-sans text-ui-xs" placeholder="Column comment" aria-label="Column {i + 1} comment" bind:value={c.comment} />
            </div>
          {/if}
          {#if errors[c.id] && (attempted || c.name.trim())}<p class="ml-8 {errCls}">{errors[c.id]}</p>{/if}
        </div>
      {/each}
    </div>
  </section>

  <!-- Foreign keys -->
  <section class="flex flex-col gap-1" aria-labelledby="create-table-fks">
    <div class={sectionHead}>
      <span class="text-ui-sm font-medium text-foreground" id="create-table-fks">Foreign keys</span>
      {#if fks.length}<span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">{fks.length}</span>{/if}
      <Button variant="ghost" size="sm" class="ml-auto text-muted-foreground" onclick={addFk}>
        <Plus class="size-3.5 shrink-0" />Add foreign key
      </Button>
    </div>
    {#each liveFks as f, i (f.id)}
      <div class="flex flex-col gap-1 rounded-md px-0.5 py-0.5 hover:bg-muted/30">
        <div class="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)_minmax(0,1fr)_minmax(0,0.8fr)_minmax(0,0.8fr)_1.5rem] items-center gap-1">
          <PickerField size="sm" mono value={f.column} placeholder="Column" search="Search columns…" empty="Name a column first"
            aria-label="Foreign key {i + 1} column" items={namedCols.map((n) => ({ value: n }))} onchange={(v) => patchFk(f.id, { column: v })} />
          <ArrowRight class="size-3.5 shrink-0 text-muted-foreground" />
          <PickerField size="sm" mono value={f.refTable} placeholder="Table" search="Search tables…" empty="No matching table"
            aria-label="Foreign key {i + 1} referenced table"
            items={[...(tableName.trim() ? [{ value: tableName.trim(), hint: 'this table' }] : []), ...tables.filter((t) => !t.kind || t.kind === 'table').map((t) => ({ value: t.name }))]}
            onchange={(v) => patchFk(f.id, { refTable: v, refColumn: '' })} />
          <PickerField size="sm" mono value={f.refColumn} placeholder="Column" search="Search columns…"
            empty={f.refTable ? 'No matching column' : 'Pick a table first'} aria-label="Foreign key {i + 1} referenced column"
            items={refColsFor(f.refTable).map((n) => ({ value: n }))} onchange={(v) => patchFk(f.id, { refColumn: v })} />
          <FieldSelect size="sm" class="h-7 w-full text-ui-xs" value={f.onDelete} options={actions} format={(v) => `On delete: ${actions.find((a) => a.value === v)?.label ?? v}`}
            aria-label="Foreign key {i + 1} on delete" onchange={(v) => patchFk(f.id, { onDelete: v })} />
          <FieldSelect size="sm" class="h-7 w-full text-ui-xs" value={f.onUpdate} options={actions} format={(v) => `On update: ${actions.find((a) => a.value === v)?.label ?? v}`}
            aria-label="Foreign key {i + 1} on update" onchange={(v) => patchFk(f.id, { onUpdate: v })} />
          <button type="button" class="{iconBtn} hover:text-destructive" aria-label="Remove foreign key {i + 1}"
            onclick={() => (fks = fks.filter((x) => x.id !== f.id))}><X class="size-3.5 shrink-0" /></button>
        </div>
        {#if errors[f.id] && attempted}<p class={errCls}>{errors[f.id]}</p>{/if}
      </div>
    {/each}
  </section>

  <!-- Indexes -->
  <section class="flex flex-col gap-1" aria-labelledby="create-table-idx">
    <div class={sectionHead}>
      <span class="text-ui-sm font-medium text-foreground" id="create-table-idx">Indexes</span>
      {#if indexes.length}<span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">{indexes.length}</span>{/if}
      <Button variant="ghost" size="sm" class="ml-auto text-muted-foreground" onclick={addIndex}>
        <Plus class="size-3.5 shrink-0" />Add index
      </Button>
    </div>
    {#each liveIndexes as ix, i (ix.id)}
      <div class="flex flex-col gap-1 rounded-md px-0.5 py-1 hover:bg-muted/30">
        <div class="flex items-start gap-2">
          <div class="flex min-w-0 flex-1 flex-wrap gap-1" role="group" aria-label="Index {i + 1} columns">
            {#each namedCols as n (n)}
              {@const on = ix.columns.includes(n)}
              <button type="button" aria-pressed={on} class={flag(on)} onclick={() => toggleIndexCol(ix.id, n)}>
                {#if on}<span class="mr-1 text-muted-foreground tabular-nums">{ix.columns.indexOf(n) + 1}</span>{/if}{n}
              </button>
            {/each}
            {#if !namedCols.length}<span class="text-ui-xs text-muted-foreground">Name a column first</span>{/if}
          </div>
          <button type="button" class={flag(ix.unique)} aria-pressed={ix.unique} title="Unique index"
            onclick={() => (indexes = indexes.map((x) => (x.id === ix.id ? { ...x, unique: !x.unique } : x)))}>Unique</button>
          <button type="button" class="{iconBtn} hover:text-destructive" aria-label="Remove index {i + 1}"
            onclick={() => (indexes = indexes.filter((x) => x.id !== ix.id))}><X class="size-3.5 shrink-0" /></button>
        </div>
        {#if tableName.trim() && ix.columns.length}
          <p class="font-mono text-ui-2xs text-muted-foreground">{indexName(tableName.trim(), ix)}</p>
        {/if}
        {#if errors[ix.id] && attempted}<p class={errCls}>{errors[ix.id]}</p>{/if}
      </div>
    {/each}
  </section>
</DdlDialogShell>
