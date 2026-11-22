<script>
  // A table that failed to load, stated where its grid would be: what broke,
  // why, and the action that fixes it (reconnect for a dropped pool, retry for
  // everything else). Split out of StudioShell.
  import AlertTriangle from '@lucide/svelte/icons/triangle-alert'
  import WifiOff from '@lucide/svelte/icons/wifi-off'
  import Unplug from '@lucide/svelte/icons/unplug'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import { connectionErrorKind } from '$lib/utils.js'
  import { humanizeDbError } from '$lib/db-error-text.js'

  let {
    error = '',
    connectionLost = false,
    retrying = false,
    onretry = () => {},
    ondismiss = () => {},
  } = $props()

  const kind = $derived(connectionErrorKind(error))
  const humanized = $derived(humanizeDbError(error))
  /** Showing the raw driver text. A new failure starts collapsed. */
  let showRawError = $state(false)
  $effect(() => { void error; showRawError = false })
</script>

<!-- ── Load failure, stated where the grid would be ────────────────
     A banner pinned to the top over an empty grid made the reader
     look in two places and left them with "dismiss to continue" as
     the only way forward. One centred state instead: what broke, why,
     and the action that fixes it - reconnect for a dropped pool,
     retry for everything else. -->
<div
  class="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 p-8 text-center"
  role="alert"
>
  {#if kind === 'unreachable'}
    <WifiOff class="size-8 shrink-0 text-muted-foreground" />
  {:else if kind === 'dropped'}
    <Unplug class="size-8 shrink-0 text-muted-foreground" />
  {:else}
    <AlertTriangle class="size-8 shrink-0 text-destructive" />
  {/if}

  <div class="flex max-w-md flex-col gap-1">
    <p class="text-ui-sm font-medium text-foreground">
      {#if kind === 'unreachable'}Cannot reach database
      {:else if kind === 'dropped'}Connection dropped
      {:else}This table failed to load{/if}
    </p>
    <!-- Drivers wrap the cause in transport noise - D1 returns its whole
         HTTP envelope around a five-word message. Show the cause; the
         raw text stays one click away and in the query log. -->
    <p class="break-words font-mono text-ui-xs leading-relaxed {kind ? 'text-muted-foreground' : 'text-destructive'}">
      {#if kind === 'unreachable'}Check your internet connection, or whether the server is still accepting connections.
      {:else if kind === 'dropped'}The pool for this connection closed. Reconnecting rebuilds it and reloads the table.
      {:else}{humanized}{/if}
    </p>
    {#if showRawError}
      <p class="mt-1 max-h-40 overflow-auto break-all text-left font-mono text-ui-3xs text-muted-foreground">{error}</p>
    {/if}
  </div>

  <div class="flex items-center gap-2">
    <button
      type="button"
      data-autofocus
      disabled={retrying}
      class="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-muted/40 px-3 text-ui-xs font-medium text-foreground transition-colors hover:bg-accent disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-ring"
      onclick={onretry}
    >
      <RefreshCw class="size-3.5 shrink-0 {retrying ? 'animate-spin' : ''}" />
      {kind === 'dropped' || connectionLost ? 'Reconnect' : 'Retry'}
    </button>
    {#if !kind}
      <button
        type="button"
        class="inline-flex h-8 items-center rounded-md px-3 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-ring"
        onclick={ondismiss}
      >Dismiss</button>
    {/if}
  </div>

  {#if humanized !== error.replace(/^Error:\s*/, '').trim()}
    <button
      type="button"
      class="text-ui-3xs text-muted-foreground underline-offset-2 transition-colors hover:text-foreground hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
      onclick={() => (showRawError = !showRawError)}
    >{showRawError ? 'Hide raw error' : 'Show raw error'}</button>
  {/if}
</div>
