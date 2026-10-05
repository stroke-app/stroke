<script>
  // Rename a database, or copy it under a new name. Both are "pick a name, run
  // one statement", and both want the same collision check and the same SQL
  // preview, so they share a dialog and differ only in wording.
  import { untrack } from 'svelte'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import Icon from './Icon.svelte'
  import { validateDbName, renameDatabaseSql, duplicateDatabaseSql } from '$lib/database-admin.js'

  let {
    open = $bindable(false),
    /** @type {'rename' | 'duplicate'} */
    mode = 'rename',
    /** @type {import('$lib/database-admin.js').AdminKind} */
    kind = 'postgres',
    /** The database being renamed or copied. */
    source = '',
    /** Every database on the server, for the "already exists" check. @type {string[]} */
    existing = [],
    /** Run the statement. Throwing keeps the dialog open with the message.
     *  @type {(args: { sql: string, name: string }) => Promise<void>} */
    onsubmit = async () => {},
  } = $props()

  let name = $state('')
  let saving = $state(false)
  let error = $state('')
  /** @type {HTMLInputElement | null} */
  let inputEl = $state(null)

  const isRename = $derived(mode === 'rename')
  const trimmed = $derived(name.trim())
  // The source name is not a collision for a rename - it is the current value.
  const others = $derived(isRename ? existing.filter((e) => e !== source) : existing)
  const nameError = $derived(trimmed ? validateDbName(trimmed, others) : '')
  const isValid = $derived(!!trimmed && !nameError && trimmed !== source)

  const sql = $derived.by(() => {
    const target = trimmed || (isRename ? 'new_name' : `${source}_copy`)
    try {
      return isRename ? renameDatabaseSql(kind, source, target) : duplicateDatabaseSql(kind, source, target)
    } catch (e) {
      return String(e)
    }
  })

  // Seed the field once per opening, and only then.
  //
  // This was two effects, and the second one read `name` to decide whether to
  // prefill it - so clearing the field re-ran it and typed the old name straight
  // back in. Backspacing looked like the input fighting the keyboard. `wasOpen`
  // is a plain variable rather than state (nothing renders it) and the writes
  // are untracked, so this effect depends on `open` alone: it runs once when the
  // dialog opens, once when it closes, and never while typing.
  let wasOpen = false
  $effect(() => {
    if (!open) {
      wasOpen = false
      untrack(() => {
        name = ''
        error = ''
        saving = false
      })
      return
    }
    if (wasOpen) return
    wasOpen = true
    untrack(() => {
      // Renaming starts from the current name (the common case is a small edit),
      // copying from a suggestion that will not collide.
      name = isRename ? source : `${source}_copy`
      error = ''
      saving = false
    })
    setTimeout(() => inputEl?.select(), 0)
  })

  async function submit() {
    if (!isValid || saving) return
    saving = true
    error = ''
    try {
      await onsubmit({ sql, name: trimmed })
      open = false
    } catch (e) {
      error = String(e).replace(/^Error:\s*/i, '')
    } finally {
      saving = false
    }
  }

  const lbl = 'mb-1.5 block text-ui-3xs font-semibold uppercase tracking-[0.07em] text-muted-foreground'
  const inp = 'h-8 w-full rounded-lg border-2 border-border bg-muted/[0.4] px-3 font-mono text-ui-xs outline-none placeholder:text-muted-foreground focus:border-border/50 focus:ring-0'
</script>

<Dialog.Root bind:open>
  <Dialog.Portal>
    <Dialog.Overlay />
    <Dialog.Content showCloseButton={false} class="w-[min(28.75rem,calc(100vw-2rem))] gap-0 overflow-hidden p-0 sm:max-w-none">

      <div class="flex items-start gap-3.5 border-b border-border/25 px-5 pt-5 pb-4">
        <div class="mt-px flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted/50">
          <Icon name={isRename ? 'pencil' : 'copy'} class="size-3.5 text-muted-foreground" />
        </div>
        <div class="min-w-0 flex-1">
          <Dialog.Title class="text-ui-sm font-semibold text-foreground">{isRename ? 'Rename database' : 'Duplicate database'}</Dialog.Title>
          <p class="mt-0.5 text-ui-2xs break-all text-muted-foreground">
            {isRename ? 'Renames' : 'Copies'} <span class="font-mono text-muted-foreground">{source}</span>
            {isRename ? 'on this server' : 'with all of its data'}
          </p>
        </div>
        <Dialog.Close class="inline-flex size-6 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/50 hover:text-muted-foreground focus-visible:outline-none" />
      </div>

      <div class="flex flex-col gap-4 px-5 py-4">
        <div>
          <label for="dbname-input" class={lbl}>{isRename ? 'New name' : 'Name for the copy'}</label>
          <input
            id="dbname-input"
            bind:this={inputEl}
            type="text"
            bind:value={name}
            maxlength="63"
            autocomplete="off"
            spellcheck="false"
            class={inp}
            onkeydown={(e) => { if (e.key === 'Enter') void submit() }}
          />
          {#if nameError}
            <p class="mt-1.5 text-ui-2xs text-destructive">{nameError}</p>
          {:else if trimmed && trimmed === source}
            <p class="mt-1.5 text-ui-2xs text-muted-foreground">That is already its name</p>
          {/if}
        </div>

        {#if !isRename}
          <p class="rounded-lg border border-border/20 bg-muted/[0.25] px-3.5 py-2.5 text-ui-2xs leading-relaxed text-muted-foreground">
            The copy is taken server-side, so a large database takes a while and needs the disk space twice over.
          </p>
        {/if}

        <div>
          <p class="{lbl} mb-2">SQL preview</p>
          <!-- Wrapped, not scrolled: a confirmation you have to drag sideways to
               read is not a confirmation, and these names are long. -->
          <pre class="rounded-lg border border-border/20 bg-muted/[0.15] px-4 py-3 font-mono text-ui-2xs leading-relaxed break-all whitespace-pre-wrap text-muted-foreground">{sql}</pre>
        </div>

        {#if error}
          <p class="rounded-lg border border-destructive/40 bg-destructive/10 px-3.5 py-2.5 text-ui-2xs text-destructive">{error}</p>
        {/if}
      </div>

      <div class="flex items-center justify-end gap-2 border-t border-border/25 px-5 py-3">
        <button type="button"
          class="inline-flex h-8 items-center rounded-lg px-3.5 text-ui-sm text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground"
          onclick={() => (open = false)}>Cancel</button>
        <button type="button"
          class="inline-flex h-8 items-center gap-1.5 rounded-lg bg-foreground px-4 text-ui-sm font-medium text-background transition-opacity hover:opacity-85 disabled:opacity-40"
          disabled={!isValid || saving}
          onclick={() => void submit()}>
          {#if saving}<Icon name="loader-2" class="size-3.5 animate-spin" />{/if}
          {isRename ? 'Rename' : 'Duplicate'}
        </button>
      </div>

    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
