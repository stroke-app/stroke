<script>
  /**
   * A failed statement, the way an editor's problems view reports one: what
   * went wrong in plain words, the database's own message and where, the
   * statement with the failing text underlined (the same mark the SQL editor
   * puts on it), and the names it probably meant, one click from fixed.
   */
  import { EditorView, Decoration } from '@codemirror/view'
  import CircleAlert from '@lucide/svelte/icons/circle-alert'
  import Copy from '@lucide/svelte/icons/copy'
  import Check from '@lucide/svelte/icons/check'
  import Wand2 from '@lucide/svelte/icons/wand-2'
  import { Button } from '$lib/components/ui/button/index.js'
  import CodeEditor from './CodeEditor.svelte'
  import { errorSpan, lineColumn, cleanErrorMessage } from '$lib/sql-error-range.js'
  import { explainSqlError, suggestNames, bareName, tablesIn } from '$lib/sql-error-explain.js'
  import { PG_KEYWORDS } from '$lib/sql-complete-data.js'

  let {
    /** The error as the backend reported it. */
    error = '',
    /** The statement that failed, as it was sent ('' when unknown). */
    sql = '',
    /** 1-based character position of the failure in `sql`, when the database gave one. */
    position = /** @type {number | null} */ (null),
    queryMs = 0,
    dialect = 'postgres',
    /** Names that exist, for "Did you mean". @type {import('$lib/sql-complete-data.js').SqlSchemaHints} */
    hints = {},
    copied = false,
    oncopy = () => {},
    /** @type {(() => void) | undefined} */
    onfixwithai = undefined,
    /**
     * Put a suggested name in place of the one that failed.
     * @type {((name: string, replacement: string) => void) | undefined}
     */
    onsuggest = undefined,
    /** Extra guidance under the message (timeouts). @type {import('svelte').Snippet | undefined} */
    children = undefined,
  } = $props()

  const message = $derived(cleanErrorMessage(error))
  const explained = $derived(explainSqlError(message))
  const span = $derived(sql ? errorSpan(sql, position) : null)
  const where = $derived(sql && position ? lineColumn(sql, /** @type {{ from: number }} */ (span).from) : null)

  /** Sentence case for a message the rules did not reword (drivers write lowercase). */
  const title = $derived(
    explained.kind === 'other' ? explained.title.charAt(0).toUpperCase() + explained.title.slice(1) : explained.title,
  )

  /** What exists of the kind of thing the error could not find. */
  const candidates = $derived.by(() => {
    switch (explained.kind) {
      case 'table': return hints.tables ?? []
      case 'schema': return hints.schemas ?? []
      case 'syntax': return /^[A-Za-z_]+$/.test(explained.name) ? PG_KEYWORDS : []
      case 'column': {
        const used = new Set(tablesIn(sql).map((t) => t.toLowerCase()))
        /** @type {string[]} */
        const names = []
        for (const [table, cols] of Object.entries(hints.columnsByTable ?? {})) {
          if (table === '__result__' || (used.size && !used.has(bareName(table).toLowerCase()))) continue
          for (const c of cols) names.push(typeof c === 'string' ? c : c.name)
        }
        return names
      }
      default: return []
    }
  })

  // Keywords are written as they are, never quoted.
  const suggestions = $derived(
    explained.name && onsuggest
      ? suggestNames(explained.name, candidates, explained.kind === 'syntax' ? '' : dialect)
      : [],
  )
  /** A suggestion that only adds quotes is the Postgres case trap: say why. */
  const quotingTrap = $derived(
    suggestions.some((s) => s.startsWith('"') && s.slice(1, -1).toLowerCase() === bareName(explained.name).toLowerCase()),
  )

  // A column error names columns the hints may not hold yet: fetch the
  // statement's tables once, and the suggestions fill in when they land.
  $effect(() => {
    if (explained.kind !== 'column') return
    const tables = tablesIn(sql)
    if (tables.length) void hints.loadColumns?.(tables)
  })

  const excerptTheme = EditorView.theme({ '.cm-content': { padding: '6px 0' } })

  // Underline only where the database pointed. Without a position the editor
  // marks the first line, which says nothing here: the title already does.
  const errorMark = $derived.by(() => {
    if (!position || !span || span.to <= span.from) return [excerptTheme]
    return [
      excerptTheme,
      EditorView.decorations.of(Decoration.set([Decoration.mark({ class: 'cm-sql-run-error' }).range(span.from, span.to)])),
      EditorView.theme({
        '.cm-sql-run-error': {
          textDecoration: 'underline wavy color-mix(in oklch, var(--destructive) 85%, transparent)',
          textUnderlineOffset: '3px',
          backgroundColor: 'color-mix(in oklch, var(--destructive) 12%, transparent)',
          borderRadius: '2px',
        },
      }),
    ]
  })
</script>

<!-- Selectable (the app is select-none by default). -->
<div data-studio-selectable="text" class="flex h-full min-h-0 flex-col">
  <div class="flex h-8 shrink-0 select-none items-center gap-2 border-b border-border/60 ps-3 pe-1">
    <CircleAlert class="size-3.5 shrink-0 text-destructive" />
    <span class="text-ui-xs font-medium text-foreground">Query failed</span>
    {#if queryMs > 0}
      <span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">{queryMs}ms</span>
    {/if}
    <div class="ms-auto flex shrink-0 items-center gap-1">
      <button
        type="button"
        class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={oncopy}
        title="Copy error"
        aria-label="Copy error"
      >
        {#if copied}<Check class="size-3.5 shrink-0" />{:else}<Copy class="size-3.5 shrink-0" />{/if}
      </button>
      {#if onfixwithai}
        <Button type="button" variant="ghost" size="sm" onclick={onfixwithai}>
          <Wand2 class="size-3.5 shrink-0" data-icon="inline-start" />
          Fix with AI
        </Button>
      {/if}
    </div>
  </div>

  <div class="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-3">
    <div class="flex min-w-0 flex-col gap-1">
      <p class="min-w-0 select-text text-ui-sm font-medium leading-snug [overflow-wrap:anywhere] text-foreground">
        {title}{#if explained.name}{' '}<span class="font-mono">{explained.name}</span>{/if}
      </p>
      {#if explained.detail || where}
        <p class="min-w-0 select-text whitespace-pre-wrap font-mono text-ui-2xs leading-relaxed [overflow-wrap:anywhere] text-muted-foreground">
          {explained.detail}{#if explained.detail && where}{' · '}{/if}{#if where}<span class="tabular-nums">Ln {where.line}, Col {where.column}</span>{/if}
        </p>
      {/if}
    </div>

    {#if sql}
      <div class="max-h-48 shrink-0 overflow-auto rounded-md bg-muted/40">
        <CodeEditor
          value={sql}
          readOnly
          lang="sql"
          {dialect}
          wrap
          gutter
          folding={false}
          suggestWhileTyping={false}
          extensions={errorMark}
          ariaLabel="Failed statement"
        />
      </div>
    {/if}

    {#if suggestions.length}
      <div class="flex flex-col gap-1.5">
        <div class="flex flex-wrap items-center gap-1.5">
          <span class="me-0.5 text-ui-xs text-muted-foreground">Did you mean</span>
          {#each suggestions as s (s)}
            <Button
              type="button"
              variant="outline"
              size="sm"
              class="font-mono"
              title="Replace {explained.name} with {s} in the editor"
              onclick={() => onsuggest?.(bareName(explained.name), s)}
            >
              {s}
            </Button>
          {/each}
        </div>
        {#if quotingTrap}
          <p class="text-ui-xs leading-relaxed text-muted-foreground">
            Postgres lowercases names that are not in double quotes, so a name with capitals only matches when quoted.
          </p>
        {/if}
      </div>
    {/if}

    {@render children?.()}
  </div>
</div>
