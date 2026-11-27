<script>
  import { cn } from '$lib/utils.js'
  import Loader from '@lucide/svelte/icons/loader'
  import { nextLoadingMessage } from '$lib/loading-messages.js'
  import { Button } from '$lib/components/ui/button/index.js'

  let {
    embedded = false,
    /** Hold the loader back 200ms (app.css .loading-reveal). Off where the caller already delays it. */
    delay = true,
    /** Offered once the wait runs long: lets go of the load. @type {(() => void) | null} */
    onstop = null,
  } = $props()

  const message = nextLoadingMessage()
  /** A read that has not answered in this long is probably not going to soon. */
  const SLOW_MS = 6000
  let slow = $state(false)
  $effect(() => {
    if (!onstop) return
    const t = setTimeout(() => (slow = true), SLOW_MS)
    return () => clearTimeout(t)
  })
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
    <p class="font-mono text-ui-sm text-muted-foreground">{slow ? 'Still waiting for the database…' : message}</p>
    {#if slow && onstop}
      <Button variant="outline" size="sm" class="mt-1" onclick={() => onstop?.()}>Stop</Button>
    {/if}
  </div>
</div>
