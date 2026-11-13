<script>
  // The account menu's way to paste a sign-in code (it has no room inline).
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import PasteCodeForm from './PasteCodeForm.svelte'

  let { open = $bindable(false) } = $props()
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title>Paste sign-in code</Dialog.Title>
      <Dialog.Description>
        If the browser couldn't hand the sign-in back to Stroke, copy the code it shows and paste it here.
      </Dialog.Description>
    </Dialog.Header>
    <!-- The waiting sign-in resolves too, and its toast says "Signed in". -->
    <PasteCodeForm autofocus ondone={() => (open = false)} />
  </Dialog.Content>
</Dialog.Root>
