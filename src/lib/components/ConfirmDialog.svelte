<script>
  /**
   * The app's one alert dialog: "are you sure?" with a single confirm action.
   *
   * Every such dialog used to hand-roll its own width, padding and footer, so
   * they came out different sizes and their buttons sat at different insets.
   * One component means one geometry - change it here and every prompt in the
   * app moves together.
   */
  import Icon from './Icon.svelte'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { cn } from '$lib/utils.js'

  let {
    open = $bindable(false),
    /** Icon in the leading badge - an icon-registry name. */
    icon = 'alert-circle',
    title = '',
    /** Main question. Use the `body` snippet instead when it needs markup. */
    description = '',
    /** Consequence spelled out under the question, muted. */
    note = '',
    confirmLabel = 'Confirm',
    /** Icon on the confirm button; '' hides it. */
    confirmIcon = 'check',
    cancelLabel = 'Cancel',
    /** 'destructive' paints the confirm button red. */
    variant = /** @type {'default' | 'destructive'} */ ('default'),
    disabled = false,
    /**
     * Enter presses the confirm button, and nothing is focused when the dialog
     * opens so that it can. Opt in only where confirming by accident is cheap to
     * undo - it is off by default so a stray Return can never drop a table.
     */
    confirmOnEnter = false,
    onconfirm = () => {},
    oncancel = () => {},
    /** Optional rich body, rendered in place of `description`. */
    body = undefined,
    /** Optional full-width block below the header - toggles, SQL previews.
     *  Separated by a hairline so it reads as its own section. */
    extra = undefined,
  } = $props()

  function confirm() {
    if (disabled) return
    open = false
    onconfirm()
  }
  function cancel() {
    open = false
    oncancel()
  }

  /** @type {HTMLElement|null} */
  let contentEl = $state(null)

  /**
   * Focus the confirm button, not Cancel and not the dialog box.
   *
   * Cancel is the first focusable child, so the default would put focus on it
   * and Enter would cancel - the opposite of the confirm-on-Enter below.
   * Parking focus on the box avoided that but left nothing on screen looking
   * focused, so the dialog opened with no visible answer to "where am I".
   *
   * The confirm button is the default action and already says so with its ↵, so
   * it is the honest place for focus: Enter now activates it the ordinary way
   * (onkeydown below steps aside for a focused button, so it fires once), and
   * Escape still cancels.
   * @param {Event} e
   */
  function onOpenAutoFocus(e) {
    if (!confirmOnEnter) return
    e.preventDefault()
    const target = contentEl?.querySelector('[data-confirm-action]') ?? contentEl
    try { /** @type {HTMLElement|null} */ (target)?.focus({ preventScroll: true }) } catch { /* not focusable */ }
  }

  /**
   * Enter confirms - the default-button behaviour every native alert dialog has,
   * so a prompt can be answered without reaching for the mouse.
   *
   * Skipped wherever Enter already means something else: on a focused button the
   * browser clicks it (confirming here too would fire Cancel *and* confirm), and
   * in a textarea or contenteditable it inserts a newline. A plain input is left
   * alone deliberately - Enter submitting a one-line field is what people expect.
   * @param {KeyboardEvent} e
   */
  function onkeydown(e) {
    if (!confirmOnEnter) return
    if (e.key !== 'Enter' || e.isComposing) return
    if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return
    const el = e.target
    if (el instanceof HTMLElement && (el.closest('button, a[href], select, textarea') || el.isContentEditable)) return
    if (disabled) return
    e.preventDefault()
    confirm()
  }
</script>

<Dialog.Root bind:open>
  <!-- 440px: wide enough that a database or table name stays on one line, which
       is what made the old 380px prompts wrap mid-sentence. -->
  <Dialog.Content
    bind:ref={contentEl}
    tabindex={-1}
    showCloseButton={false}
    {onkeydown}
    {onOpenAutoFocus}
    class="w-[min(32.5rem,calc(100vw-2rem))] sm:max-w-none gap-0 overflow-hidden p-0"
  >
    <div class="flex items-start gap-3.5 px-5 pt-5 pb-4">
      <!-- The glyph alone (DESIGN_SYSTEM.md §11: no icon wells beside titles). -->
      <Icon
        name={icon}
        class={cn('mt-0.5 size-4 shrink-0', variant === 'destructive' ? 'text-destructive' : 'text-muted-foreground')}
        aria-hidden="true"
      />
      <div class="min-w-0 flex-1">
        <Dialog.Title class="text-ui-sm font-semibold text-foreground">{title}</Dialog.Title>
        {#if body}
          <div class="mt-1 text-ui-xs leading-[1.5] text-muted-foreground">{@render body()}</div>
        {:else if description}
          <p class="mt-1 text-ui-xs leading-[1.5] text-muted-foreground">{description}</p>
        {/if}
        {#if note}
          <p class="mt-2 text-ui-xs leading-[1.5] text-muted-foreground">{note}</p>
        {/if}
      </div>
    </div>

    {#if extra}
      <div class="border-t border-border/25">{@render extra()}</div>
    {/if}

    <div class="flex items-center justify-end gap-2 border-t border-border/25 px-5 py-3">
      <!-- Both buttons come out of the same component, so the pair is one
           system by construction: same height, same corner, same focus. They
           used to be two hand-rolled buttons that happened to sit next to each
           other, and it showed - one carried an icon and a boxed kbd chip, the
           other an ✕ that said nothing "Cancel" did not already say. -->
      <Button variant="outline" onclick={cancel}>{cancelLabel}</Button>
      <!-- focus: as well as focus-visible. onOpenAutoFocus sets focus in code and
           WebKitGTK does not always count that as :focus-visible, which is the
           only state the shared button styles draw a ring for - so the dialog
           opened with its default action focused and nothing showing it. -->
      <Button
        variant={variant === 'destructive' ? 'destructive' : 'default'}
        {disabled}
        onclick={confirm}
        data-confirm-action
        class="focus:outline-2 focus:outline-offset-0 focus:outline-ring"
      >
        {#if confirmIcon}<Icon name={confirmIcon} class="size-3.5 shrink-0" />{/if}
        {confirmLabel}
        <!-- Says the key out loud, so the shortcut is discoverable rather than
             folklore. A plain glyph, not a boxed kbd: a rectangle inside a pill
             is the shape mismatch that made this pair look assembled. -->
        {#if confirmOnEnter}
          <span class="ml-0.5 font-mono text-ui-2xs opacity-60" aria-hidden="true">↵</span>
        {/if}
      </Button>
    </div>
  </Dialog.Content>
</Dialog.Root>
