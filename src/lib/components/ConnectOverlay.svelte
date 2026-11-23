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
  import Icon from './Icon.svelte'

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
    /** No network: the caller stopped dialling and waits for it to come back. */
    offline = false,
    /** @type {(() => void) | null} */
    oncancel = null,
  } = $props()

  const started = Date.now()
  let elapsed = $state(0)
  const timer = setInterval(() => { elapsed = Math.floor((Date.now() - started) / 1000) }, 1000)
  onDestroy(() => clearInterval(timer))
</script>

<!-- Flat: the engine's logo on a quiet tile, the name, and a thin moving line
     under it. No ring, no shadow. -->
<div class="flex w-full max-w-sm flex-col items-center gap-5 text-center" in:fade={{ delay, duration: 150 }}>
  <div class="flex size-14 items-center justify-center rounded-lg bg-muted/70">
    {#if icon}
      <DbIcon id={icon} class="size-7 shrink-0" />
    {:else}
      <Logo class="size-8" />
    {/if}
  </div>

  <div class="flex w-full flex-col items-center gap-1" role="status" aria-live="polite">
    {#if offline}
      <!-- Waiting on the network, not on the database: no spinner and no
           clock, since nothing is being dialled. It goes on by itself. -->
      <p class="flex max-w-full items-center gap-1.5 text-ui-sm font-medium text-foreground">
        <Icon name="wifi-off" class="size-3.5 shrink-0 text-muted-foreground" />You're offline
      </p>
      <p class="max-w-72 text-ui-xs text-pretty text-muted-foreground">
        {name ? `${name} is` : "It's"} across the network. Stroke reconnects as soon as you're back online.
      </p>
    {:else}
      <p class="max-w-full truncate text-ui-sm font-medium text-foreground">
        {verb}{name ? ` to ${name}` : ''}
      </p>
      {#if detail}
        <p class="max-w-full truncate font-mono text-ui-2xs text-muted-foreground" title={detail}>{detail}</p>
      {/if}
    {/if}
  </div>

  <!-- Indeterminate: a short segment sliding along a hairline track. It keeps
       moving (slower) under reduced motion, like the app's other spinners. -->
  {#if !offline}
    <div class="relative h-0.5 w-40 overflow-hidden rounded-full bg-muted" aria-hidden="true">
      <span class="connect-sweep absolute inset-y-0 w-1/3 rounded-full bg-foreground/50"></span>
    </div>
  {/if}

  <!-- Elapsed time from 2s: long enough that a normal connect never shows it,
       and past that a number says it is still working, not stuck. -->
  <div class="flex h-7 items-center gap-3 text-ui-2xs text-muted-foreground tabular-nums">
    {#if offline}
      <!-- Nothing to time while waiting for the network. -->
    {:else if elapsed >= 8}
      <span>{elapsed}s · a far region can take a few seconds</span>
    {:else if elapsed >= 2}
      <span>{elapsed}s</span>
    {/if}
    {#if oncancel}
      <button
        type="button"
        class="inline-flex h-7 items-center rounded-md px-2.5 text-ui-xs font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
        onclick={() => oncancel?.()}
      >
        Cancel
      </button>
    {/if}
  </div>
</div>

<style>
  /* Transform, not `left`: a layout property animates on the main thread, so
     the line stopped dead whenever the app was busy, which is exactly when a
     connection lands and the studio mounts. Percentages are of the segment
     (a third of the track): -100% starts it just off the left, 300% ends it
     just off the right. */
  .connect-sweep {
    left: 0;
    will-change: transform;
    animation: connect-sweep 1.1s cubic-bezier(0.65, 0, 0.35, 1) infinite;
  }
  @keyframes connect-sweep {
    from { transform: translateX(-100%); }
    to { transform: translateX(300%); }
  }
  @media (prefers-reduced-motion: reduce) {
    .connect-sweep { animation-duration: 2.4s; }
  }
</style>
