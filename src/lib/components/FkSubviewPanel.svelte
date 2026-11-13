<script>
  import { untrack } from 'svelte'
  import Loader from '@lucide/svelte/icons/loader'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import X from '@lucide/svelte/icons/x'
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert'
  import Inbox from '@lucide/svelte/icons/inbox'
  import Copy from '@lucide/svelte/icons/copy'
  import Link2 from '@lucide/svelte/icons/link-2'
  import Braces from '@lucide/svelte/icons/braces'
  import Table from '@lucide/svelte/icons/table'
  import Rows3 from '@lucide/svelte/icons/rows-3'
  import Columns3 from '@lucide/svelte/icons/columns-3'
  import * as ContextMenu from '$lib/components/ui/context-menu/index.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { cn } from '$lib/utils.js'
  import { clampColumnWidth, columnAlignsRight, defaultColumnWidth } from '$lib/table-column-widths.js'

  /**
   * Geometry and type sizes borrowed from the grid above (see `gridMetrics` in
   * DataTable.svelte). They are px numbers rather than `text-ui-*` steps on
   * purpose: the grid's text size is its own user setting (Settings →
   * Appearance → Grid text size) scaled by the canvas zoom, not a rung of the
   * UI scale, and the panel has to land on the same number to read as the same
   * table. The fallbacks are the shipped defaults at 100%.
   * @typedef {{ zoom: number, cellPx: number, typePx: number, rowH: number, headerH: number, padX: number, rowRules: boolean, colRules: boolean, zebra: boolean, align: string, rowNumbers: boolean, dash?: number[] | null, double?: boolean, strong?: boolean, groupEvery?: number }} GridMetrics
   */
  let {
    data,
    /** @type {GridMetrics} */
    metrics = {
      zoom: 1, cellPx: 13, typePx: 11, rowH: 28, headerH: 30, padX: 10,
      rowRules: true, colRules: true, zebra: false, align: 'numbers', rowNumbers: true,
      dash: null, double: false, strong: false, groupEvery: 0,
    },
    fkLabel = '',
    /** Small context hint shown next to the badge (e.g. "row 12"). */
    sourceHint = '',
    onclose = () => {},
    /** Navigate to the related table WITH the FK filter applied */
    onfullview = () => {},
    /**
     * The height this panel needs to show every row it has (header, column
     * header, rows, a scrollbar's worth), so the dock can shrink to it: one
     * related row in a 260px dock was a band of empty space. 0 while loading,
     * so the dock keeps its size until there is something to fit.
     */
    fitHeight = $bindable(0),
  } = $props()

  let headerH = $state(0)

  /**
   * The grid's table style, translated into what a CSS border can say.
   *
   * The grid draws on canvas and takes a dash array; a border takes a keyword,
   * so the array is matched to the nearest one - a 1px-on dash reads as dotted,
   * anything longer as dashed. `double` and `strong` are borrowed as-is. What
   * cannot cross over (corner dots, column ticks) simply does not, rather than
   * being approximated into something the grid never shows.
   */
  const rule = $derived.by(() => {
    const d = metrics.dash
    const style = metrics.double ? 'double' : !d ? 'solid' : d[0] <= 1 ? 'dotted' : 'dashed'
    // `double` needs 3px to render as two lines at all; a 1px double border is
    // drawn by every engine as a single solid one.
    const width = metrics.double ? 3 : 1
    return {
      style,
      width,
      // Dashes and dots read lighter than a solid rule of the same colour
      // because so much of the line is missing, so they are given back some
      // contrast to sit at the same weight as the grid above.
      // Bottom edge only. `border-border/40` coloured all four sides, so on a
      // heavier every-5th row the cells' right edges turned visible too: short
      // vertical rules on rows 5, 10, 15 and nowhere else.
      row: metrics.strong ? 'border-b-border/60' : style === 'solid' ? 'border-b-border/15' : 'border-b-border/30',
      col: metrics.strong ? 'border-r-border/60' : style === 'solid' ? 'border-r-border/15' : 'border-r-border/30',
      group: metrics.strong ? 'border-b-border/70' : 'border-b-border/40',
    }
  })

  /** A heavier rule every Nth row (ledger, graph, bands). 1-based like the gutter. */
  // The dock is a quick peek, not a second ledger: it keeps one faint row
  // hairline and drops the grid's every-Nth bands and column rules, which read
  // as stray borders at this size.
  function isGroupEdge(/** @type {number} */ _i) {
    return false
  }

  /**
   * The cell classes, built once instead of per cell.
   *
   * The dock follows the cell cursor, so this table re-renders on every arrow
   * key. At 50 rows and a wide table that is a few thousand cells a keystroke,
   * and it was doing a seven-argument `cn()`, three `isGroupEdge` calls and a
   * four-interpolation style string for each of them. None of that varies by
   * cell, so none of it belongs in the loop: what is left per cell is picking
   * between strings that already exist.
   */
  const cls = $derived.by(() => {
    const base = 'cursor-default overflow-hidden align-middle text-ellipsis whitespace-nowrap outline-none'
    const col = ''
    return {
      cell: base + col,
      cellRight: base + col + ' text-right tabular-nums',
      num: 'select-none text-right align-middle tabular-nums text-muted-foreground/60',
      rowRule: 'border-b-border/10',
      groupRule: rule.group,
    }
  })

  /**
   * What the panel draws. Every lookup replaces `data` with an empty
   * `{ loading: true, rows: [] }` first, so drawing `data` directly blanked the
   * table - headers and all - for the frames between clicking another row's key
   * and its result landing. Holding the last settled result until the next one
   * arrives turns that into a single swap. It is only reused for the same
   * relationship: another relationship's columns would be the wrong table.
   */
  let settled = $state.raw(/** @type {any} */ (null))
  let settledLabel = $state('')
  let settledHint = $state('')
  $effect(() => {
    if (data && !data.loading) {
      settled = data
      settledLabel = fkLabel
      settledHint = sourceHint
    }
  })
  const holding = $derived(!!(data?.loading && settled && settledLabel === fkLabel))
  const view = $derived(holding ? settled : data)
  // The title belongs to the rows under it: while the last result is held, so
  // is its "for row N", and both change together when the new rows land.
  const shownHint = $derived(holding ? settledHint : sourceHint)

  /** `data.loading`, held back ~220ms (the grid's SPAN_SHOW_AFTER) so it never flashes. */
  let loadingVisible = $state(false)
  $effect(() => {
    if (!data?.loading) { loadingVisible = false; return }
    const t = setTimeout(() => (loadingVisible = true), 220)
    return () => clearTimeout(t)
  })

  /** @param {unknown} v */
  function fmt(v) {
    if (v === null || v === undefined) return 'NULL'
    if (typeof v === 'object') return JSON.stringify(v)
    return String(v)
  }

  const rowCount = $derived(view?.rows?.length ?? 0)

  /** The table's default schema says nothing (SQLite `main`, Postgres `public`). */
  const tableLabel = $derived(fkLabel.replace(/^(main|public|dbo)\./, ''))

  // The dock fits its rows. While a lookup is in flight the height it has is
  // kept (or, on a first open, one row's worth - a forward lookup is one row):
  // dropping to 0 sent the dock to its full height and back on every open,
  // which is the jump up and down.
  $effect(() => {
    const fit = (/** @type {number} */ n) => Math.ceil(headerH + metrics.headerH + n * metrics.rowH + 12)
    if (view?.loading) {
      if (!untrack(() => fitHeight)) fitHeight = fit(1)
      return
    }
    fitHeight = !view?.error && rowCount > 0 ? fit(rowCount) : 0
  })
  const colNames = $derived((view?.columns ?? []).map((c) => c.name ?? c))

  /**
   * Column widths and alignment, from the same two helpers the grid above sizes
   * its own columns with, so a uuid column is a uuid column's width in both.
   * `table-layout: fixed` then makes these authoritative - auto layout treats a
   * width as a suggestion and lets one long JSON value stretch a column off the
   * edge of the panel, which is what made the columns here look nothing like
   * the grid's.
   */
  const cols = $derived(
    (view?.columns ?? []).map((c) => {
      const type = c.dataType ?? c.data_type ?? ''
      return {
        name: c.name ?? c,
        type,
        w: Math.round(clampColumnWidth(defaultColumnWidth(type)) * metrics.zoom),
        alignRight: columnAlignsRight(type, metrics.align),
      }
    }),
  )

  /** Row-number gutter, sized to the widest number drawn - the grid's formula. */
  const numW = $derived(
    metrics.rowNumbers ? Math.round((String(Math.max(1, rowCount)).length * 8 + 16) * metrics.zoom) : 0,
  )

  /** Fixed layout needs a total; `min-w-full` still stretches a narrow table. */
  const tableW = $derived(cols.reduce((n, c) => n + c.w, 0) + numW)

  // ── Selection ───────────────────────────────────────────────────────────────
  // A cell here is addressed by row and column index, the same as the grid above.
  // Selection drives both the copy actions and the roving tabindex, so arrow keys
  // move focus and the selection together.
  /** @type {{ r: number, c: number } | null} */
  let sel = $state(null)

  // A new result set invalidates any cell coordinate held from the previous one.
  $effect(() => {
    void view
    sel = null
  })

  /** @param {number} i row index @param {number} j column index */
  function cellAt(i, j) {
    const row = view?.rows?.[i]
    if (row === undefined) return undefined
    return Array.isArray(row) ? row[j] : row[colNames[j]]
  }

  /** @param {number} i */
  function rowObject(i) {
    /** @type {Record<string, unknown>} */
    const out = {}
    colNames.forEach((n, j) => { out[n] = cellAt(i, j) ?? null })
    return out
  }

  /** Tab-separated, which is what spreadsheets and editors paste as columns. */
  function allTsv() {
    const header = colNames.join('\t')
    const body = view.rows.map((_, i) => colNames.map((_, j) => fmt(cellAt(i, j))).join('\t'))
    return [header, ...body].join('\n')
  }

  /** @param {string} text @param {string} what */
  async function copy(text, what) {
    try {
      await navigator.clipboard.writeText(text)
      toast.success(`${what} copied`, { duration: 1800 })
    } catch (e) {
      toast.error('Could not copy', { description: String(e?.message ?? e) })
    }
  }

  /** @param {number} i @param {number} j */
  function selectCell(i, j) {
    sel = { r: i, c: j }
  }

  /**
   * One handler for the body. The cell already names itself in `data-fk-cell`,
   * so the row and column come off the event target rather than out of a
   * closure built per cell.
   * @param {Event} e
   */
  function onBodyPick(e) {
    const el = /** @type {Element | null} */ (e.target)?.closest?.('[data-fk-cell]')
    const id = el?.getAttribute('data-fk-cell')
    if (!id) return
    const [r, c] = id.split(':')
    selectCell(Number(r), Number(c))
  }

  /** Move the selection, clamped to the result set. @param {number} dr @param {number} dc */
  function moveSel(dr, dc) {
    if (!rowCount || !colNames.length) return
    const cur = sel ?? { r: 0, c: 0 }
    const r = Math.min(rowCount - 1, Math.max(0, cur.r + dr))
    const c = Math.min(colNames.length - 1, Math.max(0, cur.c + dc))
    sel = { r, c }
    // Follow the roving tabindex so the newly selected cell is also the focused
    // one, otherwise the next arrow key goes to whatever still holds focus.
    queueMicrotask(() => {
      const el = document.querySelector(`[data-fk-cell="${r}:${c}"]`)
      if (!(el instanceof HTMLElement)) return
      // focus() with its own scrolling moves EVERY scrollable ancestor to show
      // the cell - the dock and the grid's column included - so the panel's
      // title bar slid up out of sight. Focus in place, then scroll only this
      // panel's own list, leaving room for the sticky column header.
      el.focus({ preventScroll: true })
      revealCell(el)
    })
  }

  /** Scroll the panel's list (and nothing else) so `el` is fully in view. @param {HTMLElement} el */
  function revealCell(el) {
    const box = el.closest('[data-fk-subview-scroll]')
    if (!(box instanceof HTMLElement)) return
    const b = box.getBoundingClientRect()
    const r = el.getBoundingClientRect()
    const top = b.top + metrics.headerH
    if (r.top < top) box.scrollTop -= top - r.top
    else if (r.bottom > b.bottom) box.scrollTop += r.bottom - b.bottom
    const left = b.left + (numW || 0)
    if (r.left < left) box.scrollLeft -= left - r.left
    else if (r.right > b.right) box.scrollLeft += r.right - b.right
  }

  /** @param {KeyboardEvent} e */
  function onGridKey(e) {
    if (e.key === 'ArrowDown') { e.preventDefault(); moveSel(1, 0); return }
    if (e.key === 'ArrowUp') { e.preventDefault(); moveSel(-1, 0); return }
    if (e.key === 'ArrowRight') { e.preventDefault(); moveSel(0, 1); return }
    if (e.key === 'ArrowLeft') { e.preventDefault(); moveSel(0, -1); return }

    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'c') {
      // A real text selection wins - the user dragged across part of a value and
      // means to copy exactly that, so let the browser do its own copy.
      if ((window.getSelection()?.toString() ?? '') !== '') return
      if (!sel) return
      e.preventDefault()
      copy(fmt(cellAt(sel.r, sel.c)), 'Cell')
    }
  }
</script>

<!-- Docked bottom panel, fills the dock's height (flex column) and owns its
     scroll. The dock container provides the top border + resize handle. -->
<!-- overflow-clip: nothing may scroll this frame. A clipped-but-scrollable
     (overflow-hidden) box can still be scrolled by focus(), which is how the
     title bar used to slide out of view. -->
<div class="flex h-full min-h-0 w-full flex-col overflow-clip bg-background">

  <!-- Header: which table, for which row, how many - one quiet line - then
       icon actions. It was an uppercase RELATED label, a bordered chip, a
       parenthesised "(row 15)" and a separate count, which read as four facts
       where there is one sentence, and text buttons as wide as the title. -->
  <div bind:offsetHeight={headerH} class="flex h-8 shrink-0 items-center gap-2 border-b border-border/40 px-2.5">
    <Link2 class="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
    <span class="min-w-0 truncate font-mono text-ui-2xs font-medium text-foreground" title={fkLabel}>{tableLabel}</span>
    <span class="flex shrink-0 items-center gap-1.5 text-ui-2xs text-muted-foreground">
      {#if shownHint}<span>for {shownHint}</span>{/if}
      {#if !view?.loading && !view?.error}
        {#if shownHint}<span aria-hidden="true" class="text-muted-foreground/50">·</span>{/if}
        <span class="font-mono tabular-nums">{rowCount}{rowCount >= 50 ? '+' : ''} {rowCount === 1 ? 'row' : 'rows'}</span>
      {/if}
    </span>

    <div class="ml-auto flex shrink-0 items-center gap-0.5">
      {#if rowCount}
        <button
          type="button"
          class="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onclick={() => copy(allTsv(), rowCount === 1 ? 'Row' : 'All rows')}
          title="Copy rows (tab-separated, with a header)"
          aria-label="Copy rows"
        >
          <Copy class="size-3.5 shrink-0" />
        </button>
      {/if}
      <button
        type="button"
        class="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={onfullview}
        title="Open in a tab, filtered to these rows"
        aria-label="Open in a tab"
      >
        <ExternalLink class="size-3.5 shrink-0" />
      </button>
      <button
        type="button"
        class="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={onclose}
        title="Close"
        aria-label="Close related rows panel"
      >
        <X class="size-3.5 shrink-0" />
      </button>
    </div>
  </div>

  <!-- Content, three visually distinct states: loading / failed / empty -->
  {#if view?.loading}
    <!-- The row keeps its height from the first frame; only the spinner waits,
         so a lookup that lands a frame later never flashes it. -->
    <div class="loading-reveal flex flex-1 items-center gap-2 px-3 py-4">
      {#if loadingVisible}
        <span class="flex items-center gap-2 animate-in fade-in duration-150">
          <Loader class="size-3.5 shrink-0 animate-spin text-muted-foreground" />
          <span class="font-mono text-ui-2xs text-muted-foreground">Loading related rows…</span>
        </span>
      {/if}
    </div>

  {:else if view?.error}
    <div class="flex flex-1 items-start gap-2 px-3 py-3">
      <TriangleAlert class="mt-px size-3.5 shrink-0 text-destructive" />
      <div class="min-w-0">
        <div class="text-ui-2xs font-medium text-destructive">Couldn't load related rows</div>
        <div class="mt-0.5 font-mono text-ui-2xs leading-relaxed break-words text-muted-foreground">{view.error}</div>
      </div>
    </div>

  {:else if !rowCount}
    <div class="flex flex-1 items-center gap-2 px-3 py-3">
      <Inbox class="size-3.5 shrink-0 text-muted-foreground" />
      <span class="text-ui-2xs italic text-muted-foreground">No related rows</span>
    </div>

  {:else}
    <!-- The dock owns this scroll, both axes contained here, never chained to
         the grid (the panel lives outside the grid's scroll container). -->
    <div class={cn("app-scroll min-h-0 flex-1 overflow-auto overscroll-contain transition-opacity duration-200", data?.loading && loadingVisible && "opacity-60")} data-fk-subview-scroll>
      <ContextMenu.Root>
        <ContextMenu.Trigger>
          {#snippet child({ props })}
            <!-- `data-studio-selectable` opts this subtree out of the app-wide
                 select-none, so a value can be dragged across and copied the way
                 it can anywhere else data is shown. `role="grid"` carries the
                 roving tabindex below. -->
            <table
              {...props}
              role="grid"
              data-studio-selectable="text"
              class="min-w-full border-separate font-mono"
              style="border-spacing:0; table-layout:fixed; width:{tableW}px; font-size:{metrics.cellPx}px; line-height:1;
                     --row-h:{metrics.rowH}px; --pad-x:{metrics.padX}px; --num-pad:{Math.round(7 * metrics.zoom)}px;
                     --rs:solid; --rw:1px; --rwg:{Math.max(rule.width, 2)}px"
              onkeydown={onGridKey}
            >
              <colgroup>
                {#if numW}<col style="width:{numW}px" />{/if}
                {#each cols as c (c.name)}
                  <col style="width:{c.w}px" />
                {/each}
              </colgroup>

              <!-- Not sticky itself: the <th>s are, and are opaque. A sticky
                   <thead> around sticky cells is offset twice in WebKit, which
                   left a strip above the header where scrolled rows showed. -->
              <thead>
                <tr>
                  {#if numW}
                    <th
                      class="sticky top-0 z-10 border-b border-border/40 bg-[color-mix(in_oklch,var(--muted)_25%,var(--background))] text-right align-middle font-[530] text-muted-foreground/70"
                      style="height:{metrics.headerH}px; padding:0 {Math.round(7 * metrics.zoom)}px"
                      aria-label="Row number"
                    >#</th>
                  {/if}
                  {#each cols as c (c.name)}
                    <!-- Name at the grid's header weight (530), the type annotation
                         a step below it, both in a band of the grid's header height. -->
                    <th
                      class={cn(
                        // Opaque and sticky per cell: a 25% tint let rows scroll up
                        // visibly behind the names, and WebKit does not reliably
                        // paint a sticky <thead>'s own background under its cells.
                        'sticky top-0 z-10 overflow-hidden border-b border-border/40 bg-[color-mix(in_oklch,var(--muted)_25%,var(--background))] align-middle font-[530] whitespace-nowrap text-foreground/80',
                        
                        // The header's own bottom rule stays solid whatever the
                        // style: it separates the table from its labels rather
                        // than one row from the next, and a dashed version of it
                        // reads as a missing row.
                        c.alignRight ? 'text-right' : 'text-left',
                      )}
                      style="height:{metrics.headerH}px; padding:0 {metrics.padX}px"
                      title={c.type ? `${c.name} · ${c.type}` : c.name}
                    >
                      {c.name}
                      {#if c.type}
                        <span class="ml-1 font-normal text-muted-foreground" style="font-size:{metrics.typePx}px">{c.type}</span>
                      {/if}
                    </th>
                  {/each}
                </tr>
              </thead>

              <!-- One listener for the whole body instead of three per cell.
                   At 50 rows and a wide table that was a few thousand closures
                   rebuilt on every arrow key, for a target the cell already
                   names in `data-fk-cell`. -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
              <tbody onclickcapture={onBodyPick} onfocusincapture={onBodyPick} oncontextmenu={onBodyPick}>
                {#each view.rows as row, i (i)}
                  {@const lastRow = i === view.rows.length - 1}
                  {@const group = isGroupEdge(i)}
                  {@const ruled = metrics.rowRules && !lastRow}
                  {@const rowRule = ruled ? (group ? cls.groupRule : cls.rowRule) : ''}
                  <tr
                    data-rule={ruled ? 'row' : undefined}
                    class={cn(
                      'group/row',
                      // Zebra is the grid's own shading, so the two surfaces
                      // stripe in step rather than one banding and one not.
                      metrics.zebra && i % 2 === 1 && 'bg-muted/[0.06]',
                      sel?.r === i && 'bg-muted/25',
                    )}
                  >
                    {#if numW}
                      <td data-num class={cn(cls.num, rowRule)}>{i + 1}</td>
                    {/if}
                    {#each cols as c, j (c.name)}
                      {@const v = cellAt(i, j)}
                      {@const isNullVal = v === null || v === undefined}
                      {@const isSel = sel?.r === i && sel?.c === j}
                      {@const text = fmt(v)}
                      <td
                        role="gridcell"
                        data-fk-cell="{i}:{j}"
                        tabindex={isSel || (!sel && i === 0 && j === 0) ? 0 : -1}
                        aria-selected={isSel}
                        class={cn(
                          c.alignRight ? cls.cellRight : cls.cell,
                          rowRule,
                          isNullVal && 'italic text-muted-foreground/70',
                          isSel ? 'bg-accent text-foreground' : 'group-hover/row:bg-muted/15',
                        )}
                        title={isNullVal ? '' : text}
                      >{text}</td>
                    {/each}
                  </tr>
                {/each}
              </tbody>
            </table>
          {/snippet}
        </ContextMenu.Trigger>

        <ContextMenu.Content class="min-w-52">
          <ContextMenu.Item disabled={!sel} onSelect={() => sel && copy(fmt(cellAt(sel.r, sel.c)), 'Cell')}>
            <Copy />
            Copy cell
            <ContextMenu.Shortcut combo="Mod+C" />
          </ContextMenu.Item>
          <ContextMenu.Item
            disabled={!sel}
            onSelect={() => sel && copy(colNames.map((_, j) => fmt(cellAt(sel.r, j))).join('\t'), 'Row')}
          >
            <Rows3 />
            Copy row
          </ContextMenu.Item>
          <ContextMenu.Item disabled={!sel} onSelect={() => sel && copy(JSON.stringify(rowObject(sel.r), null, 2), 'Row JSON')}>
            <Braces />
            Copy row as JSON
          </ContextMenu.Item>
          <ContextMenu.Item
            disabled={!sel}
            onSelect={() => sel && copy(view.rows.map((_, i) => fmt(cellAt(i, sel.c))).join('\n'), `Column ${colNames[sel.c]}`)}
          >
            <Columns3 />
            Copy column
          </ContextMenu.Item>
          <ContextMenu.Separator />
          <ContextMenu.Item onSelect={() => copy(allTsv(), 'All rows')}>
            <Table />
            Copy all rows
          </ContextMenu.Item>
          <ContextMenu.Item onSelect={() => copy(JSON.stringify(view.rows.map((_, i) => rowObject(i)), null, 2), 'All rows JSON')}>
            <Braces />
            Copy all as JSON
          </ContextMenu.Item>
        </ContextMenu.Content>
      </ContextMenu.Root>
    </div>

    {#if rowCount >= 50}
      <div class="flex shrink-0 items-center border-t border-border/25 bg-muted/5 px-3 py-1">
        <span class="font-mono text-ui-2xs text-muted-foreground">First 50 rows shown, open in sub view for all</span>
      </div>
    {/if}
  {/if}
</div>

<style>
  /* Geometry and rule weight come off custom properties set on the table, so a
     cell carries no inline style at all. The colour still rides on a utility
     class, which is the one part that genuinely differs between a normal rule
     and a group one. */
  tbody td { height: var(--row-h); padding: 0 var(--pad-x); }
  tbody td[data-num] { padding: 0 var(--num-pad); }
  tbody tr[data-rule] > td { border-bottom-style: var(--rs); border-bottom-width: var(--rw); }
  tbody tr[data-rule='group'] > td { border-bottom-width: var(--rwg); }
  table[data-colrule] tbody td { border-right-style: var(--rs); border-right-width: var(--rw); }
</style>
