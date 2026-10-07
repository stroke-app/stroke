<script>
  // Read-only DDL tab: a bare code editor and nothing else.
  //
  // DDL used to open in a full SQL console, which meant a Run button, the query
  // toolbar and an empty results pane wrapped around a CREATE TABLE you cannot
  // usefully execute against the table it already describes. This surface is for
  // reading: the editor fills the tab, with a copy action and the object's name.
  import CodeTextView from './CodeTextView.svelte'
  import Icon from './Icon.svelte'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { saveExportAs } from '$lib/api.js'
  import { formatSql } from '$lib/format-sql.js'

  let {
    /** @type {string} */ ddl = '',
    /** @type {string} */ objectName = '',
    /** The connection's engine, for the formatter's dialect. @type {string} */ engine = '',
  } = $props()

  // Engines hand back DDL in whatever shape they stored it - SQLite returns the
  // statement exactly as typed, so a one-line CREATE TABLE stays one line.
  // formatSql returns its input unchanged if it can't parse, so a dialect it
  // doesn't understand degrades to the raw DDL rather than to mangled DDL.
  const pretty = $derived(formatSql(ddl, undefined, engine))

  let copied = $state(false)
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let copyTimer

  async function copyDdl() {
    try {
      await navigator.clipboard.writeText(pretty)
      copied = true
      clearTimeout(copyTimer)
      copyTimer = setTimeout(() => (copied = false), 1600)
    } catch (e) {
      toast.error('Could not copy', { description: String(e) })
    }
  }

  async function saveDdl() {
    try {
      const name = (objectName || 'schema').replace(/[^\w.-]+/g, '_')
      const path = await saveExportAs(pretty, `${name}.sql`, { name: 'SQL', extensions: ['sql'] })
      if (!path) return  // dialog cancelled
      toast.success('Exported DDL', { description: `Saved to ${path}` })
    } catch (e) {
      toast.error('Export failed', { description: String(e) })
    }
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border/50 px-3">
    <Icon name="file-text" class="size-3.5 shrink-0 text-muted-foreground" />
    <span class="min-w-0 truncate font-mono text-ui-xs text-muted-foreground">{objectName}</span>
    <span class="ml-auto flex shrink-0 items-center gap-1">
      <button
        type="button"
        onclick={saveDdl}
        title="Save as .sql"
        class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      >
        <Icon name="download" class="size-3.5 shrink-0" />
        Save
      </button>
      <button
        type="button"
        onclick={copyDdl}
        title="Copy DDL"
        class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      >
        <Icon name={copied ? 'check' : 'copy'} class="size-3.5 shrink-0" />
        {copied ? 'Copied' : 'Copy'}
      </button>
    </span>
  </div>
  <CodeTextView text={pretty} language="sql" wordWrap="on" />
</div>
