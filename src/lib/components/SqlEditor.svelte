<script>
  /**
   * The SQL editor of the console and notebook cells, on CodeMirror.
   *
   * It was Monaco. Monaco is 3.8MB, positions its suggestion list with
   * `position: fixed` (wrong inside any transformed ancestor), and was a second
   * editor next to the CodeMirror one the cell and review docks already use.
   * This builds on that same CodeEditor - one theme, one find panel, one
   * completion engine (cm-sql-complete.js, a port of the Monaco provider) -
   * and adds what a console needs: statement-aware run keys, the lint and
   * ran-OK marks in one glyph gutter, the active-statement bar, Format and Vim.
   *
   * Props and exported methods are unchanged, so SqlConsole and SqlCell did not
   * have to change.
   */
  import { onMount } from 'svelte'
  import { StateEffect, StateField, RangeSet, RangeSetBuilder, Prec } from '@codemirror/state'
  import { EditorView, Decoration, ViewPlugin, GutterMarker, gutter, gutterLineClass } from '@codemirror/view'
  import CodeEditor from './CodeEditor.svelte'
  import { Tick02Icon, AlertCircleIcon, Alert02Icon } from '@hugeicons/core-free-icons'
  import { hugeSvg } from '$lib/cm-huge-icon.js'
  import { formatSql } from '$lib/format-sql.js'
  import { statementAtOffset, lintSql } from '$lib/sql-statements.js'
  import { statementsOf } from '$lib/cm-sql-statements.js'
  import { appVimMode, appSqlEditor } from '$lib/stores/settings.js'
  import { sqlEditorFontSize } from '$lib/sql-editor-options.js'
  import { setVimSubMode } from '$lib/vim/vim.js'
  import { cn } from '$lib/utils.js'

  /** @typedef {import('$lib/sql-complete-data.js').SqlSchemaHints} SqlSchemaHints */
  /** @typedef {import('$lib/sql-statements.js').SqlStatement} SqlStatement */
  /** @typedef {import('$lib/sql-statements.js').SqlDiagnostic} SqlDiagnostic */

  let {
    value = $bindable(''),
    class: className = '',
    readOnly = false,
    schemaHints = /** @type {SqlSchemaHints} */ ({}),
    /** The app's Dialect id, for keyword casing and identifier quoting. */
    dialect = 'postgres',
    onmodk = undefined,
    onmodenter = undefined,
    /**
     * Run a single statement (Ctrl/Cmd+R) - receives the selected text, or the
     * statement under the cursor when there is no selection.
     * @type {((sql: string) => void) | undefined}
     */
    onrunstatement = undefined,
    onmods = undefined,
    // Global app shortcuts - bound inside the editor so they work while it has focus
    onmodi = undefined,
    onmodw = undefined,
    onmodn = undefined,
    onmodm = undefined,
    onmodt = undefined,
    onmodshifte = undefined,
    onmodshiftd = undefined,
    onmodaltd = undefined,
    onmodshifto = undefined,
    onmodj = undefined,
    onmodshiftb = undefined,
    /** @param {string} content */
    onchange = undefined,
    /** @type {(actions: { format: () => Promise<void> }) => void} */
    onactionsready = undefined,
  } = $props()

  /** @type {HTMLDivElement | null} */
  let host = $state(null)
  /** @type {{ getView: () => EditorView | null, focus: () => void } | null} */
  let editorRef = $state(null)

  // ── Statements ─────────────────────────────────────────────────────────────
  // statementsOf (cm-sql-statements.js) splits once per document version, and
  // completion reads the same split.

  /** @param {import('@codemirror/state').EditorState} state */
  function statementAtCaret(state) {
    return statementAtOffset(statementsOf(state), state.selection.main.head)
  }

  // ── Glyph gutter: ran-OK ✓ and lint dots ───────────────────────────────────

  class GlyphMarker extends GutterMarker {
    /** @param {'ok' | 'error' | 'warning'} kind @param {string} title */
    constructor(kind, title) {
      super()
      this.kind = kind
      this.title = title
    }
    /** @param {GlyphMarker} other */
    eq(other) { return other.kind === this.kind && other.title === this.title }
    toDOM() {
      const el = document.createElement('span')
      el.className = `sql-glyph sql-glyph-${this.kind}`
      el.title = this.title
      el.append(hugeSvg(this.kind === 'ok' ? Tick02Icon : this.kind === 'error' ? AlertCircleIcon : Alert02Icon))
      return el
    }
  }

  /** Line starts of the statements that just ran. Cleared by any edit. */
  const setExecuted = StateEffect.define()
  const executedField = StateField.define({
    create: () => /** @type {number[]} */ ([]),
    update(lines, tr) {
      for (const e of tr.effects) if (e.is(setExecuted)) return /** @type {number[]} */ (e.value)
      return tr.docChanged ? [] : lines
    },
  })

  /** Lint results: squiggles, plus one dot per line in the glyph gutter. */
  const setLint = StateEffect.define()
  const lintField = StateField.define({
    create: () => ({ diags: /** @type {SqlDiagnostic[]} */ ([]), deco: Decoration.none }),
    update(value, tr) {
      for (const e of tr.effects) if (e.is(setLint)) return /** @type {any} */ (e.value)
      // Until the next pass lands, keep the marks on the text they were for.
      return tr.docChanged ? { diags: [], deco: value.deco.map(tr.changes) } : value
    },
    provide: (f) => EditorView.decorations.from(f, (v) => v.deco),
  })

  /** @param {import('@codemirror/state').EditorState} state */
  function lintFor(state) {
    const text = state.doc.toString()
    const diags = readOnly || !$appSqlEditor.lint ? [] : lintSql(text)
    const deco = Decoration.set(
      diags
        .filter((d) => d.end > d.start)
        .map((d) =>
          Decoration.mark({
            class: d.severity === 'error' ? 'cm-sql-lint-error' : 'cm-sql-lint-warning',
            attributes: { title: d.message },
          }).range(Math.min(d.start, text.length), Math.min(d.end, text.length)),
        ),
      true,
    )
    return { diags, deco }
  }

  // Lint 350ms after the last keystroke, the delay the Monaco editor used.
  const lintRunner = ViewPlugin.fromClass(
    class {
      /** @param {EditorView} view */
      constructor(view) {
        this.view = view
        this.live = true
        /** @type {ReturnType<typeof setTimeout> | null} */
        this.timer = null
        // Not from the constructor itself: a plugin may not dispatch while the
        // view is still being built.
        queueMicrotask(() => this.run())
      }
      run() {
        this.timer = null
        if (this.live) this.view.dispatch({ effects: setLint.of(lintFor(this.view.state)) })
      }
      /** @param {import('@codemirror/view').ViewUpdate} u */
      update(u) {
        if (!u.docChanged) return
        if (this.timer) clearTimeout(this.timer)
        this.timer = setTimeout(() => this.run(), 350)
      }
      destroy() {
        this.live = false
        if (this.timer) clearTimeout(this.timer)
      }
    },
  )

  const glyphGutter = gutter({
    class: 'cm-sql-glyphs',
    markers(view) {
      /** @type {Map<number, GlyphMarker>} */
      const byLine = new Map()
      for (const from of view.state.field(executedField)) byLine.set(from, new GlyphMarker('ok', 'Ran successfully'))
      for (const d of view.state.field(lintField).diags) {
        const from = view.state.doc.lineAt(Math.min(d.start, view.state.doc.length)).from
        const prev = byLine.get(from)
        if (prev?.kind === 'error') continue
        byLine.set(from, new GlyphMarker(d.severity === 'error' ? 'error' : 'warning', d.message))
      }
      const builder = new RangeSetBuilder()
      for (const from of [...byLine.keys()].sort((a, b) => a - b)) builder.add(from, from, /** @type {GlyphMarker} */ (byLine.get(from)))
      return builder.finish()
    },
    initialSpacer: () => new GlyphMarker('ok', ''),
  })

  // ── Active statement: a bar beside the lines of the one under the caret ──
  // Only when the buffer holds more than one, so a single query stays clean.

  class ActiveLineMarker extends GutterMarker {
    elementClass = 'cm-stmt-active'
  }
  const activeLine = new ActiveLineMarker()
  const activeStatement = StateField.define({
    create: (state) => activeRanges(state),
    update: (v, tr) => (tr.docChanged || tr.selection ? activeRanges(tr.state) : v),
    provide: (f) => gutterLineClass.from(f),
  })
  /** @param {import('@codemirror/state').EditorState} state */
  function activeRanges(state) {
    const stmts = statementsOf(state)
    const stmt = stmts.length > 1 ? statementAtOffset(stmts, state.selection.main.head) : null
    if (!stmt) return RangeSet.empty
    const builder = new RangeSetBuilder()
    const last = state.doc.lineAt(Math.min(stmt.end, state.doc.length)).number
    for (let n = state.doc.lineAt(stmt.start).number; n <= last; n++) {
      const from = state.doc.line(n).from
      builder.add(from, from, activeLine)
    }
    return builder.finish()
  }

  // ── Keys ───────────────────────────────────────────────────────────────────
  // A handler that is not wired returns false, so the key falls through to the
  // app's global hotkeys instead of being swallowed here.

  /** @param {(() => void) | undefined} fn */
  const call = (fn) => () => { if (!fn) return false; fn(); return true }

  const keys = [
    { key: 'Mod-k', run: () => call(onmodk)() },
    { key: 'Mod-s', run: () => call(onmods)(), preventDefault: true },
    { key: 'Mod-l', run: selectStatement, preventDefault: true },
    { key: 'Mod-r', run: runStatement, preventDefault: true },
    { key: 'Mod-i', run: () => call(onmodi)() },
    { key: 'Mod-w', run: () => call(onmodw)() },
    { key: 'Mod-n', run: () => call(onmodn)() },
    { key: 'Mod-m', run: () => call(onmodm)() },
    { key: 'Mod-t', run: () => call(onmodt)() },
    { key: 'Mod-Shift-d', run: () => call(onmodshiftd)() },
    { key: 'Mod-Alt-d', run: () => call(onmodaltd)() },
    { key: 'Mod-Shift-e', run: () => call(onmodshifte)() },
    { key: 'Mod-Shift-o', run: () => call(onmodshifto)() },
    { key: 'Mod-j', run: () => call(onmodj)() },
    { key: 'Mod-Shift-b', run: () => call(onmodshiftb)() },
  ]

  /** Ctrl/Cmd+L - select the statement under the caret. @param {EditorView} view */
  function selectStatement(view) {
    const stmt = statementAtCaret(view.state)
    if (!stmt) return false
    view.dispatch({ selection: { anchor: stmt.start, head: stmt.end }, scrollIntoView: true })
    return true
  }

  /** Ctrl/Cmd+R - run the selection, else the statement under the caret. @param {EditorView} view */
  function runStatement(view) {
    if (!onrunstatement) return false
    const sel = view.state.selection.main
    const text = sel.empty ? statementAtCaret(view.state)?.text ?? '' : view.state.sliceDoc(sel.from, sel.to).trim()
    if (text) onrunstatement(text)
    return true
  }

  // ── Extensions ─────────────────────────────────────────────────────────────

  const consoleTheme = EditorView.theme({
    '.cm-content': { padding: '12px 0' },
    // Glyphs sit right against the numbers: Monaco's glyph margin, not a column.
    '.cm-sql-glyphs .cm-gutterElement': {
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      width: '14px',
      paddingLeft: '8px',
    },
    '.sql-glyph': { display: 'inline-flex', cursor: 'default' },
    '.sql-glyph svg': { width: '12px', height: '12px' },
    '.sql-glyph-ok': { color: 'var(--color-green-500, #22c55e)' },
    '.sql-glyph-error': { color: 'var(--destructive, #ef4444)' },
    '.sql-glyph-warning': { color: 'var(--color-amber-500, #f59e0b)' },
    // Three digits reserved, not the cell dock's five: a query is rarely past
    // line 999, and the reserve was the gap between the glyphs and the numbers.
    '.cm-gutters .cm-lineNumbers .cm-gutterElement': {
      minWidth: 'calc(3ch + 10px)',
      padding: '0 4px 0 6px',
    },
    '.cm-sql-lint-error': {
      textDecoration: 'underline wavy color-mix(in oklch, var(--destructive) 85%, transparent)',
      textUnderlineOffset: '3px',
    },
    '.cm-sql-lint-warning': {
      textDecoration: 'underline wavy color-mix(in srgb, var(--color-amber-500, #f59e0b) 75%, transparent)',
      textUnderlineOffset: '3px',
    },
    // The active-statement bar sits on the right edge of the last gutter
    // column - the numbers, or the glyphs when numbers are hidden.
    '.cm-gutter:last-child .cm-gutterElement.cm-stmt-active': {
      boxShadow: 'inset -2px 0 0 color-mix(in srgb, var(--primary) 45%, transparent)',
    },
    // Vim's mode / command line, where monaco-vim's status strip was.
    '.cm-vim-panel': {
      padding: '2px 12px',
      fontFamily: 'var(--font-mono)',
      fontSize: 'var(--fs-2xs)',
      lineHeight: '20px',
      color: 'var(--muted-foreground)',
      backgroundColor: 'color-mix(in oklch, var(--muted) 20%, transparent)',
      borderTop: '1px solid color-mix(in oklch, var(--border) 40%, transparent)',
    },
    '.cm-vim-panel input': { background: 'transparent', color: 'var(--foreground)', outline: 'none' },
  })

  const baseExtensions = [
    Prec.high(glyphGutter),
    executedField,
    lintField,
    lintRunner,
    activeStatement,
    consoleTheme,
  ]

  /** Loaded while Vim mode is on (lazily - it is only for the few who use it). */
  let vimExtension = $state(/** @type {import('@codemirror/state').Extension | null} */ (null))
  const extensions = $derived(vimExtension ? [Prec.highest(vimExtension), ...baseExtensions] : baseExtensions)

  $effect(() => {
    if (!$appVimMode) { vimExtension = null; return }
    let cancelled = false
    import('@replit/codemirror-vim')
      .then(({ vim }) => { if (!cancelled) vimExtension = vim({ status: true }) })
      .catch(() => {})
    return () => { cancelled = true }
  })

  // Mirror Vim's mode into the shared status-bar indicator.
  $effect(() => {
    const ext = vimExtension
    const view = editorRef?.getView()
    if (!ext || !view) return
    let cm = null
    /** @param {{ mode: string }} e */
    const onMode = (e) => setVimSubMode(e.mode === 'insert' ? 'insert' : e.mode === 'visual' ? 'visual' : 'normal')
    // A frame later: CodeEditor installs the extension in its own effect.
    import('@replit/codemirror-vim').then(({ getCM }) => requestAnimationFrame(() => {
      cm = getCM(view)
      cm?.on('vim-mode-change', onMode)
      setVimSubMode('normal')
    }))
    return () => cm?.off('vim-mode-change', onMode)
  })

  // Problem markers switched on or off in Settings: lint again (or clear) now,
  // not on the next keystroke.
  $effect(() => {
    void $appSqlEditor.lint
    const view = editorRef?.getView()
    view?.dispatch({ effects: setLint.of(lintFor(view.state)) })
  })

  // ── Mount ──────────────────────────────────────────────────────────────────

  onMount(() => {
    onactionsready?.({ format })

    // Document-level capture so Ctrl/Cmd+Enter runs even before the editor has
    // been clicked into. Skipped when this editor is hidden (an inactive tab),
    // when a real field elsewhere has focus, and when focus is in a different
    // SQL editor - several notebook cells are visible at once.
    /** @param {KeyboardEvent} e */
    function docRunHandler(e) {
      if (!host || host.clientWidth === 0) return
      if (!(e.ctrlKey || e.metaKey) || e.key !== 'Enter' || e.shiftKey || e.altKey) return
      const ae = document.activeElement
      if (ae && ae !== document.body && !host.contains(ae)) {
        if (ae.closest('.sql-editor-host')) return
        if (ae.tagName === 'INPUT' || ae.tagName === 'TEXTAREA' || /** @type {HTMLElement} */ (ae).isContentEditable) return
      }
      e.preventDefault()
      e.stopPropagation()
      onmodenter?.()
    }
    document.addEventListener('keydown', docRunHandler, { capture: true, passive: false })
    return () => document.removeEventListener('keydown', docRunHandler, { capture: true })
  })

  // ── API ────────────────────────────────────────────────────────────────────

  /** Format the whole buffer as one undoable edit. */
  async function format() {
    const view = editorRef?.getView()
    if (!view) return
    const text = view.state.doc.toString()
    const formatted = formatSql(text)
    if (formatted !== text) view.dispatch({ changes: { from: 0, to: text.length, insert: formatted } })
  }

  /**
   * Mark statement(s) as run OK with a ✓ in the glyph gutter. Pass the single
   * statement that ran (⌘R), or null for all of them (run all). The marks clear
   * on the next edit.
   * @param {string | null} [ranStatement]
   */
  export function markExecuted(ranStatement = null) {
    const view = editorRef?.getView()
    if (!view) return
    const target = typeof ranStatement === 'string' ? ranStatement.trim().replace(/;+\s*$/, '') : null
    const lines = []
    for (const stmt of statementsOf(view.state)) {
      if (target !== null && stmt.text.replace(/;+\s*$/, '') !== target) continue
      lines.push(view.state.doc.lineAt(stmt.start).from)
    }
    view.dispatch({ effects: setExecuted.of(lines) })
  }

  /** Focus the editor (called when the SQL tab becomes active). */
  export function focus() {
    editorRef?.focus()
  }

  /** Text of the statement under the caret ('' when the buffer is empty). */
  export function getStatementAtCursor() {
    const view = editorRef?.getView()
    return (view && statementAtCaret(view.state)?.text) || ''
  }

  /** Current selection text ('' when nothing is selected). */
  export function getSelectionText() {
    const view = editorRef?.getView()
    const sel = view?.state.selection.main
    return view && sel && !sel.empty ? view.state.sliceDoc(sel.from, sel.to).trim() : ''
  }
</script>

<!-- `sql-editor-host`: app.css keeps text selectable in here, and the app's
     Vim layer leaves an editor with this class to its own Vim mode. The CSS
     variables size CodeEditor's theme from Settings → SQL editor → Text size,
     a type-scale step, so it scales with zoom. -->
<div
  bind:this={host}
  class={cn('sql-editor-host flex h-full min-h-0 w-full flex-col', className)}
  style="--cm-font-size: {sqlEditorFontSize($appSqlEditor.textSize)}; --cm-line-height: 1.65; --cm-font-family: var(--editor-font-family, var(--font-mono));"
>
  <CodeEditor
    bind:this={editorRef}
    bind:value
    lang="sql"
    {dialect}
    sqlHints={schemaHints}
    {readOnly}
    wrap={$appSqlEditor.wrap}
    gutter={$appSqlEditor.lineNumbers}
    folding={$appSqlEditor.folding}
    suggestWhileTyping={$appSqlEditor.suggestWhileTyping}
    {onchange}
    {keys}
    {extensions}
    ariaLabel="SQL editor"
  />
</div>
