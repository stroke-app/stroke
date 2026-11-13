<script>
  import ConfirmDialog from './ConfirmDialog.svelte'

  let {
    open = $bindable(false),
    /** Database being switched to. */
    databaseName = '',
    /** Database currently connected to, for the "from → to" framing. */
    currentName = '',
    onconfirm = () => {},
  } = $props()
</script>

<!-- Switching is cheap to undo (switch back), so Enter confirms. -->
<ConfirmDialog
  bind:open
  icon="arrow-left-right"
  title={databaseName ? `Switch to ${databaseName}?` : 'Switch database?'}
  confirmLabel="Switch"
  confirmIcon=""
  confirmOnEnter
  {onconfirm}
>
  {#snippet body()}
    {#if currentName && currentName !== databaseName}
      You'll leave <span class="font-medium text-foreground">{currentName}</span>, and its open tabs will close.
    {:else}
      The current connection and its open tabs will close.
    {/if}
  {/snippet}
</ConfirmDialog>
