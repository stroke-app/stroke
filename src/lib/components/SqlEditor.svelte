<script>
  /**
   * The SQL editor of the console and notebook cells, on CodeMirror.
   *
   * It was Monaco. Monaco is 3.8MB, positions its suggestion list with
   * `position: fixed` (wrong inside any transformed ancestor), and was a second
   * editor next to the CodeMirror one the cell and review docks already use.
   * This builds on that same CodeEditor - one theme, one find panel, one
   * completion engine (cm-sql-complete.js, a port of the Monaco provider) -
   * and adds what a console needs: statement-aware run keys, the lint, running
   * and ran-OK marks in one glyph gutter, the active-statement bar, Format and Vim.
   *
   * Props and exported methods are unchanged, so SqlConsole and SqlCell did not
   * have to change.
   */
  import { onMount } from 'svelte'
  import { StateEffect, StateField, RangeSetBuilder, Prec } from '@codemirror/state'
  import { EditorView, Decoration, ViewPlugin, GutterMarker, gutter, hoverTooltip } from '@codemirror/view'
  import CodeEditor from './CodeEditor.svelte'
  import { AlertCircleIcon, Alert02Icon } from '@hugeicons/core-free-icons'
  import { hugeSvg } from '$lib/cm-huge-icon.js'
  import { formatSql } from '$lib/format-sql.js'
  import { statementAtOffset, lintSql } from '$lib/sql-statements.js'
  import { statementsOf } from '$lib/cm-sql-statements.js'
  import { appVimMode, appSqlEditor, setSqlEditorOption } from '$lib/stores/settings.js'
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

  // ── Glyph gutter: running, ran-OK and lint marks ───────────────────────────

  const SVG_NS = 'http://www.w3.org/2000/svg'
  /** @param {string} tag @param {Record<string, string | number>} attrs */
  function svgEl(tag, attrs) {
    const el = document.createElementNS(SVG_NS, tag)
    for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v))
    return el
  }

  /**
   * Running, ran-OK and failed are one drawing, so the end of a run reads as
   * the mark changing rather than being swapped: the spinning arc closes into a
   * ring and a tick (green) or a cross (red) draws in. Lengths are in
   * pathLength units (100), so the dash math is percentages. The arc starts at
   * 12 o'clock.
   * @param {'running' | 'ok' | 'failed'} kind
   */
  function runGlyph(kind) {
    const svg = svgEl('svg', { viewBox: '0 0 16 16', fill: 'none', 'aria-hidden': 'true' })
    if (kind === 'running') {
      svg.classList.add('animate-spin')
      svg.append(svgEl('circle', { class: 'sql-run-track', cx: 8, cy: 8, r: 6 }))
    }
    svg.append(svgEl('circle', { class: 'sql-run-ring', cx: 8, cy: 8, r: 6, pathLength: 100, transform: 'rotate(-90 8 8)' }))
    if (kind === 'ok') svg.append(svgEl('path', { class: 'sql-run-tick', d: 'M5.4 8.2l1.8 1.8 3.5-3.7', pathLength: 100 }))
    if (kind === 'failed') {
      svg.append(svgEl('path', { class: 'sql-run-tick', d: 'M5.9 5.9l4.2 4.2', pathLength: 100 }))
      svg.append(svgEl('path', { class: 'sql-run-tick', d: 'M10.1 5.9l-4.2 4.2', pathLength: 100 }))
    }
    return svg
  }

  /** How long after a run ends its ✓ still plays the close-and-tick morph. */
  const MORPH_MS = 600

  class GlyphMarker extends GutterMarker {
    /**
     * @param {'running' | 'ok' | 'failed' | 'error' | 'warning'} kind @param {string} title
     * @param {number} [at] when an 'ok' mark was set: it morphs only while fresh,
     *   not each time the gutter redraws the line (scrolled away and back).
     */
    constructor(kind, title, at = 0) {
      super()
      this.kind = kind
      this.title = title
      this.at = at
    }
    /** @param {GlyphMarker} other */
    eq(other) { return other.kind === this.kind && other.title === this.title }
    toDOM() {
      const el = document.createElement('span')
      el.className = `sql-glyph sql-glyph-${this.kind}`
      el.title = this.title
      if (this.kind === 'running' || this.kind === 'ok' || this.kind === 'failed') {
        // `animate-spin` keeps the arc turning under Reduce Motion (app.css),
        // like every "still working" spinner; the morph is cut to its end.
        el.append(runGlyph(this.kind))
        if (this.kind !== 'running' && Date.now() - this.at < MORPH_MS) el.classList.add('sql-glyph-morph')
      } else {
        el.append(hugeSvg(this.kind === 'error' ? AlertCircleIcon : Alert02Icon))
      }
      return el
    }
  }

  /**
   * The statements of the last run: running now, ran OK, or failed, each by its
   * range (the gutter mark sits on its first line), plus the text each failure
   * is underlined at. A ✓ is for the text that ran, so any edit clears it. A
   * running mark follows its statement until the run ends, and a ✗ with its
   * underline stays until the failed statement itself is edited.
   * @typedef {{ from: number, to: number, kind: 'running' | 'ok' | 'failed', title: string }} RunMark
   * @typedef {{ from: number, to: number, stmtFrom: number, stmtTo: number, message: string }} RunError
   * @typedef {{ marks: RunMark[], errors: RunError[], at: number }} RunMarks
   */
  const NO_RUN_MARKS = /** @type {RunMarks} */ ({ marks: [], errors: [], at: 0 })
  const setRunMarks = StateEffect.define()
  const runMarksField = StateField.define({
    create: () => NO_RUN_MARKS,
    update(run, tr) {
      for (const e of tr.effects) if (e.is(setRunMarks)) return /** @type {RunMarks} */ (e.value)
      if (!tr.docChanged || (!run.marks.length && !run.errors.length)) return run
      const ch = tr.changes
      const marks = run.marks
        .filter((m) => m.kind === 'running' || (m.kind === 'failed' && !ch.touchesRange(m.from, m.to)))
        .map((m) => ({ ...m, from: ch.mapPos(m.from, 1), to: ch.mapPos(m.to, -1) }))
      const errors = run.errors
        .filter((e) => !ch.touchesRange(e.stmtFrom, e.stmtTo))
        .map((e) => ({ ...e, from: ch.mapPos(e.from, 1), to: ch.mapPos(e.to, -1), stmtFrom: ch.mapPos(e.stmtFrom, 1), stmtTo: ch.mapPos(e.stmtTo, -1) }))
      return { marks, errors, at: run.at }
    },
    provide: (f) => EditorView.decorations.from(f, (run) =>
      Decoration.set(
        run.errors
          .filter((e) => e.to > e.from)
          .map((e) => Decoration.mark({ class: 'cm-sql-run-error' }).range(e.from, e.to)),
        true,
      ),
    ),
  })

  /** The database's message, on hover over the text it failed at. */
  const runErrorTooltip = hoverTooltip((view, pos) => {
    const err = view.state.field(runMarksField).errors.find((e) => pos >= e.from && pos <= e.to)
    if (!err) return null
    return {
      pos: err.from,
      end: err.to,
      above: true,
      create() {
        const dom = document.createElement('div')
        dom.className = 'cm-sql-run-error-tip'
        dom.textContent = err.message
        return { dom }
      },
    }
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
      const run = view.state.field(runMarksField)
      const doc = view.state.doc
      for (const m of run.marks) {
        byLine.set(doc.lineAt(Math.min(m.from, doc.length)).from, new GlyphMarker(m.kind, m.title, run.at))
      }
      for (const d of view.state.field(lintField).diags) {
        const from = view.state.doc.lineAt(Math.min(d.start, view.state.doc.length)).from
        const prev = byLine.get(from)
        if (prev?.kind === 'error' || prev?.kind === 'running' || prev?.kind === 'failed') continue
        byLine.set(from, new GlyphMarker(d.severity === 'error' ? 'error' : 'warning', d.message))
      }
      const builder = new RangeSetBuilder()
      for (const from of [...byLine.keys()].sort((a, b) => a - b)) builder.add(from, from, /** @type {GlyphMarker} */ (byLine.get(from)))
      return builder.finish()
    },
    initialSpacer: () => new GlyphMarker('ok', ''),
  })

  // ── Active statement: a faint band behind the one under the caret ──────
  // Only when the buffer holds more than one, so a single query stays clean.
  // It was a 2px bar on the gutter's edge, which with line numbers off sat
  // hard against the run marks.

  const activeLineDeco = Decoration.line({ class: 'cm-stmt-active' })
  const activeStatement = StateField.define({
    create: (state) => activeRanges(state),
    update: (v, tr) => (tr.docChanged || tr.selection ? activeRanges(tr.state) : v),
    provide: (f) => EditorView.decorations.from(f),
  })
  /** @param {import('@codemirror/state').EditorState} state */
  function activeRanges(state) {
    const stmts = statementsOf(state)
    const stmt = stmts.length > 1 ? statementAtOffset(stmts, state.selection.main.head) : null
    if (!stmt) return Decoration.none
    const builder = new RangeSetBuilder()
    const last = state.doc.lineAt(Math.min(stmt.end, state.doc.length)).number
    for (let n = state.doc.lineAt(stmt.start).number; n <= last; n++) {
      const from = state.doc.line(n).from
      builder.add(from, from, activeLineDeco)
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
    // Wrap on/off: VS Code's key, the one the cell dock uses too.
    { key: 'Alt-z', run: () => { setSqlEditorOption('wrap', !$appSqlEditor.wrap); return true }, preventDefault: true },
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
    // Room on both sides: the mark never touches the numbers, or the text
    // when the numbers are hidden. In em, like the marks: the editor's text
    // follows the app zoom (--cm-font-size is a type-scale step), and px marks
    // stayed small beside zoomed text.
    '.cm-sql-glyphs .cm-gutterElement': {
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      width: '1.05em',
      padding: '0 0.4em 0 0.55em',
    },
    '.sql-glyph': { display: 'inline-flex', alignItems: 'center', justifyContent: 'center', cursor: 'default' },
    '.sql-glyph svg': { width: '1em', height: '1em' },
    // Running: a blue quarter arc turning on a faint track. Done: a bold tick
    // (or cross) on its own. The ring only exists for the morph, and fades out.
    '.sql-run-track': { stroke: 'color-mix(in oklch, var(--muted-foreground) 22%, transparent)', strokeWidth: '1.75' },
    '.sql-run-ring': {
      stroke: 'var(--primary)',
      strokeWidth: '1.75',
      strokeLinecap: 'round',
      strokeDasharray: '28 100',
    },
    '.sql-glyph-ok .sql-run-ring, .sql-glyph-failed .sql-run-ring': { strokeDasharray: '100 100', opacity: '0' },
    '.sql-glyph-failed .sql-run-tick': { stroke: 'var(--destructive)' },
    '.sql-glyph-failed.sql-glyph-morph .sql-run-ring': { animation: 'cm-sql-ring-fail 360ms cubic-bezier(0.2, 0, 0, 1) both' },
    '@keyframes cm-sql-ring-fail': {
      '0%': { strokeDashoffset: '72', stroke: 'var(--primary)', opacity: '1' },
      '60%': { strokeDashoffset: '0', stroke: 'var(--destructive)', opacity: '0.5' },
      '100%': { strokeDashoffset: '0', stroke: 'var(--destructive)', opacity: '0' },
    },
    // Where the database says a statement failed, the same mark as a lint error.
    '.cm-sql-run-error': {
      textDecoration: 'underline wavy color-mix(in oklch, var(--destructive) 85%, transparent)',
      textUnderlineOffset: '3px',
    },
    '.cm-sql-run-error-tip': {
      maxWidth: '28rem',
      padding: '6px 10px',
      fontFamily: 'var(--font-sans)',
      fontSize: 'var(--fs-2xs)',
      lineHeight: '1.5',
      color: 'var(--foreground)',
      whiteSpace: 'pre-wrap',
    },
    '.sql-run-tick': {
      stroke: 'var(--success)',
      strokeWidth: '2.2',
      strokeLinecap: 'round',
      strokeLinejoin: 'round',
      strokeDasharray: '100',
    },
    // The morph: the arc grows from the spinner's quarter to the whole ring
    // (dash offset 72 → 0 shows 28 → 100), then the tick strokes in.
    '.sql-glyph-morph .sql-run-ring': { animation: 'cm-sql-ring-close 360ms cubic-bezier(0.2, 0, 0, 1) both' },
    '.sql-glyph-morph .sql-run-tick': { animation: 'cm-sql-tick-draw 220ms 170ms cubic-bezier(0.2, 0, 0, 1) both' },
    '@keyframes cm-sql-ring-close': {
      '0%': { strokeDashoffset: '72', stroke: 'var(--primary)', opacity: '1' },
      '60%': { strokeDashoffset: '0', stroke: 'var(--success)', opacity: '0.5' },
      '100%': { strokeDashoffset: '0', stroke: 'var(--success)', opacity: '0' },
    },
    '@keyframes cm-sql-tick-draw': {
      from: { strokeDashoffset: '100' },
      to: { strokeDashoffset: '0' },
    },
    '.sql-glyph-error': { color: 'var(--destructive)' },
    '.sql-glyph-warning': { color: 'var(--warning)' },
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
    // The statement Mod+R would run, when the buffer holds several.
    '.cm-line.cm-stmt-active': { backgroundColor: 'color-mix(in oklch, var(--foreground) 3.5%, transparent)' },
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
    runMarksField,
    runErrorTooltip,
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

  /** Statement text compared across the two splitters (editor's, backend's). @param {string} t */
  const sameText = (t) => t.trim().replace(/;+\s*$/, '').replace(/\s+/g, ' ')

  /**
   * Set the run marks for statement(s): the single statement that ran (⌘R), or
   * null for all of them (run all).
   * @param {'running' | 'ok'} kind @param {string | null} ranStatement
   */
  function markRun(kind, ranStatement) {
    const view = editorRef?.getView()
    if (!view) return
    const target = typeof ranStatement === 'string' ? sameText(ranStatement) : null
    /** @type {RunMark[]} */
    const marks = []
    for (const stmt of statementsOf(view.state)) {
      if (target !== null && sameText(stmt.text) !== target) continue
      marks.push({ from: stmt.start, to: stmt.end, kind, title: kind === 'running' ? 'Running' : 'Ran successfully' })
    }
    view.dispatch({ effects: setRunMarks.of({ marks, errors: [], at: Date.now() }) })
  }

  /**
   * Marks from a finished run: a ✓ or ✗ beside each statement that ran,
   * matched to the editor's statements by text in order, and each failure
   * underlined where the database says it failed.
   * @param {Array<{ sql: string, error?: string | null, position?: number | null }>} outcomes
   */
  export function markOutcomes(outcomes) {
    const view = editorRef?.getView()
    if (!view) return
    const doc = view.state.doc
    const stmts = statementsOf(view.state)
    /** @type {RunMark[]} */
    const marks = []
    /** @type {RunError[]} */
    const errors = []
    let next = 0
    for (const o of outcomes) {
      const want = sameText(o.sql ?? '')
      let i = next
      while (i < stmts.length && sameText(stmts[i].text) !== want) i++
      if (i >= stmts.length) continue
      next = i + 1
      const st = stmts[i]
      if (!o.error) {
        marks.push({ from: st.start, to: st.end, kind: 'ok', title: 'Ran successfully' })
        continue
      }
      const message = o.error.replace(/^Error:\s*/, '').replace(/^(Query|Statement \d+) failed:\s*(error returned from database:\s*)?/i, '')
      marks.push({ from: st.start, to: st.end, kind: 'failed', title: message })
      errors.push({ ...failedRange(doc, st, o.sql ?? '', o.position ?? null), stmtFrom: st.start, stmtTo: st.end, message })
    }
    view.dispatch({ effects: setRunMarks.of({ marks, errors, at: Date.now() }) })
  }

  /**
   * The text to underline for a failure: the token at the database's position
   * (1-based characters into the text it was sent), the last token when it
   * failed at the end of input, the statement's first line when it gave none.
   * @param {import('@codemirror/state').Text} doc @param {{ start: number, end: number }} st
   * @param {string} sent @param {number | null} position
   */
  function failedRange(doc, st, sent, position) {
    const word = (/** @type {number} */ p) => /[\w$"]/.test(doc.sliceString(p, p + 1))
    if (!position) return { from: st.start, to: Math.min(doc.lineAt(st.start).to, st.end) }
    const at = doc.sliceString(st.start, st.end).indexOf(sent.trim())
    const base = st.start + Math.max(0, at)
    let from = base + position - 1
    if (from >= st.end) {
      // "at end of input": point at the last thing that was written.
      let to = st.end
      while (to > st.start && /[\s;]/.test(doc.sliceString(to - 1, to))) to--
      from = to
      while (from > st.start && word(from - 1)) from--
      return { from: Math.min(from, to - 1 < st.start ? st.start : from), to: Math.max(to, from + 1) }
    }
    let to = from
    while (to < st.end && word(to)) to++
    return { from, to: to > from ? to : Math.min(from + 1, st.end) }
  }

  /** Spinner in the glyph gutter while statement(s) run. @param {string | null} [ranStatement] */
  export function markRunning(ranStatement = null) {
    markRun('running', ranStatement)
  }

  /** ✓ in the glyph gutter for statement(s) that ran OK; the next edit clears it. @param {string | null} [ranStatement] */
  export function markExecuted(ranStatement = null) {
    markRun('ok', ranStatement)
  }

  /** Drop the run marks (the run failed or was stopped). */
  export function clearRunMarks() {
    editorRef?.getView()?.dispatch({ effects: setRunMarks.of(NO_RUN_MARKS) })
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
