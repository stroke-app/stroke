<script>
  // Dropping a database is the least reversible thing in the app, so the confirm
  // asks for the name typed back rather than a single click, and shows the exact
  // statement that will run.
  import ConfirmDialog from './ConfirmDialog.svelte'
  import { cn } from '$lib/utils.js'
  import { dropDatabaseSql } from '$lib/database-admin.js'

  let {
    open = $bindable(false),
    /** @type {import('$lib/database-admin.js').AdminKind} */
    kind = 'postgres',
    /** The engine can close other sessions as part of the drop (`canForceDrop`). */
    forceable = false,
    name = '',
    /** Number of sessions currently on it, when known - '' hides the line. */
    sessions = '',
    /** @type {(args: { sql: string, force: boolean }) => void} */
    onconfirm = () => {},
  } = $props()

  let typed = $state('')
  let force = $state(false)

  const supportsForce = $derived(forceable)
  const engineName = $derived(kind === 'mssql' ? 'SQL Server' : 'Postgres')
  const matches = $derived(typed.trim() === name)
  const sql = $derived(dropDatabaseSql(kind, name, { force: supportsForce && force }))

  $effect(() => {
    if (!open) {
      typed = ''
      force = false
    }
  })
</script>

{#snippet extra()}
  <div class="px-5 py-3.5">
    <label for="drop-db-confirm" class="mb-2 block text-ui-2xs text-muted-foreground">
      Type <span class="font-mono text-foreground">{name}</span> to confirm
    </label>
    <input
      id="drop-db-confirm"
      type="text"
      bind:value={typed}
      autocomplete="off"
      spellcheck="false"
      class={cn(
        'h-8 w-full rounded-lg border-2 bg-muted/[0.4] px-3 font-mono text-ui-xs outline-none placeholder:text-muted-foreground focus:ring-0',
        matches ? 'border-border/50' : 'border-destructive/40',
      )}
    />
  </div>

  {#if supportsForce}
    <div class="h-px bg-border/25"></div>
    <div class="flex items-center justify-between gap-4 px-5 py-3.5">
      <div class="min-w-0">
        <p class="text-ui-xs font-medium text-foreground">Close other sessions</p>
        <p class="mt-0.5 text-ui-2xs text-muted-foreground">
          {sessions ? `${sessions} session(s) connected. ` : ''}{engineName} refuses the drop while anything is connected.
        </p>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={force}
        aria-label="Close other sessions"
        class={cn('app-switch', force && '!bg-destructive')}
        onclick={() => (force = !force)}
      >
        <span class="app-switch-thumb"></span>
      </button>
    </div>
  {/if}

  <div class="h-px bg-border/25"></div>
  <div class="px-5 py-4">
    <p class="mb-2 text-ui-3xs font-semibold tracking-[0.06em] uppercase text-muted-foreground">Will execute</p>
    <div class="rounded-lg border border-border/20 bg-muted/[0.3] px-3.5 py-2.5">
      <code class="font-mono text-ui-xs break-all text-destructive">{sql}</code>
    </div>
  </div>
{/snippet}

<ConfirmDialog
  bind:open
  icon="alert-triangle"
  title="Drop database"
  description="Permanently removes the database and every table, view and row in it. This cannot be undone."
  confirmLabel="Drop database"
  confirmIcon="trash-2"
  variant="destructive"
  disabled={!matches}
  {extra}
  onconfirm={() => onconfirm({ sql, force: supportsForce && force })}
/>
