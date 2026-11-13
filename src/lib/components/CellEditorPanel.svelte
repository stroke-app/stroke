<script>
  import { Button } from '$lib/components/ui/button/index.js'
  /**
   * The focused cell, full size, docked under the grid.
   *
   * A grid row is 28px tall, which is the wrong surface for a paragraph of
   * markdown, a stack trace, a 40-line JSON payload or a SQL snippet stored in a
   * text column - the inline editor shows one line of it and scrolls the rest
   * sideways. Space opens the same value here: the whole thing, wrapped,
   * editable, with the raw text and a read-only preview side by side when the
   * value is structured.
   *
   * It is a dock, not a dialog. A centred modal over a table put the value on
   * top of the rows it came from, dimmed the data you were comparing it against
   * and took the whole window for one field. The related-rows panel already
   * established where a second surface belongs in this app - along the bottom
   * edge, resizable, with the grid still live above it - so this sits in the
   * same place, wears the same chrome, and leaves the row it belongs to visible.
   *
   * It stages, it does not write. Stage change hands the value back on exactly
   * the path the inline editor uses, so it lands in the same staged-changes
   * queue with the same undo and the same Apply button.
   *
   * @typedef {Object} Props
   * @property {boolean} open
   * @property {string} colName
   * @property {string} colType
   * @property {unknown} value
   * @property {string} [sourceHint] Small context hint, e.g. "row 12".
   * @property {boolean} [readOnly]
   * @property {boolean} [detached] Showing a value with no cell behind it.
   * @property {boolean} [truncatedLoad] The value was loaded, but the server
   *   stopped at its ceiling - the text on screen is not all of it.
   * @property {() => Promise<void>} [onloadfull] Fetch the whole value for a
   *   capped cell. The parent replaces `value` with it, which is what clears
   *   `oversize` and turns the preview back into an ordinary value.
   * @property {{ bytes: number, dataType: string } | null} [oversize] Set when
   *   only a preview of the cell was loaded: the backend caps a cell at 256KB
   *   and ships the first 16KB, so what is on screen is a slice of the value,
   *   not the value.
   * @property {(next: string) => void} oncommit
   */
  import { untrack } from 'svelte'
  import Pencil from '@lucide/svelte/icons/pencil'
  import Copy from '@lucide/svelte/icons/copy'
  import Download from '@lucide/svelte/icons/download'
  import Undo2 from '@lucide/svelte/icons/undo-2'
  import WrapText from '@lucide/svelte/icons/wrap-text'
  import ListOrdered from '@lucide/svelte/icons/list-ordered'
  import X from '@lucide/svelte/icons/x'
  import { cn } from '$lib/utils.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import Kbd from './Kbd.svelte'

  let {
    open = $bindable(false),
    colName = 'value',
    colType = '',
    value = null,
    sourceHint = '',
    readOnly = false,
    detached = false,
    /** @type {{ bytes: number, dataType: string } | null} */
    oversize = null,
    /** @type {null | (() => Promise<void>)} */
    onloadfull = null,
    truncatedLoad = false,
    oncommit = /** @type {(next: string) => void} */ (() => {}),
    /** Fired when the dock dismisses itself, so the owner can take focus back. */
    onclose = /** @type {() => void} */ (() => {}),
    /**
     * Put the caret in the editor when the dock opens. Off for the grid, which
     * opens this as a preview: the cursor stays on the cell so arrows keep
     * walking the table, and the reader steps in deliberately (`focusEditor`).
     */
    autofocus = true,
  } = $props()

  /**
   * The value as text, pretty-printed when it is JSON and small enough to be
   * worth it.
   *
   * Indentation is not free. A jsonb holding a file as an array of byte
   * integers pretty-prints to one number per line: 16MB of compact JSON became
   * 41.8MB across three million lines, and that is a textarea the webview
   * cannot lay out. Past the line, the text stays exactly as it arrived.
   */
  function toText(/** @type {unknown} */ v) {
    if (v === null || v === undefined) return ''
    if (typeof v === 'object') {
      try {
        const compact = JSON.stringify(v)
        return compact.length > PRETTY_PRINT_LIMIT ? compact : JSON.stringify(v, null, 2)
      } catch { return String(v) }
    }
    const s = String(v)
    if (s.length > PRETTY_PRINT_LIMIT) return s
    // A JSON string stored in a text column is still JSON to the person reading
    // it, so it opens formatted rather than as one 4,000-character line.
    const t = s.trim()
    if ((t.startsWith('{') && t.endsWith('}')) || (t.startsWith('[') && t.endsWith(']'))) {
      try { return JSON.stringify(JSON.parse(t), null, 2) } catch { /* not JSON after all */ }
    }
    return s
  }

  /** Past this many characters, formatting costs more than it gives back. */
  const PRETTY_PRINT_LIMIT = 512 * 1024

  let draft = $state('')
  let original = $state('')
  /** @type {HTMLTextAreaElement | null} */
  let area = $state(null)
  /** The raw pane's editor (CodeMirror). `area` is now only the heavy-value window. */
  /** @type {any} */
  let cm = $state(null)
  /** @type {HTMLElement | null} */
  let root = $state(null)

  /** The cell the draft was seeded from - `colName` plus the row hint. */
  let seededCell = ''
  let wasOpen = false

  // Re-seed when the CELL changes, not on every `value` change: the parent keeps
  // the row reactive, and rewriting the draft under the cursor mid-edit loses
  // typing. The dock stays mounted and follows the grid's cell cursor, so "which
  // cell is this" has to be part of the comparison.
  //
  // Focus is taken on open only. Following the cursor must not pull focus out of
  // the grid, or the arrow key that moved it would be the last one that worked.
  $effect(() => {
    // The oversize flag is part of the identity: loading the full value swaps
    // what this cell holds without moving the cursor, and the draft has to
    // follow it.
    const cell = `${colName}\u0000${sourceHint}\u0000${detached ? 'd' : ''}\u0000${oversize ? 'preview' : 'full'}`
    // Read, not converted. `toText` used to run up here, above both guards, so
    // every re-run stringified the value and threw the result away - twice for
    // anything it pretty-prints, and on every arrow key, because the dock
    // follows the cursor. It even did it with the dock closed. The read is what
    // registers the dependency; the work belongs after the guards.
    const raw = value
    if (!open) { wasOpen = false; seededCell = ''; return }
    if (wasOpen && cell === seededCell) return
    const text = toText(raw)
    const justOpened = !wasOpen
    // An unstaged draft is about to be replaced by the cell the cursor moved to.
    // Said out loud, because losing typing silently is worse than a toast.
    if (!justOpened && draft !== original) {
      toast.info('Unstaged edit discarded', { description: seededCell.split('\u0000')[0], duration: 1800 })
    }
    seededCell = cell
    wasOpen = true
    original = text
    draft = text
    seedToken++
    // Structured text opens unwrapped: pretty-printed JSON is short lines
    // already, and unwrapped is what lets the gutter number them. Prose keeps
    // wrapping. Alt+Z still flips it either way.
    // Measured into a local and used from it. Reading `maxLineLen` back on the
    // next line made this effect depend on a value it had just written, which
    // is the read-and-write cycle Svelte refuses to run - it took down the
    // whole view with effect_update_depth_exceeded on any cell that reached it.
    const longest = longestLine(text)
    maxLineLen = longest
    wrap = longest <= MAX_WRAP_LINE && (wrapPref ?? !/^\s*[[{]/.test(text))
    // Undo/redo, word-delete and line-delete for every plain field in the app
    // live in `input-shortcuts.js`, and its history is keyed by element. This
    // textarea outlives the cell it is showing, so the history has to be
    // dropped whenever it is re-pointed - otherwise ⌘Z in one cell walks back
    // into the value of a cell you have already left.
    // Opening the dock puts the caret in the editor - but only on open. Moving
    // the grid cursor with the dock already up re-points it without taking
    // focus, so arrow keys keep walking the grid. Escape from the editor
    // closes the dock (`onRootKey`).
    if (justOpened && autofocus) {
      // Same rule as focusEditor('auto'), decided here because this is where
      // the seeded text is known: end of a short value, top of a long one.
      pendingCaret = text.length <= SMALL_VALUE_CHARS ? text.length : 0
      focusOnReady = true
    }
  })

  /**
   * Focus waits for the editor to exist. It is lazy-loaded, so on the first
   * open `cm` is still null when the cell is seeded - the old `cm?.focus()`
   * ran then and did nothing.
   */
  let focusOnReady = $state(false)
  /** Where the caret goes once the editor exists; -1 leaves it alone. */
  let pendingCaret = -1
  $effect(() => {
    if (!focusOnReady || !cm || !open) return
    focusOnReady = false
    const pos = pendingCaret
    pendingCaret = -1
    queueMicrotask(() => {
      if (pos >= 0) cm?.select(pos, pos)
      else cm?.focus()
    })
  })

  /**
   * Past this, the value is something to read from the top rather than a line
   * you are about to finish typing.
   */
  const SMALL_VALUE_CHARS = 2_000

  /**
   * Step into the editor from outside, once the dock is already up.
   *
   * `auto` puts the caret where the value says it should go: at the end of a
   * short value, which is almost always one you mean to edit, and at the start
   * of a long one, which is one you mean to read.
   * @param {'auto'|'start'|'end'} [caret]
   */
  export function focusEditor(caret = 'auto') {
    if (!open) return false
    const at = caret === 'auto' ? (draft.length <= SMALL_VALUE_CHARS ? 'end' : 'start') : caret
    const pos = at === 'end' ? draft.length : 0
    if (cm) { cm.select(pos, pos); return true }
    // Lazy-loaded: if it is not mounted yet, focus it the moment it is.
    pendingCaret = pos
    focusOnReady = true
    return true
  }

  const dirty = $derived(draft !== original)
  const isNull = $derived(value === null || value === undefined)

  /** Structured values get a tree pane; plain text does not need one. */
  /**
   * Past this, nothing runs over the whole value on its own.
   *
   * A jsonb holding a file as an array of byte integers comes back as tens of
   * megabytes of text. `JSON.parse` on 18MB is a few hundred milliseconds, and
   * a derived re-runs it on every keystroke; the search splits the same string
   * into highlight runs; a `<textarea>` with 18MB in it has to lay all of it
   * out. Each of those is fine at 100KB and none of them is fine at 18MB, so
   * above the line the tree is built when asked for, the highlight layer stands
   * down, and the raw pane is opt-in.
   */
  const HEAVY_VALUE_CHARS = 2 * 1024 * 1024

  const heavy = $derived(draft.length > HEAVY_VALUE_CHARS)

  /**
   * How much of a heavy value the raw pane shows at once.
   *
   * A `<textarea>` handed 8MB renders nothing at all in this webview - the pane
   * came up empty, which is worse than slow. So past the heavy line the raw
   * side is a read-only window onto the text, paged: it draws instantly at any
   * size, and the whole value is still reachable.
   */
  const RAW_WINDOW = 128 * 1024
  let rawOffset = $state(0)
  let seedToken = $state(0)
  // A new cell, or a value that just arrived, starts at the top.
  $effect(() => { void seedToken; rawOffset = 0 })

  /**
   * The slice on screen, and where it came from.
   *
   * Typing inside the window splices back into the full value, so the window is
   * its own state rather than a derived of `draft` - re-slicing on every
   * keystroke would cut the character just typed off the far edge.
   */
  let windowDraft = $state('')
  let windowStart = 0
  let windowLen = 0
  $effect(() => {
    void seedToken
    const at = rawOffset
    untrack(() => {
      windowStart = at
      windowDraft = draft.slice(at, at + RAW_WINDOW)
      windowLen = windowDraft.length
    })
  })
  const rawWindowEnd = $derived(windowStart + windowLen)

  /**
   * Whether the window can be typed into.
   *
   * Never on a value that was cut at the fetch ceiling: the text on screen is
   * not the whole value, and staging it would write the part that was loaded
   * over the part that was not. That is data loss, not an edit.
   */
  const canEditWindow = $derived(!readOnly && !oversize && !truncatedLoad)

  /** @param {string} next */
  function applyWindowEdit(next) {
    draft = draft.slice(0, windowStart) + next + draft.slice(windowStart + windowLen)
    windowDraft = next
    windowLen = next.length
  }

  /**
   * The one thing left of the old tree pane: saying when the text on screen is
   * not the whole value. The tree itself is gone - the editor already shows the
   * structure (colours, folding, brackets), and the tree re-ran JSON.parse over
   * the whole value on every keystroke to draw a second copy of it.
   */
  const notice = $derived.by(() => {
    // A capped cell ships its first 16KB, which for a JSON value stops
    // mid-token: parsing it can only ever fail, and reporting that failure as
    // "invalid JSON" sent people looking for a corrupt row that does not exist.
    // Name what actually happened instead.
    if (oversize) {
      // `draft.length` rather than a Blob: this runs on every keystroke, and
      // allocating a Blob to measure a string is the expensive way to ask.
      const loaded = draft.length
      return {
        ok: false,
        truncated: true,
        error: loaded
          ? `Showing the first ${formatBytes(loaded)} of ${formatBytes(oversize.bytes)}. The grid reads a preview of a column this wide, not the whole value.`
          : `The grid reads this column's size, not its contents, so a table full of cells this wide still opens fast.`,
      }
    }
    // Loaded, but the server stopped at the ceiling: the text really is cut, so
    // parsing it can only fail. Say which it is.
    if (truncatedLoad) {
      return {
        ok: false,
        truncated: true,
        error: `Loaded ${formatBytes(draft.length)}, which is as much of this value as this view holds. The rest is not shown, so it cannot be parsed or edited here - read it with a query if you need all of it.`,
      }
    }
    return null
  })

  let loadingFull = $state(false)
  async function loadFull() {
    if (!onloadfull || loadingFull) return
    loadingFull = true
    try {
      await onloadfull()
    } catch (e) {
      toast.error('Could not load the value', { description: String(e?.message ?? e) })
    } finally {
      loadingFull = false
    }
  }

  /** @param {number} n */
  function formatBytes(n) {
    if (n < 1024) return `${n} B`
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} KB`
    return `${(n / 1024 / 1024).toFixed(1)} MB`
  }
  /** The editor is the only pane now; kept as a name for the heavy-window paths. */
  const rawOpen = true


  /**
   * Put the stored value back.
   *
   * Written through the textarea rather than by assigning `draft`, for the same
   * reason the Tab handler below does: a state write changes the value without an
   * `input` event, so the shared undo stack never records the step and the revert
   * would be the one edit ⌘Z could not bring back. Reverting is a big change to
   * make with one keystroke - it should be as undoable as any other.
   */
  function revert() {
    if (readOnly || !dirty) return
    // The tree pane has no textarea to write through; nothing to keep undoable
    // there either, since the tree is not what you edited it with.
    if (!rawOpen || !cm) {
      draft = original
      return
    }
    // One editor transaction, so ⌘Z brings the edited text back.
    cm.replaceAll(original)
  }

  // `split` allocates an array the size of the line count; on a one-line 18MB
  // value that is cheap, on a large multi-line one it is not, and the number is
  // chrome either way.
  const lines = $derived.by(() => {
    if (!draft) return 0
    if (heavy) return 0
    return draft.split('\n').length
  })
  const chars = $derived(draft.length)

  /**
   * Soft wrap, ON by default.
   *
   * The values that need this panel are prose, JSON and stack traces - one long
   * line each - and the point of opening a cell full size is to read it, which
   * no-wrap turns into horizontal scrolling. An editor defaults the other way
   * because it holds source, where a line is a unit; a cell is not.
   *
   * Wrapped, logical line 4 can occupy six rows, so the gutter would have to
   * either number visual rows (disagreeing with the count in the bar and with
   * any error quoting a line) or lie: it hides instead, and Alt+Z turns wrap off
   * to bring it back.
   */
  let wrap = $state(true)
  // Wrapping is what a multi-megabyte value cannot afford: one 18MB line laid
  // out across the pane's width is the most expensive thing this panel can be
  // asked to do. It comes back the moment the value is small again, and the
  // toggle still works if you want it.
  $effect(() => { if (heavy) wrap = false })

  /**
   * Longest line in the value, measured once per cell rather than per keystroke
   * and without a `split('\n')`, which would allocate a second copy of a value
   * already big enough to be the problem.
   */
  let maxLineLen = $state(0)
  function longestLine(/** @type {string} */ text) {
    let max = 0
    let at = 0
    for (;;) {
      const nl = text.indexOf('\n', at)
      if (nl === -1) return Math.max(max, text.length - at)
      if (nl - at > max) max = nl - at
      at = nl + 1
    }
  }

  /**
   * The line length past which soft wrap is refused, not merely defaulted off.
   *
   * CodeMirror virtualises by line: rows outside the viewport cost nothing, but
   * a single line always lays out whole. A jsonb column holding a file arrives
   * as one line of half a million characters, and wrapping that means measuring
   * every one of them into a few thousand visual rows in one frame. The editor
   * stops answering, which is what it did here before this line existed.
   *
   * Unwrapped there is no such cost: the line is one row and the view draws the
   * slice that is on screen. So past the cap the toggle is disabled rather than
   * merely off, because turning it on is the hang.
   */
  const MAX_WRAP_LINE = 10_000
  const canWrap = $derived(maxLineLen <= MAX_WRAP_LINE)
  // Forced off because the value cannot afford it, which is not a preference and
  // must not be saved as one.
  $effect(() => { if (!canWrap) wrap = false })

  /**
   * Whether to wrap is a reading preference, not a property of the cell, so it
   * outlives the cell. Stored as the answer the reader last gave; until they
   * give one, structured text opens unwrapped and prose opens wrapped.
   */
  const WRAP_PREF_KEY = 'stroke:cell-editor-wrap'
  /** @type {boolean | null} */
  let wrapPref = (() => {
    try {
      const v = localStorage.getItem(WRAP_PREF_KEY)
      return v === '1' ? true : v === '0' ? false : null
    } catch { return null }
  })()

  /**
   * Line numbers, remembered the same way soft wrap is.
   *
   * It holds while wrapped too. The gutter numbers logical lines, so a wrapped
   * line carries its number on its first visual row and nothing on the rest,
   * which is what every editor that wraps does and is the reading everyone
   * already has. Tying this to wrap only took the choice away.
   */
  const GUTTER_PREF_KEY = 'stroke:cell-editor-gutter'
  let showGutter = $state((() => {
    try { return localStorage.getItem(GUTTER_PREF_KEY) !== '0' } catch { return true }
  })())

  function toggleGutter() {
    showGutter = !showGutter
    try { localStorage.setItem(GUTTER_PREF_KEY, showGutter ? '1' : '0') } catch { /* private window, or storage is full */ }
  }

  /** The toggle and Alt+Z. Only an explicit answer is remembered. */
  function toggleWrap() {
    if (!canWrap) return
    wrap = !wrap
    wrapPref = wrap
    try { localStorage.setItem(WRAP_PREF_KEY, wrap ? '1' : '0') } catch { /* private window, or storage is full */ }
  }


  // The editor's type metrics, as whole pixels. The gutter has to sit on the
  // same baseline grid as the text beside it, and a fractional line-height
  // (1.5 × 13px = 19.5px) drifts a pixel per line against a separate box that
  // rounds it the other way - by line 30 the numbers no longer line up.
  const LINE_H = 20


  /**
   * Close the dock and tell the owner. Closing alone left focus on a element
   * that was about to be removed, so it fell back to <body> and the grid
   * stopped answering arrow keys - every dismissal has to hand focus back.
   */
  function dismiss() {
    open = false
    onclose()
  }

  function apply() {
    if (readOnly || !dirty) { dismiss(); return }
    oncommit(draft)
    dismiss()
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(draft)
      toast.success('Copied', { duration: 1800 })
    } catch (e) {
      toast.error('Could not copy', { description: String(e?.message ?? e) })
    }
  }

  /**
   * Escape, from anywhere in the dock.
   *
   * It used to live on the textarea alone, which was fine while the textarea
   * was the only thing in here and always had focus. It is not: a JSON cell
   * opens on the tree with the textarea hidden, `focus()` on a hidden element
   * does nothing, and a click on a tree row or a bar button leaves focus on
   * something with no handler - so Escape went nowhere. On the root it catches
   * the key whichever child it bubbled from.
   *
   * @param {KeyboardEvent} e
   */
  function onRootKey(e) {
    // A key the editor already handled (Escape closing its find panel, Mod-F
    // opening it) is not the dock's: without this, the Escape that closed find
    // also closed the whole dock.
    if (e.defaultPrevented) return
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      dismiss()
      return
    }
    // ⌘F / Ctrl+F from anywhere in the dock opens the editor's find panel.
    // Inside the editor its own keymap has already done it (defaultPrevented).
    if ((e.metaKey || e.ctrlKey) && (e.key === 'f' || e.key === 'F')) {
      e.preventDefault()
      e.stopPropagation()
      cm?.find()
      return
    }
  }

  /** @param {KeyboardEvent} e */
  /** The panel's keys, handed to the editor so they win over its defaults. */
  const editorKeys = [
    // Cmd/Ctrl+Enter applies, matching every other multi-line editor in the app.
    { key: 'Mod-Enter', run: () => { apply(); return true } },
    // Alt+Z toggles wrap, Alt+R reverts - VS Code's keys.
    { key: 'Alt-z', run: () => { toggleWrap(); return true } },
    { key: 'Alt-l', run: () => { toggleGutter(); return true } },
    { key: 'Alt-r', run: () => { revert(); return true } },
  ]

</script>

<!-- Docked bottom panel, fills the dock's height (flex column). The dock
     container provides the top border + resize handle, exactly as it does for
     the related-rows panel. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- `tabindex=-1` so the panel itself can hold focus when the raw textarea is
     hidden, which is what gives Escape somewhere to land. -->
<div
  bind:this={root}
  tabindex="-1"
  role="group"
  aria-label="{colName} value"
  class="flex h-full min-h-0 w-full flex-col bg-background outline-none"
  onkeydown={onRootKey}
>

  <!-- One bar, not a header and a footer: the counts and the two actions fit
       beside the name, and a 28px row of value did not need 92px of chrome. -->
  <div class="flex h-8 shrink-0 items-center gap-2 border-b border-border/40 bg-muted/20 px-2.5">
    <Pencil class="size-3.5 shrink-0 text-muted-foreground" />
    <!-- One shrinking group for everything that describes the value, so a
         narrow dock takes room from the description rather than pushing the
         buttons out of the bar. Every badge in here is shrink-0, so without a
         container that can give way the row simply grew past its own width and
         Stage and Close went off the end of it. -->
    <div class="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
    <!-- title: this is the only place the column is named, and it is the first
         thing to be truncated when the bar runs out of room. -->
    <span
      class="min-w-0 truncate font-mono text-ui-2xs font-medium text-foreground/85"
      title={colType ? `${colName} · ${colType}` : colName}
    >{colName}</span>
    {#if colType}
      <span class="shrink-0 rounded-[3px] border border-border/50 bg-muted/40 px-1.5 py-px font-mono text-ui-3xs text-muted-foreground">{colType}</span>
    {/if}
    {#if sourceHint}
      <!-- ms-2: the gap between identity (name, type) and the facts about
           this value is twice the gap inside either group. -->
      <!-- tabular-nums: this counts up as the cursor moves, and proportional
           digits made everything after it shift on the step from row 9 to 10. -->
      <span class="ms-2 shrink-0 font-mono text-ui-3xs tabular-nums text-muted-foreground">{sourceHint}</span>
    {/if}
    {#if isNull && !dirty}
      <span class="shrink-0 font-mono text-ui-3xs text-muted-foreground">NULL</span>
    {/if}
    {#if readOnly}
      <span class="shrink-0 font-mono text-ui-3xs text-muted-foreground">
        {detached ? 'inside a row · read-only' : 'read-only'}
      </span>
    {/if}
    <span class="shrink-0 font-mono text-ui-3xs tabular-nums text-muted-foreground">
      {lines.toLocaleString()}L · {chars.toLocaleString()}c
    </span>
    {#if oversize}
      <span
        class="shrink-0 rounded-[3px] border border-warning/30 bg-warning/10 px-1.5 py-px font-mono text-ui-3xs text-warning"
        title="A wide column reports its size per row instead of its contents - the value is not loaded and cannot be edited until it is"
      >{formatBytes(oversize.bytes)} · not loaded</span>
    {/if}
    {#if dirty && !readOnly}
      <span class="shrink-0 text-ui-3xs text-primary">edited</span>
    {/if}
    </div>

    <!-- Three groups, spaced apart more than their members: view tools
         (wrap, copy, revert), the commit (Stage), and close. Find is Mod+F in
         the editor and needs no button here. -->
    <div class="flex shrink-0 items-center gap-0.5">
      <button
        type="button"
        disabled={!canWrap}
        aria-pressed={wrap}
        class={cn(
          'inline-flex size-7 items-center justify-center rounded-md transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40 disabled:hover:bg-transparent',
          wrap ? 'text-foreground' : 'text-muted-foreground',
        )}
        onclick={toggleWrap}
        title={canWrap
          ? 'Soft wrap (Alt+Z)'
          : `Soft wrap is off for this value: its longest line is ${maxLineLen.toLocaleString()} characters, and wrapping one line that long lays it out all at once`}
      >
        <WrapText class="size-3.5 shrink-0" />
      </button>
      <!-- Works wrapped too: a wrapped line keeps its number on its first
           visual row, the way every wrapping editor does it. -->
      <button
        type="button"
        aria-pressed={showGutter}
        class={cn(
          'inline-flex size-7 items-center justify-center rounded-md transition-colors hover:bg-accent hover:text-foreground',
          showGutter ? 'text-foreground' : 'text-muted-foreground',
        )}
        onclick={toggleGutter}
        title="{showGutter ? 'Hide' : 'Show'} line numbers (Alt+L)"
      >
        <ListOrdered class="size-3.5 shrink-0" />
      </button>
      <!-- Icon only, like the wrap toggle beside it. The word "Copy" next to a
           copy glyph is the label saying what the picture already says, and this
           row has to fit a find box, three actions and a close button. -->
      <button
        type="button"
        class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={copy}
        title="Copy the value as text"
        aria-label="Copy the value as text"
      >
        <Copy class="size-3.5 shrink-0" />
      </button>
      {#if !readOnly}
        <button
          type="button"
          class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40 disabled:hover:bg-transparent"
          disabled={!dirty}
          onclick={revert}
          title="Revert to the stored value (Alt+R)"
          aria-label="Revert to the stored value"
        >
          <Undo2 class="size-3.5 shrink-0" />
        </button>
        <button
          type="button"
          class="ms-3 inline-flex h-7 items-center gap-1.5 rounded-md bg-primary/10 px-2.5 font-mono text-ui-2xs text-primary transition-colors hover:bg-primary/15 disabled:bg-transparent disabled:text-muted-foreground disabled:opacity-60"
          disabled={!dirty}
          onclick={apply}
          title="Stage this value with your other pending edits"
        >
          Stage change
          <Kbd combo="Mod+Enter" />
        </button>
      {/if}
      <button
        type="button"
        class="ms-2 flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={dismiss}
        aria-label="Close cell editor"
      >
        <X class="size-3.5 shrink-0" />
      </button>
    </div>
  </div>

  <!-- Editor, and the preview beside it only when there is something to
       preview. A second pane holding a copy of the same plain text would be two
       views of one thing. -->
  <!-- `relative`: the raw pane's highlight layer is absolutely positioned inside
       this box, beside the gutter rather than under it. -->
  <!-- Partly loaded: a slim bar over the editor, with the one action that fixes
       it. It used to live in the tree pane, which is gone. -->
  {#if notice}
    <div class="flex shrink-0 items-start gap-3 border-b border-border/30 bg-muted/15 px-3 py-2">
      <p class="min-w-0 flex-1 text-ui-2xs leading-normal text-muted-foreground text-pretty">{notice.error}</p>
      {#if oversize && onloadfull}
        <Button
          disabled={loadingFull}
          class="shrink-0" size="sm"
          onclick={() => void loadFull()}
>
          {#if loadingFull}
            <span class="size-3 shrink-0 animate-spin rounded-full border border-current border-t-transparent"></span>
            Loading {formatBytes(oversize?.bytes ?? 0)}
          {:else}
            Load {formatBytes(oversize?.bytes ?? 0)}
          {/if}
        </Button>
      {/if}
    </div>
  {/if}
  <div class="relative flex min-h-0 flex-1">
    {#if heavy && rawOpen}
      <!-- Read-only, and only a slice of it. Everything a textarea gives you -
           editing, undo, a caret - costs the browser a full layout of the value,
           and at this size that is a pane that never paints. -->
      <div class="flex min-h-0 flex-1 flex-col">
        <div class="flex h-7 shrink-0 items-center gap-2 border-b border-border/30 px-2.5">
          <span class="font-mono text-ui-3xs text-muted-foreground tabular-nums">
            {formatBytes(rawOffset)}–{formatBytes(rawWindowEnd)} of {formatBytes(draft.length)}
          </span>
          {#if canEditWindow}
            <span class="rounded-[3px] border border-primary/30 bg-primary/10 px-1.5 py-px font-mono text-ui-3xs text-primary">editable</span>
          {:else}
            <span
              class="rounded-[3px] border border-border/40 px-1.5 py-px font-mono text-ui-3xs text-muted-foreground"
              title={truncatedLoad
                ? 'Only part of this value is loaded. Editing it here would write what was loaded over what was not.'
                : 'This value is not loaded'}
            >read-only</span>
          {/if}
          <div class="ml-auto flex items-center gap-1">
            <button
              type="button"
              disabled={rawOffset === 0}
              class="inline-flex h-6 items-center rounded-md px-2 font-mono text-ui-3xs text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
              onclick={() => (rawOffset = Math.max(0, rawOffset - RAW_WINDOW))}
            >Back</button>
            <button
              type="button"
              disabled={rawWindowEnd >= draft.length}
              class="inline-flex h-6 items-center rounded-md px-2 font-mono text-ui-3xs text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
              onclick={() => (rawOffset = Math.min(draft.length - 1, rawOffset + RAW_WINDOW))}
            >More</button>
          </div>
        </div>
        <textarea
          value={windowDraft}
          readonly={!canEditWindow}
          spellcheck="false"
          aria-label="{colName} value, characters {windowStart} to {rawWindowEnd}"
          oninput={(e) => applyWindowEdit(/** @type {HTMLTextAreaElement} */ (e.currentTarget).value)}
          class="no-field-frame app-scroll min-h-0 flex-1 resize-none whitespace-pre-wrap [overflow-wrap:anywhere] bg-transparent px-3 py-2 font-mono text-ui-2xs text-foreground outline-none"
          style="line-height:{LINE_H}px; tab-size:2"
        ></textarea>
      </div>
    {/if}
    <!-- `hidden`, not unmounted: the editor holds the draft, the undo history
         and the caret. Tearing it down to show the tree would discard all three
         and re-seed the value on the way back. -->
    <div class={cn('min-h-0 min-w-0 flex-1 flex-col', !rawOpen || heavy ? 'hidden' : 'flex')}>
      <!-- Loaded on first use: CodeMirror is ~170KB gzipped, and in the boot
           chunk every launch paid for an editor most sessions never open. -->
      {#await import('./CodeEditor.svelte') then { default: CodeEditor }}
      <CodeEditor
        bind:this={cm}
        bind:value={draft}
        {readOnly}
        {wrap}
        gutter={showGutter}
        placeholder={isNull ? 'NULL' : ''}
        ariaLabel="{colName} value"
        keys={editorKeys}
      />
      {/await}
    </div>
  </div>
</div>
