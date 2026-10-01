<script>
  /**
   * Review a write before it runs, as a dock under the grid.
   *
   * It used to be a centred dialog. That covered the rows being changed - the
   * one thing worth looking at while reading the SQL - and the dialog's
   * transform threw off Monaco's `position: fixed` suggestion list, which
   * landed far from the caret and over the footer buttons.
   *
   * Same editor as the cell dock (CellEditorPanel): CodeMirror, here in SQL
   * mode with keyword and column completion. The SQL is editable in place;
   * editing it switches Apply to run it exactly as written.
   */
  import { onMount } from 'svelte'
  import Icon from './Icon.svelte'
  import Kbd from './Kbd.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { cn } from '$lib/utils.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { appSqlEditor } from '$lib/stores/settings.js'
  import { get } from 'svelte/store'

  let {
    title = '',
    description = '',
    confirmLabel = 'Apply',
    destructive = false,
    /** The SQL that will run. Bound: editing it changes what Apply runs. */
    sql = $bindable(''),
    /** The generated SQL, for Revert. */
    originalSql = '',
    /** True once `sql` differs from `originalSql`. */
    edited = false,
    /** The app's Dialect id, for keywords. */
    dialect = '',
    /** Schemas, tables and columns to complete, passed through to CodeEditor. */
    sqlHints = {},
    running = false,
    onconfirm = () => {},
    oncancel = () => {},
  } = $props()

  /** @type {HTMLDivElement | null} */
  let root = $state(null)

  // Reading preferences, so they outlive one review. Until the bar's toggles
  // are used, they start from Settings → SQL editor.
  const WRAP_KEY = 'stroke:review-wrap'
  const GUTTER_KEY = 'stroke:review-gutter'
  /** @param {string} key @param {boolean} fallback */
  function loadPref(key, fallback) {
    try {
      const v = localStorage.getItem(key)
      return v === null ? fallback : v !== '0'
    } catch { return fallback }
  }
  /** @param {string} key @param {boolean} on */
  function savePref(key, on) {
    try { localStorage.setItem(key, on ? '1' : '0') } catch { /* private window, or storage is full */ }
  }
  let wrap = $state(loadPref(WRAP_KEY, get(appSqlEditor).wrap))
  let showGutter = $state(loadPref(GUTTER_KEY, get(appSqlEditor).lineNumbers))
  function toggleWrap() { wrap = !wrap; savePref(WRAP_KEY, wrap) }
  function toggleGutter() { showGutter = !showGutter; savePref(GUTTER_KEY, showGutter) }

  // Take focus so Escape and Mod+Enter work straight away. The panel, not the
  // editor or the Apply button: a stray keystroke should neither change the
  // SQL nor run it.
  onMount(() => root?.focus({ preventScroll: true }))

  /** Called by the parent when the same write is requested again. */
  export function focus() {
    root?.focus({ preventScroll: true })
  }

  function revert() {
    if (!running) sql = originalSql
  }

  /** @param {KeyboardEvent} e */
  function onRootKey(e) {
    // The editor handled it (Escape closing completion or find, Mod+Enter via editorKeys).
    if (e.defaultPrevented || running) return
    const mod = e.metaKey || e.ctrlKey
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      oncancel()
    } else if (e.key === 'Enter' && mod) {
      e.preventDefault()
      e.stopPropagation()
      onconfirm()
    } else if (e.altKey && !mod && (e.code === 'KeyZ' || e.code === 'KeyL')) {
      // Alt+Z / Alt+L when focus is on the bar rather than in the editor.
      e.preventDefault()
      if (e.code === 'KeyZ') toggleWrap()
      else toggleGutter()
    }
  }

  /** Handed to the editor so they win over its defaults - the cell dock's keys. */
  const editorKeys = [
    { key: 'Mod-Enter', run: () => { if (!running) onconfirm(); return true } },
    { key: 'Alt-z', run: () => { toggleWrap(); return true } },
    { key: 'Alt-l', run: () => { toggleGutter(); return true } },
    { key: 'Alt-r', run: () => { revert(); return true } },
  ]

  async function copy() {
    try {
      await navigator.clipboard.writeText(sql)
      toast.success('Copied SQL', { duration: 1800 })
    } catch (e) {
      toast.error('Could not copy', { description: String(e?.message ?? e) })
    }
  }

  const toolBtn = 'text-muted-foreground aria-pressed:bg-muted/60 aria-pressed:text-foreground'
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  bind:this={root}
  tabindex="-1"
  role="region"
  aria-label={title}
  class="flex h-full min-h-0 w-full flex-col bg-background outline-none"
  onkeydown={onRootKey}
>
  <div class="flex h-10 shrink-0 items-center gap-3 border-b border-border/40 px-3">
    <Icon name="file-diff" class="size-4 shrink-0 text-muted-foreground" />
    <!-- One shrinking group, so a narrow dock takes room from the description
         rather than pushing the actions out of the bar. -->
    <div class="flex min-w-0 flex-1 items-baseline gap-2 overflow-hidden">
      <span class="shrink-0 text-ui-xs font-semibold text-foreground">{title}</span>
      {#if edited}
        <span class="min-w-0 truncate text-ui-2xs text-warning">Edited · Apply runs it exactly as written</span>
      {:else if description}
        <span class={cn('min-w-0 truncate text-ui-2xs', destructive ? 'text-destructive' : 'text-muted-foreground')}>
          {description}
        </span>
      {/if}
    </div>

    <!-- View tools, then the decision. The rule between them keeps Revert from
         reading as the first of the two commit buttons. -->
    <div class="flex shrink-0 items-center gap-0.5">
      <Button
        variant="ghost"
        size="icon-sm"
        class={toolBtn}
        aria-pressed={wrap}
        aria-label="Soft wrap"
        title="Soft wrap (Alt+Z)"
        onclick={toggleWrap}
      >
        <Icon name="wrap-text" class="size-3.5" />
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        class={toolBtn}
        aria-pressed={showGutter}
        aria-label="Line numbers"
        title="{showGutter ? 'Hide' : 'Show'} line numbers (Alt+L)"
        onclick={toggleGutter}
      >
        <Icon name="list-ordered" class="size-3.5" />
      </Button>
      <Button variant="ghost" size="icon-sm" class={toolBtn} aria-label="Copy SQL" title="Copy SQL" onclick={copy}>
        <Icon name="copy" class="size-3.5" />
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        class={toolBtn}
        disabled={!edited || running}
        aria-label="Revert to the generated SQL"
        title="Revert to the generated SQL (Alt+R)"
        onclick={revert}
      >
        <Icon name="undo-2" class="size-3.5" />
      </Button>

      <span class="mx-2 h-4 w-px bg-border/70" aria-hidden="true"></span>

      <Button variant="ghost" size="sm" class="text-muted-foreground" disabled={running} title="Cancel (Esc)" onclick={oncancel}>
        Cancel
      </Button>
      <Button
        variant={destructive ? 'destructive' : 'default'}
        size="sm"
        class={cn(
          'ms-1 gap-1.5 pe-1.5',
          destructive
            ? '[&_kbd]:bg-destructive/15 [&_kbd]:text-destructive'
            : '[&_kbd]:bg-primary-foreground/15 [&_kbd]:text-primary-foreground/85',
        )}
        disabled={running}
        onclick={onconfirm}
      >
        {#if running}
          <Icon name="loader-2" class="size-3.5 animate-spin" />
        {:else}
          <Icon name={destructive ? 'trash-2' : 'check'} class="size-3.5" />
        {/if}
        {confirmLabel}
        <Kbd combo="Mod+Enter" class="ms-1" />
      </Button>
    </div>
  </div>

  <div class="flex min-h-0 flex-1 flex-col">
    {#await import('./CodeEditor.svelte') then { default: CodeEditor }}
      <CodeEditor
        bind:value={sql}
        lang="sql"
        {dialect}
        {sqlHints}
        {wrap}
        gutter={showGutter}
        folding={$appSqlEditor.folding}
        suggestWhileTyping={$appSqlEditor.suggestWhileTyping}
        readOnly={running}
        keys={editorKeys}
        ariaLabel="SQL to run"
      />
    {/await}
  </div>
</div>
