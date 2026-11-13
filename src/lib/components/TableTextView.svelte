<script module>
  // Column picks per table, module-scoped like the JSON view's JSONPath: they
  // survive switching views and tabs, and never leak from one table to another.
  /** @type {Map<string, string[]>} */
  const hiddenByTable = new Map()
</script>

<script>
  import { onDestroy } from 'svelte'
  import { Popover, PopoverTrigger, PopoverContent } from '$lib/components/ui/popover/index.js'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'
  import CodeTextView from './CodeTextView.svelte'
  import {
    rowsToCsv,
    rowsToTsv,
    rowsToMarkdown,
    rowsToJsonl,
    rowsToJson,
    rowsToSql,
    buildExportFilename,
    saveExportFile,
  } from '$lib/export.js'

  /**
   * Text mode for the data table - CSV / TSV / Markdown / JSON Lines views of
   * the current page, rendered through the shared read-only CodeMirror surface
   * (smooth virtualized scrolling, ⌘F find, full selection). CSV/TSV get
   * colored values via a tiny Monarch tokenizer; Markdown tables are
   * column-aligned and copy exactly what's displayed.
   */
  let {
    /** @type {Array<{ name: string }>} */
    columns = [],
    /** @type {unknown[][]} */
    rows = [],
    /** @type {string | null} */
    tableName = null,
  } = $props()


  /** @typedef {'csv' | 'tsv' | 'md' | 'jsonl' | 'json' | 'sql'} Format */
  /** @type {Array<{ id: Format, label: string, lang: string, header: boolean }>} */
  const FORMATS = [
    { id: 'csv', label: 'CSV', lang: 'csv', header: true },
    { id: 'tsv', label: 'TSV', lang: 'tsv', header: true },
    { id: 'md', label: 'Markdown', lang: 'markdown', header: true },
    { id: 'json', label: 'JSON', lang: 'json', header: false },
    { id: 'jsonl', label: 'JSON Lines', lang: 'json', header: false },
    { id: 'sql', label: 'SQL', lang: 'sql', header: false },
  ]

  const STORAGE_KEY = 'stroke:text-view-format'

  /** @returns {Format} */
  function loadFormat() {
    try {
      const v = localStorage.getItem(STORAGE_KEY)
      const found = FORMATS.find((f) => f.id === v)
      return found ? found.id : 'csv'
    } catch {
      return 'csv'
    }
  }

  let format = $state(loadFormat())

  /** @param {Format} next */
  function setFormat(next) {
    format = next
    try {
      localStorage.setItem(STORAGE_KEY, next)
    } catch {
      // localStorage unavailable - format just won't persist
    }
  }

  /**
   * How many rows the on-screen text is built from.
   *
   * This view serialises the whole page into ONE string and hands it to the editor,
   * which then holds a second copy in its model - so a page of 200k rows costs
   * the row array, a multi-megabyte string, and the editor's own document, all built
   * synchronously on the main thread. Nobody reads 200k rows of CSV by eye; the
   * cap keeps the view instant and Copy/Download still use the full page, which
   * is what those are for.
   */
  const PREVIEW_ROWS = 5000
  const truncated = $derived(rows.length > PREVIEW_ROWS)
  const previewRows = $derived(truncated ? rows.slice(0, PREVIEW_ROWS) : rows)

  // ── Options: which columns, header row, wrap ──────────────────────────────
  const OPTS_KEY = 'stroke:text-view-opts'
  function loadOpts() {
    try {
      const o = JSON.parse(localStorage.getItem(OPTS_KEY) ?? '{}')
      return { header: o.header !== false, wrap: o.wrap === true }
    } catch {
      return { header: true, wrap: false }
    }
  }
  const _opts = loadOpts()
  let includeHeader = $state(_opts.header)
  let wrap = $state(_opts.wrap)
  $effect(() => {
    const v = JSON.stringify({ header: includeHeader, wrap })
    try { localStorage.setItem(OPTS_KEY, v) } catch {}
  })

  const tableKey = $derived(tableName ?? '')
  let hidden = $state(/** @type {Set<string>} */ (new Set()))
  // Restore this table's picks whenever the table changes.
  $effect(() => {
    hidden = new Set(hiddenByTable.get(tableKey) ?? [])
  })
  /** @param {Set<string>} next */
  function setHidden(next) {
    hidden = next
    hiddenByTable.set(tableKey, [...next])
  }
  /** @param {string} name */
  function toggleColumn(name) {
    const next = new Set(hidden)
    if (next.has(name)) next.delete(name)
    else next.add(name)
    setHidden(next)
  }
  let columnQuery = $state('')
  const pickable = $derived(
    columnQuery.trim()
      ? columns.filter((c) => c.name.toLowerCase().includes(columnQuery.trim().toLowerCase()))
      : columns,
  )

  /** Indices of the columns that are shown, in table order. */
  const keep = $derived(columns.map((c, i) => (hidden.has(c.name) ? -1 : i)).filter((i) => i >= 0))
  const shownColumns = $derived(keep.length === columns.length ? columns : keep.map((i) => columns[i]))
  /** @param {unknown[][]} src */
  function project(src) {
    if (keep.length === columns.length) return src
    return src.map((r) => keep.map((i) => r[i]))
  }

  const formatHasHeader = $derived(FORMATS.find((f) => f.id === format)?.header ?? false)

  /** Drop the first `n` lines without splitting the whole document. @param {string} t @param {number} n */
  function dropLines(t, n) {
    let at = 0
    for (let k = 0; k < n; k++) {
      const nl = t.indexOf('\n', at)
      if (nl < 0) return ''
      at = nl + 1
    }
    return t.slice(at)
  }

  /** @param {unknown[][]} src */
  function serialise(src) {
    if (shownColumns.length === 0) return ''
    const rowsOut = project(src)
    /** @type {string} */
    let out
    if (format === 'tsv') out = rowsToTsv(shownColumns, rowsOut)
    else if (format === 'md') out = rowsToMarkdown(shownColumns, rowsOut)
    else if (format === 'jsonl') out = rowsToJsonl(shownColumns, rowsOut)
    else if (format === 'json') out = rowsToJson(shownColumns, rowsOut)
    else if (format === 'sql') out = rowsToSql(shownColumns, rowsOut, tableName ?? 'exported_table')
    else out = rowsToCsv(shownColumns, rowsOut)
    if (formatHasHeader && !includeHeader) out = dropLines(out, format === 'md' ? 2 : 1)
    return out
  }

  const text = $derived(serialise(previewRows))

  const language = $derived(FORMATS.find((f) => f.id === format)?.lang ?? 'plaintext')

  let copied = $state(false)
  /** @type {ReturnType<typeof setTimeout> | null} */
  let copiedTimer = null

  onDestroy(() => {
    if (copiedTimer) clearTimeout(copiedTimer)
  })

  function handleCopy() {
    // The full page, not the capped preview: copying is the point of this view.
    navigator.clipboard.writeText(truncated ? serialise(rows) : text).then(() => {
      copied = true
      if (copiedTimer) clearTimeout(copiedTimer)
      copiedTimer = setTimeout(() => {
        copied = false
      }, 2000)
    })
  }

  function handleDownload() {
    void saveExportFile(
      truncated ? serialise(rows) : text,
      buildExportFilename(tableName, format),
      format,
    )
  }
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
  <!-- Format bar -->
  <div class="studio-chrome flex h-9 shrink-0 items-center gap-2 border-b border-border bg-panel px-2">
    <!-- One segmented control, not four loose buttons: the formats are mutually
         exclusive, so they get a shared boundary and the app's standard active
         fill. Labels are sans - mono is for data, not chrome. -->
    <div
      role="group"
      aria-label="Text format"
      class= "field-surface inline-flex h-7 shrink-0 items-center gap-0.5 bg-muted/20 p-0.5"
    >
      {#each FORMATS as f (f.id)}
        <button
          type="button"
          class={cn(
            // rounded-sm: concentric with the 10px frame and its 2px inset.
            'inline-flex h-full items-center rounded-sm px-2.5 text-ui-xs font-medium transition-colors',
            'focus-visible:ring-2 focus-visible:ring-ring/18 focus-visible:outline-none',
            format === f.id
              ? 'bg-background text-foreground elevate-1'
              : 'text-muted-foreground hover:text-foreground',
          )}
          aria-pressed={format === f.id}
          onclick={() => setFormat(f.id)}
        >
          {f.label}
        </button>
      {/each}
    </div>

    <Popover>
      <PopoverTrigger
        class={cn(
          'inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-ui-xs transition-colors hover:bg-accent hover:text-foreground',
          hidden.size ? 'text-foreground' : 'text-muted-foreground',
        )}
        title="Choose columns"
      >
        <Icon name="columns-3" class="size-3.5 shrink-0" />
        Columns
        {#if hidden.size}
          <span class="rounded-sm bg-muted px-1 text-ui-3xs tabular-nums">{columns.length - hidden.size}/{columns.length}</span>
        {/if}
      </PopoverTrigger>
      <PopoverContent align="start" sideOffset={4} class="w-64 p-1">
        <div data-slot="menu-search" class="flex items-center gap-1.5 px-2 pb-1.5 pt-1">
          <Icon name="search" class="size-3.5 shrink-0 text-muted-foreground" />
          <input
            type="text"
            bind:value={columnQuery}
            placeholder="Find column"
            aria-label="Find column"
            class="no-focus-ring h-6 min-w-0 flex-1 bg-transparent text-ui-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
          />
        </div>
        <div class="flex items-center gap-1 px-1 py-1 text-ui-2xs">
          <button type="button" class="rounded-sm px-1.5 py-0.5 text-muted-foreground hover:bg-accent hover:text-foreground" onclick={() => setHidden(new Set())}>All</button>
          <button type="button" class="rounded-sm px-1.5 py-0.5 text-muted-foreground hover:bg-accent hover:text-foreground" onclick={() => setHidden(new Set(columns.map((c) => c.name)))}>None</button>
          <span class="ml-auto pr-1 tabular-nums text-muted-foreground">{columns.length - hidden.size} of {columns.length}</span>
        </div>
        <div class="max-h-72 overflow-y-auto">
          {#each pickable as c (c.name)}
            <button
              type="button"
              role="checkbox"
              aria-checked={!hidden.has(c.name)}
              class="flex min-h-7 w-full min-w-0 items-center gap-2 rounded-sm px-2 py-1 text-left text-ui-xs hover:bg-accent"
              onclick={() => toggleColumn(c.name)}
            >
              <span class={cn('flex size-3.5 shrink-0 items-center justify-center rounded-sm border', hidden.has(c.name) ? 'border-muted-foreground/40' : 'border-primary bg-primary text-primary-foreground')}>
                {#if !hidden.has(c.name)}<Icon name="check" class="size-2.5 shrink-0" />{/if}
              </span>
              <span class="min-w-0 flex-1 truncate font-mono">{c.name}</span>
            </button>
          {:else}
            <p class="px-2 py-2 text-ui-xs text-muted-foreground">No column matches.</p>
          {/each}
        </div>
      </PopoverContent>
    </Popover>

    {#if formatHasHeader}
      <button
        type="button"
        aria-pressed={includeHeader}
        title={includeHeader ? 'Header row included' : 'Header row left out'}
        class={cn(
          'inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-ui-xs transition-colors hover:bg-accent hover:text-foreground',
          includeHeader ? 'text-foreground' : 'text-muted-foreground',
        )}
        onclick={() => (includeHeader = !includeHeader)}
      >
        <Icon name={includeHeader ? 'check' : 'minus'} class="size-3.5 shrink-0" />
        Header
      </button>
    {/if}
    <button
      type="button"
      aria-pressed={wrap}
      title="Wrap long lines"
      class={cn(
        'inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-ui-xs transition-colors hover:bg-accent hover:text-foreground',
        wrap ? 'text-foreground' : 'text-muted-foreground',
      )}
      onclick={() => (wrap = !wrap)}
    >
      <Icon name="wrap-text" class="size-3.5 shrink-0" />
      Wrap
    </button>

    <div class="ml-auto flex shrink-0 items-center gap-1">
      {#if rows.length > 0}
        <span class="select-none px-1 text-ui-2xs tabular-nums text-muted-foreground">
          {rows.length.toLocaleString('en-US')}
          {rows.length === 1 ? 'row' : 'rows'}{#if hidden.size} · {shownColumns.length} of {columns.length} columns{/if}
        </span>
        <div class="mx-0.5 h-4 w-px bg-border"></div>
      {/if}

      <button
        type="button"
        title="Copy to clipboard"
        class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/18 focus-visible:outline-none"
        onclick={handleCopy}
      >
        {#if copied}
          <Icon name="check" class="size-3.5 shrink-0 text-success" />
        {:else}
          <Icon name="copy" class="size-3.5 shrink-0" />
        {/if}
        <!-- Fixed width: "Copy" → "Copied" otherwise resizes the button under the
             cursor at the exact moment it is clicked, nudging Download sideways. -->
        <span class="w-14 text-left">{copied ? 'Copied' : 'Copy'}</span>
      </button>

      <button
        type="button"
        title="Download file"
        aria-label="Download file"
        class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/18 focus-visible:outline-none"
        onclick={handleDownload}
      >
        <Icon name="download" class="size-3.5 shrink-0" />
      </button>
    </div>
  </div>

  <!-- Text body (⌘F to search) -->
  {#if columns.length === 0}
    <div class="flex min-h-0 flex-1 items-center justify-center bg-panel">
      <p class="font-mono text-ui-sm text-muted-foreground">No data to display</p>
    </div>
  {:else}
    {#if truncated}
      <!-- Says what was cut and what still isn't: silently showing the first
           5,000 rows of 200,000 would read as "this is the page". -->
      <p
        class="flex shrink-0 items-center gap-2 border-b border-border/40 bg-warning/[0.06] px-3 py-1.5 text-ui-2xs text-muted-foreground"
        role="status"
      >
        <Icon name="alert-triangle" class="size-3.5 shrink-0 text-warning" />
        Previewing the first {PREVIEW_ROWS.toLocaleString()} of
        {rows.length.toLocaleString()} rows. Copy and Download use all of them.
      </p>
    {/if}
    {#if shownColumns.length === 0}
      <div class="flex min-h-0 flex-1 items-center justify-center bg-panel">
        <p class="text-ui-sm text-muted-foreground">No columns selected.</p>
      </div>
    {:else}
      <CodeTextView {text} {language} wordWrap={wrap ? 'on' : 'off'} />
    {/if}
  {/if}
</div>
