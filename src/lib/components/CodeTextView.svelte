<script>
  import CodeEditor from './CodeEditor.svelte'

  /**
   * Shared read-only surface for large generated documents: the table's JSON
   * and text views, DDL, ORM schemas. CodeMirror draws only the lines on
   * screen and parses lazily around them, so a multi-megabyte payload scrolls
   * and selects like a short one; ⌘F finds in it. Whole-document work is
   * still kept off the huge ones:
   *
   * - the language falls back to plain text above PLAINTEXT_OVER_CHARS, so a
   *   giant payload is not parsed at all
   * - fold arrows (an indent scan per line) turn off above FOLDING_MAX_LINES
   * - CodeEditor itself drops highlighting for a line past 20,000 characters
   */
  let {
    text = '',
    /** Language id (cm-languages.js); may be downgraded to plain text for huge documents. */
    language = 'plaintext',
    /** Off by default: wrapping re-lays-out every line on a width change, which
     *  the huge JSON / text payloads this shows can't afford. Views showing
     *  hand-sized documents (DDL) turn it on. */
    wordWrap = /** @type {'on' | 'off'} */ ('off'),
    ariaLabel = '',
  } = $props()

  const PLAINTEXT_OVER_CHARS = 4_000_000
  const FOLDING_MAX_LINES = 50_000

  const lang = $derived(text.length > PLAINTEXT_OVER_CHARS ? 'plaintext' : language)

  /** Lines in `text`, counted without splitting a multi-MB string into an array. */
  const folding = $derived.by(() => {
    let lines = 1
    for (let i = text.indexOf('\n'); i !== -1 && lines <= FOLDING_MAX_LINES; i = text.indexOf('\n', i + 1)) lines++
    return lines <= FOLDING_MAX_LINES
  })
</script>

<div class="relative flex min-h-0 flex-1 flex-col overflow-hidden">
  <CodeEditor value={text} readOnly {lang} wrap={wordWrap === 'on'} {folding} ariaLabel={ariaLabel || `${language} document`} />
</div>
