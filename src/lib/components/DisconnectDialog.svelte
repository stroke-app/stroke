<script>
  import ConfirmDialog from './ConfirmDialog.svelte'
  import { Button } from '$lib/components/ui/button/index.js'

  let {
    open = $bindable(false),
    connectionName = '',
    /** Labs: the other databases still open, by name. Disconnecting this one moves to one of them. */
    others = /** @type {string[]} */ ([]),
    ondisconnect = () => {},
    /** Close every open database at once. */
    ondisconnectall = () => {},
  } = $props()

  const question = $derived(
    connectionName ? `Disconnect from ${connectionName}?` : 'Disconnect from the current database?',
  )
  // With others open, Disconnect lands on the next one, so getting out of all
  // of them used to mean answering this same question once per database.
  const note = $derived(
    others.length === 1 ? `${others[0]} stays open, and Stroke moves to it.`
      : others.length > 1 ? `${others.length} other databases stay open, and Stroke moves to ${others.at(-1)}.`
      : '',
  )
</script>

{#snippet allRow()}
  <div class="flex items-center justify-between gap-3 px-5 py-3">
    <p class="text-ui-xs text-muted-foreground">Done with all of them?</p>
    <Button variant="outline" size="sm" onclick={() => { open = false; ondisconnectall() }}>
      Disconnect all
    </Button>
  </div>
{/snippet}

<ConfirmDialog
  bind:open
  icon="unplug"
  title="Disconnect"
  description={question}
  {note}
  confirmLabel="Disconnect"
  confirmIcon="unplug"
  variant="destructive"
  confirmOnEnter
  onconfirm={ondisconnect}
  extra={others.length ? allRow : undefined}
/>
