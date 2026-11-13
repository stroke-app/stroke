<script>
  import ConfirmDialog from './ConfirmDialog.svelte'

  let {
    open = $bindable(false),
    title = '',
    description = '',
    /** One or more SQL statements to preview */
    sql = /** @type {string | string[]} */ (''),
    confirmLabel = 'Apply',
    /** 'destructive' turns the confirm button red */
    variant = /** @type {'default' | 'destructive'} */ ('default'),
    loading = false,
    onconfirm = () => {},
  } = $props()

  const sqlLines = $derived(Array.isArray(sql) ? sql : [sql])
  const isDestructive = $derived(variant === 'destructive')
</script>

{#snippet extra()}
  <!-- The statements themselves: confirming DDL blind is how you drop the wrong
       column. Each one is its own row so a multi-statement change is countable. -->
  <div class="px-5 py-4">
    <p class="mb-2 text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground">SQL preview</p>
    <div class="rounded-lg border border-border/20 bg-muted/[0.3] px-3.5 py-3">
      {#each sqlLines as line, i}
        <div class="{i > 0 ? 'mt-2 pt-2 border-t border-border/15' : ''} flex items-start gap-2.5">
          <span class="mt-[5px] size-1.5 shrink-0 rounded-full {isDestructive ? 'bg-destructive/50' : 'bg-muted-foreground/25'}"></span>
          <code class="min-w-0 break-all font-mono text-ui-xs text-foreground/80">{line}</code>
        </div>
      {/each}
    </div>
  </div>
{/snippet}

<ConfirmDialog
  bind:open
  icon={isDestructive ? 'alert-triangle' : 'terminal'}
  {title}
  {description}
  {confirmLabel}
  confirmIcon={loading ? 'loader-2' : isDestructive ? 'trash-2' : 'check'}
  {variant}
  disabled={loading}
  {extra}
  {onconfirm}
/>
