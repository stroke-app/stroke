<script>
  /**
   * A small real editor (CodeMirror 6) for values that are code-shaped: JSON,
   * HTML, SQL snippets, stack traces, long text.
   *
   * The cell panel used to fake this with a <textarea> and a coloured copy of
   * the text drawn behind it. That only works while both boxes lay text out to
   * the pixel identically, and they do not: wrap width, scrollbar space and
   * font fallback all drift, and the caret ends up over the wrong character.
   * CodeMirror owns its layout, caret, selection, gutter and undo, so none of
   * that can disagree. It is a fraction of Monaco's size, which is the editor
   * this panel deliberately does not load for one cell.
   *
   * @typedef {Object} Props
   * @property {string} value
   * @property {boolean} [readOnly]
   * @property {boolean} [wrap]
   * @property {string} [placeholder]
   * @property {string} [ariaLabel]
   * @property {import('@codemirror/view').KeyBinding[]} [keys] Extra bindings, checked first.
   * @property {'' | 'sql'} [lang] Force a language. Empty guesses from the text.
   * @property {string} [dialect] SQL dialect for `lang="sql"` (the app's Dialect ids).
   * @property {import('$lib/sql-complete-data.js').SqlSchemaHints} [sqlHints]
   *   Schemas, tables, columns, enums and functions for SQL completion.
   * @property {import('@codemirror/state').Extension} [extensions] More extensions (a surface's
   *   own gutters, keymaps, lint). Reconfigured in place when the value changes.
   */
  import { onMount, onDestroy } from 'svelte'
  import { EditorState, Compartment, Prec } from '@codemirror/state'
  import {
    EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter,
    drawSelection, placeholder as placeholderExt, tooltips,
  } from '@codemirror/view'
  import {
    search, searchKeymap, openSearchPanel, closeSearchPanel, highlightSelectionMatches,
    SearchQuery, setSearchQuery, getSearchQuery, findNext, findPrevious, replaceNext, replaceAll as replaceAllMatches,
  } from '@codemirror/search'
  import { history, defaultKeymap, historyKeymap, insertTab } from '@codemirror/commands'
  import { HighlightStyle, syntaxHighlighting, bracketMatching, foldGutter, codeFolding, foldKeymap, foldAll, unfoldAll } from '@codemirror/language'
  import { json } from '@codemirror/lang-json'
  import { html } from '@codemirror/lang-html'
  import { sql } from '@codemirror/lang-sql'
  import { sqlDialectFor } from '$lib/cm-sql-dialects.js'
  import {
    autocompletion, acceptCompletion, closeBrackets, closeBracketsKeymap, completionStatus, closeCompletion,
    snippetKeymap, nextSnippetField, prevSnippetField, clearSnippet, completionKeymap,
  } from '@codemirror/autocomplete'
  import { sqlCompletionSource, enterPastTypedWord } from '$lib/cm-sql-complete.js'
  import { ArrowDown01Icon, ArrowRight01Icon } from '@hugeicons/core-free-icons'
  import { hugeSvg } from '$lib/cm-huge-icon.js'
  import { tags as t } from '@lezer/highlight'

  let {
    value = $bindable(''),
    readOnly = false,
    wrap = true,
    placeholder = '',
    ariaLabel = '',
    keys = [],
    gutter = true,
    lang = '',
    dialect = '',
    sqlHints = /** @type {import('$lib/sql-complete-data.js').SqlSchemaHints} */ ({}),
    /** Fold arrows in the gutter (independent of line numbers). */
    folding = true,
    /** SQL: open the completion list while typing (Ctrl+Space works regardless). */
    suggestWhileTyping = true,
    /** SQL: Enter takes the highlighted suggestion. Off, only Tab does. */
    acceptOnEnter = true,
    /** Called with the new text on every edit (the same string `value` gets). */
    onchange = /** @type {((text: string) => void) | undefined} */ (undefined),
    extensions = [],
  } = $props()

  /** @type {HTMLDivElement | null} */
  let host = $state(null)
  /** @type {EditorView | null} */
  let view = null

  const wrapC = new Compartment()
  const gutterC = new Compartment()
  const readOnlyC = new Compartment()
  const langC = new Compartment()
  const extraC = new Compartment()
  const measureC = new Compartment()
  const completeC = new Compartment()
  /** Two empty themes to flip between: see `remeasure`. */
  const MEASURE_FLIP = [EditorView.theme({}), EditorView.theme({})]
  let measureFlip = 0

  /**
   * Longest logical line, without splitting the string into an array - a
   * `split('\n')` on a multi-megabyte value allocates a second copy of it.
   */
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
   * Past this, no language and so no parse or highlight. A jsonb column holding
   * a file comes through as one line of a few hundred thousand characters, and
   * handing that to the JSON parser costs more than the colour is worth. VS
   * Code draws the same line at 20,000 characters
   * (`editor.maxTokenizationLineLength`), for the same reason.
   */
  const MAX_TOKENIZE_LINE = 20_000



  /** JSON by its first character, markup by an early tag; everything else plain. */
  function languageFor(/** @type {string} */ text) {
    if (longestLine(text) > MAX_TOKENIZE_LINE) return []
    if (lang === 'sql') {
      return sql({ dialect: sqlDialectFor(dialect) })
    }
    if (/^\s*[[{]/.test(text)) return json()
    if (/<[A-Za-z!/]/.test(text.slice(0, 2000))) return html()
    return []
  }

  /** The same token colours the JSON tree uses, so both panes agree. */
  const highlight = HighlightStyle.define([
    { tag: t.propertyName, color: 'var(--json-key)' },
    { tag: [t.string, t.attributeValue], color: 'var(--json-string)' },
    { tag: t.number, color: 'var(--json-number)' },
    { tag: t.bool, color: 'var(--json-boolean)' },
    { tag: t.null, color: 'var(--json-null)', fontStyle: 'italic' },
    { tag: [t.tagName, t.typeName], color: 'var(--json-key)' },
    { tag: t.attributeName, color: 'var(--json-number)' },
    { tag: [t.comment, t.blockComment], color: 'var(--json-null)', fontStyle: 'italic' },
    { tag: [t.punctuation, t.bracket, t.angleBracket, t.separator], color: 'var(--muted-foreground)' },
    // SQL. A quoted identifier is a name, not a string: without its own rule it
    // inherits the string green and "name" = 'ad' reads as two strings.
    { tag: t.keyword, color: 'var(--json-boolean)' },
    { tag: t.operator, color: 'var(--muted-foreground)' },
    { tag: t.special(t.string), color: 'var(--foreground)' },
    { tag: t.lineComment, color: 'var(--json-null)', fontStyle: 'italic' },
  ])

  /*
   * One type system for the whole surface: the gutter uses the code's font,
   * size and line-height, so number N sits on text row N's baseline. Only the
   * selection CodeMirror draws is styled; the browser's native ::selection is
   * left to drawSelection(), which hides it. Styling both painted a pale block
   * over the text and washed it out.
   */
  const theme = EditorView.theme({
    // Scoped to `.cm-editor`: CodeMirror puts every theme class on the tooltip
    // host it appends to <body> as well (tooltips({ parent })), and a 100%-tall
    // host made the body scrollable, so a focus() scrolled the whole app chrome
    // 23px up under the window edge.
    '&.cm-editor': { height: '100%' },
    '&': {
      backgroundColor: 'transparent',
      color: 'var(--foreground)',
      // A surface can set these on an ancestor: the SQL console follows the
      // editor font setting, the docks stay at the UI's code size.
      fontSize: 'var(--cm-font-size, var(--fs-2xs))',
    },
    '&.cm-focused': { outline: 'none' },
    '.cm-scroller': {
      fontFamily: 'var(--cm-font-family, var(--font-mono))',
      lineHeight: 'var(--cm-line-height, 1.6)',
      overflow: 'auto',
    },
    '.cm-content': { padding: '6px 0', caretColor: 'var(--foreground)' },
    '.cm-line': { padding: '0 14px 0 10px' },
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--foreground)', borderLeftWidth: '1.5px' },
    // The app-wide ::selection rule (app.css) reaches in here too and paints the
    // native selection on top of the one drawSelection() draws. Hide it inside
    // the editor so there is one selection, CodeMirror's.
    '.cm-content ::selection, .cm-line::selection, .cm-content::selection': {
      backgroundColor: 'transparent !important',
    },
    '.cm-selectionBackground': {
      backgroundColor: 'color-mix(in oklch, var(--foreground) 10%, transparent) !important',
    },
    '&.cm-focused .cm-selectionBackground': {
      backgroundColor: 'color-mix(in oklch, var(--foreground) 17%, transparent) !important',
    },
    '.cm-activeLine': { backgroundColor: 'color-mix(in oklch, var(--muted) 30%, transparent)' },
    '.cm-gutters': {
      backgroundColor: 'transparent',
      color: 'color-mix(in oklch, var(--muted-foreground) 50%, transparent)',
      border: 'none',
      borderRight: '1px solid color-mix(in oklch, var(--border) 40%, transparent)',
      fontFamily: 'var(--cm-font-family, var(--font-mono))',
      fontSize: 'var(--cm-font-size, var(--fs-2xs))',
      fontVariantNumeric: 'tabular-nums',
    },
    // Five digits reserved up front. The gutter sizes to the widest number, so
    // typing line 100 (or 1,000) widened it and pushed every line of text
    // sideways mid-keystroke. `ch` is one digit in the mono face; the gutter
    // elements are border-box, hence the padding added in. Past 99,999 lines
    // it still grows, once.
    '.cm-lineNumbers .cm-gutterElement': {
      padding: '0 6px 0 12px',
      minWidth: 'calc(5ch + 18px)',
      textAlign: 'right',
    },
    '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--muted-foreground)' },
    // Fold arrows only where they apply and only while the gutter is hovered,
    // so an unfoldable plain-text value has no blank strip beside its numbers.
    '.cm-foldGutter .cm-gutterElement': {
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      padding: '0',
      width: '12px',
      cursor: 'pointer',
      opacity: '0',
      transition: 'opacity 120ms',
    },
    '.cm-fold-marker': { display: 'inline-flex', color: 'var(--muted-foreground)' },
    '.cm-fold-marker:hover': { color: 'var(--foreground)' },
    '.cm-fold-marker svg': { width: '12px', height: '12px' },
    '.cm-gutters:hover .cm-foldGutter .cm-gutterElement': { opacity: '1' },
    '.cm-foldPlaceholder': {
      backgroundColor: 'color-mix(in oklch, var(--muted) 60%, transparent)',
      border: 'none',
      color: 'var(--muted-foreground)',
      padding: '0 4px',
      borderRadius: '3px',
    },
    '.cm-matchingBracket': {
      backgroundColor: 'color-mix(in oklch, var(--foreground) 12%, transparent)',
      outline: 'none',
    },
    '.cm-find-hit': {
      backgroundColor: 'color-mix(in oklch, var(--warning) 35%, transparent)',
      borderRadius: '2px',
    },
    '.cm-placeholder': { color: 'var(--muted-foreground)' },
    // SQL completion list: small, quiet, one line per name. Row metrics are in
    // em, not px: the text follows the app zoom (--fs-*), and fixed 22px rows
    // with 8px gaps went cramped as soon as the zoom went past 100%. At the
    // default size a row is 25px, the app's menu row.
    '.cm-tooltip': {
      backgroundColor: 'var(--popover)',
      color: 'var(--popover-foreground)',
      border: '1px solid color-mix(in oklch, var(--border) 80%, transparent)',
      borderRadius: '8px',
      boxShadow: '0 8px 24px rgba(0, 0, 0, 0.35), 0 1px 3px rgba(0, 0, 0, 0.2)',
      overflow: 'hidden',
    },
    '.cm-tooltip.cm-tooltip-autocomplete > ul': {
      fontFamily: 'var(--font-mono)',
      fontSize: 'var(--fs-2xs)',
      padding: '0.3em',
      minWidth: '16em',
      maxWidth: '34em',
      // Seven and a half rows: the half row says the list scrolls.
      maxHeight: 'calc(7.5 * 1.9em + 0.6em)',
    },
    '.cm-tooltip.cm-tooltip-autocomplete > ul > li': {
      display: 'flex',
      alignItems: 'center',
      height: '1.9em',
      padding: '0 0.65em',
      borderRadius: '0.4em',
      color: 'color-mix(in oklch, var(--foreground) 78%, transparent)',
    },
    '.cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]': {
      backgroundColor: 'color-mix(in oklch, var(--foreground) 9%, transparent)',
      color: 'var(--foreground)',
    },
    '.cm-completionLabel': { overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' },
    '.cm-completionMatchedText': { textDecoration: 'none', color: 'var(--foreground)', fontWeight: '600' },
    '.cm-completion-kind': {
      flexShrink: '0',
      width: '0.45em',
      height: '0.45em',
      marginRight: '0.7em',
      borderRadius: '9999px',
      backgroundColor: 'var(--muted-foreground)',
    },
    '.cm-completion-kind-column': { backgroundColor: 'var(--json-key)' },
    '.cm-completion-kind-table': { backgroundColor: 'var(--json-number)' },
    '.cm-completion-kind-keyword': { backgroundColor: 'var(--json-boolean)' },
    '.cm-completion-kind-type': { backgroundColor: 'var(--json-string)' },
    '.cm-completion-kind-schema': { backgroundColor: 'color-mix(in oklch, var(--json-number) 55%, transparent)' },
    '.cm-completion-kind-function': { backgroundColor: 'var(--json-string)' },
    '.cm-completion-kind-snippet': { backgroundColor: 'transparent', boxShadow: 'inset 0 0 0 1.5px var(--muted-foreground)' },
    '.cm-completion-kind-enum': { backgroundColor: 'var(--primary)' },
    // The doc beside the list for functions and snippets.
    '.cm-snippet-preview': {
      margin: '0',
      fontFamily: 'var(--font-mono)',
      fontSize: 'var(--fs-3xs)',
      lineHeight: '1.55',
      color: 'var(--foreground)',
      whiteSpace: 'pre',
    },
    '.cm-tooltip.cm-completionInfo': {
      maxWidth: '38em',
      padding: '0.6em 0.9em',
      fontFamily: 'var(--font-sans)',
      fontSize: 'var(--fs-3xs)',
      lineHeight: '1.5',
      color: 'var(--muted-foreground)',
      whiteSpace: 'pre-wrap',
    },
    '.cm-completionDetail': {
      flexShrink: '0',
      marginLeft: 'auto',
      paddingLeft: '1.75em',
      fontStyle: 'normal',
      fontSize: 'var(--fs-3xs)',
      color: 'var(--muted-foreground)',
    },
    '.cm-searchMatch': {
      backgroundColor: 'color-mix(in oklch, var(--warning) 30%, transparent)',
      borderRadius: '2px',
    },
    '.cm-searchMatch-selected': {
      backgroundColor: 'color-mix(in oklch, var(--warning) 60%, transparent)',
    },
    '.cm-selectionMatch': { backgroundColor: 'color-mix(in oklch, var(--foreground) 8%, transparent)' },
    // Find/replace panel (createFindPanel): the app's field, icon-button and
    // focus language. 28px controls, 6px radius, the field hairline, one ring.
    '.cm-panels': { backgroundColor: 'var(--background)', color: 'var(--foreground)' },
    '.cm-panels-top': { borderBottom: '1px solid color-mix(in oklch, var(--border) 45%, transparent)' },
    '.cm-find': { display: 'flex', flexDirection: 'column', gap: '4px', padding: '6px 8px' },
    '.cm-find-row': { display: 'flex', alignItems: 'center', gap: '2px', minWidth: '0' },
    '.cm-find-row[hidden]': { display: 'none' },
    '.cm-find-field': {
      display: 'flex',
      alignItems: 'center',
      gap: '6px',
      flex: '1 1 auto',
      minWidth: '0',
      maxWidth: '340px',
      height: '28px',
      marginInlineEnd: '4px',
      paddingInlineStart: '8px',
      paddingInlineEnd: '3px',
      color: 'var(--muted-foreground)',
      backgroundColor: 'color-mix(in oklch, var(--muted) 25%, transparent)',
      border: 'var(--field-border-width) solid var(--field-border)',
      borderRadius: 'var(--radius-field)',
      cursor: 'text',
    },
    '.cm-find-field:focus-within': { outline: '2px solid var(--ring)', outlineOffset: '1px' },
    '.cm-find-invalid': { borderColor: 'var(--destructive)' },
    '.cm-find-field svg': { flexShrink: '0' },
    // The app's global input styles (border, radius, fill, focus ring) reach
    // this input too, which drew a second box inside the field's frame. The
    // frame is the field; the input inside is bare text.
    '.cm-find-field input, .cm-find-field input:focus, .cm-find-field input:focus-visible': {
      flex: '1 1 auto',
      minWidth: '0',
      height: '100%',
      margin: '0',
      padding: '0',
      appearance: 'none',
      background: 'transparent !important',
      border: 'none !important',
      borderRadius: '0 !important',
      boxShadow: 'none !important',
      outline: 'none !important',
      color: 'var(--foreground)',
      fontFamily: 'var(--font-mono)',
      fontSize: 'var(--fs-2xs)',
    },
    '.cm-find-field input::placeholder': { color: 'var(--muted-foreground)' },
    '.cm-find-none': { color: 'var(--destructive) !important' },
    '.cm-find-count': {
      flexShrink: '0',
      minWidth: '10ch',
      paddingInline: '4px 6px',
      fontFamily: 'var(--font-mono)',
      fontSize: 'var(--fs-3xs)',
      fontVariantNumeric: 'tabular-nums',
      color: 'var(--muted-foreground)',
      whiteSpace: 'nowrap',
    },
    '.cm-find-icon, .cm-find-toggle, .cm-find-text': {
      display: 'inline-flex',
      alignItems: 'center',
      justifyContent: 'center',
      flexShrink: '0',
      height: '28px',
      minWidth: '28px',
      padding: '0',
      color: 'var(--muted-foreground)',
      background: 'transparent',
      border: 'none',
      borderRadius: 'var(--radius-field)',
      fontFamily: 'var(--font-mono)',
      fontSize: 'var(--fs-2xs)',
      cursor: 'pointer',
      transition: 'background-color 120ms, color 120ms',
    },
    // Replace / All: bordered, so they read as buttons and not as labels.
    '.cm-find-text': {
      paddingInline: '10px',
      marginInlineStart: '4px',
      border: '1px solid color-mix(in oklch, var(--border) 70%, transparent)',
    },
    // Toggles live inside the field, a size down from the row's buttons.
    '.cm-find-field .cm-find-toggle': { height: '22px', minWidth: '22px', paddingInline: '4px', fontSize: 'var(--fs-3xs)' },
    '.cm-find-expand svg': { transition: 'transform 150ms' },
    '.cm-find-expand[aria-expanded=true] svg': { transform: 'rotate(90deg)' },
    '.cm-find-expand[hidden]': { display: 'none' },
    // Same width as the expand toggle, so both fields start on one edge.
    '.cm-find-spacer': { flexShrink: '0', width: '28px', height: '1px' },
    '.cm-find-icon:hover, .cm-find-toggle:hover, .cm-find-text:hover': {
      color: 'var(--foreground)',
      backgroundColor: 'color-mix(in oklch, var(--muted) 55%, transparent)',
    },
    '.cm-find-toggle[aria-pressed=true]': {
      color: 'var(--foreground)',
      backgroundColor: 'color-mix(in oklch, var(--foreground) 14%, transparent)',
    },
    '.cm-find-icon:focus-visible, .cm-find-toggle:focus-visible, .cm-find-text:focus-visible': {
      outline: '2px solid var(--ring)',
      outlineOffset: '1px',
    },
    '.cm-find-close': { marginInlineStart: 'auto' },
  })

  const ICON = {
    up: '<path d="m18 15-6-6-6 6"/>',
    down: '<path d="m6 9 6 6 6-6"/>',
    x: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
    search: '<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>',
    chevron: '<path d="m9 18 6-6-6-6"/>',
  }
  /** @param {keyof typeof ICON} name */
  const svg = (name) =>
    `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICON[name]}</svg>`

  /** Set by the replace shortcut so the panel it opens starts with replace shown. */
  let openWithReplace = false
  /** The open panel's controls, so the shortcut can reveal replace in place. */
  /** @type {{ showReplace: () => void } | null} */
  let livePanel = null

  /** Past this many matches the count reads "1000+" instead of walking on. */
  const COUNT_CAP = 1000

  /**
   * Find and replace, drawn in the app's own controls on CodeMirror's search
   * engine. The stock panel was one long row of pill buttons with lowercase
   * words ("next", "all", "by word") in a different visual language from
   * everything around it.
   *
   * Row 1: field · count · previous / next · Aa .* W toggles · close.
   * Row 2: replace field · Replace · All (not rendered when read-only).
   *
   * The match count walks the document once per query or edit, capped, and the
   * "n of" index is a binary search in that list - moving the selection does
   * not rescan.
   * @param {EditorView} view
   */
  function createFindPanel(view) {
    const dom = document.createElement('div')
    dom.className = 'cm-find'
    const initial = getSearchQuery(view.state)
    let caseSensitive = initial.caseSensitive
    let regexp = initial.regexp
    let wholeWord = initial.wholeWord

    const row = (/** @type {string} */ cls) => {
      const r = document.createElement('div')
      r.className = `cm-find-row ${cls}`
      dom.appendChild(r)
      return r
    }
    const field = (/** @type {string} */ label, /** @type {string} */ value) => {
      const wrap = document.createElement('label')
      wrap.className = 'cm-find-field'
      const input = document.createElement('input')
      input.type = 'text'
      input.value = value
      input.placeholder = label
      input.setAttribute('aria-label', label)
      input.spellcheck = false
      input.setAttribute('main-field', label === 'Find' ? 'true' : 'false')
      wrap.appendChild(input)
      return { wrap, input }
    }
    const btn = (/** @type {string} */ html, /** @type {string} */ title, /** @type {() => void} */ onclick, cls = 'cm-find-icon') => {
      const b = document.createElement('button')
      b.type = 'button'
      b.className = cls
      b.innerHTML = html
      b.title = title
      b.setAttribute('aria-label', title)
      b.addEventListener('click', (e) => { e.preventDefault(); onclick() })
      return b
    }

    // ── row 1: [›] [find field: icon · text · count · Aa .* W] [↑ ↓] ... [×]
    const r1 = row('cm-find-main')
    const expand = btn(svg('chevron'), 'Toggle replace (Ctrl+H)', () => setReplace(r2.hidden), 'cm-find-icon cm-find-expand')
    r1.appendChild(expand)
    const find = field('Find', initial.search)
    find.wrap.insertAdjacentHTML('afterbegin', svg('search'))
    // The count sits outside the field, after the arrows: inside, "No results"
    // plus three toggles squeezed what you typed down to a few characters.
    const count = document.createElement('span')
    count.className = 'cm-find-count'
    count.setAttribute('aria-live', 'polite')
    /** Flip functions by Alt+key, shared by the buttons and the fields. */
    /** @type {Record<string, () => void>} */
    const flips = {}
    /**
     * An option toggle inside the field. Not a tab stop: Tab goes Find to
     * Replace, the way every editor's find bar does, and Alt+C / Alt+R / Alt+W
     * flip these from either field (VS Code's keys).
     * @param {string} text @param {string} title @param {string} altKey @param {() => boolean} get @param {(v: boolean) => void} set
     */
    const toggle = (text, title, altKey, get, set) => {
      const b = btn(text, `${title} (Alt+${altKey.toUpperCase()})`, () => flip(), 'cm-find-toggle')
      b.tabIndex = -1
      const flip = () => { set(!get()); b.setAttribute('aria-pressed', String(get())); commit() }
      flips[altKey] = flip
      b.setAttribute('aria-pressed', String(get()))
      find.wrap.appendChild(b)
    }
    toggle('Aa', 'Match case', 'c', () => caseSensitive, (v) => (caseSensitive = v))
    toggle('.*', 'Regular expression', 'r', () => regexp, (v) => (regexp = v))
    toggle('W', 'Whole word', 'w', () => wholeWord, (v) => (wholeWord = v))
    r1.appendChild(find.wrap)
    // field · count · arrows: the count reads as the field's result, and the
    // arrows act on it.
    r1.appendChild(count)
    r1.appendChild(btn(svg('up'), 'Previous match (Shift+Enter)', () => { findPrevious(view) }))
    r1.appendChild(btn(svg('down'), 'Next match (Enter)', () => { findNext(view) }))
    const close = btn(svg('x'), 'Close (Escape)', () => { closeSearchPanel(view); view.focus() })
    close.classList.add('cm-find-close')
    // Everything in row 1 but the field has a key (Enter, Shift+Enter, Escape,
    // Ctrl+H), so the field is the row's only tab stop.
    for (const b of r1.querySelectorAll('button')) b.tabIndex = -1
    r1.appendChild(close)

    // ── row 2 (on request): [spacer] [replace field] [Replace] [All]
    const r2 = row('cm-find-replace')
    const spacer = document.createElement('span')
    spacer.className = 'cm-find-spacer'
    r2.appendChild(spacer)
    const replace = field('Replace', initial.replace)
    r2.appendChild(replace.wrap)
    r2.appendChild(btn('Replace', 'Replace this match (Enter)', () => { replaceNext(view) }, 'cm-find-text'))
    r2.appendChild(btn('All', 'Replace every match (Ctrl+Alt+Enter)', () => { replaceAllMatches(view) }, 'cm-find-text'))

    /** @param {boolean} on */
    function setReplace(on) {
      const show = on && !view.state.readOnly
      r2.hidden = !show
      expand.setAttribute('aria-expanded', String(show))
      if (show) { replace.input.focus(); replace.input.select() }
    }
    expand.hidden = view.state.readOnly
    setReplace(openWithReplace)
    const startWithReplace = openWithReplace
    openWithReplace = false

    /** Sorted match starts for the current query, capped at COUNT_CAP. */
    let starts = /** @type {number[]} */ ([])
    let capped = false
    /** The query the count was last taken for, by value. */
    let countedKey = ''
    const keyOf = (/** @type {SearchQuery} */ q) => `${q.search}\0${q.caseSensitive}${q.regexp}${q.wholeWord}`
    /**
     * Recount against the state being rendered. Keyed on the query's VALUE,
     * not on spotting a setSearchQuery effect in the update: that check missed
     * cases, and reading view.state instead of the update's state could count
     * the previous query - both showed "No results" over a visible match.
     * @param {EditorState} state
     */
    function recount(state = view.state) {
      const q = getSearchQuery(state)
      countedKey = keyOf(q)
      starts = []
      capped = false
      if (!q.search || !q.valid) { paintCount(); return }
      const cur = q.getCursor(state)
      for (let m = cur.next(); !m.done; m = cur.next()) {
        starts.push(m.value.from)
        if (starts.length >= COUNT_CAP) { capped = true; break }
      }
      paintCount(state)
    }
    function paintCount(state = view.state) {
      const q = getSearchQuery(state)
      find.wrap.classList.toggle('cm-find-invalid', !!q.search && !q.valid)
      if (!q.search) { count.textContent = ''; count.classList.remove('cm-find-none'); return }
      if (!q.valid) { count.textContent = 'invalid'; return }
      count.classList.toggle('cm-find-none', !starts.length)
      if (!starts.length) { count.textContent = 'No results'; return }
      const at = state.selection.main.from
      // binary search: index of the match the selection sits on, if any
      let lo = 0, hi = starts.length - 1, idx = -1
      while (lo <= hi) {
        const mid = (lo + hi) >> 1
        if (starts[mid] === at) { idx = mid; break }
        if (starts[mid] < at) lo = mid + 1; else hi = mid - 1
      }
      const total = `${starts.length}${capped ? '+' : ''}`
      count.textContent = idx >= 0 ? `${idx + 1} of ${total}` : total
    }
    function commit() {
      const q = new SearchQuery({ search: find.input.value, replace: replace.input.value, caseSensitive, regexp, wholeWord })
      if (!q.eq(getSearchQuery(view.state))) view.dispatch({ effects: setSearchQuery.of(q) })
    }

    find.input.addEventListener('input', commit)
    replace.input.addEventListener('input', commit)
    /** Alt+C / Alt+R / Alt+W from either field. `code` so layouts that put
     *  another character on Alt+letter (macOS: ç, ®, ∑) still work. */
    const altFlip = (/** @type {KeyboardEvent} */ e) => {
      if (!e.altKey || e.ctrlKey || e.metaKey) return false
      const k = e.code.replace(/^Key/, '').toLowerCase()
      if (!flips[k]) return false
      e.preventDefault()
      flips[k]()
      return true
    }
    const isTab = (/** @type {KeyboardEvent} */ e) => e.key === 'Tab' || e.code === 'Tab'
    /** Plain text-editing chords stay in the field: native select-all, copy,
     *  paste, cut and caret moves run, and no global hotkey sees them. */
    const FIELD_CHORDS = new Set(['a', 'c', 'v', 'x', 'arrowleft', 'arrowright', 'home', 'end'])
    const keepInField = (/** @type {KeyboardEvent} */ e) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && FIELD_CHORDS.has(e.key.toLowerCase())) e.stopPropagation()
    }
    find.input.addEventListener('keydown', (e) => {
      keepInField(e)
      if (altFlip(e)) return
      if (e.key === 'Enter') { e.preventDefault(); (e.shiftKey ? findPrevious : findNext)(view) }
      else if (e.key === 'Escape') { e.preventDefault(); closeSearchPanel(view); view.focus() }
      // Tab: Find to Replace when replace is showing, otherwise back into the text.
      else if (isTab(e) && !e.shiftKey) {
        e.preventDefault()
        if (!r2.hidden) { replace.input.focus(); replace.input.select() }
        else view.focus()
      }
    })
    replace.input.addEventListener('keydown', (e) => {
      keepInField(e)
      if (altFlip(e)) return
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && e.altKey) { e.preventDefault(); replaceAllMatches(view) }
      else if (e.key === 'Enter') { e.preventDefault(); replaceNext(view) }
      else if (e.key === 'Escape') { e.preventDefault(); closeSearchPanel(view); view.focus() }
      else if (isTab(e) && e.shiftKey) { e.preventDefault(); find.input.focus(); find.input.select() }
    })

    return {
      dom,
      top: true,
      mount() {
        livePanel = { showReplace: () => setReplace(true) }
        if (!startWithReplace) { find.input.focus(); find.input.select() }
        recount()
      },
      destroy() { livePanel = null },
      /** @param {import('@codemirror/view').ViewUpdate} u */
      update(u) {
        const q = getSearchQuery(u.state)
        // Keep the fields in step when the query is set from outside (Mod-F
        // with a selection seeds it), without fighting the user's typing.
        if (document.activeElement !== find.input && q.search !== find.input.value) find.input.value = q.search
        if (document.activeElement !== replace.input && q.replace !== replace.input.value) replace.input.value = q.replace
        if (u.docChanged || keyOf(q) !== countedKey) recount(u.state)
        else if (u.selectionSet) paintCount(u.state)
        expand.hidden = view.state.readOnly
        if (view.state.readOnly) r2.hidden = true
      },
    }
  }

  /** @param {EditorView} v */
  function openReplace(v) {
    if (v.state.readOnly) return false
    if (livePanel) { livePanel.showReplace(); return true }
    openWithReplace = true
    openSearchPanel(v)
    return true
  }

  const foldMarkers = foldGutter({
    markerDOM(open) {
      const el = document.createElement('span')
      el.className = 'cm-fold-marker'
      el.title = open ? 'Fold' : 'Unfold'
      el.append(hugeSvg(open ? ArrowDown01Icon : ArrowRight01Icon))
      return el
    },
  })

  /** A 6px dot before each suggestion: what kind of thing it is, by colour. */
  const kindDot = {
    position: 20,
    render(/** @type {import('@codemirror/autocomplete').Completion} */ c) {
      const el = document.createElement('span')
      el.className = `cm-completion-kind cm-completion-kind-${c.type ?? 'text'}`
      return el
    },
  }

  /**
   * SQL only: context-aware completion (cm-sql-complete.js) and paired quotes
   * and brackets, so typing `"` opens a name with the column list already up.
   * A JSON cell has nothing worth suggesting. The list renders on <body> so
   * the dock's edge never clips it.
   */
  /** @param {boolean} onTyping @param {boolean} [enterAccepts] */
  const completionConfig = (onTyping, enterAccepts = true) => [
    autocompletion({
      override: [sqlCompletionSource(() => sqlHints, () => dialect)],
      activateOnTyping: onTyping,
      icons: false,
      addToOptions: [kindDot],
      maxRenderedOptions: 50,
      // Off, the list's keys are bound below without Enter, so Enter always
      // breaks the line and Tab is the one way to take a suggestion.
      defaultKeymap: enterAccepts,
    }),
    ...(enterAccepts ? [] : [Prec.highest(keymap.of(completionKeymap.filter((b) => b.key !== 'Enter')))]),
  ]

  /**
   * Snippet keys. Tab with the list open takes the suggestion; otherwise it
   * moves to the next field. A field opens no list by itself: typing over the
   * placeholder does, so a field kept as it is (`*`, `100`) costs one Tab.
   * Escape closes the list before it leaves the snippet.
   * @type {import('@codemirror/view').KeyBinding[]}
   */
  const snippetKeys = [
    {
      key: 'Tab',
      run: (v) => (completionStatus(v.state) === 'active' && acceptCompletion(v)) || nextSnippetField(v),
      shift: prevSnippetField,
    },
    { key: 'Escape', run: (v) => (completionStatus(v.state) ? closeCompletion(v) : clearSnippet(v)) },
  ]

  const sqlEditing = () =>
    lang === 'sql'
      ? [
          // Ahead of the completion keymap (same precedence, listed first): Enter
          // on a word already typed out breaks the line instead of re-inserting it.
          Prec.highest(keymap.of([{ key: 'Enter', run: enterPastTypedWord }])),
          completeC.of(completionConfig(suggestWhileTyping, acceptOnEnter)),
          closeBrackets(),
          keymap.of(closeBracketsKeymap),
          snippetKeymap.of(snippetKeys),
          tooltips({ parent: document.body }),
        ]
      : []

  /**
   * Fold arrows only beside line numbers. Without numbers the arrow column
   * was an empty 16px strip (the arrows show on hover only) between the run
   * marks and the text; folding still works from the keyboard and the menu.
   * @param {boolean} numbers @param {boolean} fold
   */
  const gutterExt = (numbers, fold) => [...(numbers ? [lineNumbers()] : []), ...(numbers && fold ? [foldMarkers] : [])]

  function freshState(/** @type {string} */ doc) {
    return EditorState.create({
      doc,
      extensions: [
        // Reconfigurable: the panel hides the gutter on request, and hiding it
        // means dropping the fold gutter with it - a fold arrow column with no
        // numbers beside it is a stripe nothing explains.
        gutterC.of(gutterExt(gutter, folding)),
        codeFolding(),
        highlightActiveLine(),
        highlightActiveLineGutter(),
        drawSelection(),
        // A template field used twice (a trigger's `ON ${4:table}` and the
        // `UPDATE ${4:table}` in its body) is one cursor per copy. Without
        // this the state keeps only the first, so typing in one left the other
        // as the placeholder - and the trigger was created on `table_name`.
        EditorState.allowMultipleSelections.of(true),
        history(),
        bracketMatching(),
        // Find and replace: Mod-F opens it at the top, Enter / Shift-Enter step,
        // Escape closes. It marks matches only in the lines on screen and finds
        // the next one lazily, instead of scanning the whole value per keystroke
        // the way the panel's own find box did.
        search({ top: true, createPanel: createFindPanel }),
        highlightSelectionMatches(),
        EditorState.tabSize.of(2),
        keymap.of([
          ...keys,
          // Tab indents - this is an editor, and the values that need it are
          // JSON and SQL. Shift+Tab is left alone so it still leaves the field.
          { key: 'Tab', run: (v) => acceptCompletion(v) || insertTab(v) },
          // Replace: Ctrl+H (VS Code on Windows/Linux) and Mod+Alt+F (its macOS
          // binding - macOS takes Cmd+H to hide the app).
          { key: 'Mod-h', run: openReplace, preventDefault: true },
          { key: 'Mod-Alt-f', run: openReplace, preventDefault: true },
          ...searchKeymap,
          ...historyKeymap,
          // Fold the statement at the caret (Ctrl+Shift+[ / ]), or all of
          // them (Ctrl+Alt+[ / ]). A multi-line statement folds to its first
          // line; a block comment to its markers.
          ...foldKeymap,
          ...defaultKeymap,
        ]),
        syntaxHighlighting(highlight),
        theme,
        sqlEditing(),
        placeholderExt(placeholder),
        EditorView.contentAttributes.of({ 'aria-label': ariaLabel, spellcheck: 'false' }),
        wrapC.of(wrap ? EditorView.lineWrapping : []),
        readOnlyC.of(EditorState.readOnly.of(readOnly)),
        langC.of(languageFor(doc)),
        extraC.of(extensions),
        measureC.of(MEASURE_FLIP[measureFlip]),
        EditorView.updateListener.of((u) => {
          if (!u.docChanged) return
          // One copy of the text per edit, shared by the binding and onchange.
          const text = u.state.doc.toString()
          emitted = text
          value = text
          onchange?.(text)
        }),
      ],
    })
  }

  /**
   * Make CodeMirror measure the text again. It measures line height and
   * character width once, then again only when content redraws or web fonts
   * finish - not when the root's CSS changes. Zoom, the font setting and a
   * theme switch all change it, and the gutter rows then kept the old height
   * and drifted off their lines (Monaco needed its own remeasure for the same
   * reason). Swapping the theme facet is the public way to request it.
   */
  function remeasure() {
    if (!view) return
    measureFlip ^= 1
    view.dispatch({ effects: measureC.reconfigure(MEASURE_FLIP[measureFlip]) })
  }

  onMount(() => {
    if (!host) return
    view = new EditorView({ parent: host, state: freshState(value) })
    // A hidden character inside the scroller, so it takes the editor's own font
    // and line height from the theme: its box changes exactly when the text
    // metrics do - the theme or a stylesheet landing after mount, zoom, the font
    // setting, a web font swapping in. Each change (and the first report)
    // re-measures.
    const probe = document.createElement('span')
    probe.textContent = 'M'
    probe.setAttribute('aria-hidden', 'true')
    probe.style.cssText = 'position:absolute;top:0;left:0;visibility:hidden;pointer-events:none;white-space:pre;font-family:var(--cm-font-family, var(--font-mono))'
    view.scrollDOM.append(probe)
    let raf = 0
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(raf)
      raf = requestAnimationFrame(remeasure)
    })
    ro.observe(probe)
    // A chord the editor acted on is the editor's. CodeMirror marks it handled
    // (preventDefault) and lets it bubble, and the app's hotkeys on `document`
    // do not look at that: Ctrl+/ commented the line and then opened the
    // shortcuts panel, Ctrl+W closed two tabs, Ctrl+J and Ctrl+Shift+B toggled
    // twice and so did nothing. Keys the editor passed on still reach the app.
    const ownChord = (/** @type {KeyboardEvent} */ e) => {
      if (e.defaultPrevented && (e.ctrlKey || e.metaKey || e.altKey)) e.stopPropagation()
    }
    view.dom.addEventListener('keydown', ownChord)
    return () => { cancelAnimationFrame(raf); ro.disconnect(); probe.remove(); view?.dom.removeEventListener('keydown', ownChord) }
  })
  onDestroy(() => view?.destroy())

  // A value set from outside (a new cell) is a new document: fresh state, so
  // undo cannot walk back into the previous cell. Our own edits come back
  // through `value` equal to the doc and are skipped here.
  /** The text this editor last handed out, so its own edits coming back through
   *  `value` are recognised without copying the document again. */
  let emitted = /** @type {string | null} */ (null)
  $effect(() => {
    const next = value
    if (!view || next === emitted) return
    if (next !== view.state.doc.toString()) view.setState(freshState(next))
  })
  $effect(() => { const w = wrap; view?.dispatch({ effects: wrapC.reconfigure(w ? EditorView.lineWrapping : []) }) })
  $effect(() => {
    const g = gutter
    const f = folding
    view?.dispatch({ effects: gutterC.reconfigure(gutterExt(g, f)) })
  })
  $effect(() => { const r = readOnly; view?.dispatch({ effects: readOnlyC.reconfigure(EditorState.readOnly.of(r)) }) })
  // A new dialect's keywords, without resetting the doc. Tables and columns
  // are read by the completion source on every query and need nothing here.
  $effect(() => {
    void dialect
    if (lang === 'sql') view?.dispatch({ effects: langC.reconfigure(languageFor(view.state.doc.toString())) })
  })

  $effect(() => { const x = extensions; view?.dispatch({ effects: extraC.reconfigure(x) }) })
  $effect(() => {
    const t = suggestWhileTyping
    const e = acceptOnEnter
    if (lang === 'sql') view?.dispatch({ effects: completeC.reconfigure(completionConfig(t, e)) })
  })

  export function focus() { view?.focus() }

  /** Fold every statement and block comment down to its first line. */
  export function foldEverything() { if (view) foldAll(view) }
  /** Open every fold. */
  export function unfoldEverything() { if (view) unfoldAll(view) }

  /** The live view, for a surface that drives it (SqlEditor). */
  export function getView() { return view }

  /** Open find and replace (the panel's toolbar button). */
  export function find() {
    if (!view) return
    openSearchPanel(view)
  }

  /** Select a range and scroll it into view (find next / previous). */
  export function select(/** @type {number} */ from, /** @type {number} */ to) {
    if (!view) return
    view.dispatch({ selection: { anchor: from, head: to }, scrollIntoView: true })
    view.focus()
  }

  /** Replace the whole document as ONE undoable edit (revert). */
  export function replaceAll(/** @type {string} */ text) {
    if (!view) return
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text }, selection: { anchor: 0 } })
    view.focus()
  }
</script>

<div bind:this={host} class="relative min-h-0 min-w-0 flex-1 overflow-hidden"></div>
