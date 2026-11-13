<script>
  /**
   * One frame for the Create trigger / sequence / table dialogs: a title with a
   * muted subtitle and the close ×, the form, a live SQL preview, and a footer
   * with "Open in SQL editor" on the left and Cancel + the primary action on
   * the right. Enter in a text field submits when the form is valid; Esc closes.
   */
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import DdlSqlPreview from './DdlSqlPreview.svelte'
  import { cn } from '$lib/utils.js'
  import SquareTerminal from '@lucide/svelte/icons/square-terminal'
  import Loader2 from '@lucide/svelte/icons/loader-2'

  let {
    open = $bindable(false),
    title = '',
    subtitle = '',
    /** Statement(s) shown in the preview; may hold `‹placeholders›`. */
    sql = '',
    canSubmit = false,
    busy = false,
    submitLabel = 'Create',
    busyLabel = 'Creating…',
    /** Width utility for the panel. */
    size = 'sm:max-w-xl',
    onsubmit = () => {},
    /** Opens `sql` in a new editor tab; the footer action is hidden without it. @type {(() => void) | null} */
    onopensql = null,
    /** @type {(open: boolean) => void} */
    onOpenChange = () => {},
    children,
  } = $props()

  let previewOpen = $state(true)

  /**
   * Enter in a single-line field submits; buttons, menus and textareas keep
   * theirs. It calls `onsubmit` even while the form is incomplete: the dialog
   * then shows what is missing inline instead of the button just staying grey.
   * @param {KeyboardEvent} e
   */
  function onkeydown(e) {
    if (e.key !== 'Enter' || e.isComposing || e.shiftKey || e.altKey) return
    const el = /** @type {HTMLElement} */ (e.target)
    if (!(el instanceof HTMLInputElement) || el.type === 'checkbox') return
    e.preventDefault()
    if (!busy) onsubmit()
  }
</script>

<Dialog.Root bind:open {onOpenChange}>
  <Dialog.Content
    class={cn('flex max-h-[min(88vh,52rem)] flex-col gap-0 overflow-hidden p-0', size)}
    {onkeydown}
  >
    <Dialog.Header class="shrink-0 gap-1 px-5 pt-4 pb-3 pr-12">
      <Dialog.Title class="text-ui-lg">{title}</Dialog.Title>
      <Dialog.Description class="text-ui-xs text-muted-foreground">
        {#if subtitle}{subtitle}{:else}<span class="sr-only">{title}</span>{/if}
      </Dialog.Description>
    </Dialog.Header>

    <div class="app-scroll flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-5 pt-1 pb-4">
      {@render children?.()}
      <DdlSqlPreview {sql} bind:expanded={previewOpen} />
    </div>

    <div class="flex shrink-0 items-center gap-2 border-t border-border/60 px-5 py-3">
      {#if onopensql}
        <Button variant="ghost" size="sm" class="-ml-1.5 text-muted-foreground" onclick={() => onopensql?.()}>
          <SquareTerminal class="size-3.5 shrink-0" aria-hidden="true" />
          Open in SQL editor
        </Button>
      {/if}
      <div class="ml-auto flex items-center gap-2">
        <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
        <Button disabled={!canSubmit || busy} onclick={() => onsubmit()}>
          {#if busy}
            <Loader2 class="size-3.5 shrink-0 animate-spin" aria-hidden="true" />
            {busyLabel}
          {:else}
            {submitLabel}
          {/if}
        </Button>
      </div>
    </div>
  </Dialog.Content>
</Dialog.Root>
