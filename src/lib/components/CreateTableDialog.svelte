<script>
  import FieldSelect from './FieldSelect.svelte';
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import Plus from '@lucide/svelte/icons/plus'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Checkbox } from '$lib/components/ui/checkbox/index.js'
  import { cn } from '$lib/utils.js'

  let {
    open = $bindable(false),
    activeSchema = 'public',
    /** 'postgres' | 'sqlite' | 'mysql' | 'd1' */
    dbType = 'postgres',
    saving = false,
    /** @param {string} sql */
    onexecute = /** @type {(sql: string) => Promise<void>} */ (async () => {}),
    /** @param {string} tableName */
    oncreated = /** @type {(name: string) => void} */ (() => {}),
  } = $props()

  /**
   * @typedef {{ id: string, name: string, type: string, pk: boolean, notNull: boolean, defaultVal: string }} ColDef
   */

  const PG_TYPES = ['serial', 'bigserial', 'integer', 'bigint', 'text', 'varchar(255)', 'boolean', 'numeric', 'float8', 'timestamptz', 'date', 'uuid', 'jsonb', 'bytea']
  const SQLITE_TYPES = ['INTEGER', 'TEXT', 'REAL', 'NUMERIC', 'BLOB']
  const MYSQL_TYPES = ['INT', 'BIGINT', 'VARCHAR(255)', 'TEXT', 'TINYINT(1)', 'FLOAT', 'DECIMAL(10,2)', 'DATETIME', 'DATE', 'JSON']

  const typeOptions = $derived(
    dbType === 'sqlite' || dbType === 'd1' ? SQLITE_TYPES :
    dbType === 'mysql' ? MYSQL_TYPES :
    PG_TYPES
  )

  const defaultIdType = $derived(
    dbType === 'sqlite' || dbType === 'd1' ? 'INTEGER' :
    dbType === 'mysql' ? 'INT' :
    'serial'
  )

  const defaultIdExtra = $derived(
    dbType === 'mysql' ? 'AUTO_INCREMENT' : ''
  )

  function makeDefaultCols() {
    /** @type {ColDef[]} */
    const cols = [
      { id: crypto.randomUUID(), name: 'id', type: defaultIdType, pk: true, notNull: true, defaultVal: defaultIdExtra },
    ]
    if (dbType === 'postgres') {
      cols.push({ id: crypto.randomUUID(), name: 'created_at', type: 'timestamptz', pk: false, notNull: true, defaultVal: 'NOW()' })
    }
    return cols
  }

  let tableName = $state('')
  let submitting = $state(false)
  let /** @type {ColDef[]} */ cols = $state(makeDefaultCols())

  $effect(() => {
    if (!open) {
      tableName = ''
      cols = makeDefaultCols()
      submitting = false
    }
  })

  function addCol() {
    const defaultType = typeOptions[typeOptions.length > 4 ? 4 : 0]
    cols = [...cols, { id: crypto.randomUUID(), name: '', type: defaultType, pk: false, notNull: false, defaultVal: '' }]
  }

  function removeCol(/** @type {string} */ id) {
    cols = cols.filter(c => c.id !== id)
  }

  /** @param {string} id @param {Partial<ColDef>} patch */
  function patchCol(id, patch) {
    cols = cols.map(c => c.id === id ? { ...c, ...patch } : c)
  }

  const sql = $derived.by(() => {
    const name = tableName.trim()
    if (!name) return ''
    const schema = activeSchema || 'public'
    const validCols = cols.filter(c => c.name.trim())
    if (!validCols.length) return ''

    const pkCols = validCols.filter(c => c.pk)
    const isSerial = (type) => type === 'serial' || type === 'bigserial'

    const lines = validCols.map(c => {
      const isAuto = isSerial(c.type) || c.defaultVal.trim().toUpperCase() === 'AUTO_INCREMENT'
      let def = `  "${c.name.trim()}" ${c.type}`
      if (c.defaultVal.trim() && c.defaultVal.trim().toUpperCase() !== 'AUTO_INCREMENT') {
        def += ` DEFAULT ${c.defaultVal.trim()}`
      }
      if (isAuto && dbType === 'mysql') def += ' AUTO_INCREMENT'
      if (pkCols.length === 1 && c.pk && dbType !== 'sqlite') def += ' PRIMARY KEY'
      if (c.notNull && !isAuto) def += ' NOT NULL'
      if (dbType === 'sqlite' && c.pk) def += ' PRIMARY KEY'
      return def
    })

    if (pkCols.length > 1 && dbType !== 'sqlite') {
      lines.push(`  PRIMARY KEY (${pkCols.map(c => `"${c.name.trim()}"`).join(', ')})`)
    }

    if (dbType === 'sqlite' || dbType === 'd1') {
      return `CREATE TABLE "${name}" (\n${lines.join(',\n')}\n);`
    }
    if (dbType === 'mysql') {
      return `CREATE TABLE \`${schema}\`.\`${name}\` (\n${lines.join(',\n')}\n);`
    }
    return `CREATE TABLE "${schema}"."${name}" (\n${lines.join(',\n')}\n);`
  })

  const canSubmit = $derived(
    tableName.trim().length > 0 &&
    cols.some(c => c.name.trim().length > 0) &&
    !submitting &&
    !saving
  )

  async function handleCreate() {
    if (!canSubmit || !sql) return
    submitting = true
    try {
      await onexecute(sql)
      toast.success(`Table "${tableName.trim()}" created`)
      oncreated(tableName.trim())
      open = false
    } catch (err) {
      toast.error('Could not create table', { description: String(err) })
    } finally {
      submitting = false
    }
  }

</script>

<!-- The column list is an editable table in the app's `cell-fields` idiom (the
     structure editor's): the cell is the field, rows are divided by hairlines, and
     focus highlights the cell. It used to be a row of separately bordered pills
     per column, five frames wide, with a bare checkbox and a trash can floating
     at the end. -->
<Dialog.Root bind:open>
  <Dialog.Content class="flex max-h-[90vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl">
    <Dialog.Header class="shrink-0 gap-1 px-6 pt-6 pb-4">
      <Dialog.Title>New table</Dialog.Title>
      {#if dbType !== 'sqlite' && dbType !== 'd1'}
        <Dialog.Description class="flex items-center gap-1.5 text-ui-xs text-muted-foreground">
          in schema
          <span class="rounded border border-border/60 bg-muted/40 px-1.5 py-px font-mono text-ui-2xs text-foreground/80">{activeSchema}</span>
        </Dialog.Description>
      {:else}
        <Dialog.Description class="sr-only">Name the table and define its columns.</Dialog.Description>
      {/if}
    </Dialog.Header>

    <div class="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-6 pb-5">
      <div class="flex flex-col gap-1.5">
        <label class="text-ui-xs font-medium text-foreground/80" for="create-table-name">Table name</label>
        <Input
          id="create-table-name"
          class="h-9 font-mono text-ui-sm"
          placeholder="users"
          bind:value={tableName}
          onkeydown={(e) => e.key === 'Enter' && handleCreate()}
          autocomplete="off"
          spellcheck="false"
        />
      </div>

      <div class="flex flex-col gap-2">
        <div class="flex items-baseline gap-2">
          <span class="text-ui-xs font-medium text-foreground/80" id="create-table-cols">Columns</span>
          <span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">{cols.filter(c => c.name.trim()).length}</span>
        </div>

        <div class="overflow-hidden rounded-xl border border-border/60">
          <table class="cell-fields w-full table-fixed border-collapse text-ui-xs" aria-labelledby="create-table-cols">
            <colgroup>
              <col />
              <col class="w-[26%]" />
              <col class="w-16" />
              <col class="w-16" />
              <col class="w-[22%]" />
              <col class="w-9" />
            </colgroup>
            <thead class="bg-muted/30">
              <tr class="text-left text-ui-2xs font-medium text-muted-foreground">
                <th scope="col" class="px-3 py-2 font-medium">Name</th>
                <th scope="col" class="px-3 py-2 font-medium">Type</th>
                <th scope="col" class="px-1 py-2 text-center font-medium" title="Primary key">Primary</th>
                <th scope="col" class="px-1 py-2 text-center font-medium" title="NOT NULL">Required</th>
                <th scope="col" class="px-3 py-2 font-medium">Default</th>
                <th scope="col"><span class="sr-only">Remove</span></th>
              </tr>
            </thead>
            <tbody>
              {#each cols as col, i (col.id)}
                <tr class="group border-t border-border/50 transition-colors hover:bg-muted/20">
                  <td class="p-0">
                    <input
                      class="h-9 w-full min-w-0 px-3 font-mono text-ui-xs text-foreground outline-none placeholder:text-muted-foreground/70"
                      placeholder="column_name"
                      aria-label="Column {i + 1} name"
                      bind:value={col.name}
                      autocomplete="off"
                      spellcheck="false"
                    />
                  </td>
                  <td class="p-0">
                    <FieldSelect
                      size="sm"
                      class="h-9 w-full px-3 font-mono text-ui-xs"
                      aria-label="Column {i + 1} type"
                      bind:value={col.type}
                      options={typeOptions.map((t) => ({ value: t, label: t }))}
                    />
                  </td>
                  <td class="p-0 text-center">
                    <Checkbox
                      checked={col.pk}
                      aria-label="Column {i + 1} is the primary key"
                      onCheckedChange={() => patchCol(col.id, { pk: !col.pk })}
                    />
                  </td>
                  <td class="p-0 text-center">
                    <Checkbox
                      checked={col.notNull}
                      aria-label="Column {i + 1} is required (NOT NULL)"
                      onCheckedChange={() => patchCol(col.id, { notNull: !col.notNull })}
                    />
                  </td>
                  <td class="p-0">
                    <input
                      class="h-9 w-full min-w-0 px-3 font-mono text-ui-xs text-foreground outline-none placeholder:text-muted-foreground/60"
                      placeholder="none"
                      aria-label="Column {i + 1} default"
                      bind:value={col.defaultVal}
                      autocomplete="off"
                      spellcheck="false"
                    />
                  </td>
                  <td class="p-0 text-center">
                    <!-- Shown on row hover or focus; always reachable by keyboard. -->
                    <Button
                      variant="ghost"
                      size="icon-xs"
                      class="text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:text-destructive disabled:opacity-0"
                      onclick={() => removeCol(col.id)}
                      aria-label="Remove column {col.name || i + 1}"
                      title="Remove column"
                      disabled={cols.length === 1}
                    >
                      <Trash2 class="size-3.5" aria-hidden="true" />
                    </Button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
          <button
            type="button"
            class="flex h-9 w-full items-center gap-1.5 border-t border-border/50 px-3 text-left text-ui-xs text-muted-foreground transition-colors hover:bg-muted/20 hover:text-foreground focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
            onclick={addCol}
          >
            <Plus class="size-3.5 shrink-0" aria-hidden="true" />
            Add column
          </button>
        </div>
      </div>

      {#if sql}
        <div class="flex flex-col gap-1.5">
          <span class="text-ui-xs font-medium text-foreground/80">SQL preview</span>
          <pre class="overflow-x-auto rounded-xl border border-border/60 bg-muted/30 px-3.5 py-3 font-mono text-ui-2xs leading-relaxed whitespace-pre-wrap break-all text-muted-foreground">{sql}</pre>
        </div>
      {/if}
    </div>

    <div class="flex shrink-0 items-center justify-end gap-2 border-t border-border/60 px-6 py-3.5">
      <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
      <Button disabled={!canSubmit} onclick={handleCreate}>
        {#if submitting}
          <Loader2 class="size-3.5 animate-spin" aria-hidden="true" />
          Creating…
        {:else}
          Create table
        {/if}
      </Button>
    </div>
  </Dialog.Content>
</Dialog.Root>
