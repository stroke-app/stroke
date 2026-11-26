<script>
  /**
   * Draws the request from `askConfirm()` (`$lib/stores/confirm.js`). Mounted
   * once in App.svelte. Closing the dialog any way but the confirm button
   * (Cancel, Escape, the overlay) answers false.
   */
  import ConfirmDialog from './ConfirmDialog.svelte'
  import { pendingConfirm, answerConfirm } from '$lib/stores/confirm.js'

  let open = $state(false)
  let confirmed = false

  $effect(() => {
    if ($pendingConfirm) {
      confirmed = false
      open = true
    }
  })
  $effect(() => {
    if (!open && $pendingConfirm) answerConfirm(confirmed)
  })
</script>

{#if $pendingConfirm}
  <ConfirmDialog
    bind:open
    icon={$pendingConfirm.destructive ? 'alert-triangle' : 'alert-circle'}
    title={$pendingConfirm.title}
    description={$pendingConfirm.description ?? ''}
    confirmLabel={$pendingConfirm.confirmLabel ?? 'Continue'}
    confirmIcon={$pendingConfirm.destructive ? 'trash-2' : 'check'}
    variant={$pendingConfirm.destructive ? 'destructive' : 'default'}
    onconfirm={() => { confirmed = true }}
  />
{/if}
