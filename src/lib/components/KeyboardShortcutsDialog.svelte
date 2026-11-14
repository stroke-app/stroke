<script>
  /**
   * The shortcuts page as an overlay, for when no connection is open and so
   * there is no tab bar to open it in. Connected, ⌘/ opens it as a tab.
   */
  import KeyboardShortcutsPage from "./KeyboardShortcutsPage.svelte";

  let { open = $bindable(false) } = $props();
</script>

{#if open}
  <!-- Inset below the title bar and above the status bar, like the connection
       manager. Covering the window chrome took the menu bar, the window controls
       and the drag region with it - so the one dialog that exists to teach you
       the app's keys hid the app while you read them, and left nothing to drag
       the window by. -->
  <div
    style="top: var(--app-titlebar-h, 38px); bottom: var(--app-statusbar-h, 0px);"
    class="fixed inset-x-0 z-50 flex flex-col bg-background"
    role="document"
  >
    <KeyboardShortcutsPage active={open} onclose={() => (open = false)} />
  </div>
{/if}
