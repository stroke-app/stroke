<script>
  import FieldSelect from './FieldSelect.svelte';
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import CalendarClock from '@lucide/svelte/icons/calendar-clock'
  import Fingerprint from '@lucide/svelte/icons/fingerprint'
  import Hash from '@lucide/svelte/icons/hash'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { cn } from '$lib/utils.js'
  import {
    buildInsertPayload,
    getColumnEnumValues,
    isBooleanType,
    isDateOnlyType,
    isEditableType,
    isLikelyAutoColumn,
    isTimeOnlyType,
  } from '$lib/cell-value.js'
  import {
    columnTimestampRole,
    defaultInsertDraft,
    generateCuid,
    generateUuid,
    nowDateOnly,
    nowDateTimeLocal,
    nowTimeOnly,
    shouldOfferCuidGenerator,
    shouldOfferUuidGenerator,
    shouldUseDateTimePicker,
  } from '$lib/insert-field.js'

  let {
    open = $bindable(false),
    tableLabel = '',
    columns = [],
    primaryKey = [],
    saving = false,
    /** @param {Record<string, unknown>} values */
    oninsert = async () => {},
  } = $props()

  /** @type {Record<string, string>} */
  let drafts = $state({})

  const editableColumns = $derived(
    columns.filter((c) => isEditableType(c.dataType ?? c.data_type ?? '')),
  )

  const fieldClass =
    'h-8 w-full min-w-0 rounded-md border-2 border-border bg-background/60 px-2.5 font-mono text-ui-sm shadow-none outline-none transition-colors placeholder:text-muted-foreground focus-visible:border-ring/55 focus-visible:ring-1 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-50'

  const actionBtnClass =
    'inline-flex h-7 shrink-0 items-center gap-1 rounded-md border border-border/70 bg-background/70 px-2 font-mono text-ui-2xs text-muted-foreground transition-colors hover:bg-accent/80 hover:text-foreground disabled:pointer-events-none disabled:opacity-50'

  function resetDrafts() {
    /** @type {Record<string, string>} */
    const next = {}
    for (const col of editableColumns) {
      next[col.name] = defaultInsertDraft(col, primaryKey)
    }
    drafts = next
  }

  $effect(() => {
    if (open) resetDrafts()
  })

  /** @param {string} name @param {string} value */
  function setDraft(name, value) {
    drafts = { ...drafts, [name]: value }
  }

  async function submit() {
    const built = buildInsertPayload(editableColumns, primaryKey, drafts)
    if (!built.ok) {
      toast.error('Cannot insert row', { description: built.message })
      return
    }
    await oninsert(/** @type {Record<string, unknown>} */ (built.values))
  }

  /** @param {KeyboardEvent} e */
  function onDialogKeydown(e) {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault()
      if (!saving && editableColumns.length > 0) void submit()
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    class="flex max-h-[min(88vh,40rem)] max-w-lg flex-col gap-0 overflow-hidden p-0 sm:max-w-lg"
    onkeydown={onDialogKeydown}
  >
    <Dialog.Header class="shrink-0 space-y-1 px-5 pt-5 pb-4">
      <Dialog.Title class="text-ui-sm font-semibold tracking-tight">Insert row</Dialog.Title>
      <Dialog.Description class="text-ui-xs text-muted-foreground">
        {#if tableLabel}
          <span class="font-mono text-foreground/80">{tableLabel}</span>
        {:else}
          Add a new record to this table
        {/if}
      </Dialog.Description>
    </Dialog.Header>

    <div class="app-scroll min-h-0 flex-1 overflow-y-auto px-5 pb-4">
      {#if editableColumns.length === 0}
        <p class="rounded-lg border border-dashed border-border/80 px-4 py-6 text-center text-ui-sm text-muted-foreground">
          No editable columns in this table.
        </p>
      {:else}
        <div
          class="flex flex-col gap-4 rounded-lg border border-border/80 bg-muted/10 p-3 sm:p-4"
        >
          {#each editableColumns as col (col.name)}
            {@const dataType = col.dataType ?? col.data_type ?? 'text'}
            {@const enumValues = getColumnEnumValues(col)}
            {@const auto = isLikelyAutoColumn(dataType, col.name, primaryKey)}
            {@const required = !col.nullable && !auto}
            {@const tsRole = columnTimestampRole(col.name)}
            {@const dateTimePicker = shouldUseDateTimePicker(dataType, col.name)}
            {@const dateOnly = isDateOnlyType(dataType)}
            {@const timeOnly = isTimeOnlyType(dataType)}
            {@const showUuid = shouldOfferUuidGenerator(dataType, col.name, primaryKey)}
            {@const showCuid = shouldOfferCuidGenerator(dataType, col.name, primaryKey)}
            <div class="flex flex-col gap-1.5">
              <div class="flex items-start justify-between gap-2">
                <div class="flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-0.5">
                  <label
                    for="ins-{col.name}"
                    class="font-mono text-ui-xs font-medium text-foreground"
                  >
                    {col.name}
                    {#if required}
                      <span class="text-destructive" title="Required">*</span>
                    {/if}
                  </label>
                  <span class="font-mono text-ui-2xs text-muted-foreground">{dataType}</span>
                  {#if auto}
                    <span
                      class="rounded-full bg-muted/70 px-1.5 py-px text-ui-2xs font-medium text-muted-foreground"
                      >auto</span
                    >
                  {:else if tsRole === 'created'}
                    <span
                      class="rounded-full bg-muted/70 px-1.5 py-px text-ui-2xs font-medium text-muted-foreground"
                      >created</span
                    >
                  {:else if tsRole === 'updated'}
                    <span
                      class="rounded-full bg-muted/70 px-1.5 py-px text-ui-2xs font-medium text-muted-foreground"
                      >updated</span
                    >
                  {/if}
                </div>
                {#if showUuid || showCuid}
                  <div class="flex shrink-0 items-center gap-1">
                    {#if showUuid}
                      <button
                        type="button"
                        class={actionBtnClass}
                        disabled={saving}
                        title="Generate UUID v4"
                        onclick={() => setDraft(col.name, generateUuid())}
                      >
                        <Hash class="size-3 shrink-0 opacity-70" />
                        UUID
                      </button>
                    {/if}
                    {#if showCuid}
                      <button
                        type="button"
                        class={actionBtnClass}
                        disabled={saving}
                        title="Generate CUID"
                        onclick={() => setDraft(col.name, generateCuid())}
                      >
                        <Fingerprint class="size-3 shrink-0 opacity-70" />
                        CUID
                      </button>
                    {/if}
                  </div>
                {/if}
              </div>

              {#if enumValues}
                <FieldSelect
                  id="ins-{col.name}"
                  class="w-full text-ui-xs"
                  value={drafts[col.name] ?? ''}
                  disabled={saving}
                  onchange={(v) => setDraft(col.name, v)}
                  options={[
                    { value: '', label: col.nullable ? 'Default or NULL' : 'Default' },
                    ...enumValues.map((/** @type {string} */ o) => ({ value: o, label: o })),
                  ]}
                />
              {:else if isBooleanType(dataType)}
                <FieldSelect
                  id="ins-{col.name}"
                  class="w-full text-ui-xs"
                  value={drafts[col.name] ?? ''}
                  disabled={saving}
                  onchange={(v) => setDraft(col.name, v)}
                  options={[
                    { value: '', label: col.nullable ? 'Default or NULL' : 'Default' },
                    { value: 'true', label: 'true' },
                    { value: 'false', label: 'false' },
                  ]}
                />
              {:else if dateTimePicker}
                <div class="flex gap-1.5">
                  <Input
                    id="ins-{col.name}"
                    type="datetime-local"
                    value={drafts[col.name] ?? ''}
                    disabled={saving}
                    class={cn(fieldClass, 'flex-1')}
                    oninput={(e) => setDraft(col.name, e.currentTarget.value)}
                  />
                  <button
                    type="button"
                    class={actionBtnClass}
                    disabled={saving}
                    title="Set to current date and time"
                    onclick={() => setDraft(col.name, nowDateTimeLocal())}
                  >
                    <CalendarClock class="size-3.5 shrink-0 opacity-70" />
                    Now
                  </button>
                </div>
              {:else if dateOnly}
                <div class="flex gap-1.5">
                  <Input
                    id="ins-{col.name}"
                    type="date"
                    value={drafts[col.name] ?? ''}
                    disabled={saving}
                    class={cn(fieldClass, 'flex-1')}
                    oninput={(e) => setDraft(col.name, e.currentTarget.value)}
                  />
                  <button
                    type="button"
                    class={actionBtnClass}
                    disabled={saving}
                    onclick={() => setDraft(col.name, nowDateOnly())}
                  >
                    Today
                  </button>
                </div>
              {:else if timeOnly}
                <div class="flex gap-1.5">
                  <Input
                    id="ins-{col.name}"
                    type="time"
                    value={drafts[col.name] ?? ''}
                    disabled={saving}
                    class={cn(fieldClass, 'flex-1')}
                    oninput={(e) => setDraft(col.name, e.currentTarget.value)}
                  />
                  <button
                    type="button"
                    class={actionBtnClass}
                    disabled={saving}
                    onclick={() => setDraft(col.name, nowTimeOnly())}
                  >
                    Now
                  </button>
                </div>
              {:else}
                <Input
                  id="ins-{col.name}"
                  value={drafts[col.name] ?? ''}
                  disabled={saving}
                  placeholder={auto
                    ? 'Auto-generated if empty'
                    : col.nullable
                      ? 'NULL or value'
                      : 'Required'}
                  class={fieldClass}
                  oninput={(e) => setDraft(col.name, e.currentTarget.value)}
                />
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <Dialog.Footer
      class="flex shrink-0 items-center gap-2 border-t border-border/80 bg-muted/15 px-5 py-3"
    >
      <p class="mr-auto hidden text-ui-2xs text-muted-foreground sm:block">
        <kbd>⌘↵</kbd>
        insert
      </p>
      <Button variant="ghost" size="sm" class="h-8" disabled={saving} onclick={() => (open = false)}>
        Cancel
      </Button>
      <Button
        size="sm"
        class="h-8 min-w-[5.5rem]"
        disabled={saving || editableColumns.length === 0}
        onclick={() => void submit()}
      >
        {saving ? 'Inserting…' : 'Insert row'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
