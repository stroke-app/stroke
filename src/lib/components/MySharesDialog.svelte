<script>
  /**
   * Shared links as an overlay, for when no connection is open and so there is
   * no tab bar to open the page in. Connected, it opens as a tab.
   */
  import SharesPage from './SharesPage.svelte'

  let { open = $bindable(false) } = $props()
</script>

{#if open}
  <!-- Inset below the title bar and above the status bar, like the shortcuts
       overlay, so the window stays draggable. -->
  <div
    style="top: var(--app-titlebar-h, 38px); bottom: var(--app-statusbar-h, 0px);"
    class="fixed inset-x-0 z-50 flex flex-col bg-background"
    role="document"
  >
    <SharesPage active={open} onclose={() => (open = false)} />
  </div>
{/if}
