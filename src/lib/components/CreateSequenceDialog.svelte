<script>
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { executeSql, getTableColumnStructure } from '$lib/api.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import PickerField from './PickerField.svelte'
  import SegmentedControl from './SegmentedControl.svelte'
  import DdlDialogShell from './DdlDialogShell.svelte'
  import {
    buildSequenceSql, validateSequence, hasPlaceholder, placeholderText, toSnippet, SEQUENCE_TYPES,
  } from '$lib/ddl-builders.js'
  import X from '@lucide/svelte/icons/x'

  let {
    open = $bindable(false),
    schema = '',
    /** Tables for "Owned by". @type {{ name: string }[]} */
    tables = [],
    onrefresh = () => {},
    /** Opens SQL in a new editor tab. @type {((spec: { text: string, title: string, snippet?: string }) => void) | null} */
    onopensql = null,
  } = $props()

  /** @typedef {keyof typeof SEQUENCE_TYPES} SeqType */
  const TYPE_OPTS = [
    { value: 'smallint', label: 'smallint' },
    { value: 'integer', label: 'integer' },
    { value: 'bigint', label: 'bigint' },
  ]

  let name = $state('')
  let dataType = $state(/** @type {SeqType} */ ('bigint'))
  let startValue = $state('1')
  let minValue = $state('1')
  let maxValue = $state(SEQUENCE_TYPES.bigint.hi)
  let increment = $state('1')
  let cache = $state('1')
  let cycle = $state(false)
  let ownedTable = $state('')
  let ownedColumn = $state('')
  let saving = $state(false)
  let attempted = $state(false)
  let touched = $state(/** @type {Record<string, boolean>} */ ({}))
  /** @type {Record<string, string[]>} */
  let columnsByTable = $state({})

  /** Max value follows the type unless it was edited away from a type's default. @param {SeqType} t */
  function onType(t) {
    const defaults = Object.values(SEQUENCE_TYPES).map((r) => r.hi)
    if (!maxValue.trim() || defaults.includes(maxValue.trim())) maxValue = SEQUENCE_TYPES[t].hi
  }

  $effect(() => {
    const t = ownedTable
    if (!open || !t || columnsByTable[t]) return
    getTableColumnStructure(schema, t)
      .then((/** @type {any[]} */ cols) => { columnsByTable = { ...columnsByTable, [t]: (cols ?? []).map((c) => c.name) } })
      .catch(() => { columnsByTable = { ...columnsByTable, [t]: [] } })
  })

  const spec = $derived({
    schema, name, dataType, increment, minValue, maxValue, startValue, cache, cycle, ownedTable, ownedColumn,
  })
  const sql = $derived(buildSequenceSql(spec))
  const errors = $derived(validateSequence(spec))
  const isValid = $derived(Object.keys(errors).length === 0 && !hasPlaceholder(sql))
  /** @param {string} k */
  const err = (k) => (k === 'name' && !touched.name && !attempted ? '' : errors[k] ?? '')

  function reset() {
    name = ''; dataType = 'bigint'; startValue = '1'; minValue = '1'
    maxValue = SEQUENCE_TYPES.bigint.hi; increment = '1'; cache = '1'; cycle = false
    ownedTable = ''; ownedColumn = ''; attempted = false; touched = {}
  }

  async function handleCreate() {
    attempted = true
    if (!isValid || saving) return
    saving = true
    try {
      await executeSql(sql)
      toast.success(`Sequence "${name.trim()}" created`)
      onrefresh(); reset(); open = false
    } catch (e) {
      toast.error('Could not create sequence', { description: String(e) })
    } finally { saving = false }
  }

  function openInEditor() {
    onopensql?.({
      text: placeholderText(sql),
      title: name.trim() ? `Sequence · ${name.trim()}` : 'New sequence',
      snippet: hasPlaceholder(sql) ? toSnippet(sql) : undefined,
    })
    open = false
  }

  const lbl = 'text-ui-xs text-muted-foreground'
  const errCls = 'text-ui-2xs text-destructive'
  const num = 'h-8 font-mono tabular-nums'
</script>

{#snippet numField(/** @type {string} */ id, /** @type {string} */ label, /** @type {string} */ key, /** @type {() => string} */ get, /** @type {(v: string) => void} */ set)}
  <div class="flex min-w-0 flex-col gap-1.5">
    <label for={id} class={lbl}>{label}</label>
    <Input {id} class={num} inputmode="numeric" value={get()} oninput={(e) => set(e.currentTarget.value)}
      aria-invalid={!!err(key) || undefined} />
    {#if err(key)}<p class={errCls}>{err(key)}</p>{/if}
  </div>
{/snippet}

<DdlDialogShell
  bind:open
  title="New sequence"
  subtitle={`in ${schema}`}
  {sql}
  canSubmit={isValid}
  busy={saving}
  submitLabel="Create sequence"
  onsubmit={handleCreate}
  onopensql={onopensql ? openInEditor : null}
  onOpenChange={(v) => { if (!v) reset() }}
>
  <div class="grid grid-cols-2 gap-x-3 gap-y-4">
    <div class="flex min-w-0 flex-col gap-1.5">
      <label for="seq-name" class={lbl}>Name</label>
      <!-- svelte-ignore a11y_autofocus -->
      <Input id="seq-name" class="h-8 font-mono" bind:value={name} placeholder="user_id_seq" autofocus
        aria-invalid={!!err('name') || undefined} onblur={() => (touched.name = true)} />
      {#if err('name')}<p class={errCls}>{err('name')}</p>{/if}
    </div>
    <div class="flex min-w-0 flex-col gap-1.5">
      <span class={lbl}>Data type</span>
      <SegmentedControl options={TYPE_OPTS} mono bind:value={dataType} ariaLabel="Data type" onchange={onType} />
    </div>

    {@render numField('seq-start', 'Start with', 'startValue', () => startValue, (v) => (startValue = v))}
    {@render numField('seq-inc', 'Increment by', 'increment', () => increment, (v) => (increment = v))}
    {@render numField('seq-min', 'Min value', 'minValue', () => minValue, (v) => (minValue = v))}
    {@render numField('seq-max', 'Max value', 'maxValue', () => maxValue, (v) => (maxValue = v))}
    {@render numField('seq-cache', 'Cache', 'cache', () => cache, (v) => (cache = v))}

    <label class="flex min-w-0 cursor-pointer items-center justify-between gap-3 self-end pb-1.5">
      <span class="flex min-w-0 flex-col">
        <span class="text-ui-sm text-foreground">Cycle</span>
        <span class="text-ui-xs text-muted-foreground">Restart at the min after the max</span>
      </span>
      <Switch bind:checked={cycle} aria-label="Cycle" />
    </label>

    <div class="col-span-2 flex min-w-0 flex-col gap-1.5">
      <span class={lbl}>Owned by <span class="text-muted-foreground">(optional, dropped with the column)</span></span>
      <div class="flex items-center gap-2">
        <div class="min-w-0 flex-1">
          <PickerField mono bind:value={ownedTable} placeholder="Table" search="Search tables…" empty="No matching table"
            aria-label="Owning table" items={tables.map((t) => ({ value: t.name }))} onchange={() => (ownedColumn = '')} />
        </div>
        <span class="text-muted-foreground">.</span>
        <div class="min-w-0 flex-1">
          <PickerField mono bind:value={ownedColumn} placeholder="Column" search="Search columns…"
            empty={ownedTable ? 'No matching column' : 'Pick a table first'} aria-label="Owning column"
            invalid={!!(ownedTable && err('ownedColumn'))}
            items={(columnsByTable[ownedTable] ?? []).map((c) => ({ value: c }))} />
        </div>
        {#if ownedTable}
          <button type="button" class="hit-area inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            aria-label="Clear owner" onclick={() => { ownedTable = ''; ownedColumn = '' }}>
            <X class="size-3.5 shrink-0" />
          </button>
        {/if}
      </div>
      {#if ownedTable && err('ownedColumn')}<p class={errCls}>{err('ownedColumn')}</p>{/if}
    </div>
  </div>
</DdlDialogShell>
