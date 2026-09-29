<script>
  /**
   * The "waiting on a database" state: a ring around the database's own mark,
   * what is being dialled, how long it has taken, and a way out.
   *
   * One look for every connect. The full-screen reconnect on launch and a
   * connect started from the connection dialog used to disagree: the dialog
   * only had a 12px spinner in its footer next to the form's host, which was
   * not the host being dialled when the connect came from a provider or a
   * saved entry.
   */
  import { fade } from 'svelte/transition'
  import { onDestroy } from 'svelte'
  import Logo from './Logo.svelte'
  import DbIcon from './DbIcon.svelte'

  let {
    /** DbIcon id (provider or engine). Empty shows the app mark. */
    icon = '',
    /** "Connecting" | "Reconnecting" */
    verb = 'Connecting',
    /** Connection name. */
    name = '',
    /** Where it is going, e.g. `host:5432`. Optional. */
    detail = '',
    /** Hold the ring and text back this long, so a fast connect never flashes. */
    delay = 350,
    /** @type {(() => void) | null} */
    oncancel = null,
  } = $props()

  const started = Date.now()
  let elapsed = $state(0)
  const timer = setInterval(() => { elapsed = Math.floor((Date.now() - started) / 1000) }, 1000)
  onDestroy(() => clearInterval(timer))
</script>

<div class="flex flex-col items-center gap-6 text-center">
  <div class="relative flex size-[88px] items-center justify-center" in:fade={{ delay, duration: 150 }}>
    <svg class="absolute inset-0 size-full motion-safe:animate-spin" viewBox="0 0 88 88" fill="none" aria-hidden="true">
      <circle cx="44" cy="44" r="42" stroke="currentColor" stroke-width="1.5" class="text-foreground/[0.07]" />
      <circle cx="44" cy="44" r="42" stroke="currentColor" stroke-width="1.5"
        stroke-dasharray="44 220" stroke-linecap="round" class="text-foreground/45" />
    </svg>
    <div class="flex size-[72px] items-center justify-center rounded-full border border-border/60 bg-card ring-1 ring-inset ring-white/[0.04] shadow-[0_10px_30px_-14px_rgba(0,0,0,0.7)]">
      {#if icon}
        <DbIcon id={icon} class="size-8 shrink-0" />
      {:else}
        <Logo class="size-9" />
      {/if}
    </div>
  </div>

  <div class="flex max-w-sm flex-col items-center gap-1" role="status" aria-live="polite" in:fade={{ delay, duration: 150 }}>
    <p class="max-w-full truncate text-ui-sm font-medium text-foreground/80">
      {verb}{name ? ` to ${name}` : ''}
    </p>
    {#if detail}
      <p class="max-w-full truncate font-mono text-ui-2xs text-muted-foreground" title={detail}>{detail}</p>
    {/if}
    <!-- Elapsed time from 2s: long enough that a normal connect never shows it,
         and past that a number says it is still working, not stuck. -->
    <p class="mt-1 h-4 text-ui-2xs tabular-nums text-muted-foreground/80">
      {#if elapsed >= 8}
        {elapsed}s · a far region can take a few seconds to answer
      {:else if elapsed >= 2}
        {elapsed}s
      {/if}
    </p>
    {#if oncancel}
      <button
        type="button"
        class="mt-2 rounded-md px-2 py-1 text-ui-2xs text-muted-foreground underline underline-offset-4 transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
        onclick={() => oncancel?.()}
      >
        Cancel
      </button>
    {/if}
  </div>
</div>
