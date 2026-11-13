<script>
  import FieldSelect from './FieldSelect.svelte';
  import { untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { formatCellValue, anyDisplayExtEnabled } from '$lib/plugins/registry.js'
  import { pluginState } from '$lib/stores/plugins.js'
  import {
    normalizeCellValue,
    formatJsonValue,
    formatNormalValue,
  } from '$lib/row-inspector.js'
  import {
    isEditableType,
    isBooleanType,
    getColumnEnumValues,
    valueToEditString,
    parseCellInput,
    normalizeColumnType,
    oversizeCellInfo,
  } from '$lib/cell-value.js'

  /**
   * Record mode for the data table - shows one row at a time as a vertical
   * field list (DBeaver-style "record view") with prev/next navigation and
   * the same inline field editing as the row inspector panel.
   */
  let {
    /** @type {Array<{ name: string, dataType?: string, data_type?: string, nullable?: boolean }>} */
    columns = [],
    /** @type {unknown[][]} */
    rows = [],
    /** @type {string[]} */
    primaryKey = [],
    /** @type {Set<string>} */
    hiddenColumns = new Set(),
    /** Absolute offset of the first loaded row (for the "Record N of M" label). */
    offset = 0,
    /** Total rows in the result (-1 while the count is still being fetched). */
    total = 0,
    initialIndex = 0,
    /** @type {(index: number) => void} */
    onindexchange = () => {},
    hasPrevPage = false,
    hasNextPage = false,
    onprevpage = () => {},
    onnextpage = () => {},
    /** @type {((detail: { rowIdx: number, colIdx: number, value: unknown }) => Promise<void>) | null} */
    onsave = null,
    readonly = false,
  } = $props()

  // svelte-ignore state_referenced_locally - the initial index deliberately
  // captures the mount-time rows; later row changes are handled by the effect below.
  let idx = $state(Math.min(Math.max(initialIndex ?? 0, 0), Math.max(rows.length - 1, 0)))
  let fieldSearch = $state('')
  // When navigating to the previous page, land on its last record once rows arrive.
  let landAtEnd = false

  /** @param {number} i */
  function setIdx(i) {
    idx = i
    onindexchange(i)
  }

  // Keep idx valid when the loaded page changes (pagination, refresh, filters).
  $effect(() => {
    void rows
    untrack(() => {
      if (landAtEnd) {
        landAtEnd = false
        setIdx(Math.max(0, rows.length - 1))
      } else if (idx > rows.length - 1) {
        setIdx(Math.max(0, rows.length - 1))
      }
    })
  })

  const canPrev = $derived(idx > 0 || hasPrevPage)
  const canNext = $derived(idx < rows.length - 1 || hasNextPage)

  function goPrev() {
    if (idx > 0) setIdx(idx - 1)
    else if (hasPrevPage) {
      landAtEnd = true
      onprevpage()
    }
  }

  function goNext() {
    if (idx < rows.length - 1) setIdx(idx + 1)
    else if (hasNextPage) {
      onnextpage()
      setIdx(0)
    }
  }

  const recordLabel = $derived.by(() => {
    if (rows.length === 0) return 'No records'
    const n = (offset + idx + 1).toLocaleString('en-US')
    const of = total < 0 ? '…' : total.toLocaleString('en-US')
    return `${n} of ${of}`
  })

  // ── Fields ────────────────────────────────────────────────────────────────
  const pkSet = $derived(new Set(primaryKey))

  /**
   * Which field is currently being edited, by column index.
   *
   * Every editable field used to render a live <input>/<textarea>/<select> the
   * whole time. On a 136-column table that is 136 form controls with three
   * listeners each, sitting in the DOM whether or not anybody is typing - and
   * the whole set was wrapped in `{#key idx}`, so stepping to the next record
   * destroyed and rebuilt all of them. That teardown is what the arrow keys felt
   * like. A field is plain text until you click it.
   * @type {number | null}
   */
  let editingIdx = $state(null)
  /** Grow a textarea with its content (up to 8 lines, then scroll). @param {HTMLTextAreaElement} node */
  function autosize(node) {
    const fit = () => {
      node.style.height = 'auto'
      const max = 8 * 20 + 6
      node.style.height = `${Math.min(node.scrollHeight + 2, max)}px`
      node.style.overflowY = node.scrollHeight + 2 > max ? 'auto' : 'hidden'
    }
    fit()
    node.addEventListener('input', fit)
    return { destroy: () => node.removeEventListener('input', fit) }
  }
  /** @param {HTMLElement} node */
  function focusOnMount(node) {
    const el = /** @type {HTMLInputElement | HTMLTextAreaElement} */ (
      node.matches('input, textarea') ? node : node.querySelector('input, textarea')
    )
    el?.focus()
    if (el && 'select' in el) el.select()
  }
  // Leaving a record closes the editor with it; the draft is already committed
  // by the blur handler, so there is nothing to carry across.
  $effect(() => { void idx; editingIdx = null })

  /** Unix seconds or milliseconds, in a column named like a time. */
  const TIME_NAME = /(_at|_on|_time|_ts|^ts|timestamp|date|_seen|expires|expiry)$/i
  /** @param {number} ms */
  function ago(ms) {
    const s = Math.round((Date.now() - ms) / 1000)
    const a = Math.abs(s)
    const w = a < 60 ? 'just now' : a < 3600 ? `${Math.floor(a / 60)} min` : a < 86400 ? `${Math.floor(a / 3600)}h` : a < 86400 * 60 ? `${Math.floor(a / 86400)}d` : a < 86400 * 730 ? `${Math.floor(a / (86400 * 30))}mo` : `${Math.floor(a / (86400 * 365))}y`
    return w === 'just now' ? w : s >= 0 ? `${w} ago` : `in ${w}`
  }
  /**
   * A readable second line for a value: what the display extensions make of
   * it (when any are on), else a date for an epoch number in a time column.
   * @param {unknown} raw @param {string} type @param {string} name @param {number} colIdx @param {unknown[]} row
   * @returns {string}
   */
  function hintFor(raw, type, name, colIdx, row) {
    if (raw === null || raw === undefined || typeof raw === 'object') return ''
    if (extOn) {
      const d = formatCellValue(raw, type, name, { row, colIdx, fk: null })
      const shown = d?.display != null && String(d.display) !== String(raw) ? String(d.display) : ''
      const trail = typeof d?.trail?.text === 'string' ? d.trail.text : ''
      const text = [shown, trail].filter(Boolean).join(' ')
      if (text) return text
    }
    if (TIME_NAME.test(name)) {
      const t = String(raw).trim()
      const ms = /^\d{13}$/.test(t) ? Number(t) : /^\d{10}$/.test(t) ? Number(t) * 1000 : NaN
      if (Number.isFinite(ms)) {
        return `${new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })} · ${ago(ms)}`
      }
    }
    return ''
  }
  const extOn = $derived.by(() => { void $pluginState; return anyDisplayExtEnabled() })

  const fields = $derived.by(() => {
    if (columns.length === 0 || rows.length === 0) return []
    const row = rows[idx] ?? []
    const canEdit = !!onsave && primaryKey.length > 0 && !readonly
    /** @type {Array<Record<string, any>>} */
    const out = []
    columns.forEach((col, i) => {
      if (hiddenColumns.has(col.name)) return
      const raw = normalizeCellValue(row[i])
      const dataType = col.dataType ?? col.data_type ?? ''
      const normalType = normalizeColumnType(dataType)
      const enumValues = getColumnEnumValues(col)
      const isPk = pkSet.has(col.name)
      // Same rules as the row inspector: PKs, non-editable types and truncated
      // oversize cells stay read-only.
      const editable = canEdit && !isPk && isEditableType(dataType) && !oversizeCellInfo(raw)
      const isNull = raw === null
      const isEmpty = typeof raw === 'string' && raw === ''
      out.push({
        colIdx: i,
        name: col.name,
        dataType,
        raw,
        isNull,
        isEmpty,
        displayValue: isNull ? 'NULL' : isEmpty ? 'EMPTY' : formatNormalValue(raw),
        initialEditStr: valueToEditString(raw),
        editable,
        isPk,
        isBoolean: isBooleanType(dataType),
        enumValues,
        isMultiline: typeof raw === 'object' && raw !== null,
        isJsonType: normalType.startsWith('json'),
        // Precomputed: this used to run a /\n/g match per field on every
        // render, allocating an array each time just to count lines.
        textRows: Math.min(8, Math.max(2, (valueToEditString(raw).match(/\n/g)?.length ?? 0) + 1)),
        nullable: col.nullable !== false,
        hint: hintFor(raw, dataType, col.name, i, row),
      })
    })
    return out
  })

  const filteredFields = $derived(
    fieldSearch
      ? fields.filter(
          (f) =>
            f.name.toLowerCase().includes(fieldSearch.toLowerCase()) ||
            f.displayValue.toLowerCase().includes(fieldSearch.toLowerCase()),
        )
      : fields,
  )

  // ── Field editing (mirrors RowDetailPanel) ────────────────────────────────
  /** @type {Record<number, boolean>} */
  let savingFields = $state({})
  /** @type {Record<number, string | undefined>} */
  let fieldErrors = $state({})
  /** @type {Map<number, ReturnType<typeof setTimeout>>} */
  const debounceTimers = new Map()

  // Cancel pending debounced saves when the shown record changes.
  $effect(() => {
    void idx
    return () => {
      for (const t of debounceTimers.values()) clearTimeout(t)
      debounceTimers.clear()
      fieldErrors = {}
    }
  })

  /**
   * Stable string representation used for change-detection.
   * @param {unknown} v
   */
  function stableStr(v) {
    if (v === null || v === undefined) return '\x00NULL'
    if (typeof v === 'object') {
      try { return JSON.stringify(v) } catch { return String(v) }
    }
    return String(v)
  }

  /**
   * @param {number} colIdx
   * @param {string} rawStr
   * @param {boolean} [force]
   */
  async function saveField(colIdx, rawStr, force = false) {
    if (!onsave) return
    const col = columns[colIdx]
    if (!col) return
    const dataType = col.dataType ?? col.data_type ?? ''
    const enumValues = getColumnEnumValues(col)
    const result = parseCellInput(rawStr, dataType, enumValues)
    if (!result.ok) {
      fieldErrors = { ...fieldErrors, [colIdx]: result.message }
      return
    }
    fieldErrors = { ...fieldErrors, [colIdx]: undefined }
    if (!force) {
      const original = fields.find((f) => f.colIdx === colIdx)?.raw ?? null
      if (stableStr(result.value) === stableStr(original)) return
    }
    await commitValue(colIdx, result.value)
  }

  /**
   * @param {number} colIdx
   * @param {unknown} value
   */
  async function commitValue(colIdx, value) {
    if (!onsave) return
    const col = columns[colIdx]
    if (!col) return
    const t = debounceTimers.get(colIdx)
    if (t) { clearTimeout(t); debounceTimers.delete(colIdx) }
    savingFields = { ...savingFields, [colIdx]: true }
    try {
      await onsave({ rowIdx: idx, colIdx, value })
    } catch (e) {
      toast.error(`Failed to save ${col.name}`, { description: String(e) })
    } finally {
      savingFields = { ...savingFields, [colIdx]: false }
    }
  }

  /**
   * @param {number} colIdx
   * @param {string} rawStr
   */
  function scheduleFieldSave(colIdx, rawStr) {
    const existing = debounceTimers.get(colIdx)
    if (existing) clearTimeout(existing)
    const t = setTimeout(() => {
      debounceTimers.delete(colIdx)
      void saveField(colIdx, rawStr)
    }, 600)
    debounceTimers.set(colIdx, t)
  }

  /**
   * @param {number} colIdx
   * @param {KeyboardEvent} e
   */
  function handleFieldKeydown(colIdx, e) {
    const el = /** @type {HTMLInputElement | HTMLTextAreaElement} */ (e.currentTarget)
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      const existing = debounceTimers.get(colIdx)
      if (existing) { clearTimeout(existing); debounceTimers.delete(colIdx) }
      void saveField(colIdx, el.value)
      el.blur()
    }
    if (e.key === 'Escape') {
      const existing = debounceTimers.get(colIdx)
      if (existing) { clearTimeout(existing); debounceTimers.delete(colIdx) }
      el.blur()
    }
  }

  /**
   * @param {number} colIdx
   * @param {FocusEvent} e
   */
  function handleFieldBlur(colIdx, e) {
    const existing = debounceTimers.get(colIdx)
    if (existing) { clearTimeout(existing); debounceTimers.delete(colIdx) }
    const el = /** @type {HTMLInputElement | HTMLTextAreaElement} */ (e.currentTarget)
    void saveField(colIdx, el.value)
  }

  /** @param {{ raw: unknown, name: string }} field */
  async function copyFieldValue(field) {
    const text = field.raw === null ? 'NULL' : typeof field.raw === 'object' ? formatJsonValue(field.raw) : String(field.raw)
    try {
      await navigator.clipboard.writeText(text)
      toast.success(`Copied ${field.name}`)
    } catch {
      toast.error('Could not copy to clipboard')
    }
  }

  async function copyRecordJson() {
    /** @type {Record<string, unknown>} */
    const record = {}
    for (const f of fields) record[f.name] = f.raw
    try {
      await navigator.clipboard.writeText(formatJsonValue(record))
      toast.success('Copied record JSON')
    } catch {
      toast.error('Could not copy to clipboard')
    }
  }

  /** @param {KeyboardEvent} e */
  function handleKeydown(e) {
    const target = /** @type {HTMLElement} */ (e.target)
    if (target.closest('input, textarea, select')) return
    if (e.key === 'ArrowLeft' || e.key === 'PageUp') {
      e.preventDefault()
      goPrev()
    } else if (e.key === 'ArrowRight' || e.key === 'PageDown') {
      e.preventDefault()
      goNext()
    }
  }

  const navBtn =
    'inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-30'
  /** Where this record sits in the result, 0..1, for the position bar. */
  const position = $derived(total > 0 ? Math.min(1, (offset + idx + 1) / total) : rows.length ? (idx + 1) / rows.length : 0)
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-panel outline-none"
  role="group"
  aria-label="Record view"
  tabindex={0}
  onkeydown={handleKeydown}
>
  <!-- Navigation bar: the table toolbar's height and fields. -->
  <div class="studio-chrome relative flex h-11 shrink-0 items-center gap-1.5 border-b border-border/40 bg-panel px-3">
    <div class="flex items-center gap-0.5">
      <button type="button" class={navBtn} title="First record on page" aria-label="First record on page" disabled={idx === 0 || rows.length === 0} onclick={() => setIdx(0)}>
        <Icon name="chevrons-left" class="size-3.5 shrink-0" />
      </button>
      <button type="button" class={navBtn} title="Previous record (←)" aria-label="Previous record" disabled={!canPrev} onclick={goPrev}>
        <Icon name="chevron-left" class="size-4 shrink-0" />
      </button>
      <span class="min-w-[7.5rem] select-none px-1.5 text-center text-ui-sm tabular-nums text-muted-foreground">
        {#if rows.length}Record <span class="font-medium text-foreground">{(offset + idx + 1).toLocaleString('en-US')}</span> of {total < 0 ? '…' : total.toLocaleString('en-US')}{:else}No records{/if}
      </span>
      <button type="button" class={navBtn} title="Next record (→)" aria-label="Next record" disabled={!canNext} onclick={goNext}>
        <Icon name="chevron-right" class="size-4 shrink-0" />
      </button>
      <button type="button" class={navBtn} title="Last record on page" aria-label="Last record on page" disabled={rows.length === 0 || idx === rows.length - 1} onclick={() => setIdx(rows.length - 1)}>
        <Icon name="chevrons-right" class="size-3.5 shrink-0" />
      </button>
    </div>

    <span class="flex-1"></span>

    <div class="relative flex h-8 w-64 min-w-[10rem] shrink items-center">
      <Icon name="search" class="pointer-events-none absolute left-2.5 size-3.5 shrink-0 text-muted-foreground" />
      <input
        type="text"
        placeholder="Search fields and values"
        aria-label="Search fields"
        bind:value={fieldSearch}
        onkeydown={(e) => { if (e.key === 'Escape' && fieldSearch) { e.preventDefault(); e.stopPropagation(); fieldSearch = '' } }}
        class="field-surface h-8 w-full min-w-0 pl-8 pr-7 text-ui-sm text-foreground shadow-none outline-none placeholder:text-muted-foreground"
      />
      {#if fieldSearch}
        <button type="button" class="hit-area absolute right-1.5 inline-flex size-5 items-center justify-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Clear search" onclick={() => (fieldSearch = '')}>
          <Icon name="x" class="size-3 shrink-0" />
        </button>
      {/if}
    </div>
    <button
      type="button"
      title="Copy record as JSON"
      class="field-surface inline-flex h-8 shrink-0 items-center gap-1.5 px-2.5 text-ui-sm text-foreground/85 shadow-none transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40"
      disabled={rows.length === 0}
      onclick={copyRecordJson}
    >
      <Icon name="braces" class="size-3.5 shrink-0" />
      Copy JSON
    </button>

    <!-- Where this record sits in the result. -->
    <div class="pointer-events-none absolute inset-x-0 bottom-0 h-px bg-transparent" aria-hidden="true">
      <div class="h-full bg-primary/60 transition-[width] duration-200 ease-out" style="width: {position * 100}%"></div>
    </div>
  </div>

  <!-- Field list -->
  <div class="app-scroll min-h-0 flex-1 overflow-y-auto">
    {#if rows.length === 0}
      <div class="flex h-full items-center justify-center">
        <p class="text-ui-sm text-muted-foreground">No rows on this page</p>
      </div>
    {:else}
      <div class="mx-auto w-full max-w-4xl px-5 pb-5 pt-3">
        <!-- Icon, name, count, then a hairline to the edge: the list header the
             Config and Activity pages use. -->
        <div class="mb-1 flex items-center gap-2 px-3 py-1.5">
          <Icon name="columns-3" class="size-4 shrink-0 text-muted-foreground" />
          <h3 class="text-ui-sm text-muted-foreground">{fieldSearch ? 'Matching fields' : 'Fields'}</h3>
          <span class="text-ui-sm tabular-nums text-muted-foreground/70">{filteredFields.length}{fieldSearch ? ` of ${fields.length}` : ''}</span>
          <span class="ml-2 h-px flex-1 bg-border/50" aria-hidden="true"></span>
        </div>
        <!-- A label/value list, not a form. Every value used to sit in its own
             bordered box whether or not it was editable, which made twelve rows
             read as twelve inputs and gave the panel a ragged right edge: the
             actions column is `auto`, and an editable field has two buttons
             where a read-only one has one, so every row ended at a different x.
             Values are text now, the control appears when you click one, and the
             actions column is a fixed width so the value column is too. -->
        <dl class="flex flex-col gap-px">
          {#each filteredFields as field (field.colIdx)}
            {@const editing = editingIdx === field.colIdx}
            <div class={cn("group/field grid grid-cols-[minmax(8rem,13rem)_minmax(0,1fr)_3rem] items-start gap-x-4 rounded-md px-2.5 py-1 transition-colors [contain-intrinsic-size:auto_30px] [content-visibility:auto] hover:bg-muted/25", editing && "bg-muted/30")}>
              <!-- One line: name, key, and the type as a muted suffix. -->
              <dt class="flex min-w-0 items-baseline gap-1.5 py-1" title="{field.name} · {field.dataType || 'unknown'}{field.nullable ? '' : ' · not null'}">
                <span class="min-w-0 truncate font-mono text-ui-xs text-foreground">{field.name}</span>
                {#if field.isPk}
                  <Icon name="key-round" class="size-3 shrink-0 self-center text-warning" aria-label="Primary key" />
                {/if}
                <span class="ml-auto min-w-0 shrink truncate text-ui-2xs text-muted-foreground/70">{field.dataType}</span>
              </dt>

              <dd class="relative min-w-0">
                {#if field.editable && editing}
                  {#if field.isBoolean || field.enumValues}
                    <div use:focusOnMount>
                      <FieldSelect
                        class="h-6 w-full bg-transparent text-ui-xs"
                        value={field.initialEditStr}
                        disabled={savingFields[field.colIdx]}
                        onchange={(v) => { void saveField(field.colIdx, v); editingIdx = null }}
                        options={[
                          ...(field.nullable ? [{ value: '', label: 'NULL' }] : []),
                          ...(field.isBoolean
                            ? [{ value: 'true', label: 'true' }, { value: 'false', label: 'false' }]
                            : field.enumValues.map((/** @type {string} */ opt) => ({ value: opt, label: opt }))),
                        ]}
                      />
                    </div>
                  {:else if field.isMultiline}
                    <textarea
                      use:focusOnMount
                      value={field.initialEditStr}
                      rows={field.textRows}
                      disabled={savingFields[field.colIdx]}
                      placeholder={field.isNull ? 'NULL' : ''}
                      class={cn(
                        'no-field-frame block w-full resize-none rounded-sm border border-ring/70 bg-background/60 px-1.5 py-0.5 font-mono text-ui-xs leading-5 text-foreground shadow-none outline-none placeholder:text-muted-foreground disabled:opacity-50',
                        fieldErrors[field.colIdx] && 'border-destructive',
                      )}
                      oninput={(e) => scheduleFieldSave(field.colIdx, /** @type {HTMLTextAreaElement} */ (e.currentTarget).value)}
                      onblur={(e) => { handleFieldBlur(field.colIdx, e); editingIdx = null }}
                      onkeydown={(e) => handleFieldKeydown(field.colIdx, e)}
                    ></textarea>
                  {:else}
                    <!-- A textarea sized to its text, in the read-only value's exact
                         box (border, padding, line height): clicking a value no
                         longer changes the row's height. Enter still saves. -->
                    <textarea
                      use:focusOnMount
                      use:autosize
                      rows="1"
                      value={field.initialEditStr}
                      disabled={savingFields[field.colIdx]}
                      placeholder={field.isNull ? 'NULL' : ''}
                      class={cn(
                        'no-field-frame block w-full resize-none overflow-hidden rounded-sm border border-ring/70 bg-background/60 px-1.5 py-0.5 font-mono text-ui-xs leading-5 break-all text-foreground shadow-none outline-none placeholder:text-muted-foreground disabled:opacity-50',
                        fieldErrors[field.colIdx] && 'border-destructive',
                      )}
                      oninput={(e) => scheduleFieldSave(field.colIdx, /** @type {HTMLTextAreaElement} */ (e.currentTarget).value)}
                      onblur={(e) => { handleFieldBlur(field.colIdx, e); editingIdx = null }}
                      onkeydown={(e) => handleFieldKeydown(field.colIdx, e)}
                    ></textarea>
                  {/if}
                {:else}
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div
                    role={field.editable ? 'button' : undefined}
                    tabindex={field.editable ? 0 : undefined}
                    aria-label={field.editable ? `Edit ${field.name}` : undefined}
                    class={cn(
                      'w-full rounded-sm border border-transparent px-1.5 py-0.5 font-mono text-ui-xs leading-5',
                      field.editable && 'cursor-text hover:border-field-border hover:bg-background/40',
                      field.isNull || field.isEmpty ? 'text-muted-foreground/70' : 'text-foreground',
                      field.isMultiline ? 'line-clamp-3 break-all whitespace-pre-wrap' : 'line-clamp-2 break-all',
                    )}
                    title={field.displayValue}
                    onclick={() => { if (field.editable) editingIdx = field.colIdx }}
                    onkeydown={(e) => {
                      if (!field.editable) return
                      if (e.key !== 'Enter' && e.key !== ' ') return
                      e.preventDefault()
                      editingIdx = field.colIdx
                    }}
                  >{#if field.isNull}<span class="rounded-sm bg-muted/60 px-1 text-ui-2xs">NULL</span>{:else if field.isEmpty}<span class="rounded-sm bg-muted/60 px-1 text-ui-2xs">empty</span>{:else}{field.displayValue}{/if}{#if field.hint}<span class="ml-2 font-sans text-ui-2xs text-muted-foreground" title={field.hint}>{field.hint}</span>{/if}</div>
                {/if}

                {#if savingFields[field.colIdx]}
                  <Icon name="loader-2" class="absolute -left-4 top-1.5 size-3 animate-spin text-muted-foreground" />
                {/if}
                {#if fieldErrors[field.colIdx]}
                  <p class="mt-0.5 font-mono text-ui-3xs text-destructive">{fieldErrors[field.colIdx]}</p>
                {/if}
              </dd>

              <!-- Fixed width, so a row with a Set-NULL button and one without
                   still end their value column at the same x. -->
              <div class="flex items-center justify-end gap-0.5 pt-0.5">
                <button
                  type="button"
                  class="hit-area opacity-0 inline-flex size-5 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-opacity hover:bg-accent hover:text-foreground group-hover/field:opacity-100"
                  title="Copy value"
                  aria-label="Copy {field.name}"
                  onclick={() => void copyFieldValue(field)}
                >
                  <Icon name="copy" class="size-3 shrink-0" />
                </button>
                {#if field.editable && field.nullable}
                  <button
                    type="button"
                    class="hit-area opacity-0 inline-flex size-5 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-opacity hover:bg-accent hover:text-foreground group-hover/field:opacity-100"
                    title="Set NULL"
                    aria-label="Set {field.name} to NULL"
                    disabled={field.isNull || savingFields[field.colIdx]}
                    onclick={() => void commitValue(field.colIdx, null)}
                  >
                    <Icon name="eraser" class="size-3 shrink-0" />
                  </button>
                {/if}
              </div>
            </div>
          {/each}

          {#if filteredFields.length === 0 && fieldSearch}
            <p class="px-3 py-10 text-center text-ui-sm text-muted-foreground">No fields match "{fieldSearch}"</p>
          {/if}
        </dl>
        <p class="mt-4 flex select-none flex-wrap items-center justify-center gap-x-3 gap-y-1 text-ui-xs text-muted-foreground/80">
          <span><kbd class="rounded-sm border border-border/60 px-1 font-sans text-ui-2xs">←</kbd> <kbd class="rounded-sm border border-border/60 px-1 font-sans text-ui-2xs">→</kbd> records</span>
          {#if onsave && !readonly}
            <span>Click a value to edit</span>
            <span><kbd class="rounded-sm border border-border/60 px-1 font-sans text-ui-2xs">Enter</kbd> saves</span>
            <span><kbd class="rounded-sm border border-border/60 px-1 font-sans text-ui-2xs">Esc</kbd> reverts</span>
          {/if}
        </p>
      </div>
    {/if}
  </div>
</div>
