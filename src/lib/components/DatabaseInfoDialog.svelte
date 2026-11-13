<script>
  // Read-only properties for one database: owner, encoding, collation, size,
  // open sessions. The shell runs the catalog query and hands the rows over, so
  // this file has no engine knowledge in it.
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import Icon from './Icon.svelte'

  let {
    open = $bindable(false),
    name = '',
    /** @type {import('$lib/database-admin.js').DbInfoRow[]} */
    rows = [],
    loading = false,
    error = '',
  } = $props()
</script>

<Dialog.Root bind:open>
  <Dialog.Portal>
    <Dialog.Overlay />
    <Dialog.Content showCloseButton={false} class="w-[min(27.5rem,calc(100vw-2rem))] gap-0 overflow-hidden p-0 sm:max-w-none">

      <div class="flex items-start gap-3.5 border-b border-border/25 px-5 pt-5 pb-4">
        <div class="mt-px flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted/50">
          <Icon name="database" class="size-3.5 shrink-0 text-muted-foreground" />
        </div>
        <div class="min-w-0 flex-1">
          <Dialog.Title class="truncate font-mono text-ui-sm font-semibold text-foreground">{name}</Dialog.Title>
          <p class="mt-0.5 text-ui-2xs text-muted-foreground">Database properties</p>
        </div>
        <Dialog.Close class="inline-flex size-6 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground focus-visible:outline-none" />
      </div>

      <div class="loading-reveal px-5 py-4">
        {#if loading}
          <p class="py-6 text-center text-ui-xs text-muted-foreground">Reading the catalog…</p>
        {:else if error}
          <p class="rounded-lg border border-destructive/40 bg-destructive/10 px-3.5 py-2.5 text-ui-2xs text-destructive">{error}</p>
        {:else if rows.length === 0}
          <p class="py-6 text-center text-ui-xs text-muted-foreground">No properties returned.</p>
        {:else}
          <dl class="divide-y divide-border/20">
            {#each rows as row (row.label)}
              <div class="flex items-baseline justify-between gap-4 py-2">
                <dt class="shrink-0 text-ui-2xs text-muted-foreground">{row.label}</dt>
                <dd class="min-w-0 truncate text-right font-mono text-ui-xs text-foreground/85" title={row.value}>{row.value}</dd>
              </div>
            {/each}
          </dl>
        {/if}
      </div>

      <div class="flex items-center justify-end border-t border-border/25 px-5 py-3">
        <button type="button"
          class="inline-flex h-8 items-center rounded-md px-3.5 text-ui-sm text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground"
          onclick={() => (open = false)}>Close</button>
      </div>

    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
