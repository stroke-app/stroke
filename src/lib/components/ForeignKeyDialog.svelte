<script>
  import FieldSelect from './FieldSelect.svelte';
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { executeSql } from '$lib/api.js'
  import Loader from '@lucide/svelte/icons/loader'
  import Link from '@lucide/svelte/icons/link'
  import ArrowRight from '@lucide/svelte/icons/arrow-right'
  import DdlConfirmDialog from './DdlConfirmDialog.svelte'

  /** @typedef {'NO ACTION'|'CASCADE'|'SET NULL'|'SET DEFAULT'|'RESTRICT'} FkAction */

  const FK_ACTIONS = /** @type {FkAction[]} */ (['NO ACTION', 'CASCADE', 'SET NULL', 'SET DEFAULT', 'RESTRICT'])

  let {
    open = $bindable(false),
    schema = '',
    table = '',
    column = '',
    constraintName = /** @type {string|null} */ (null),
    existingFk = /** @type {string|null} */ (null),
    tables = /** @type {{ name: string }[]} */ ([]),
    onrefresh = () => {},
  } = $props()

  /** @type {FkAction} */
  let onUpdate = $state('NO ACTION')
  /** @type {FkAction} */
  let onDelete = $state('NO ACTION')
  let refTable = $state('')
  let refColumn = $state('')
  let saving = $state(false)
  let deleting = $state(false)

  $effect(() => {
    if (!open) return
    if (existingFk) {
      const parts = existingFk.split('.')
      refTable = parts[1] ?? ''
      refColumn = parts[2] ?? ''
    } else {
      refTable = ''
      refColumn = ''
    }
    onUpdate = 'NO ACTION'
    onDelete = 'NO ACTION'
  })

  const isValid = $derived(refTable.trim().length > 0 && refColumn.trim().length > 0)

  async function handleSave() {
    if (!isValid) { toast.error('Referenced table and column are required'); return }
    saving = true
    try {
      if (constraintName) {
        await executeSql(`ALTER TABLE "${schema}"."${table}" DROP CONSTRAINT "${constraintName}"`)
      }
      const name = constraintName ?? `fk_${table}_${column}`
      await executeSql(
        `ALTER TABLE "${schema}"."${table}" ADD CONSTRAINT "${name}" ` +
        `FOREIGN KEY ("${column}") REFERENCES "${schema}"."${refTable}" ("${refColumn}") ` +
        `ON UPDATE ${onUpdate} ON DELETE ${onDelete}`
      )
      toast.success('Foreign key saved')
      open = false
      onrefresh()
    } catch (e) {
      toast.error('Could not save foreign key', { description: String(e) })
    } finally {
      saving = false
    }
  }

  let dropConfirmOpen = $state(false)

  async function executeDrop() {
    if (!constraintName) return
    deleting = true
    try {
      await executeSql(`ALTER TABLE "${schema}"."${table}" DROP CONSTRAINT "${constraintName}"`)
      toast.success('Foreign key removed')
      open = false
      onrefresh()
    } catch (e) {
      toast.error('Could not remove foreign key', { description: String(e) })
    } finally {
      deleting = false
    }
  }

  const lbl = 'mb-1.5 block text-ui-3xs font-semibold uppercase tracking-widest text-muted-foreground'
  const inp = 'h-9 w-full rounded-lg border-2 border-border bg-muted/[0.3] px-3 font-mono text-ui-sm text-foreground placeholder:text-muted-foreground outline-none focus:border-primary/40 focus:ring-1 focus:ring-primary/15 transition-colors'
  const sel = 'h-9 w-full appearance-none rounded-lg border border-border/30 bg-muted/[0.3] px-3 pr-7 font-mono text-ui-sm text-foreground outline-none focus:border-primary/40 transition-colors'

  /**
   * Action pill class - active uses primary styling for both sections
   * (destructive styling only for truly dangerous actions like CASCADE in On Delete).
   */
  function actionCls(a, active, section) {
    if (!active) return 'rounded-md px-2.5 py-1 font-mono text-ui-2xs font-medium text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground'
    const isDanger = section === 'delete' && (a === 'CASCADE')
    const isWarn = section === 'delete' && (a === 'SET NULL' || a === 'SET DEFAULT')
    if (isDanger) return 'rounded-md px-2.5 py-1 font-mono text-ui-2xs font-medium bg-destructive/10 text-destructive ring-1 ring-destructive/25'
    if (isWarn)   return 'rounded-md px-2.5 py-1 font-mono text-ui-2xs font-medium bg-warning/10 text-warning ring-1 ring-warning/25'
    return 'rounded-md px-2.5 py-1 font-mono text-ui-2xs font-medium bg-accent text-foreground ring-1 ring-border/60'
  }
</script>

<DdlConfirmDialog
  bind:open={dropConfirmOpen}
  title="Remove foreign key"
  description="Removes the foreign key constraint. Referential integrity will no longer be enforced."
  sql={constraintName ? `ALTER TABLE "${schema}"."${table}" DROP CONSTRAINT "${constraintName}"` : ''}
  confirmLabel="Remove FK"
  variant="destructive"
  loading={deleting}
  onconfirm={executeDrop}
/>

<Dialog.Root bind:open>
  <Dialog.Portal>
    <Dialog.Overlay />
    <Dialog.Content showCloseButton={false} class="w-[min(31.25rem,calc(100vw-2rem))] sm:max-w-none gap-0 overflow-hidden p-0">

      <!-- Header -->
      <div class="flex items-center gap-3 border-b border-border/25 px-5 py-3.5">
        <div class="flex size-7 shrink-0 items-center justify-center rounded-lg bg-muted/60">
          <Link class="size-3.5 shrink-0 text-muted-foreground" />
        </div>
        <div class="min-w-0 flex-1">
          <Dialog.Title class="text-ui-sm font-semibold leading-none text-foreground">
            {constraintName ? 'Edit foreign key' : 'Add foreign key'}
          </Dialog.Title>
          <p class="mt-1 truncate font-mono text-ui-2xs text-muted-foreground">
            {table}.<span class="text-foreground/60">{column}</span>
          </p>
        </div>
        <Dialog.Close class="ml-2 inline-flex size-6 shrink-0 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground focus-visible:outline-none" />
      </div>

      <!-- Body -->
      <div class="flex flex-col gap-5 px-5 py-4">

        <!-- FK path preview -->
        <div class="flex min-w-0 items-center gap-2 overflow-hidden rounded-lg border border-border/20 bg-muted/[0.18] px-3.5 py-2.5">
          <span class="min-w-0 shrink truncate font-mono text-ui-xs">
            <span class="text-muted-foreground">{schema}.</span><span class="text-foreground/70">{table}</span>.<span class="font-semibold text-foreground">{column}</span>
          </span>
          <ArrowRight class="size-3 shrink-0 text-muted-foreground" />
          {#if refTable && refColumn}
            <span class="min-w-0 shrink truncate font-mono text-ui-xs">
              <span class="text-muted-foreground">{schema}.</span><span class="text-foreground/70">{refTable}</span>.<span class="font-semibold text-info">{refColumn}</span>
            </span>
          {:else}
            <span class="shrink-0 font-mono text-ui-2xs text-muted-foreground italic">select table and column below</span>
          {/if}
        </div>

        <!-- Referenced table + column -->
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label for="fk-ref-table" class={lbl}>Referenced table</label>
            <div class="relative">
              <FieldSelect mono
                id="fk-ref-table"
                class="w-full text-ui-xs"
                bind:value={refTable}
                placeholder="Select table…"
                options={[{ value: '', label: 'Select table…' }, ...tables.map((t) => ({ value: t.name, label: t.name }))]}
              />
            </div>
          </div>
          <div>
            <label for="fk-ref-col" class={lbl}>Referenced column</label>
            <input id="fk-ref-col" type="text" bind:value={refColumn} placeholder="id" class={inp} />
          </div>
        </div>

        <!-- ON UPDATE + ON DELETE, stacked so each pill row has full width -->
        <div class="flex flex-col gap-3.5">
          <div>
            <p class={lbl}>On update</p>
            <div class="flex flex-wrap gap-1.5 pt-1">
              {#each FK_ACTIONS as a (a)}
                <button type="button" class={actionCls(a, onUpdate === a, 'update')} onclick={() => (onUpdate = a)}>{a}</button>
              {/each}
            </div>
          </div>
          <div class="border-t border-border/15 pt-3.5">
            <p class={lbl}>On delete</p>
            <div class="flex flex-wrap gap-1.5 pt-1">
              {#each FK_ACTIONS as a (a)}
                <button type="button" class={actionCls(a, onDelete === a, 'delete')} onclick={() => (onDelete = a)}>{a}</button>
              {/each}
            </div>
          </div>
        </div>

      </div>

      <!-- Footer -->
      <div class="flex items-center gap-2 border-t border-border/25 px-5 py-3">
        {#if constraintName}
          <button type="button" disabled={deleting || saving}
            class="mr-auto inline-flex h-8 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md px-3 text-ui-xs text-destructive transition-colors hover:bg-destructive/10 hover:text-destructive disabled:opacity-40"
            onclick={() => (dropConfirmOpen = true)}>
            {#if deleting}<Loader class="size-3 shrink-0 animate-spin" />{/if}
            Remove FK
          </button>
        {/if}
        <div class="ml-auto flex items-center gap-2">
          <button type="button"
            class="inline-flex h-8 items-center whitespace-nowrap rounded-md px-3.5 text-ui-sm text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground"
            onclick={() => (open = false)}>Cancel</button>
          <button type="button" disabled={!isValid || saving || deleting}
            class="inline-flex h-8 items-center gap-1.5 whitespace-nowrap rounded-lg bg-foreground px-4 text-ui-sm font-medium text-background transition-opacity hover:opacity-85 disabled:opacity-40"
            onclick={handleSave}>
            {#if saving}<Loader class="size-3 shrink-0 animate-spin" />{/if}
            {constraintName ? 'Update FK' : 'Add FK'}
          </button>
        </div>
      </div>

    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
