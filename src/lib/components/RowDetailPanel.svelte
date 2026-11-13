<script>
  import FieldSelect from './FieldSelect.svelte';
  import { getAppScale } from '$lib/app-zoom.js'
  import X from '@lucide/svelte/icons/x'
  import Copy from '@lucide/svelte/icons/copy'
  import Search from '@lucide/svelte/icons/search'
  import Loader from '@lucide/svelte/icons/loader'
  import Ellipsis from '@lucide/svelte/icons/ellipsis'
  import CircleSlash from '@lucide/svelte/icons/circle-slash'
  import Eraser from '@lucide/svelte/icons/eraser'
  import Braces from '@lucide/svelte/icons/braces'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import ShikiBlock from './ShikiBlock.svelte'
  import ResizeHandle from './ResizeHandle.svelte'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import {
    normalizeCellValue,
    formatJsonValue,
    formatNormalValue,
    formatNormalRecord,
    formatNormalRecords,
    rowToRecord,
    rowsToJsonPayload,
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
  import {
    clampInspectorWidth,
    loadLayout,
    saveLayout,
  } from '$lib/stores/layout.js'
  import { formatCompactCount } from '$lib/table-list.js'
  import { cellLinkHref, cellUrlType } from '$lib/cell-display.js'
  import ExternalLink from '@lucide/svelte/icons/external-link'

  /**
   * @typedef {{ kind: 'cell', rowIdx: number, colIdx: number } | { kind: 'row', rowIdx: number } | { kind: 'rows', rowIndices: number[] }} InspectorTarget
   */

  let {
    columns = [],
    rows = [],
    primaryKey = [],
    target = null,
    onclose = () => {},
    onsave = null,
    /** Bumped by the parent when a row changes in place (`rows` is $state.raw). */
    dataVersion = 0,
  } = $props()

  const initialLayout = loadLayout()
  let width = $state(initialLayout.inspectorWidth)
  let resizeStartWidth = initialLayout.inspectorWidth
  /** App scale sampled at drag start - `dx` is screen px, `width` is px at 100%. */
  let resizeScale = 1
  /** @type {'details' | 'normal' | 'json' | 'preview'} */
  let viewMode = $state(initialLayout.inspectorView)
  let fieldSearch = $state('')

  /** @type {Record<number, boolean>} */
  let savingFields = $state({})
  /** @type {Record<number, string | undefined>} */
  let fieldErrors = $state({})
  /** @type {Map<number, ReturnType<typeof setTimeout>>} */
  const debounceTimers = new Map()

  const meta = $derived.by(() => {
    void dataVersion
    if (!target || columns.length === 0) return null

    if (target.kind === 'cell') {
      const col = columns[target.colIdx]
      if (!col) return null
      const value = rows[target.rowIdx]?.[target.colIdx]
      return {
        title: col.name,
        subtitle: col.dataType ?? col.data_type ?? 'unknown',
        badge: `Row ${target.rowIdx + 1}`,
        normalText: formatNormalValue(value),
        jsonText: formatJsonValue(value),
      }
    }

    if (target.kind === 'row') {
      const record = rowToRecord(columns, rows[target.rowIdx] ?? [])
      return {
        title: 'Row',
        subtitle: `${columns.length} fields`,
        badge: `Row ${target.rowIdx + 1}`,
        normalText: formatNormalRecord(record),
        jsonText: formatJsonValue(record),
      }
    }

    const payload = rowsToJsonPayload(columns, rows, target.rowIndices)
    const records = target.rowIndices.map((idx) => rowToRecord(columns, rows[idx] ?? []))
    return {
      title: 'Selection',
      subtitle: `${formatCompactCount(target.rowIndices.length)} rows`,
      badge: null,
      normalText: formatNormalRecords(records),
      jsonText: formatJsonValue(payload),
    }
  })

  const displayCode = $derived(
    meta ? (viewMode === 'json' ? meta.jsonText : meta.normalText) : '',
  )
  const shikiLang = $derived(viewMode === 'json' ? 'json' : 'plaintext')

  const cellPreviewHref = $derived.by(() => {
    void dataVersion
    if (!target || target.kind !== 'cell') return null
    const value = rows[target.rowIdx]?.[target.colIdx]
    if (typeof value !== 'string') return null
    return cellLinkHref(value)
  })
  const cellPreviewType = $derived(cellUrlType(cellPreviewHref))
  const hasPreview = $derived(cellPreviewType === 'image' || cellPreviewType === 'pdf')

  // Field list for row details view
  const pkSet = $derived(new Set(primaryKey))

  const fields = $derived.by(() => {
    void dataVersion
    if (!target || target.kind !== 'row' || columns.length === 0) return []
    const row = rows[target.rowIdx] ?? []
    const canEdit = !!onsave && primaryKey.length > 0
    return columns.map((col, i) => {
      const raw = normalizeCellValue(row[i])
      const dataType = col.dataType ?? col.data_type ?? ''
      const normalType = normalizeColumnType(dataType)
      const enumValues = getColumnEnumValues(col)
      const isBoolean = isBooleanType(dataType)
      const isPk = pkSet.has(col.name)
      // PK columns, non-editable types, and truncated oversize cells (only a
      // preview was loaded - saving would write it back) are read-only.
      const editable = canEdit && !isPk && isEditableType(dataType) && !oversizeCellInfo(raw)
      const isNull = raw === null
      const isEmpty = typeof raw === 'string' && raw === ''
      const isJsonType = normalType.startsWith('json')
      const isTextType = /^(text|varchar|char|character|string|citext|uuid|name)/.test(normalType)
      const isMultiline = typeof raw === 'object' && raw !== null
      const nullable = col.nullable !== false
      const initialEditStr = valueToEditString(raw)
      return {
        colIdx: i,
        name: col.name,
        dataType,
        raw,
        isNull,
        isEmpty,
        displayValue: isNull ? 'NULL' : (isEmpty ? 'EMPTY' : formatNormalValue(raw)),
        initialEditStr,
        editable,
        isPk,
        isBoolean,
        enumValues,
        isMultiline,
        isJsonType,
        isTextType,
        nullable,
      }
    })
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

  $effect(() => {
    if (target?.kind === 'row' && viewMode !== 'json') viewMode = 'details'
    if (target?.kind !== 'row' && viewMode === 'details') viewMode = 'normal'
  })

  // Cancel all pending debounce timers when the inspected row changes.
  $effect(() => {
    void (target?.kind === 'row' ? target.rowIdx : null)
    return () => {
      for (const t of debounceTimers.values()) clearTimeout(t)
      debounceTimers.clear()
    }
  })

  $effect(() => {
    if (viewMode === 'preview' && !hasPreview) viewMode = 'normal'
  })

  $effect(() => {
    if (viewMode !== 'preview' && viewMode !== 'details')
      saveLayout({ inspectorView: viewMode })
  })

  async function openPreviewExternal() {
    if (!cellPreviewHref) return
    const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
    if (isTauri) {
      const { openUrl: open } = await import('@tauri-apps/plugin-opener')
      await open(cellPreviewHref)
    } else {
      window.open(cellPreviewHref, '_blank', 'noopener,noreferrer')
    }
  }

  async function copyJson() {
    if (!meta?.jsonText) return
    try {
      await navigator.clipboard.writeText(meta.jsonText)
      toast.success('Copied JSON')
    } catch {
      toast.error('Could not copy to clipboard')
    }
  }

  /**
   * Stable string representation used for change-detection.
   * Objects/arrays are compared via JSON so key-order doesn't matter.
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
   * @param {boolean} [force] skip change-detection (used for dropdown actions)
   */
  async function saveField(colIdx, rawStr, force = false) {
    if (!onsave || target?.kind !== 'row') return
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
    // Skip if value didn't change
    if (!force) {
      const original = fields.find((f) => f.colIdx === colIdx)?.raw ?? null
      if (stableStr(result.value) === stableStr(original)) return
    }
    await commitValue(colIdx, result.value)
  }

  /**
   * Direct value save, bypassing string parsing (for dropdown actions).
   * @param {number} colIdx
   * @param {unknown} value
   */
  async function commitValue(colIdx, value) {
    if (!onsave || target?.kind !== 'row') return
    const col = columns[colIdx]
    if (!col) return
    // Cancel any pending debounce for this field
    const t = debounceTimers.get(colIdx)
    if (t) { clearTimeout(t); debounceTimers.delete(colIdx) }
    savingFields = { ...savingFields, [colIdx]: true }
    try {
      await onsave({ rowIdx: target.rowIdx, colIdx, value })
    } catch (e) {
      toast.error(`Failed to save ${col.name}`, { description: String(e) })
    } finally {
      savingFields = { ...savingFields, [colIdx]: false }
    }
  }

  /**
   * Schedule a debounced save (600 ms). Resets on every keystroke.
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
   * On blur: flush any pending debounce and save immediately.
   * @param {number} colIdx
   * @param {FocusEvent} e
   */
  function handleFieldBlur(colIdx, e) {
    const existing = debounceTimers.get(colIdx)
    if (existing) { clearTimeout(existing); debounceTimers.delete(colIdx) }
    const el = /** @type {HTMLInputElement | HTMLTextAreaElement} */ (e.currentTarget)
    void saveField(colIdx, el.value)
  }

  /**
   * @param {{ raw: unknown, initialEditStr: string, isJsonType: boolean, colIdx: number, name: string }} field
   */
  async function copyFieldValue(field) {
    const text = field.raw === null ? 'NULL' : String(field.raw)
    try {
      await navigator.clipboard.writeText(text)
      toast.success(`Copied ${field.name}`)
    } catch {
      toast.error('Could not copy to clipboard')
    }
  }

  /**
   * @param {{ colIdx: number, initialEditStr: string, isJsonType: boolean }} field
   */
  async function prettyFormat(field) {
    if (!field.isJsonType) return
    try {
      const parsed = JSON.parse(field.initialEditStr)
      const pretty = JSON.stringify(parsed, null, 2)
      await commitValue(field.colIdx, pretty)
    } catch {
      toast.error('Invalid JSON, cannot pretty-format')
    }
  }
</script>

{#if target && meta}
  <div class="flex h-full shrink-0" style:width="calc({width}px * var(--app-scale, 1))">
    <ResizeHandle
      edge="start"
      onresizestart={() => {
        resizeScale = getAppScale()
        resizeStartWidth = width
      }}
      onresize={(dx) => {
        width = clampInspectorWidth(resizeStartWidth + dx / resizeScale)
      }}
      onresizeend={() => {
        resizeStartWidth = width
        saveLayout({ inspectorWidth: width })
      }}
    />
    <aside
      class="flex h-full min-w-0 flex-1 flex-col border-l border-border bg-sidebar text-sidebar-foreground"
    >
      <header
        class="studio-chrome flex shrink-0 items-start gap-2 border-b border-sidebar-border px-3 py-2.5"
        data-studio-chrome
      >
        <div class="min-w-0 flex-1">
          <p class="truncate font-mono text-ui-sm font-medium text-foreground">{meta.title}</p>
          <p class="truncate font-mono text-ui-2xs text-muted-foreground">{meta.subtitle}</p>
          {#if meta.badge}
            <p class="mt-0.5 font-mono text-ui-2xs text-muted-foreground">{meta.badge}</p>
          {/if}
        </div>
        <button
          type="button"
          class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="Copy JSON"
          onclick={copyJson}
        >
          <Copy class="size-3.5 shrink-0" />
        </button>
        <button
          type="button"
          class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="Close panel (Esc)"
          onclick={onclose}
        >
          <X class="size-3.5 shrink-0" />
        </button>
      </header>

      <Tabs.Root bind:value={viewMode} class="flex min-h-0 flex-1 flex-col">
        <Tabs.List class="studio-chrome mx-2 mt-2 h-8 w-auto shrink-0" data-studio-chrome>
          {#if target?.kind === 'row'}
            <Tabs.Trigger value="details" class="px-3 text-ui-xs">Details</Tabs.Trigger>
            <Tabs.Trigger value="json" class="px-3 text-ui-xs">JSON</Tabs.Trigger>
          {:else}
            <Tabs.Trigger value="normal" class="px-3 text-ui-xs">Normal</Tabs.Trigger>
            <Tabs.Trigger value="json" class="px-3 text-ui-xs">JSON</Tabs.Trigger>
            {#if hasPreview}
              <Tabs.Trigger value="preview" class="px-3 text-ui-xs">Preview</Tabs.Trigger>
            {/if}
          {/if}
        </Tabs.List>

        <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
          {#if viewMode === 'details' && target?.kind === 'row'}
            <div class="flex min-h-0 flex-1 flex-col">
              <!-- Search -->
              <div class="shrink-0 px-2.5 pb-1.5 pt-2">
                <div class="relative flex items-center">
                  <Search class="pointer-events-none absolute left-2.5 size-3.5 text-muted-foreground" />
                  <input
                    type="text"
                    placeholder="Search fields and values"
                    aria-label="Search fields"
                    bind:value={fieldSearch}
                    onkeydown={(e) => { if (e.key === 'Escape' && fieldSearch) { e.preventDefault(); e.stopPropagation(); fieldSearch = '' } }}
                    class="field-surface h-8 w-full min-w-0 pl-8 pr-2.5 text-ui-sm text-foreground shadow-none outline-none placeholder:text-muted-foreground"
                  />
                </div>
              </div>

              <!-- Field list -->
              <div class="app-scroll flex-1 overflow-y-auto px-1.5 pb-3">
                {#key target.rowIdx}
                  {#each filteredFields as field (field.colIdx)}
                    <!-- The record view's row, stacked for a narrow panel: name, key and
                         type on one line, the value under it as text that becomes a
                         field only on hover and focus. -->
                    <div class={`group/field rounded-md px-1 py-1 transition-colors hover:bg-muted/25${fields.length > 100 ? ' [content-visibility:auto] [contain-intrinsic-size:auto_46px]' : ''}`}>
                      <div class="flex h-5 items-center gap-1.5 px-1.5">
                        <span class="min-w-0 truncate font-mono text-ui-xs text-foreground/90">{field.name}</span>
                        {#if field.isPk}
                          <KeyRound class="size-3 shrink-0 text-warning" aria-label="Primary key, cannot be changed" />
                        {/if}
                        <span class="ml-auto min-w-0 shrink truncate text-ui-2xs text-muted-foreground/70" title={field.dataType}>{field.dataType}</span>

                        {#if field.editable}
                          <DropdownMenu.Root>
                            <DropdownMenu.Trigger
                              class="opacity-0 ml-0.5 inline-flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground group-hover/field:opacity-100"
                              aria-label="Field actions"
                            >
                              <Ellipsis class="size-3 shrink-0" />
                            </DropdownMenu.Trigger>
                            <DropdownMenu.Content class="min-w-40 text-ui-xs" align="end" sideOffset={4}>
                              <DropdownMenu.Item
                                class="gap-2 text-ui-xs"
                                disabled={savingFields[field.colIdx]}
                                onSelect={() => void copyFieldValue(field)}
                              >
                                <Copy class="size-3.5 shrink-0 text-muted-foreground" />
                                Copy value
                              </DropdownMenu.Item>

                              {#if field.nullable}
                                <DropdownMenu.Separator />
                                <DropdownMenu.Item
                                  class="gap-2 text-ui-xs"
                                  disabled={field.isNull || savingFields[field.colIdx]}
                                  onSelect={() => void commitValue(field.colIdx, null)}
                                >
                                  <CircleSlash class="size-3.5 shrink-0 text-muted-foreground" />
                                  Set NULL
                                </DropdownMenu.Item>
                              {/if}

                              {#if field.isTextType || field.isBoolean === false && !field.enumValues && !field.isJsonType}
                                <DropdownMenu.Item
                                  class="gap-2 text-ui-xs"
                                  disabled={field.isEmpty || savingFields[field.colIdx]}
                                  onSelect={() => void commitValue(field.colIdx, '')}
                                >
                                  <Eraser class="size-3.5 shrink-0 text-muted-foreground" />
                                  Set empty
                                </DropdownMenu.Item>
                              {/if}

                              {#if field.isJsonType}
                                <DropdownMenu.Item
                                  class="gap-2 text-ui-xs"
                                  disabled={field.isNull || savingFields[field.colIdx]}
                                  onSelect={() => void prettyFormat(field)}
                                >
                                  <Braces class="size-3.5 shrink-0 text-muted-foreground" />
                                  Pretty format
                                </DropdownMenu.Item>
                              {/if}
                            </DropdownMenu.Content>
                          </DropdownMenu.Root>
                        {:else}
                          <!-- Non-editable: copy only -->
                          <button
                            type="button"
                            class="hit-area opacity-0 ml-0.5 inline-flex size-4 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground group-hover/field:opacity-100"
                            title="Copy value"
                            onclick={() => copyFieldValue(field)}
                          >
                            <Copy class="size-3 shrink-0" />
                          </button>
                        {/if}
                      </div>

                      <!-- Value control -->
                      {#if !field.editable}
                        <!-- Read-only -->
                        <div
                          class="w-full px-1.5 font-mono text-ui-xs leading-5 {field.isNull || field.isEmpty ? 'text-muted-foreground/70' : 'text-foreground'} {field.isMultiline ? 'line-clamp-6 whitespace-pre-wrap break-all' : 'line-clamp-2 break-all'}"
                          title={field.displayValue}
                        >{#if field.isNull}<span class="rounded-sm bg-muted/60 px-1 text-ui-2xs">NULL</span>{:else if field.isEmpty}<span class="rounded-sm bg-muted/60 px-1 text-ui-2xs">empty</span>{:else}{field.displayValue}{/if}</div>

                      {:else if field.isBoolean}
                        <div class="relative">
                          <FieldSelect
                            class="h-6 w-full bg-transparent text-ui-xs"
                            value={field.initialEditStr}
                            disabled={savingFields[field.colIdx]}
                            onchange={(v) => void saveField(field.colIdx, v)}
                            options={[
                              { value: '', label: 'NULL' },
                              { value: 'true', label: 'true' },
                              { value: 'false', label: 'false' },
                            ]}
                          />
                          {#if savingFields[field.colIdx]}
                            <span class="pointer-events-none absolute right-7 top-1/2 -translate-y-1/2 text-muted-foreground">
                              <Loader class="size-3 shrink-0 animate-spin" />
                            </span>
                          {/if}
                        </div>

                      {:else if field.enumValues}
                        <div class="relative">
                          <FieldSelect
                            class="h-6 w-full bg-transparent text-ui-xs"
                            value={field.initialEditStr}
                            disabled={savingFields[field.colIdx]}
                            onchange={(v) => void saveField(field.colIdx, v)}
                            options={[
                              { value: '', label: 'NULL' },
                              ...field.enumValues.map((/** @type {string} */ o) => ({ value: o, label: o })),
                            ]}
                          />
                          {#if savingFields[field.colIdx]}
                            <span class="pointer-events-none absolute right-7 top-1/2 -translate-y-1/2 text-muted-foreground">
                              <Loader class="size-3 shrink-0 animate-spin" />
                            </span>
                          {/if}
                        </div>

                      {:else if field.isMultiline}
                        <div class="relative">
                          <textarea
                            value={field.initialEditStr}
                            rows={Math.min(6, Math.max(2, (field.initialEditStr.match(/\n/g)?.length ?? 0) + 1))}
                            disabled={savingFields[field.colIdx]}
                            placeholder={field.isNull ? 'NULL' : ''}
                            class="no-field-frame block w-full resize-none py-0.5 rounded-sm border bg-transparent px-1.5 font-mono text-ui-xs leading-5 text-foreground shadow-none outline-none transition-colors placeholder:text-muted-foreground hover:border-field-border hover:bg-background/40 focus:border-ring/70 focus:bg-background/60 disabled:opacity-50 {fieldErrors[field.colIdx] ? 'border-destructive' : 'border-transparent'}"
                            oninput={(e) => scheduleFieldSave(field.colIdx, /** @type {HTMLTextAreaElement} */ (e.currentTarget).value)}
                            onblur={(e) => handleFieldBlur(field.colIdx, e)}
                            onkeydown={(e) => handleFieldKeydown(field.colIdx, e)}
                          ></textarea>
                          {#if savingFields[field.colIdx]}
                            <Loader class="absolute right-2 top-2 size-3 animate-spin text-muted-foreground" />
                          {/if}
                        </div>

                      {:else}
                        <div class="relative">
                          <input
                            type="text"
                            value={field.initialEditStr}
                            disabled={savingFields[field.colIdx]}
                            placeholder={field.isNull ? 'NULL' : ''}
                            class="no-field-frame block h-6 w-full rounded-sm border bg-transparent px-1.5 font-mono text-ui-xs leading-5 text-foreground shadow-none outline-none transition-colors placeholder:text-muted-foreground hover:border-field-border hover:bg-background/40 focus:border-ring/70 focus:bg-background/60 disabled:opacity-50 {fieldErrors[field.colIdx] ? 'border-destructive' : 'border-transparent'}"
                            oninput={(e) => scheduleFieldSave(field.colIdx, /** @type {HTMLInputElement} */ (e.currentTarget).value)}
                            onblur={(e) => handleFieldBlur(field.colIdx, e)}
                            onkeydown={(e) => handleFieldKeydown(field.colIdx, e)}
                          />
                          {#if savingFields[field.colIdx]}
                            <Loader class="absolute right-2 top-1/2 size-3 -translate-y-1/2 animate-spin text-muted-foreground" />
                          {/if}
                        </div>
                      {/if}

                      {#if fieldErrors[field.colIdx]}
                        <p class="mt-0.5 px-1.5 text-ui-2xs text-destructive">{fieldErrors[field.colIdx]}</p>
                      {/if}
                    </div>
                  {/each}

                  {#if filteredFields.length === 0 && fieldSearch}
                    <p class="py-8 text-center text-ui-sm text-muted-foreground">No fields match "{fieldSearch}"</p>
                  {/if}
                {/key}
              </div>
            </div>

          {:else if viewMode === 'preview' && cellPreviewHref}
            <div class="flex min-h-0 flex-1 flex-col items-center justify-start gap-3 overflow-auto p-3">
              {#if cellPreviewType === 'image'}
                <img
                  src={cellPreviewHref}
                  alt=""
                  class="max-w-full rounded border border-border object-contain shadow-sm"
                  style="max-height: calc(100% - 2.5rem)"
                />
              {:else if cellPreviewType === 'pdf'}
                <div class="flex flex-col items-center gap-3 rounded-lg border border-border bg-muted/30 px-6 py-8 text-center">
                  <svg class="size-10 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                    <polyline points="14 2 14 8 20 8"/>
                    <path d="M9 13h1.5a1.5 1.5 0 0 1 0 3H9v-3z"/>
                    <path d="M13 13h2v1.5h-2V13z"/>
                    <path d="M16 13v3"/>
                  </svg>
                  <p class="text-ui-sm font-medium text-foreground">PDF Document</p>
                  <p class="text-ui-xs text-muted-foreground">Opens in your system PDF viewer</p>
                </div>
              {/if}
              <button
                type="button"
                class="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-border px-2.5 py-1.5 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                onclick={openPreviewExternal}
              >
                <ExternalLink class="size-3 shrink-0" />
                Open in browser
              </button>
            </div>

          {:else}
            <ShikiBlock
              code={displayCode}
              lang={shikiLang}
              jsonInteractive={viewMode === 'json'}
            />
          {/if}
        </div>
      </Tabs.Root>
    </aside>
  </div>
{/if}
