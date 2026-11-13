<script>
  import { untrack } from 'svelte'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { executeSql, listFunctions, getTableColumnStructure } from '$lib/api.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import PickerField from './PickerField.svelte'
  import SegmentedControl from './SegmentedControl.svelte'
  import DdlDialogShell from './DdlDialogShell.svelte'
  import { snippetText } from '$lib/object-templates.js'
  import {
    buildTriggerSql, validateTrigger, triggerFunctionSnippet,
    hasPlaceholder, placeholderText, toSnippet, TRIGGER_EVENTS,
  } from '$lib/ddl-builders.js'
  import { cn } from '$lib/utils.js'
  import Plus from '@lucide/svelte/icons/plus'

  let {
    open = $bindable(false),
    schema = '',
    defaultTable = '',
    /** @type {{ name: string }[]} */
    tables = [],
    onrefresh = () => {},
    /** Opens SQL in a new editor tab. @type {((spec: { text: string, title: string, snippet?: string }) => void) | null} */
    onopensql = null,
  } = $props()

  const TIMINGS = [
    { value: 'BEFORE', label: 'Before' },
    { value: 'AFTER', label: 'After' },
    { value: 'INSTEAD OF', label: 'Instead of' },
  ]
  const EVENT_OPTS = TRIGGER_EVENTS.map((e) => ({ value: e, label: e[0] + e.slice(1).toLowerCase() }))

  let name = $state('')
  let targetTable = $state(untrack(() => defaultTable))
  let timing = $state('AFTER')
  let events = $state(/** @type {string[]} */ (['INSERT']))
  let rowLevel = $state(true)
  let functionName = $state('')
  let functionArgs = $state('')
  let updateColumns = $state(/** @type {string[]} */ ([]))
  let when = $state('')
  let saving = $state(false)
  let attempted = $state(false)
  let touched = $state(/** @type {Record<string, boolean>} */ ({}))

  /** @type {{ name: string, signature: string }[]} */
  let functions = $state([])
  let functionsFailed = $state(false)
  /** @type {Record<string, string[]>} */
  let columnsByTable = $state({})

  $effect(() => { targetTable = defaultTable })
  $effect(() => { if (timing === 'INSTEAD OF') rowLevel = true })
  // A new table has other columns; drop the picks that don't belong to it.
  $effect(() => { targetTable; untrack(() => { updateColumns = [] }) })

  // Trigger functions in this schema, fetched each time the dialog opens.
  $effect(() => {
    if (!open || !schema) return
    let cancelled = false
    functionsFailed = false
    listFunctions(schema)
      .then((/** @type {any[]} */ rows) => {
        if (cancelled) return
        functions = (rows ?? []).filter((f) => String(f.returnType).toLowerCase() === 'trigger')
      })
      .catch(() => { if (!cancelled) functionsFailed = true })
    return () => { cancelled = true }
  })

  // The chosen table's columns, once UPDATE is picked (for UPDATE OF).
  $effect(() => {
    const t = targetTable
    if (!open || !t || !events.includes('UPDATE') || untrack(() => columnsByTable[t])) return
    getTableColumnStructure(schema, t)
      .then((/** @type {any[]} */ cols) => { columnsByTable = { ...columnsByTable, [t]: (cols ?? []).map((c) => c.name) } })
      .catch(() => { columnsByTable = { ...columnsByTable, [t]: [] } })
  })

  const tableColumns = $derived(columnsByTable[targetTable] ?? [])

  const spec = $derived({
    schema, name, table: targetTable, timing, events, rowLevel, when, functionName, functionArgs,
    updateColumns: events.includes('UPDATE') && timing !== 'INSTEAD OF' ? updateColumns : [],
  })
  const sql = $derived(buildTriggerSql(spec))
  const errors = $derived(validateTrigger(spec))
  const isValid = $derived(Object.keys(errors).length === 0 && !hasPlaceholder(sql))
  /** Missing values show once the field was left or a submit was tried; conflicts show at once. @param {string} k */
  const err = (k) => (['events', 'when'].includes(k) || touched[k] || attempted ? errors[k] : '')

  /** @param {string[]} next */
  function onEvents(next) {
    // TRUNCATE only fires per statement; follow it rather than show an error.
    if (next.includes('TRUNCATE') && !events.includes('TRUNCATE') && timing !== 'INSTEAD OF') rowLevel = false
  }

  /** @param {string} col */
  function toggleColumn(col) {
    updateColumns = updateColumns.includes(col) ? updateColumns.filter((c) => c !== col) : [...updateColumns, col]
  }

  function reset() {
    name = ''; targetTable = defaultTable; timing = 'AFTER'
    events = ['INSERT']; rowLevel = true; functionName = ''; functionArgs = ''
    updateColumns = []; when = ''; attempted = false; touched = {}
  }

  async function handleCreate() {
    attempted = true
    if (!isValid || saving) return
    saving = true
    try {
      await executeSql(sql)
      toast.success(`Trigger "${name.trim()}" created`)
      onrefresh(); reset(); open = false
    } catch (e) {
      toast.error('Could not create trigger', { description: String(e) })
    } finally { saving = false }
  }

  function openInEditor() {
    onopensql?.({
      text: placeholderText(sql),
      title: name.trim() ? `Trigger · ${name.trim()}` : 'New trigger',
      snippet: hasPlaceholder(sql) ? toSnippet(sql) : undefined,
    })
    open = false
  }

  function writeNewFunction() {
    const snippet = triggerFunctionSnippet(schema)
    onopensql?.({ text: snippetText(snippet), title: 'New trigger function', snippet })
    open = false
  }

  const lbl = 'text-ui-xs text-muted-foreground'
  const errCls = 'text-ui-2xs text-destructive'
</script>

<DdlDialogShell
  bind:open
  title="New trigger"
  subtitle={`in ${schema}`}
  {sql}
  canSubmit={isValid}
  busy={saving}
  submitLabel="Create trigger"
  onsubmit={handleCreate}
  onopensql={onopensql ? openInEditor : null}
  onOpenChange={(v) => { if (!v) reset() }}
>
  <div class="grid grid-cols-2 gap-x-3 gap-y-4">
    <div class="flex min-w-0 flex-col gap-1.5">
      <label for="trig-name" class={lbl}>Name</label>
      <!-- svelte-ignore a11y_autofocus -->
      <Input id="trig-name" class="h-8 font-mono" bind:value={name} placeholder="on_user_insert" autofocus
        aria-invalid={!!err('name') || undefined} onblur={() => (touched.name = true)} />
      {#if err('name')}<p class={errCls}>{err('name')}</p>{/if}
    </div>
    <div class="flex min-w-0 flex-col gap-1.5">
      <label for="trig-table" class={lbl}>Table</label>
      <PickerField id="trig-table" mono bind:value={targetTable} placeholder="Select a table"
        search="Search tables…" empty="No matching table" invalid={!!err('table')}
        items={tables.map((t) => ({ value: t.name }))} onchange={() => (touched.table = true)} />
      {#if err('table')}<p class={errCls}>{err('table')}</p>{/if}
    </div>

    <div class="flex min-w-0 flex-col gap-1.5">
      <span class={lbl} id="trig-timing">Timing</span>
      <SegmentedControl options={TIMINGS} bind:value={timing} ariaLabel="Timing" />
    </div>
    <div class="flex min-w-0 flex-col gap-1.5">
      <span class={lbl}>Fires</span>
      <SegmentedControl
        options={[{ value: 'row', label: 'Per row' }, { value: 'stmt', label: 'Per statement' }]}
        value={rowLevel ? 'row' : 'stmt'}
        disabled={timing === 'INSTEAD OF'}
        ariaLabel="Fires per"
        onchange={(v) => (rowLevel = v === 'row')}
      />
    </div>

    <div class="col-span-2 flex min-w-0 flex-col gap-1.5">
      <span class={lbl}>Events</span>
      <SegmentedControl options={EVENT_OPTS} multiple bind:value={events} ariaLabel="Events" onchange={onEvents} />
      {#if err('events')}<p class={errCls}>{err('events')}</p>{/if}
    </div>

    {#if events.includes('UPDATE') && timing !== 'INSTEAD OF' && targetTable}
      <div class="col-span-2 flex min-w-0 flex-col gap-1.5">
        <span class={lbl}>
          Update of <span class="text-muted-foreground">· {updateColumns.length ? `${updateColumns.length} selected` : 'any column'}</span>
        </span>
        {#if tableColumns.length}
          <div class="flex flex-wrap gap-1">
            {#each tableColumns as col (col)}
              {@const on = updateColumns.includes(col)}
              <button type="button" aria-pressed={on}
                class={cn('h-7 rounded-md px-2 font-mono text-ui-xs transition-colors',
                  on ? 'bg-accent text-foreground ring-1 ring-border/60' : 'bg-muted/40 text-muted-foreground hover:text-foreground')}
                onclick={() => toggleColumn(col)}>{col}</button>
            {/each}
          </div>
        {:else}
          <p class="loading-reveal text-ui-xs text-muted-foreground">Loading columns…</p>
        {/if}
      </div>
    {/if}

    <div class="flex min-w-0 flex-col gap-1.5">
      <label for="trig-fn" class={lbl}>Function</label>
      {#if functionsFailed}
        <Input id="trig-fn" class="h-8 font-mono" bind:value={functionName} placeholder="my_trigger_fn"
          aria-invalid={!!err('functionName') || undefined} onblur={() => (touched.functionName = true)} />
      {:else}
        <PickerField id="trig-fn" mono bind:value={functionName} placeholder="Select a function"
          search="Search trigger functions…" empty="No trigger functions in {schema}" invalid={!!err('functionName')}
          items={functions.map((f) => ({ value: f.name, hint: f.signature.slice(f.name.length) }))}
          onchange={() => (touched.functionName = true)}>
          {#snippet menuFooter({ close })}
            {#if onopensql}
              <button type="button"
                class="flex h-7 w-full items-center gap-2 rounded-sm px-2 text-left text-ui-xs text-foreground/85 hover:bg-accent hover:text-foreground"
                onclick={() => { close(); writeNewFunction() }}>
                <Plus class="size-3.5 shrink-0 text-muted-foreground" />Write a new function…
              </button>
            {/if}
          {/snippet}
        </PickerField>
      {/if}
      {#if err('functionName')}<p class={errCls}>{err('functionName')}</p>{/if}
    </div>
    <div class="flex min-w-0 flex-col gap-1.5">
      <label for="trig-args" class={lbl}>Arguments <span class="text-muted-foreground">(optional)</span></label>
      <Input id="trig-args" class="h-8 font-mono" bind:value={functionArgs} placeholder="'arg1', 'arg2'" />
    </div>

    <div class="col-span-2 flex min-w-0 flex-col gap-1.5">
      <label for="trig-when" class={lbl}>When <span class="text-muted-foreground">(optional condition)</span></label>
      <Input id="trig-when" class="h-8 font-mono" bind:value={when} placeholder="NEW.status IS DISTINCT FROM OLD.status"
        aria-invalid={!!err('when') || undefined} />
      {#if err('when')}<p class={errCls}>{err('when')}</p>{/if}
    </div>
  </div>
</DdlDialogShell>
