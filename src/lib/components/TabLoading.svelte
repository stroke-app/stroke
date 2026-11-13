<script>
  import Loader from "@lucide/svelte/icons/loader";
  import { EASTER_EGGS } from "$lib/easter-eggs.js";
  import { nextLoadingMessage } from "$lib/loading-messages.js";
  // Fallback shown while a lazy tab chunk downloads, so switching to a tab for
  // the first time fades to a spinner over the app background instead of a
  // black flash. Fills its container.
  let { label = "" } = $props();

  // With no label there is nothing to read while the chunk lands - show a
  // credit instead, same pool the table loader draws from.
  const message = label || nextLoadingMessage(EASTER_EGGS);

  // Held back like the grid's loader: a chunk that lands in a frame or two
  // (most of them) never flashes a spinner and a line of text on its way past.
  // The box itself is there from the start, so nothing shifts when it fills.
  let shown = $state(false);
  $effect(() => {
    const t = setTimeout(() => (shown = true), 200);
    return () => clearTimeout(t);
  });
</script>

<div
  class="flex min-h-0 flex-1 flex-col items-center justify-center bg-background"
  role="status"
  aria-live="polite"
  aria-busy="true"
>
  {#if shown}
    <Loader class="size-5 shrink-0 animate-spin text-muted-foreground" />
    <span class="mt-2 text-ui-xs text-muted-foreground">{message}</span>
  {/if}
</div>
