<script>
  /**
   * A failed statement, the way a SQL console prints one: the database's
   * message, where it failed, and the statement itself in a read-only editor
   * with the failing text underlined (the same mark the SQL editor puts on it).
   * Compact on purpose: it sits in the results pane, under the grid's chrome.
   */
  import { EditorView, Decoration } from '@codemirror/view'
  import CircleAlert from '@lucide/svelte/icons/circle-alert'
  import Copy from '@lucide/svelte/icons/copy'
  import Check from '@lucide/svelte/icons/check'
  import Wand2 from '@lucide/svelte/icons/wand-2'
  import CodeEditor from './CodeEditor.svelte'
  import { errorSpan, lineColumn, cleanErrorMessage } from '$lib/sql-error-range.js'

  let {
    /** The error as the backend reported it. */
    error = '',
    /** The statement that failed, as it was sent ('' when unknown). */
    sql = '',
    /** 1-based character position of the failure in `sql`, when the database gave one. */
    position = /** @type {number | null} */ (null),
    queryMs = 0,
    dialect = 'postgres',
    copied = false,
    oncopy = () => {},
    /** @type {(() => void) | undefined} */
    onfixwithai = undefined,
    /** Extra guidance under the message (timeouts, quoting). @type {import('svelte').Snippet | undefined} */
    children = undefined,
  } = $props()

  const message = $derived(cleanErrorMessage(error))
  const span = $derived(sql ? errorSpan(sql, position) : null)
  const where = $derived(sql && position ? lineColumn(sql, /** @type {{ from: number }} */ (span).from) : null)

  const errorMark = $derived.by(() => {
    if (!span || span.to <= span.from) return []
    return [
      EditorView.decorations.of(Decoration.set([Decoration.mark({ class: 'cm-sql-run-error' }).range(span.from, span.to)])),
      EditorView.theme({
        '.cm-sql-run-error': {
          textDecoration: 'underline wavy color-mix(in oklch, var(--destructive) 85%, transparent)',
          textUnderlineOffset: '3px',
          backgroundColor: 'color-mix(in oklch, var(--destructive) 12%, transparent)',
          borderRadius: '2px',
        },
        '.cm-content': { padding: '4px 0' },
      }),
    ]
  })
</script>

<!-- Selectable (the app is select-none by default). -->
<div data-studio-selectable="text" class="flex h-full min-h-0 flex-col">
  <div class="flex h-8 shrink-0 select-none items-center gap-1.5 border-b border-border/60 pl-3 pr-1.5">
    <CircleAlert class="size-3.5 shrink-0 text-destructive" />
    <span class="text-ui-2xs font-medium text-foreground">Query failed</span>
    {#if queryMs > 0}
      <span class="text-ui-3xs tabular-nums text-muted-foreground">after {queryMs}ms</span>
    {/if}
    <div class="ml-auto flex shrink-0 items-center gap-1">
      <button
        type="button"
        class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={oncopy}
        title="Copy error"
        aria-label="Copy error"
      >
        {#if copied}<Check class="size-3.5 shrink-0" />{:else}<Copy class="size-3.5 shrink-0" />{/if}
      </button>
      {#if onfixwithai}
        <button
          type="button"
          class="inline-flex h-6 items-center gap-1 rounded-md border border-border px-2 text-ui-2xs text-foreground transition-colors hover:bg-accent"
          onclick={onfixwithai}
        >
          <Wand2 class="size-3 shrink-0" />
          Fix with AI
        </button>
      {/if}
    </div>
  </div>

  <div class="min-h-0 flex-1 overflow-auto px-3 py-2.5">
    <p class="flex items-baseline gap-2 font-mono text-ui-2xs leading-relaxed">
      <span class="shrink-0 font-semibold tracking-wide text-destructive">ERROR</span>
      <span class="min-w-0 select-text whitespace-pre-wrap [overflow-wrap:anywhere] text-foreground/90">{message}</span>
    </p>
    {#if where}
      <p class="mt-0.5 pl-[calc(5ch+0.5rem)] font-mono text-ui-3xs text-muted-foreground">line {where.line}, column {where.column}</p>
    {/if}
    {#if sql}
      <div class="mt-2 max-h-48 overflow-auto rounded-md border border-border/60 bg-background/40">
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
    {@render children?.()}
  </div>
</div>
