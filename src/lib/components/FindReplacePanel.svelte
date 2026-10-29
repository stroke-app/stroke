<script>
  /**
   * Find & replace, as a sidebar panel.
   *
   * It was a 40rem modal over the grid: it covered the rows it was about to
   * rewrite, so the one thing you wanted while reading a before → after list -
   * the table itself - was behind the dialog, and every look at it meant closing
   * and reopening the search. A panel is the shape this work actually has. It
   * lives beside the data, it stays open while you click through the matches,
   * and each match can put the cell cursor on the row it belongs to.
   *
   * Matching runs live over the rows the grid has loaded and every change is
   * shown as a preview; applying writes each cell through the app's normal
   * parameterized save pipeline - nothing is written blind.
   *
   * @typedef {Object} Props
   * @property {Array<{ name: string, dataType?: string }>} columns
   * @property {unknown[][]} rows
   * @property {string[]} primaryKey
   * @property {Array<{ columns: string[] }>} foreignKeys
   * @property {string | null} tableName
   * @property {(edits: Array<{ rowIdx: number, colIdx: number, value: string }>) => Promise<void>} onapply
   * @property {(rowIdx: number, colIdx: number) => void} [onreveal]
   */
  import SearchableMenu from './SearchableMenu.svelte'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'
  import { annotateColumns, allTextColumnsAreKeys } from '$lib/find-replace-columns.js'
  import { keycaps } from '$lib/shortcuts.js'
  import { searchOptionHotkey, SEARCH_OPTION_KEYS } from '$lib/search-options.js'

  let {
    /** @type {Array<{ name: string, dataType?: string }>} */
    columns = [],
    /** @type {unknown[][]} */
    rows = [],
    /** @type {string[]} */
    primaryKey = [],
    /** @type {Array<{ columns: string[] }>} */
    foreignKeys = [],
    /** @type {string | null} */
    tableName = null,
    /** @type {(edits: Array<{ rowIdx: number, colIdx: number, value: string }>) => Promise<void>} */
    onapply = async () => {},
    /** Put the grid's cell cursor on a match. */
    onreveal = /** @type {(rowIdx: number, colIdx: number) => void} */ (() => {}),
    /** Assigned here; the shell calls it to focus the Find field. */
    focusFind = $bindable(/** @type {() => void} */ (() => {})),
    // What was typed and how it matches. Bindable so the sidebar can hold them:
    // the panel remounts on every visit, and local state came back empty.
    findText = $bindable(''),
    replaceText = $bindable(''),
    useRegex = $bindable(false),
    wholeValue = $bindable(false),
    caseSensitive = $bindable(false),
  } = $props()

  /**
   * Which column to search, or `ANY_COLUMN` for all of them at once.
   *
   * One column at a time is the safe default, but a rename that has to land in
   * `email`, `fullName` and `address` is three passes of the same search with
   * the same replacement - and the apply path is already per-cell, so searching
   * every editable column costs nothing but the loop.
   */
  const ANY_COLUMN = -2
  let colIdx = $state(-1)
  // The three matching options (useRegex, wholeValue, caseSensitive, props
  // above) are independent, the way they are in an editor's find widget. They
  // were two booleans and a three-way `mode`, so "whole value" and "regular
  // expression" were the same field and turning one on turned the other off -
  // only ever two of the three lit up, and the pair that combines most usefully
  // (a pattern that has to match the whole cell) could not be expressed at all.
  let applying = $state(false)
  /** @type {HTMLInputElement | null} */
  let findEl = $state(null)

  $effect(() => {
    focusFind = () => {
      findEl?.focus()
      findEl?.select()
    }
  })

  // The rule itself lives in `find-replace-columns.js`, with tests: it is the
  // reason a replacement fails after the preview said it would work, which is
  // not something to leave asserted only by a component.
  const allCols = $derived(annotateColumns(columns, { primaryKey, foreignKeys }))
  const editableCols = $derived(allCols.filter((c) => !c.blocked))
  /**
   * Applying needs a primary key: the batch update keys each new value to the
   * row it belongs to. Said here rather than thrown from the apply, which is
   * after the search, the replacement and the preview have all been read.
   */
  const noKey = $derived(!!tableName && primaryKey.length === 0)

  let colMenuOpen = $state(false)
  // Blocked columns stay in the list, disabled, with the reason. Dropping them
  // silently left the one column someone came here to change simply absent,
  // with nothing to say why.
  const colItems = $derived([
    ...(editableCols.length > 1
      ? [{
          value: String(ANY_COLUMN),
          label: 'Any column',
          keywords: ['any', 'all', 'every', 'columns'],
          idx: ANY_COLUMN,
          dataType: `${editableCols.length} searchable`,
          blocked: '',
          disabled: false,
          active: colIdx === ANY_COLUMN,
        }]
      : []),
    ...allCols.map((c) => ({
      value: String(c.idx),
      label: c.name,
      keywords: [c.name],
      idx: c.idx,
      dataType: c.dataType ?? '',
      blocked: c.blocked,
      disabled: !!c.blocked,
      active: c.idx === colIdx,
    })),
  ])

  /**
   * The columns this search runs over. Keys stay out of it whichever way the
   * picker is set: rewriting inside a primary or foreign key changes which row
   * a row is, not what it says.
   */
  const targetCols = $derived(
    colIdx === ANY_COLUMN ? editableCols : editableCols.filter((c) => c.idx === colIdx),
  )

  // Default to the first editable column, and re-pick when the table changes
  // under the panel - the sidebar keeps it mounted across tab switches.
  $effect(() => {
    void tableName
    if (colIdx === ANY_COLUMN) {
      // Still meaningful on any table that has something to search.
      if (!editableCols.length) colIdx = -1
      return
    }
    if (colIdx < 0 || !editableCols.some((c) => c.idx === colIdx)) {
      colIdx = editableCols[0]?.idx ?? -1
    }
  })

  const escapeRe = (/** @type {string} */ s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

  /**
   * The one pattern every option feeds into: `wholeValue` anchors it, `useRegex`
   * decides whether the text is a pattern or a literal, and `caseSensitive`
   * picks the flags. Whole + regex therefore means "this pattern must match the
   * entire cell", which is what the combination should mean.
   * @returns {RegExp | null}
   */
  function buildMatcher() {
    if (!findText) return null
    const body = useRegex ? findText : escapeRe(findText)
    const flags = caseSensitive ? '' : 'i'
    // A whole-value match runs once per cell, so it needs no `g`; a substring
    // replace has to walk the value, so it does.
    return wholeValue ? new RegExp(`^(?:${body})$`, flags) : new RegExp(body, `${flags}g`)
  }

  const regexError = $derived.by(() => {
    if (!findText) return ''
    try {
      buildMatcher()
      return ''
    } catch (e) {
      return String(/** @type {Error} */ (e).message ?? e)
    }
  })

  /** @typedef {{ rowIdx: number, colIdx: number, old: string, value: string }} Match */

  const matches = $derived.by(() => {
    /** @type {Match[]} */
    const out = []
    if (!targetCols.length || !findText || regexError) return out
    /** @type {RegExp | null} */
    let re = null
    try {
      re = buildMatcher()
    } catch {
      return out
    }
    if (!re) return out
    for (let r = 0; r < rows.length; r++) {
      for (const col of targetCols) {
        const v = rows[r]?.[col.idx]
        // Only string cells - rewriting numbers or JSON through a string replace
        // is a footgun.
        if (typeof v !== 'string') continue
        // `lastIndex` survives a call on a /g/ regex, so a shared instance would
        // skip every other row.
        re.lastIndex = 0
        const next = v.replace(re, replaceText)
        if (next !== v) out.push({ rowIdx: r, colIdx: col.idx, old: v, value: next })
      }
    }
    return out
  })

  /** Rendering every match in a narrow panel costs more than it tells you. */
  const PREVIEW_CAP = 200
  const shown = $derived(matches.slice(0, PREVIEW_CAP))
  const activeColName = $derived(
    colIdx === ANY_COLUMN
      ? `any of ${editableCols.length} columns`
      : editableCols.find((c) => c.idx === colIdx)?.name ?? '',
  )
  /** Column name per match - only worth the row when the search spans columns. */
  const showMatchColumn = $derived(colIdx === ANY_COLUMN)
  /** @param {number} idx */
  const colNameAt = (idx) => allCols.find((c) => c.idx === idx)?.name ?? ''

  async function handleApply() {
    if (!matches.length || applying) return
    applying = true
    try {
      await onapply(matches.map(({ rowIdx, colIdx: ci, value }) => ({ rowIdx, colIdx: ci, value })))
      // The rows the grid holds are rewritten in place, so the matches
      // disappear by themselves. The query stays: the next page of a long table
      // usually needs the same replacement.
    } finally {
      applying = false
    }
  }

  /** @param {KeyboardEvent} e */
  function onFieldKey(e) {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault()
      void handleApply()
      return
    }
    const opt = searchOptionHotkey(e)
    if (opt) {
      e.preventDefault()
      if (opt === 'matchCase') caseSensitive = !caseSensitive
      else if (opt === 'regex') useRegex = !useRegex
      else wholeValue = !wholeValue
    }
  }

  /** The three matching options, so the markup stays a loop. Any combination. */
  const OPTIONS = $derived([
    { id: 'case', cap: 'Aa', keys: SEARCH_OPTION_KEYS.matchCase, title: `Match case (${SEARCH_OPTION_KEYS.matchCase})`, on: () => caseSensitive, toggle: () => (caseSensitive = !caseSensitive) },
    { id: 'regex', cap: '.*', keys: SEARCH_OPTION_KEYS.regex, title: `Use a regular expression (${SEARCH_OPTION_KEYS.regex})`, on: () => useRegex, toggle: () => (useRegex = !useRegex) },
    { id: 'whole', cap: 'ab', keys: SEARCH_OPTION_KEYS.wholeWord, title: `Match the whole value (${SEARCH_OPTION_KEYS.wholeWord})`, on: () => wholeValue, toggle: () => (wholeValue = !wholeValue) },
  ])

  /**
   * One frame for every control in this panel.
   *
   * The border never moves: hover and focus change colour, not width, and focus
   * draws the app's own 2px outline just outside the edge rather than thickening
   * the border - which is what made the column picker read as a heavier control
   * than the two fields underneath it while it had focus.
   */
  const fieldCls =
    'no-focus-ring h-7 w-full min-w-0 rounded-md border border-border/50 bg-input/30 px-2 font-mono text-ui-2xs text-foreground transition-colors placeholder:text-muted-foreground hover:border-border focus:border-border focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-ring'
</script>

<div class="flex h-full min-h-0 w-full flex-col">
  <!-- Query. One field per row, because the panel is ~260px wide: the dialog's
       three-across control row could not survive here, and stacking is what
       lets every field keep a full-width hit target.
       -
       One type scale, too: mono `text-ui-2xs` for anything holding a value (a
       column name, a search, a replacement) and `text-ui-3xs` for chrome, the
       same split the related-rows and cell-editor panels use. It had a 10px
       uppercase sans "IN" label sitting over 13px mono fields over a 10px
       footnote - three sizes and two families in 120px of panel. -->
  <div class="flex shrink-0 flex-col gap-1.5 border-b border-sidebar-border px-2 py-2">
    <SearchableMenu
      bind:open={colMenuOpen}
      items={colItems}
      placeholder="Search columns…"
      contentClass="min-w-60"
      onselect={(it) => (colIdx = it.idx)}
    >
      {#snippet trigger(props)}
        <!-- The icon replaces the label: a column picker that shows a column
             name does not need a word telling you it is a column. -->
        <button
          {...props}
          type="button"
          aria-label="Column to search"
          class={cn(
            fieldCls,
            'flex items-center gap-1.5 text-left',
            colMenuOpen && 'border-border outline-2 -outline-offset-0 outline-ring',
          )}
        >
          <Icon name="columns-3" class="size-3.5 shrink-0 text-muted-foreground" />
          <span class="min-w-0 flex-1 truncate">{colIdx === ANY_COLUMN ? 'Any column' : (activeColName || 'No text column')}</span>
          <Icon name="chevron-down" class="size-3 shrink-0 text-muted-foreground" />
        </button>
      {/snippet}
      {#snippet item(it)}
        <span class="min-w-0 flex-1 truncate font-mono">{it.label}</span>
        {#if it.blocked}
          <span class="shrink-0 text-ui-3xs text-muted-foreground">{it.blocked}</span>
        {:else if it.dataType}
          <span class="shrink-0 text-ui-3xs text-muted-foreground">{it.dataType}</span>
        {/if}
        {#if it.active}<Icon name="check" class="size-3.5 shrink-0 text-primary" />{/if}
      {/snippet}
    </SearchableMenu>

    <div class="flex min-w-0 items-center gap-1.5">
      <input
        bind:this={findEl}
        type="text"
        aria-label="Find"
        class={fieldCls}
        placeholder={useRegex ? '^(\\w+)@…' : 'Find'}
        bind:value={findText}
        onkeydown={onFieldKey}
      />
      <!-- One segmented control, not three separate chips: they are three
           settings for the field beside them, and each in its own bordered box
           read as three more buttons competing with it. -->
      <div class="flex h-7 shrink-0 items-center overflow-hidden rounded-md border border-border/50 bg-input/30">
        {#each OPTIONS as opt, i (opt.id)}
          <button
            type="button"
            aria-pressed={opt.on()}
            aria-label={opt.title}
            aria-keyshortcuts={opt.keys}
            title={opt.title}
            tabindex="-1"
            class={cn(
              'inline-flex h-full w-7 items-center justify-center font-mono text-ui-3xs transition-colors',
              i > 0 && 'border-l border-border/40',
              opt.on() ? 'bg-primary/15 text-foreground' : 'text-muted-foreground hover:bg-muted/40 hover:text-foreground',
            )}
            onclick={opt.toggle}
          >{opt.cap}</button>
        {/each}
      </div>
    </div>

    <div class="flex min-w-0 items-center gap-1.5">
      <input
        type="text"
        aria-label="Replace with"
        class={fieldCls}
        placeholder={useRegex ? '$1 …' : 'Replace'}
        bind:value={replaceText}
        onkeydown={onFieldKey}
      />
      <button
        type="button"
        class="inline-flex h-7 shrink-0 items-center gap-1 rounded-md bg-primary px-2 font-mono text-ui-3xs font-medium text-primary-foreground transition-[opacity,transform] hover:opacity-90 active:scale-[0.97] disabled:pointer-events-none disabled:opacity-40"
        disabled={matches.length === 0 || applying || !!regexError || noKey}
        onclick={handleApply}
        title={`Replace in ${matches.length} cell(s) · ${keycaps('Mod+Enter').join(' ')}`}
      >
        {#if applying}<Icon name="loader-2" class="size-3 shrink-0 animate-spin" />{/if}
        {matches.length ? `Replace ${matches.length.toLocaleString('en-US')}` : 'Replace'}
      </button>
    </div>

    {#if regexError}
      <p class="font-mono text-ui-3xs leading-relaxed text-destructive">{regexError}</p>
    {:else if noKey}
      <!-- Before the search, not after it: the apply keys each new value to its
           row by primary key, and without one it threw "Batch update requires a
           primary key" once the whole query had already been typed. -->
      <p class="flex items-start gap-1.5 text-ui-3xs leading-relaxed text-muted-foreground">
        <Icon name="info" class="mt-px size-3 shrink-0" aria-hidden="true" />
        <span class="min-w-0">This table has no primary key, so a replacement cannot be written back to a specific row. Matches still preview below.</span>
      </p>
    {/if}
  </div>

  <!-- Results -->
  {#if !tableName}
    <p class="px-3 py-3 text-ui-3xs text-muted-foreground">Open a table to search its values.</p>
  {:else if editableCols.length === 0}
    <p class="px-3 py-3 text-ui-3xs leading-relaxed text-muted-foreground">
      {#if allTextColumnsAreKeys(allCols)}
        Every text column here is a key. Replacing inside a primary or foreign key
        would rewrite the relationship rather than the value, so the picker lists
        them but will not search them.
      {:else}
        No text columns in this table.
      {/if}
    </p>
  {:else if !findText}
    <p class="px-3 py-3 text-ui-3xs leading-relaxed text-muted-foreground">
      Type to preview changes in <span class="font-mono text-foreground/80">{activeColName}</span>.
    </p>
  {:else if matches.length === 0}
    <p class="px-3 py-3 text-ui-3xs text-muted-foreground">No matches on this page.</p>
  {:else}
    <div class="flex h-7 shrink-0 items-center gap-1.5 border-b border-sidebar-border/60 px-3">
      <span class="text-ui-3xs tabular-nums text-muted-foreground">
        {matches.length.toLocaleString('en-US')} cell{matches.length === 1 ? '' : 's'} will change
      </span>
    </div>
    <div class="app-scroll min-h-0 flex-1 overflow-y-auto py-0.5">
      {#each shown as m (`${m.rowIdx}:${m.colIdx}`)}
        <!-- Each match is a button: clicking it moves the grid's cell cursor to
             that cell, which is the whole reason this belongs beside the table
             rather than on top of it. -->
        <button
          type="button"
          class="group flex w-full min-w-0 flex-col gap-0.5 px-3 py-1 text-left outline-none transition-colors hover:bg-sidebar-accent/60 focus-visible:bg-sidebar-accent/60"
          onclick={() => onreveal(m.rowIdx, m.colIdx)}
          title={`Row ${m.rowIdx + 1} · ${m.old} → ${m.value}`}
        >
          <span class="flex min-w-0 items-center gap-1.5">
            <span class="shrink-0 font-mono text-ui-3xs tabular-nums text-muted-foreground/60">{m.rowIdx + 1}</span>
            {#if showMatchColumn}
              <span class="shrink-0 truncate font-mono text-ui-3xs text-primary/80">{colNameAt(m.colIdx)}</span>
            {/if}
            <span class="min-w-0 flex-1 truncate font-mono text-ui-3xs text-muted-foreground line-through decoration-destructive/40">{m.old}</span>
          </span>
          <span class="flex min-w-0 items-center gap-1.5">
            <span class="w-[1.5ch] shrink-0"></span>
            <span class="min-w-0 flex-1 truncate font-mono text-ui-2xs text-success">{m.value}</span>
          </span>
        </button>
      {/each}
      {#if matches.length > PREVIEW_CAP}
        <p class="px-3 py-1.5 text-ui-3xs text-muted-foreground">
          …and {(matches.length - PREVIEW_CAP).toLocaleString('en-US')} more, all included in All
        </p>
      {/if}
    </div>
  {/if}

  <p class="shrink-0 truncate border-t border-sidebar-border px-3 py-1.5 text-ui-3xs text-muted-foreground"
     title="Only text cells on the page the grid has loaded are affected. Each change is written as its own parameterized update, keyed by primary key.">
    Loaded rows · one update per row
  </p>
</div>
