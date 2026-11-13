<script>
  import { cn } from '$lib/utils.js'
  import Loader from '@lucide/svelte/icons/loader'
  import { nextLoadingMessage } from '$lib/loading-messages.js'

  let {
    embedded = false,
    /** Hold the loader back 200ms (app.css .loading-reveal). Off where the caller already delays it. */
    delay = true,
  } = $props()

  const message = nextLoadingMessage()
</script>

<div
  class={cn(
    'flex w-full items-center justify-center',
    delay && 'loading-reveal',
    embedded ? 'h-40' : 'min-h-0 flex-1',
  )}
  role="status"
  aria-label="Loading"
>
  <div class="flex flex-col items-center gap-2.5">
    <Loader class="size-5 shrink-0 animate-spin text-muted-foreground" aria-hidden="true" />
    <p class="font-mono text-ui-sm text-muted-foreground">{message}</p>
  </div>
</div>
