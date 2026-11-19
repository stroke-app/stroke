<script>
  /**
   * One line of SQL that filters the grid: `id > 200 AND name LIKE 'a%'`.
   *
   * It is the WHERE clause without the WHERE, highlighted and completed like the
   * SQL editor - the table's columns first, then functions and operators. What
   * is typed is a draft: it applies on Enter (or when focus leaves) and Escape
   * puts back what was applied, so a half-typed condition never reloads the
   * grid with an error. The backend checks the text before it runs it.
   */
  import { EditorState } from '@codemirror/state'
  import { completionStatus } from '@codemirror/autocomplete'
  import CodeEditor from './CodeEditor.svelte'
  import { filterCompletionSource } from '$lib/cm-filter-complete.js'

  let {
    /** The condition currently applied. */
    value = '',
    /** The app's Dialect id, for highlighting, quoting and functions. */
    dialect = 'postgres',
    /** The table's columns. @type {{ name: string, dataType?: string, data_type?: string }[]} */
    columns = [],
    /** Apply this condition (called on Enter / blur when it changed). @type {(text: string) => void} */
    oncommit = () => {},
    /** The database rejected the applied condition: why. */
    error = '',
  } = $props()

  // Filled from `value` by the effect below, before first paint.
  let draft = $state('')
  // A change applied from elsewhere (filters cleared, a saved view loaded)
  // replaces the draft.
  $effect(() => {
    draft = value
  })

  const columnHints = $derived(columns.map((c) => ({ name: c.name, type: c.dataType ?? c.data_type ?? '' })))
  const source = filterCompletionSource(() => columnHints, () => dialect)

  // The example uses this table's own columns: `id > 200 AND name LIKE 'a%'`
  // read as a suggestion to type, and on a table with no `id` it was the
  // condition that failed.
  const example = $derived.by(() => {
    const q = dialect === 'mysql' || dialect === 'mariadb' ? '`' : '"'
    const ident = (/** @type {string} */ n) => (/^[a-z_][a-z0-9_]*$/.test(n) ? n : `${q}${n.replace(new RegExp(q, 'g'), q + q)}${q}`)
    const typeOf = (/** @type {any} */ c) => String(c.dataType ?? c.data_type ?? '').toLowerCase()
    const num = columns.find((c) => /int|serial|numeric|decimal|float|double|real|money/.test(typeOf(c)))
    const text = columns.find((c) => /char|text|string|varchar/.test(typeOf(c)))
    const parts = [
      num && `${ident(num.name)} > 100`,
      text && `${ident(text.name)} LIKE 'a%'`,
    ].filter(Boolean)
    return parts.length ? parts.join(' AND ') : columns[0] ? `${ident(columns[0].name)} IS NOT NULL` : 'WHERE condition'
  })

  function commit() {
    const next = draft.replace(/\s+/g, ' ').trim()
    if (next !== value.trim()) oncommit(next)
  }

  /** @type {import('@codemirror/view').KeyBinding[]} */
  const keys = [
    {
      key: 'Enter',
      // With the suggestion list open, Enter takes the suggestion.
      run: (view) => {
        if (completionStatus(view.state) === 'active') return false
        commit()
        return true
      },
    },
    {
      key: 'Escape',
      run: (view) => {
        if (completionStatus(view.state) === 'active') return false
        if (draft === value) return false
        draft = value
        return true
      },
    },
  ]

  // One line: a pasted newline becomes a space instead of a second line.
  const singleLine = EditorState.transactionFilter.of((tr) => {
    if (!tr.docChanged || tr.newDoc.lines === 1) return tr
    const text = tr.newDoc.toString().replace(/\r?\n/g, ' ')
    return [tr, { changes: { from: 0, to: tr.newDoc.length, insert: text }, sequential: true }]
  })
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  title={error || undefined}
  class:border-destructive!={!!error}
  class="field-surface flex h-7 min-w-[12rem] flex-1 items-center overflow-hidden px-1 text-ui-sm [&_.cm-content]:py-0 [&_.cm-editor]:bg-transparent [&_.cm-line]:leading-[1.625rem] [&_.cm-scroller]:overflow-hidden"
  onfocusout={(e) => {
    // Leaving for the suggestion list (rendered on <body>) isn't leaving.
    if (e.relatedTarget instanceof Node && e.currentTarget.contains(e.relatedTarget)) return
    commit()
  }}
>
  <CodeEditor
    bind:value={draft}
    lang="sql"
    {dialect}
    gutter={false}
    folding={false}
    wrap={false}
    placeholder={example}
    ariaLabel="SQL filter condition"
    completionSource={source}
    priorityKeys={keys}
    extensions={[singleLine]}
  />
</div>
{#if error}
  <span class="min-w-0 max-w-[24rem] truncate font-mono text-ui-2xs text-destructive" title={error} role="alert">{error}</span>
{/if}
