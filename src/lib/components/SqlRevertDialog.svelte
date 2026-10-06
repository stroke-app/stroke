<script>
  /**
   * Confirm a revert from a statement's lens. Shows the statement that ran and
   * the SQL the revert runs, one above the other, so what is about to happen is
   * read rather than inferred; then what it can't undo, and a button that names
   * the action ("Drop table", "Restore 6 rows"). sql-undo.js wrote the revert
   * when the statement ran.
   */
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import CodeEditor from './CodeEditor.svelte'
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert'
  import Info from '@lucide/svelte/icons/info'
  import Undo2 from '@lucide/svelte/icons/undo-2'
  import ArrowDown from '@lucide/svelte/icons/arrow-down'
  import { describeUndo } from '$lib/sql-undo.js'

  let {
    /** @type {import('$lib/sql-undo.js').UndoRecord | null} */
    undo = null,
    dialect = 'postgres',
    onconfirm = () => {},
    oncancel = () => {},
  } = $props()

  let open = $state(false)
  /** Answered once, however the dialog closes (Escape and the overlay included). */
  let settled = false
  $effect(() => {
    open = !!undo
    if (undo) settled = false
  })
  /** @param {boolean} ok */
  function finish(ok) {
    if (settled) return
    settled = true
    open = false
    if (ok) onconfirm()
    else oncancel()
  }

  const words = $derived(undo ? describeUndo(undo) : null)

  /** "just now", "4 min ago", else the time it ran. @param {number} at */
  function ranAgo(at) {
    const s = Math.round((Date.now() - at) / 1000)
    if (s < 45) return 'just now'
    if (s < 3600) return `${Math.max(1, Math.round(s / 60))} min ago`
    return new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  }

  /** The revert's SQL, its first lines when it is long (a 10,000-row revert is 20 statements). */
  const PREVIEW_LINES = 40
  const revertSql = $derived.by(() => {
    if (!undo) return ''
    const lines = undo.statements.join('\n\n').split('\n')
    if (lines.length <= PREVIEW_LINES) return lines.join('\n')
    return `${lines.slice(0, PREVIEW_LINES).join('\n')}\n-- ${(lines.length - PREVIEW_LINES).toLocaleString()} more lines not shown`
  })

  /** @type {HTMLElement | null} */
  let cancelEl = $state(null)
  /** Cancel takes focus: a stray Enter must never drop a table. @param {Event} e */
  function onOpenAutoFocus(e) {
    e.preventDefault()
    cancelEl?.focus({ preventScroll: true })
  }
</script>

<Dialog.Root bind:open={() => open, (v) => { if (!v) finish(false) }}>
  <Dialog.Content
    showCloseButton={false}
    {onOpenAutoFocus}
    class="w-[min(32.5rem,calc(100vw-2rem))] sm:max-w-none gap-0 overflow-hidden p-0"
  >
    {#if undo && words}
      <div class="flex flex-col gap-4 px-5 pt-5 pb-4">
        <div class="flex flex-col gap-1">
          <div class="flex items-baseline gap-2">
            <Dialog.Title class="min-w-0 flex-1 text-ui-sm font-semibold text-foreground">{words.title}</Dialog.Title>
            <span class="shrink-0 text-ui-2xs tabular-nums text-muted-foreground">ran {ranAgo(undo.at)}</span>
          </div>
          <Dialog.Description class="text-ui-xs leading-relaxed text-muted-foreground">{words.body}</Dialog.Description>
        </div>

        <!-- What ran, then what reverting runs: the pair reads top to bottom. -->
        <div class="flex flex-col">
          <span class="mb-1.5 text-ui-2xs font-medium text-muted-foreground">Ran</span>
          <div class="max-h-32 overflow-auto rounded-lg bg-muted/30 opacity-80">
            <CodeEditor value={undo.sql} readOnly lang="sql" {dialect} wrap gutter={false} folding={false} suggestWhileTyping={false} ariaLabel="Statement that ran" />
          </div>
          <div class="flex h-6 items-center ps-3 text-muted-foreground" aria-hidden="true">
            <ArrowDown class="size-3.5 shrink-0" />
          </div>
          <span class="mb-1.5 text-ui-2xs font-medium text-muted-foreground">
            Revert runs{#if undo.statements.length > 1}<span class="tabular-nums"> · {undo.statements.length} statements</span>{/if}
          </span>
          <div class="max-h-48 overflow-auto rounded-lg bg-muted/50">
            <CodeEditor value={revertSql} readOnly lang="sql" {dialect} wrap={false} gutter={false} folding={false} suggestWhileTyping={false} ariaLabel="Revert SQL" />
          </div>
        </div>

        {#if undo.warnings.length || words.note}
          <ul class="flex flex-col gap-2">
            {#each undo.warnings as w (w)}
              <li class="flex items-start gap-2 text-ui-xs leading-relaxed text-foreground/85">
                <TriangleAlert class="mt-0.5 size-3.5 shrink-0 text-warning" />
                <span>{w}</span>
              </li>
            {/each}
            {#if words.note}
              <li class="flex items-start gap-2 text-ui-xs leading-relaxed text-muted-foreground">
                <Info class="mt-0.5 size-3.5 shrink-0" />
                <span>{words.note}</span>
              </li>
            {/if}
          </ul>
        {/if}
      </div>

      <div class="flex items-center justify-end gap-2 border-t border-border/25 px-5 py-3">
        <Button bind:ref={cancelEl} variant="outline" onclick={() => finish(false)}>Cancel</Button>
        <Button
          variant={words.destructive ? 'destructive' : 'default'}
          onclick={() => finish(true)}
          class="focus:outline-2 focus:outline-offset-0 focus:outline-ring"
        >
          <Undo2 class="size-3.5 shrink-0" />
          {words.action}
        </Button>
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
