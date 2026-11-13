<script>
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import { quoteIdent } from '$lib/dml-preview.js'

  let {
    open = $bindable(false),
    /** Existing schema names - used to flag duplicates. @type {string[]} */
    existingSchemas = [],
    /** Run the DDL against the database. @type {(sql: string) => Promise<unknown>} */
    onexecute = async () => {},
    /** Called after a successful create with the new schema name. @type {(name: string) => void | Promise<void>} */
    oncreated = async () => {},
  } = $props()

  const PLACEHOLDER = 'analytics'

  let name = $state('')
  let saving = $state(false)

  const trimmed = $derived(name.trim())
  const duplicate = $derived(existingSchemas.includes(trimmed))
  const isValid = $derived(trimmed.length > 0 && !duplicate)

  // The identifier is quoted (embedded quotes doubled), so any name is safe to
  // send. With nothing typed yet the preview shows the placeholder, dimmed like
  // the field's own placeholder, instead of a made-up `schema_name`.
  const sql = $derived(`CREATE SCHEMA ${quoteIdent(trimmed || PLACEHOLDER, 'postgres')};`)

  $effect(() => {
    if (!open) {
      name = ''
      saving = false
    }
  })

  // The SQL is on screen before the button, and CREATE SCHEMA changes nothing
  // that exists, so this runs on click - the same as New table. It used to open
  // a second dialog that repeated the preview it had just shown.
  async function handleCreate() {
    if (!isValid || saving) return
    saving = true
    try {
      await onexecute(sql)
      const created = trimmed
      open = false
      await oncreated(created)
    } catch {
      // onexecute surfaces its own toast on failure
    } finally {
      saving = false
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="flex flex-col gap-0 overflow-hidden p-0 sm:max-w-md">
    <Dialog.Header class="shrink-0 gap-1 px-6 pt-6 pb-4">
      <Dialog.Title>New schema</Dialog.Title>
      <Dialog.Description class="text-ui-xs text-muted-foreground">
        A namespace for tables, views and functions.
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-5 px-6 pb-5">
      <div class="flex flex-col gap-1.5">
        <label class="text-ui-xs font-medium text-foreground/80" for="create-schema-name">Schema name</label>
        <!-- svelte-ignore a11y_autofocus -->
        <Input
          id="create-schema-name"
          class="h-9 font-mono text-ui-sm placeholder:text-muted-foreground/50"
          placeholder={PLACEHOLDER}
          bind:value={name}
          autofocus
          autocomplete="off"
          spellcheck="false"
          aria-invalid={duplicate || undefined}
          aria-describedby={duplicate ? 'create-schema-error' : undefined}
          onkeydown={(e) => e.key === 'Enter' && handleCreate()}
        />
        {#if duplicate}
          <p id="create-schema-error" class="text-ui-2xs text-destructive">
            A schema named <span class="font-mono">{trimmed}</span> already exists.
          </p>
        {/if}
      </div>

      <div class="flex flex-col gap-1.5">
        <span class="text-ui-xs font-medium text-foreground/80">SQL preview</span>
        <pre
          class="overflow-x-auto rounded-lg border border-border/60 bg-muted/30 px-3.5 py-3 font-mono text-ui-2xs leading-relaxed whitespace-pre-wrap break-all {trimmed ? 'text-muted-foreground' : 'text-muted-foreground/50'}"
        >{sql}</pre>
      </div>
    </div>

    <div class="flex shrink-0 items-center justify-end gap-2 border-t border-border/60 px-6 py-3.5">
      <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
      <Button disabled={!isValid || saving} onclick={handleCreate}>
        {#if saving}
          <Loader2 class="size-3.5 shrink-0 animate-spin" aria-hidden="true" />
          Creating…
        {:else}
          Create schema
        {/if}
      </Button>
    </div>
  </Dialog.Content>
</Dialog.Root>
