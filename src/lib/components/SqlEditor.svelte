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
  import { StateEffect, StateField, RangeSetBuilder, Prec, EditorState } from '@codemirror/state'
  import { EditorView, Decoration, ViewPlugin, GutterMarker, WidgetType, gutter, hoverTooltip, keymap, drawSelection, highlightWhitespace } from '@codemirror/view'
  import { indentUnit, foldable, foldEffect, unfoldEffect, foldedRanges, syntaxTree } from '@codemirror/language'
  import { insertNewlineKeepIndent } from '@codemirror/commands'
  import { snippet, completionStatus, hasNextSnippetField, hasPrevSnippetField } from '@codemirror/autocomplete'
  import { wantsTerminator } from '$lib/sql-terminator.js'
  import { IS_MAC } from '$lib/shortcuts.js'
  import CodeEditor from './CodeEditor.svelte'
  import { AlertCircleIcon, Alert02Icon, ArrowDown01Icon, ArrowRight01Icon } from '@hugeicons/core-free-icons'
  import { hugeSvg } from '$lib/cm-huge-icon.js'
  import { formatSql } from '$lib/format-sql.js'
  import { statementAtOffset, lintSql } from '$lib/sql-statements.js'
  import { checkObjectSql } from '$lib/sql-object-check.js'
  import { statementsOf } from '$lib/cm-sql-statements.js'
  import { appVimMode, appSqlEditor, setSqlEditorOption } from '$lib/stores/settings.js'
  import { sqlEditorFontSize } from '$lib/sql-editor-options.js'
  import { extractSqlParams } from '$lib/sql-params.js'
  import { formatRunInfo } from '$lib/sql-run-info.js'
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
    /** Ctrl/Cmd+Shift+S: save as a new query. Unwired, the key goes on to the app. */
    onmodshifts = undefined,
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
    /**
     * A statement action from the row above it (Settings → SQL editor →
     * Statement actions). Unwired, there is no row: a notebook cell has its own
     * run button. Select is handled here.
     * @type {((action: 'run' | 'newtab' | 'json' | 'variables' | 'ai', sql: string) => void) | undefined}
     */
    onlens = undefined,
    /**
     * Revert a run from its statement's lens: the id of the undo copy the run
     * kept (sql-undo.js). Shown only on a statement whose last run kept one.
     * @type {((undoId: string) => void) | undefined}
     */
    onrevert = undefined,
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
   * is underlined at. A ✓ or ✗ stays with its statement through edits elsewhere
   * and goes when that statement itself is edited: it is about the text that
   * ran. A running mark follows its statement until the run ends. `info` is
   * the note written after the statement (`478ms · 12 rows`).
   * `undo` is the run's undo copy, while it can still be reverted.
   * @typedef {{ from: number, to: number, kind: 'running' | 'ok' | 'failed', title: string, info?: string, undo?: { id: string, kind: string } | null }} RunMark
   * @typedef {{ from: number, to: number, stmtFrom: number, stmtTo: number, message: string }} RunError
   * @typedef {{ marks: RunMark[], errors: RunError[], at: number }} RunMarks
   */
  const NO_RUN_MARKS = /** @type {RunMarks} */ ({ marks: [], errors: [], at: 0 })
  const setRunMarks = StateEffect.define()
  /** A run's revert went through: its mark loses the button and says so. */
  const markRevertedEffect = StateEffect.define()
  const runMarksField = StateField.define({
    create: () => NO_RUN_MARKS,
    update(run, tr) {
      for (const e of tr.effects) {
        if (e.is(setRunMarks)) return /** @type {RunMarks} */ (e.value)
        if (e.is(markRevertedEffect)) {
          const id = /** @type {string} */ (e.value)
          run = {
            ...run,
            marks: run.marks.map((m) => (m.undo?.id === id
              ? { ...m, undo: null, title: 'Reverted', info: m.info ? `${m.info} · reverted` : 'reverted' }
              : m)),
          }
        }
      }
      if (!tr.docChanged || (!run.marks.length && !run.errors.length)) return run
      const ch = tr.changes
      const marks = run.marks
        .filter((m) => m.kind === 'running' || !ch.touchesRange(m.from, m.to))
        .map((m) => ({ ...m, from: ch.mapPos(m.from, 1), to: ch.mapPos(m.to, -1) }))
      const errors = run.errors
        .filter((e) => !ch.touchesRange(e.stmtFrom, e.stmtTo))
        .map((e) => ({ ...e, from: ch.mapPos(e.from, 1), to: ch.mapPos(e.to, -1), stmtFrom: ch.mapPos(e.stmtFrom, 1), stmtTo: ch.mapPos(e.stmtTo, -1) }))
      return { marks, errors, at: run.at }
    },
    provide: (f) => EditorView.decorations.from(f, (run) =>
      Decoration.set(
        [
          ...run.errors
            .filter((e) => e.to > e.from)
            .map((e) => Decoration.mark({ class: 'cm-sql-run-error' }).range(e.from, e.to)),
          ...run.marks
            .filter((m) => m.kind === 'ok' && m.info)
            .map((m) => Decoration.widget({ widget: new RunInfoWidget(/** @type {string} */ (m.info)), side: 1 }).range(m.to)),
        ],
        true,
      ),
    ),
  })

  /** After a statement that ran: how long it took and what it returned. */
  class RunInfoWidget extends WidgetType {
    /** @param {string} text */
    constructor(text) {
      super()
      this.text = text
    }
    /** @param {RunInfoWidget} other */
    eq(other) { return other.text === this.text }
    toDOM() {
      const el = document.createElement('span')
      el.className = 'cm-sql-run-info'
      el.textContent = this.text
      el.setAttribute('aria-hidden', 'true')
      return el
    }
  }

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

  /** Tables whose columns the object check asked for, once each. */
  const columnsAsked = new Set()
  /** Load the columns a trigger body's NEW / SET names are checked against;
   *  the new hints lint the editor again. @param {string[]} tables */
  function loadColumnsFor(tables) {
    const fresh = tables.filter((t) => !columnsAsked.has(t))
    if (!fresh.length || !schemaHints.loadColumns) return
    for (const t of fresh) columnsAsked.add(t)
    void schemaHints.loadColumns(fresh).catch(() => {})
  }

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
    const caret = state.selection.main.head
    const caretLine = state.doc.lineAt(caret)
    // The statement being typed, caret at its end, is not missing its `;` yet:
    // the faint `;` after the caret already offers it, and a squiggle and a
    // tooltip over the same words said it twice. Flagged again once the caret
    // leaves the line.
    const typing = (/** @type {import('$lib/sql-statements.js').SqlDiagnostic} */ d) =>
      !!d.fix && d.fix.insert === ';' && d.fix.from === d.end &&
      caret >= d.end && d.end >= caretLine.from && !text.slice(d.end, caret).trim()
    let diags = readOnly || !$appSqlEditor.lint ? [] : lintSql(text).filter((d) => !typing(d))
    if (!readOnly && $appSqlEditor.lint) {
      // A trigger, routine or view naming a table or column the schema lacks:
      // the engine would create it and fail when it runs.
      const objects = checkObjectSql(text, schemaHints, dialect)
      if (objects.diags.length) diags = [...diags, ...objects.diags].sort((a, b) => a.start - b.start)
      loadColumnsFor(objects.missing)
    }
    const deco = Decoration.set(
      diags
        .filter((d) => d.end > d.start)
        .map((d) =>
          Decoration.mark({
            class: d.severity === 'error' ? 'cm-sql-lint-error' : 'cm-sql-lint-warning',
          }).range(Math.min(d.start, text.length), Math.min(d.end, text.length)),
        ),
      true,
    )
    return { diags, deco }
  }

  /**
   * Apply a problem's fix and put the caret after it.
   * @param {EditorView} view @param {import('$lib/sql-statements.js').SqlFix} fix
   */
  function applyFix(view, fix) {
    view.dispatch({
      changes: { from: fix.from, to: fix.to, insert: fix.insert },
      selection: { anchor: fix.from + fix.insert.length },
      userEvent: 'input',
    })
    view.focus()
  }

  /**
   * The problem's message on hover over its squiggle, in the editor's own
   * tooltip, with its fix as a button. The squiggle used to carry a native
   * `title`, which the OS draws in its own style and nothing can restyle.
   */
  const lintTooltip = hoverTooltip((view, pos) => {
    const hits = view.state.field(lintField).diags.filter((d) => pos >= d.start && pos <= d.end)
    if (!hits.length) return null
    return {
      pos: Math.min(...hits.map((d) => d.start)),
      end: Math.max(...hits.map((d) => d.end)),
      // Below the squiggle: above, it covered the line just written.
      above: false,
      create() {
        const dom = document.createElement('div')
        dom.className = 'cm-sql-lint-tip'
        for (const d of hits) {
          const row = document.createElement('div')
          row.className = 'cm-sql-lint-tip-row'
          const dot = document.createElement('span')
          dot.className = d.severity === 'error' ? 'cm-sql-lint-tip-dot is-error' : 'cm-sql-lint-tip-dot'
          dot.setAttribute('aria-hidden', 'true')
          const msg = document.createElement('span')
          msg.className = 'cm-sql-lint-tip-msg'
          msg.textContent = d.message
          row.append(dot, msg)
          const fix = d.fix
          if (fix && !readOnly) {
            const b = document.createElement('button')
            b.type = 'button'
            b.className = 'cm-sql-lint-tip-fix'
            b.textContent = fix.label
            b.title = `${fix.label} (${IS_MAC ? '⌘.' : 'Ctrl+.'})`
            b.addEventListener('mousedown', (e) => e.preventDefault())
            b.addEventListener('click', () => applyFix(view, fix))
            row.append(b)
          }
          dom.append(row)
        }
        return { dom }
      },
    }
  }, { hideOnChange: true })

  /** Ctrl/⌘+. : the fix of the problem at the caret, as in VS Code. */
  const quickFixKeys = Prec.high(keymap.of([{
    key: 'Mod-.',
    run: (view) => {
      if (readOnly) return false
      const pos = view.state.selection.main.head
      const d = view.state.field(lintField).diags.find((x) => x.fix && pos >= x.start && pos <= x.end)
      if (!d?.fix) return false
      applyFix(view, d.fix)
      return true
    },
  }]))

  // Lint 350ms after the last keystroke, the delay the Monaco editor used.
  const lintRunner = ViewPlugin.fromClass(
    class {
      /** @param {EditorView} view */
      constructor(view) {
        this.view = view
        this.live = true
        /** @type {ReturnType<typeof setTimeout> | null} */
        this.timer = null
        this.line = view.state.doc.lineAt(view.state.selection.main.head).number
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
        // Also when the caret moves to another line: the statement it left
        // may now owe its `;` (see `typing` in lintFor). Not on every move
        // along a line, so a long script is not re-linted per keypress.
        const line = u.state.doc.lineAt(u.state.selection.main.head).number
        const movedLine = u.selectionSet && line !== this.line
        this.line = line
        if (!u.docChanged && !movedLine) return
        if (this.timer) clearTimeout(this.timer)
        this.timer = setTimeout(() => this.run(), u.docChanged ? 350 : 120)
      }
      destroy() {
        this.live = false
        if (this.timer) clearTimeout(this.timer)
      }
    },
  )

  /**
   * Run and lint marks by line start. Rebuilt only when either changes: the
   * gutter asks once per visible line.
   */
  let glyphMemo = { run: /** @type {RunMarks | null} */ (null), lint: /** @type {unknown} */ (null), map: new Map() }
  /** @param {import('@codemirror/state').EditorState} state @returns {Map<number, GlyphMarker>} */
  function glyphsOf(state) {
    const run = state.field(runMarksField)
    const lint = state.field(lintField)
    if (glyphMemo.run === run && glyphMemo.lint === lint) return glyphMemo.map
    /** @type {Map<number, GlyphMarker>} */
    const map = new Map()
    const doc = state.doc
    for (const m of run.marks) map.set(doc.lineAt(Math.min(m.from, doc.length)).from, new GlyphMarker(m.kind, m.title, run.at))
    for (const d of lint.diags) {
      const from = doc.lineAt(Math.min(d.start, doc.length)).from
      const prev = map.get(from)
      if (prev?.kind === 'error' || prev?.kind === 'running' || prev?.kind === 'failed') continue
      map.set(from, new GlyphMarker(d.severity === 'error' ? 'error' : 'warning', d.message))
    }
    glyphMemo = { run, lint, map }
    return map
  }

  /** The fold sitting on `line`, if it is folded. @param {import('@codemirror/state').EditorState} state @param {{ from: number, to: number }} line */
  function foldOn(state, line) {
    /** @type {{ from: number, to: number } | null} */
    let found = null
    foldedRanges(state).between(line.from, line.to, (from, to) => { if (!found || found.from > from) found = { from, to } })
    return found
  }

  /**
   * One cell per line for the run mark and the fold arrow, instead of a column
   * each: they took a third of the gutter between them, mostly blank. The mark
   * shows; while the pointer is over the gutter a foldable line shows its arrow
   * in its place, and a folded statement keeps its arrow (it is the only sign
   * the text is there).
   */
  class StatusMarker extends GutterMarker {
    /** @param {GlyphMarker | null} glyph @param {'open' | 'closed' | null} fold */
    constructor(glyph, fold) {
      super()
      this.glyph = glyph
      this.fold = fold
    }
    /** @param {StatusMarker} other */
    eq(other) {
      if (other.fold !== this.fold) return false
      if (!other.glyph || !this.glyph) return other.glyph === this.glyph
      return other.glyph.eq(this.glyph)
    }
    toDOM() {
      const el = document.createElement('span')
      el.className = this.fold ? `sql-cell sql-cell-fold sql-cell-${this.fold}` : 'sql-cell'
      if (this.glyph) el.append(this.glyph.toDOM())
      if (this.fold) {
        const arrow = document.createElement('span')
        arrow.className = 'cm-fold-marker'
        arrow.append(hugeSvg(this.fold === 'open' ? ArrowDown01Icon : ArrowRight01Icon))
        el.append(arrow)
        el.title = [this.glyph?.title, this.fold === 'open' ? 'Fold' : 'Unfold'].filter(Boolean).join('\n')
      }
      return el
    }
  }

  const statusGutter = gutter({
    class: 'cm-sql-glyphs',
    lineMarker(view, line) {
      const glyph = glyphsOf(view.state).get(line.from) ?? null
      /** @type {'open' | 'closed' | null} */
      let fold = null
      if (view.state.field(configField).fold) {
        if (foldOn(view.state, line)) fold = 'closed'
        else if (foldable(view.state, line.from, line.to)) fold = 'open'
      }
      return glyph || fold ? new StatusMarker(glyph, fold) : null
    },
    lineMarkerChange: (u) => u.docChanged || u.viewportChanged
      || u.transactions.some((tr) => tr.effects.some((e) => e.is(setRunMarks) || e.is(markRevertedEffect) || e.is(foldEffect) || e.is(unfoldEffect) || e.is(setConfig)))
      || u.startState.field(lintField) !== u.state.field(lintField)
      || syntaxTree(u.startState) !== syntaxTree(u.state),
    initialSpacer: () => new StatusMarker(new GlyphMarker('ok', ''), null),
    domEventHandlers: {
      click(view, line) {
        if (!view.state.field(configField).fold) return false
        const folded = foldOn(view.state, line)
        if (folded) {
          view.dispatch({ effects: unfoldEffect.of(folded) })
          return true
        }
        const range = foldable(view.state, line.from, line.to)
        if (!range) return false
        view.dispatch({ effects: foldEffect.of(range) })
        return true
      },
    },
  })

  // ── Active statement: a faint band behind the one under the caret ──────
  // Only when the buffer holds more than one, so a single query stays clean.
  // It was a 2px bar on the gutter's edge, which with line numbers off sat
  // hard against the run marks.

  // ── Settings the editor's own fields read ──────────────────────────────
  // Pushed in as an effect when they change, so the fields below recompute
  // without the editor being rebuilt.

  /** @typedef {{ lens: 'off' | 'current' | 'all', highlight: boolean, variables: boolean, endHint: boolean, fold: boolean }} EditorConfig */
  const setConfig = StateEffect.define()
  /** @returns {EditorConfig} */
  function currentConfig() {
    return {
      lens: onlens && !readOnly ? $appSqlEditor.codeLens : 'off',
      highlight: $appSqlEditor.highlightBlock,
      variables: $appSqlEditor.variables,
      endHint: $appSqlEditor.endHint && !readOnly,
      // Beside the numbers only: a fold arrow with no numbers is a mark nothing explains.
      fold: $appSqlEditor.lineNumbers && $appSqlEditor.folding,
    }
  }
  const configField = StateField.define({
    create: () => currentConfig(),
    update(v, tr) {
      for (const e of tr.effects) if (e.is(setConfig)) return /** @type {EditorConfig} */ (e.value)
      return v
    },
  })
  $effect(() => {
    const next = currentConfig()
    editorRef?.getView()?.dispatch({ effects: setConfig.of(next) })
  })
  /** @param {import('@codemirror/state').Transaction} tr */
  const configChanged = (tr) => tr.effects.some((e) => e.is(setConfig))

  const activeLineDeco = Decoration.line({ class: 'cm-stmt-active' })
  const activeStatement = StateField.define({
    create: (state) => activeRanges(state),
    update: (v, tr) => (tr.docChanged || tr.selection || configChanged(tr) ? activeRanges(tr.state) : v),
    provide: (f) => EditorView.decorations.from(f),
  })
  /** @param {import('@codemirror/state').EditorState} state */
  function activeRanges(state) {
    if (!state.field(configField).highlight) return Decoration.none
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

  // ── Statement actions: a row of text buttons above a statement ──────────
  // A block widget at the start of the statement's first line, above the
  // statement under the caret (or every statement). It changes only when the
  // caret moves to another statement, never while typing in one: its widget
  // compares equal and CodeMirror keeps the same DOM.

  /** Whether a statement has variables, by its text: one scan per text. */
  const varsByText = new Map()
  /** @param {string} text */
  function hasVariables(text) {
    let v = varsByText.get(text)
    if (v === undefined) {
      v = extractSqlParams(text, { engine: dialect }).length > 0
      if (varsByText.size > 500) varsByText.clear()
      varsByText.set(text, v)
    }
    return v
  }

  /**
   * The statement a row on `line` acts for: the one under the caret when it
   * starts on that line, else the first that does.
   * @param {import('@codemirror/state').EditorState} state @param {{ from: number, to: number }} line
   */
  function lensTarget(state, line) {
    const stmts = statementsOf(state)
    const caret = statementAtOffset(stmts, state.selection.main.head)
    if (caret && caret.start >= line.from && caret.start <= line.to) return caret
    return stmts.find((s) => s.start >= line.from && s.start <= line.to) ?? null
  }

  const LENS_ACTIONS = /** @type {const} */ ([
    { id: 'run', label: 'Run', title: 'Run this statement (Ctrl+R)' },
    { id: 'select', label: 'Select', title: 'Select this statement (Ctrl+L)' },
    { id: 'newtab', label: 'New tab', title: 'Run this statement in a new editor tab' },
    { id: 'json', label: 'JSON', title: 'Run this statement and show the result as JSON' },
    { id: 'variables', label: 'Variables', title: 'Set the values of this statement\'s variables' },
    { id: 'ai', label: 'Ask AI', title: 'Ask the AI chat about this statement' },
  ])

  class LensWidget extends WidgetType {
    /**
     * @param {boolean} vars
     * @param {boolean} [float] pinned over the right end of the statement's
     *   first line instead of a row of its own (the caret mode)
     */
    constructor(vars, float = false, undo = '') {
      super()
      this.vars = vars
      this.float = float
      /** The statement's last run kept an undo copy: offer Revert. */
      this.undo = undo
    }
    /** @param {LensWidget} other */
    eq(other) { return other.vars === this.vars && other.float === this.float && other.undo === this.undo }
    /** @param {EditorView} view */
    toDOM(view) {
      if (this.float) return this.floatDOM(view)
      const row = document.createElement('div')
      row.className = 'cm-sql-lens'
      row.setAttribute('role', 'toolbar')
      row.setAttribute('aria-label', 'Statement actions')
      if (this.undo) {
        const b = document.createElement('button')
        b.type = 'button'
        b.className = 'cm-sql-lens-revert'
        b.textContent = 'Revert'
        b.title = 'Put back what the last run of this statement changed'
        const undo = this.undo
        b.addEventListener('mousedown', (e) => e.preventDefault())
        b.addEventListener('click', () => onrevert?.(undo))
        row.append(b)
      }
      for (const a of LENS_ACTIONS) {
        if (a.id === 'variables' && !this.vars) continue
        if (row.childElementCount) {
          const sep = document.createElement('span')
          sep.className = 'cm-sql-lens-sep'
          sep.setAttribute('aria-hidden', 'true')
          row.append(sep)
        }
        const b = document.createElement('button')
        b.type = 'button'
        b.textContent = a.label
        b.title = a.title
        // The caret and the focus stay where they are.
        b.addEventListener('mousedown', (e) => e.preventDefault())
        b.addEventListener('click', () => lensAction(view, row, a.id))
        row.append(b)
      }
      return row
    }

    /**
     * The caret mode: `▶ Run ⋯` hanging from a zero-height block above the
     * line, so it shares no position with the text (the caret and the
     * completion list still measure the text) and adds no height. Compact,
     * with the rest behind ⋯: the full row of words covered the end of a long
     * first line. Lines keep a right margin as wide as this (lensRoom), so a
     * long one wraps short of it rather than running underneath.
     * @param {EditorView} view
     */
    floatDOM(view) {
      const anchor = document.createElement('div')
      anchor.className = 'cm-sql-lens-anchor'
      const chip = document.createElement('span')
      chip.className = 'cm-sql-lens cm-sql-lens-float'
      chip.setAttribute('role', 'toolbar')
      chip.setAttribute('aria-label', 'Statement actions')
      anchor.append(chip)

      const button = (/** @type {string} */ label, /** @type {string} */ title, /** @type {() => void} */ onclick, cls = '') => {
        const b = document.createElement('button')
        b.type = 'button'
        b.className = cls
        b.innerHTML = label
        b.title = title
        b.addEventListener('mousedown', (e) => e.preventDefault())
        b.addEventListener('click', (e) => { e.stopPropagation(); onclick() })
        return b
      }
      if (this.undo) {
        const undo = this.undo
        chip.append(button(
          '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3.5 2.5 7 6 10.5M3 7h6.5a3.5 3.5 0 0 1 0 7H8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>Revert',
          'Put back what the last run of this statement changed',
          () => onrevert?.(undo),
          'cm-sql-lens-revert',
        ))
        const gap = document.createElement('span')
        gap.className = 'cm-sql-lens-sep'
        gap.setAttribute('aria-hidden', 'true')
        chip.append(gap)
      }
      chip.append(button(
        '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3.5v9l7-4.5z" fill="currentColor"/></svg>Run',
        'Run this statement (Ctrl+R)',
        () => lensAction(view, chip, 'run'),
        'cm-sql-lens-run',
      ))
      const sep = document.createElement('span')
      sep.className = 'cm-sql-lens-sep'
      sep.setAttribute('aria-hidden', 'true')
      chip.append(sep)

      /** @type {HTMLElement | null} */
      let menu = null
      const close = () => {
        menu?.remove()
        menu = null
        more.setAttribute('aria-expanded', 'false')
        document.removeEventListener('mousedown', outside, true)
        document.removeEventListener('keydown', onKey, true)
      }
      const outside = (/** @type {MouseEvent} */ e) => { if (!anchor.contains(/** @type {Node} */ (e.target))) close() }
      const onKey = (/** @type {KeyboardEvent} */ e) => { if (e.key === 'Escape') { e.stopPropagation(); close(); view.focus() } }
      const more = button(
        '<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="3.5" cy="8" r="1.3" fill="currentColor"/><circle cx="8" cy="8" r="1.3" fill="currentColor"/><circle cx="12.5" cy="8" r="1.3" fill="currentColor"/></svg>',
        'More actions',
        () => {
          if (menu) { close(); return }
          menu = document.createElement('div')
          menu.className = 'cm-sql-lens-menu'
          menu.setAttribute('role', 'menu')
          for (const a of LENS_ACTIONS) {
            if (a.id === 'run' || (a.id === 'variables' && !this.vars)) continue
            const item = button(a.label, a.title, () => { close(); lensAction(view, chip, a.id) })
            item.setAttribute('role', 'menuitem')
            menu.append(item)
          }
          anchor.append(menu)
          more.setAttribute('aria-expanded', 'true')
          document.addEventListener('mousedown', outside, true)
          document.addEventListener('keydown', onKey, true)
        },
        'cm-sql-lens-more',
      )
      more.setAttribute('aria-haspopup', 'menu')
      more.setAttribute('aria-expanded', 'false')
      chip.append(more)
      return anchor
    }

    /** @param {HTMLElement} dom */
    destroy(dom) {
      // A menu left open when the chip moves to another statement goes with it.
      dom.querySelector('.cm-sql-lens-menu')?.remove()
    }
  }

  /**
   * @param {EditorView} view @param {HTMLElement} row
   * @param {'run' | 'select' | 'newtab' | 'json' | 'variables' | 'ai'} action
   */
  function lensAction(view, row, action) {
    const pos = view.posAtDOM(row)
    const st = lensTarget(view.state, view.state.doc.lineAt(Math.min(pos, view.state.doc.length)))
    if (!st) return
    if (action === 'select') {
      view.dispatch({ selection: { anchor: st.start, head: st.end }, scrollIntoView: true })
      view.focus()
      return
    }
    onlens?.(action, st.text)
  }

  /** @param {import('@codemirror/state').EditorState} state */
  function lensRanges(state) {
    const { lens, variables } = state.field(configField)
    if (lens === 'off') return Decoration.none
    const stmts = statementsOf(state)
    if (!stmts.length) return Decoration.none
    const doc = state.doc
    const caret = statementAtOffset(stmts, state.selection.main.head)
    const builder = new RangeSetBuilder()
    const runMarks = onrevert ? (state.field(runMarksField, false)?.marks ?? []) : []
    /** The undo copy of the statement's last run, while it has one. @param {{ start: number, end: number }} st */
    const undoOf = (st) => runMarks.find((m) => m.kind === 'ok' && m.undo && m.from < st.end && m.to > st.start)?.undo?.id ?? ''
    /** @param {{ from: number }} line @param {{ text: string, start: number, end: number }} st */
    const add = (line, st) => builder.add(line.from, line.from, Decoration.widget({
      widget: new LensWidget(variables && hasVariables(st.text), false, undoOf(st)),
      block: true,
      side: -1,
    }))
    if (lens === 'current') {
      // Over the right end of the statement's first line, taking no space: a
      // row of its own moved the whole text up and down each time the caret
      // went to another statement. Not an inline widget at the line's end:
      // there it sat where the caret does, and CodeMirror measured the caret
      // (and placed the completion list) at the chip on the far right.
      if (caret) {
        const line = doc.lineAt(caret.start)
        builder.add(line.from, line.from, Decoration.widget({ widget: new LensWidget(variables && hasVariables(caret.text), true, undoOf(caret)), block: true, side: -1 }))
      }
      return builder.finish()
    }
    const caretLine = caret ? doc.lineAt(caret.start).number : -1
    let last = -1
    for (const st of stmts) {
      const line = doc.lineAt(st.start)
      // Two statements on one line share its row, and it acts for the one
      // under the caret when that is one of them (lensTarget, on click).
      if (line.number === last) continue
      last = line.number
      add(line, line.number === caretLine && caret ? caret : st)
    }
    return builder.finish()
  }

  const lensField = StateField.define({
    create: (state) => lensRanges(state),
    update: (v, tr) => (tr.docChanged || tr.selection || configChanged(tr) || tr.effects.some((e) => e.is(setRunMarks) || e.is(markRevertedEffect))
      ? lensRanges(tr.state)
      : v),
    provide: (f) => EditorView.decorations.from(f),
  })

  // ── The closing `;` ────────────────────────────────────────────────────────
  // A faint `;` after the caret when the statement ending there reads finished
  // (sql-terminator.js); Tab writes it. Not while the completion list is open
  // or a snippet still has fields to visit: Tab belongs to those.

  class SemicolonHint extends WidgetType {
    eq() { return true }
    toDOM() {
      const el = document.createElement('span')
      el.className = 'cm-semi-hint'
      el.setAttribute('aria-hidden', 'true')
      el.textContent = ';'
      const key = document.createElement('span')
      key.className = 'cm-semi-hint-key'
      key.textContent = 'Tab'
      el.append(key)
      return el
    }
    ignoreEvent() { return false }
  }
  const semicolonHint = Decoration.widget({ widget: new SemicolonHint(), side: 1 })

  /**
   * Where the `;` would go, or -1: the caret, alone, at the end of a line that
   * ends a statement which looks finished and has none.
   * @param {import('@codemirror/state').EditorState} state
   */
  function semicolonAt(state) {
    if (!state.field(configField).endHint) return -1
    const sel = state.selection.main
    if (!sel.empty || state.selection.ranges.length > 1) return -1
    if (completionStatus(state) || hasNextSnippetField(state) || hasPrevSnippetField(state)) return -1
    const pos = sel.head
    const line = state.doc.lineAt(pos)
    if (state.doc.sliceString(pos, line.to).trim()) return -1
    const stmt = statementAtOffset(statementsOf(state), pos)
    // The statement has to end here: more of it on a later line means it is
    // still going. And on this line: a new blank line after a statement is
    // where the next one starts, not where this one's `;` belongs.
    if (!stmt || stmt.end > pos || stmt.start > pos) return -1
    if (state.doc.lineAt(Math.max(stmt.start, stmt.end - 1)).number !== line.number) return -1
    return wantsTerminator(state.doc.sliceString(stmt.start, pos)) ? pos : -1
  }

  const semicolonField = StateField.define({
    create: () => Decoration.none,
    update(_v, tr) {
      const at = semicolonAt(tr.state)
      return at < 0 ? Decoration.none : Decoration.set([semicolonHint.range(at)])
    },
    provide: (f) => EditorView.decorations.from(f),
  })

  const semicolonKeys = Prec.high(keymap.of([{
    key: 'Tab',
    run: (view) => {
      const at = semicolonAt(view.state)
      if (at < 0) return false
      view.dispatch({ changes: { from: at, insert: ';' }, selection: { anchor: at + 1 }, userEvent: 'input.type' })
      return true
    },
  }]))

  // ── Editing basics (Settings → SQL editor) ─────────────────────────────────
  // VS Code's behaviour by default. Rebuilt when a setting changes; the editor
  // keeps its document and undo history across the swap.

  /**
   * The caret mode's chip floats over the right end of a line. Every line
   * keeps that much room on its right, the same on every line, so a long one
   * wraps short of the chip and moving the caret never re-wraps anything.
   */
  const lensRoom = EditorView.theme({ '.cm-content .cm-line': { paddingRight: '6.5em' } })

  /** Theme bits that only exist to switch something off. */
  const noActiveLine = EditorView.theme({ '.cm-activeLine': { backgroundColor: 'transparent' } })
  const noAutoClose = EditorState.languageData.of(() => [{ closeBrackets: { brackets: [] } }])

  /** @param {import('$lib/sql-editor-options.js').SqlEditorOptions} o */
  function editingExtensions(o) {
    return [
      // Ahead of the editor's own tab size (2) and indent unit.
      Prec.high(EditorState.tabSize.of(o.tabSize)),
      Prec.high(indentUnit.of(o.indentTabs ? '\t' : ' '.repeat(o.tabSize))),
      // Enter and Shift+Enter break the line the same way. Kept indentation is
      // VS Code's: the SQL grammar's continuation indent pushed the line after
      // a finished statement in by a level, which read as a stray indent.
      ...(o.smartIndent ? [] : [Prec.high(keymap.of([{ key: 'Enter', run: insertNewlineKeepIndent, shift: insertNewlineKeepIndent }]))]),
      ...(o.autoClose ? [] : [Prec.high(noAutoClose)]),
      ...(o.activeLine ? [] : [noActiveLine]),
      ...(o.whitespace ? [highlightWhitespace()] : []),
      // drawSelection takes the lowest blink rate it is given: 0 holds it still.
      ...(o.cursorBlink ? [] : [drawSelection({ cursorBlinkRate: 0 })]),
    ]
  }

  // ── Keys ───────────────────────────────────────────────────────────────────
  // A handler that is not wired returns false, so the key falls through to the
  // app's global hotkeys instead of being swallowed here.

  /** @param {(() => void) | undefined} fn */
  const call = (fn) => () => { if (!fn) return false; fn(); return true }

  const keys = [
    { key: 'Mod-k', run: () => call(onmodk)() },
    // No preventDefault: unwired (a notebook cell), Ctrl+S has to reach the
    // notebook's own save; a wired handler returns true and claims it anyway.
    { key: 'Mod-s', run: () => call(onmods)() },
    { key: 'Mod-Shift-s', run: () => call(onmodshifts)() },
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
    // The gutter is two columns: the run mark (which the fold arrow shares)
    // and the line numbers, then g = 0.5em to the text. The mark sits 0.45em
    // from the edge and 0.3em from the numbers: tight, because a third column
    // and a full g either side of the mark made the gutter ~100px at 125% zoom.
    // In em throughout: the text follows the app zoom (--cm-font-size is a
    // type-scale step), so a px gap drifted against it.
    '.cm-sql-glyphs .cm-gutterElement': {
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      boxSizing: 'content-box',
      width: '1em',
      padding: '0 0.3em 0 0.45em',
    },
    // Mark and fold arrow share the cell: stacked, and swapped while the
    // pointer is over the gutter (a folded line keeps its arrow).
    '.sql-cell': { position: 'relative', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', width: '1em', height: '1em' },
    '.sql-cell-fold': { cursor: 'pointer' },
    '.sql-cell .cm-fold-marker': {
      position: 'absolute',
      inset: '0',
      alignItems: 'center',
      justifyContent: 'center',
      opacity: '0',
      transition: 'opacity 120ms',
    },
    '.sql-cell .sql-glyph': { transition: 'opacity 120ms' },
    '.cm-gutters:hover .sql-cell-fold .cm-fold-marker, .sql-cell-closed .cm-fold-marker': { opacity: '1' },
    '.cm-gutters:hover .sql-cell-fold .sql-glyph, .sql-cell-closed .sql-glyph': { opacity: '0' },
    '.cm-content .cm-line': { paddingLeft: '0.5em' },
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
    // A problem on hover: its message, and its fix as a button, a little
    // clear of the line it is about.
    '.cm-tooltip-hover:has(> .cm-sql-lint-tip)': { marginTop: '4px' },
    '.cm-sql-lint-tip': { display: 'flex', flexDirection: 'column', gap: '2px', padding: '4px', maxWidth: '30rem' },
    '.cm-sql-lint-tip-row': {
      display: 'flex',
      alignItems: 'center',
      gap: '8px',
      padding: '3px 4px 3px 6px',
      fontFamily: 'var(--font-sans)',
      fontSize: 'var(--fs-2xs)',
      lineHeight: '1.45',
      color: 'var(--foreground)',
    },
    '.cm-sql-lint-tip-dot': { flex: 'none', width: '6px', height: '6px', borderRadius: '999px', backgroundColor: 'var(--warning)' },
    '.cm-sql-lint-tip-dot.is-error': { backgroundColor: 'var(--destructive)' },
    '.cm-sql-lint-tip-msg': { flex: '1', minWidth: '0' },
    '.cm-sql-lint-tip-fix': {
      flex: 'none',
      height: '1.75em',
      padding: '0 8px',
      border: '1px solid color-mix(in oklch, var(--border) 90%, transparent)',
      borderRadius: '6px',
      background: 'color-mix(in oklch, var(--foreground) 6%, transparent)',
      color: 'var(--foreground)',
      font: 'inherit',
      fontWeight: '500',
      cursor: 'pointer',
    },
    '.cm-sql-lint-tip-fix:hover': { background: 'color-mix(in oklch, var(--foreground) 12%, transparent)' },
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
    // Two digits reserved, not the cell dock's five: most consoles stay under
    // line 100, and a reserve the numbers never use is just a blank strip. A
    // longer script widens the column once, at line 100.
    '.cm-gutters .cm-lineNumbers .cm-gutterElement': {
      minWidth: 'calc(2ch + 0.5em)',
      padding: '0 0.5em 0 0',
    },
    '.cm-gutters .cm-fold-marker svg': { width: '0.85em', height: '0.85em' },
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
    // Statement actions: dense chrome in the sans face, muted until pointed
    // at. The first label lines up with the statement's text: a line's left
    // padding (g, in the editor's em) less a button's own 6px.
    '.cm-sql-lens': {
      display: 'flex',
      alignItems: 'center',
      height: '1.7em',
      paddingLeft: 'max(0px, calc(var(--cm-font-size, 13px) * 0.5 - 6px))',
      fontFamily: 'var(--font-sans)',
      fontSize: 'var(--fs-2xs)',
      lineHeight: '1',
      color: 'var(--muted-foreground)',
      userSelect: 'none',
    },
    '.cm-sql-lens button': {
      height: '1.45em',
      padding: '0 6px',
      border: 'none',
      borderRadius: '4px',
      background: 'none',
      color: 'inherit',
      font: 'inherit',
      cursor: 'pointer',
    },
    '.cm-sql-lens button:hover': {
      color: 'var(--foreground)',
      backgroundColor: 'color-mix(in oklch, var(--foreground) 7%, transparent)',
    },
    '.cm-sql-lens-sep': { width: '1px', height: '0.9em', backgroundColor: 'var(--border)' },
    // The suggested `;`: ghost text, with the key that writes it.
    '.cm-semi-hint': { color: 'var(--muted-foreground)', opacity: '0.7', pointerEvents: 'none' },
    '.cm-semi-hint-key': {
      marginLeft: '1ch',
      padding: '0 4px',
      borderRadius: '3px',
      border: '1px solid color-mix(in oklch, var(--border) 80%, transparent)',
      fontFamily: 'var(--font-sans)',
      fontSize: 'var(--fs-3xs)',
      verticalAlign: '1px',
    },
    // The caret mode's lens floats: absolutely placed in its line, so it adds
    // no height and no width, and a long first line runs under it rather than
    // being pushed. A solid chip, because the editor is transparent over
    // whatever surface holds it and a fade could not match every one.
    '.cm-sql-lens-anchor': { position: 'relative', height: '0', overflow: 'visible' },
    '.cm-sql-lens-float button': { display: 'inline-flex', alignItems: 'center', gap: '3px' },
    '.cm-sql-lens-float svg': { width: '0.95em', height: '0.95em', flex: 'none' },
    '.cm-sql-lens-run': { color: 'var(--foreground) !important' },
    '.cm-sql-lens-run svg': { color: 'var(--success)' },
    '.cm-sql-lens-revert': { color: 'var(--foreground) !important' },
    '.cm-sql-lens-revert svg': { color: 'var(--warning)' },
    '.cm-sql-lens-more': { padding: '0 4px !important' },
    '.cm-sql-lens-menu': {
      position: 'absolute',
      top: '1.75em',
      right: '6px',
      zIndex: '5',
      display: 'flex',
      flexDirection: 'column',
      minWidth: '9rem',
      padding: '4px',
      borderRadius: '8px',
      border: '1px solid color-mix(in oklch, var(--border) 80%, transparent)',
      backgroundColor: 'var(--popover)',
      boxShadow: '0 8px 24px rgba(0, 0, 0, 0.35), 0 1px 3px rgba(0, 0, 0, 0.2)',
      fontFamily: 'var(--font-sans)',
      fontSize: 'var(--fs-2xs)',
    },
    '.cm-sql-lens-menu button': {
      height: '1.9em',
      padding: '0 8px',
      border: 'none',
      borderRadius: '5px',
      background: 'none',
      color: 'var(--foreground)',
      font: 'inherit',
      textAlign: 'left',
      cursor: 'pointer',
    },
    '.cm-sql-lens-menu button:hover': { backgroundColor: 'color-mix(in oklch, var(--foreground) 8%, transparent)' },
    '.cm-sql-lens-float': {
      position: 'absolute',
      top: '0.1em',
      right: '6px',
      height: '1.5em',
      padding: '0 2px',
      borderRadius: '6px',
      border: '1px solid color-mix(in oklch, var(--border) 80%, transparent)',
      backgroundColor: 'var(--popover)',
      zIndex: '1',
    },
    // After a statement that ran: its time and what came back.
    '.cm-sql-run-info': {
      marginLeft: '1.5ch',
      fontFamily: 'var(--font-sans)',
      fontSize: 'var(--fs-2xs)',
      color: 'var(--success)',
      userSelect: 'none',
      pointerEvents: 'none',
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
    configField,
    Prec.high(statusGutter),
    runMarksField,
    runErrorTooltip,
    lintField,
    lintRunner,
    lintTooltip,
    quickFixKeys,
    activeStatement,
    lensField,
    semicolonField,
    semicolonKeys,
    consoleTheme,
  ]

  /** Loaded while Vim mode is on (lazily - it is only for the few who use it). */
  let vimExtension = $state(/** @type {import('@codemirror/state').Extension | null} */ (null))
  const editing = $derived([
    ...editingExtensions($appSqlEditor),
    ...(onlens && !readOnly && $appSqlEditor.codeLens === 'current' ? [lensRoom] : []),
  ])
  const extensions = $derived([...(vimExtension ? [Prec.highest(vimExtension)] : []), ...baseExtensions, ...editing])

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

  // Problem markers switched on or off in Settings, or the schema the object
  // check reads changed: lint again (or clear) now, not on the next keystroke.
  $effect(() => {
    void $appSqlEditor.lint
    // New schema hints (a table list, a table's columns): the object check
    // reads them.
    void schemaHints
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
   * @param {string} [info] the note after a statement that ran OK
   */
  function markRun(kind, ranStatement, info = '') {
    const view = editorRef?.getView()
    if (!view) return
    const target = typeof ranStatement === 'string' ? sameText(ranStatement) : null
    /** @type {RunMark[]} */
    const marks = []
    for (const stmt of statementsOf(view.state)) {
      if (target !== null && sameText(stmt.text) !== target) continue
      marks.push({ from: stmt.start, to: stmt.end, kind, title: kind === 'running' ? 'Running' : 'Ran successfully', info: kind === 'ok' ? info : '' })
    }
    view.dispatch({ effects: setRunMarks.of({ marks, errors: [], at: Date.now() }) })
  }

  /**
   * Marks from a finished run: a ✓ or ✗ beside each statement that ran,
   * matched to the editor's statements by text in order, and each failure
   * underlined where the database says it failed. `sent` is the text that
   * went to the database when it differs from the editor's (variables filled
   * in, a LIMIT added), for placing the failure; `ms` and `rows` or `affected`
   * make the note after a statement that ran.
   * `undo` is the undo copy a write kept, for the lens's Revert; `undoNote`
   * says why one that could have kept a copy did not.
   * @param {Array<{ sql: string, sent?: string, error?: string | null, position?: number | null, ms?: number | null, rows?: number | null, affected?: number | null, undo?: { id: string, kind: string } | null, undoNote?: string }>} outcomes
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
        const info = formatRunInfo({ ms: o.ms, rows: o.rows, affected: o.affected })
        const title = (info ? `Ran successfully · ${info}` : 'Ran successfully') + (o.undoNote ? `\nNo revert: ${o.undoNote}` : '')
        marks.push({ from: st.start, to: st.end, kind: 'ok', title, info, undo: o.undo ?? null })
        continue
      }
      const message = o.error.replace(/^Error:\s*/, '').replace(/^(Query|Statement \d+) failed:\s*(error returned from database:\s*)?/i, '')
      marks.push({ from: st.start, to: st.end, kind: 'failed', title: message })
      errors.push({ ...failedRange(doc, st, o.sent ?? o.sql ?? '', o.position ?? null), stmtFrom: st.start, stmtTo: st.end, message })
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

  /**
   * ✓ in the glyph gutter for statement(s) that ran OK, with the note after
   * them; an edit to the statement clears it.
   * @param {string | null} [ranStatement] @param {{ ms?: number | null, rows?: number | null, affected?: number | null }} [run]
   */
  export function markExecuted(ranStatement = null, run = {}) {
    markRun('ok', ranStatement, formatRunInfo(run))
  }

  /** The run whose undo copy this is was reverted. @param {string} undoId */
  export function markReverted(undoId) {
    editorRef?.getView()?.dispatch({ effects: markRevertedEffect.of(undoId) })
  }

  /** Drop the run marks (the run failed or was stopped). */
  export function clearRunMarks() {
    editorRef?.getView()?.dispatch({ effects: setRunMarks.of(NO_RUN_MARKS) })
  }

  /**
   * The error console's "Did you mean": rewrite `name` inside the statement
   * that failed, preferring the occurrence the database pointed at. Only while
   * that statement is unedited since the run (its failure mark is still there),
   * so it never rewrites text the error is no longer about.
   * @param {string} name the bare name the error reported
   * @param {string} replacement already quoted as the dialect needs
   * @returns {boolean} whether anything was replaced
   */
  export function replaceInFailed(name, replacement) {
    const view = editorRef?.getView()
    if (!view || !name) return false
    const esc = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    // As written: bare in any case (the error reports the folded name), or quoted.
    const re = new RegExp(`"${esc}"|\`${esc}\`|\\[${esc}\\]|(?<![\\w$"\`])${esc}(?![\\w$])`, 'gi')
    for (const e of view.state.field(runMarksField).errors) {
      const text = view.state.doc.sliceString(e.stmtFrom, e.stmtTo)
      /** @type {{ from: number, to: number } | null} */
      let hit = null
      for (const m of text.matchAll(re)) {
        const from = e.stmtFrom + (m.index ?? 0)
        const to = from + m[0].length
        const atError = from <= e.to && to >= e.from
        if (!hit || atError) hit = { from, to }
        if (atError) break
      }
      if (!hit) continue
      // A keyword goes in the case the statement is written in.
      const was = view.state.doc.sliceString(hit.from, hit.to)
      const insert = /^[A-Z_]+$/.test(replacement) && was === was.toLowerCase() ? replacement.toLowerCase() : replacement
      view.dispatch({
        changes: { from: hit.from, to: hit.to, insert },
        selection: { anchor: hit.from, head: hit.from + insert.length },
        scrollIntoView: true,
        userEvent: 'input.complete',
      })
      view.focus()
      return true
    }
    return false
  }

  /** Focus the editor (called when the SQL tab becomes active). */
  /** Fold every statement to its first line (the editor menu's Fold all). */
  export function foldAll() { editorRef?.foldEverything?.() }
  /** Open every folded statement. */
  export function unfoldAll() { editorRef?.unfoldEverything?.() }

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

  /**
   * Fill the editor with a template whose placeholders are snippet fields, so
   * Tab walks them (the sidebar's "New function" and friends). False when the
   * editor is not up yet.
   * @param {string} template CodeMirror snippet syntax: `${1:name}`, `${0}`
   */
  export function insertSnippet(template) {
    const view = editorRef?.getView()
    if (!view) return false
    snippet(template)(view, null, 0, view.state.doc.length)
    view.focus()
    return true
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
    folding={false}
    suggestWhileTyping={$appSqlEditor.suggestWhileTyping}
    acceptOnEnter={$appSqlEditor.acceptOnEnter}
    {onchange}
    {keys}
    {extensions}
    ariaLabel="SQL editor"
  />
</div>
