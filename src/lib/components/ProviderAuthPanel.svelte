<script>
  /**
   * The frame the provider sign-in flows live in (Cloudflare and the OAuth
   * providers): sign in, waiting on the browser, loading, failed. One block
   * for every step, the same size each time, so a sign-in never resizes the
   * pane under the cursor.
   *
   * A quiet surface: the provider mark in a well, the copy beside it, one
   * action on the right, and an optional footer under a hairline for the way
   * out (browser didn't open, copy the link). While it waits, a ring turns
   * around the mark and, given `timeout`, a countdown says how long is left,
   * which is the one thing a person staring at a browser tab wants to know.
   */
  import { onDestroy } from 'svelte'
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import { cn } from '$lib/utils.js'

  let {
    /** 'idle' waits on the user · 'busy' waits on the network · 'error' failed. */
    tone = /** @type {'idle' | 'busy' | 'error'} */ ('idle'),
    title = '',
    subtitle = '',
    /** Quiet third line: the reassurance, not the instruction. */
    hint = '',
    /** Turn the ring around the mark, for a wait with no percentage to report. */
    progress = false,
    /** Seconds until the wait gives up: shown as a live "4:32 left". */
    timeout = 0,
    /** The provider logo, in a fixed well so every provider aligns. */
    mark = undefined,
    /** The one button for this step. */
    action = undefined,
    /** Under a hairline: the way out of a stuck step. */
    footer = undefined,
  } = $props()

  // The countdown starts when the panel shows the wait, which is when the
  // sign-in page opens; it is close enough to the server's own clock to be
  // honest, and it stops at zero instead of going negative.
  let left = $state(0)
  /** @type {ReturnType<typeof setInterval> | undefined} */
  let tick
  $effect(() => {
    clearInterval(tick)
    if (!timeout) { left = 0; return }
    const end = Date.now() + timeout * 1000
    left = timeout
    tick = setInterval(() => {
      left = Math.max(0, Math.round((end - Date.now()) / 1000))
      if (!left) clearInterval(tick)
    }, 1000)
    return () => clearInterval(tick)
  })
  onDestroy(() => clearInterval(tick))
  const clock = $derived(`${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')}`)
</script>

<section
  class={cn(
    'overflow-hidden rounded-lg border bg-muted/15',
    tone === 'error' ? 'border-destructive/30' : 'border-border/50',
  )}
  aria-busy={tone === 'busy'}
>
  <div class="flex items-start gap-3.5 p-4">
    <div class="relative size-10 shrink-0">
      <div
        class={cn(
          'flex size-10 items-center justify-center rounded-full',
          tone === 'error' ? 'bg-destructive/10' : 'bg-muted/60',
        )}
      >
        {#if mark}{@render mark()}{:else}<Loader2 class="size-4 shrink-0 animate-spin text-muted-foreground" />{/if}
      </div>
      {#if progress}
        <!-- A quarter arc turning round the mark: alive, and in one place. -->
        <svg class="ring-turn pointer-events-none absolute -inset-1 size-12" viewBox="0 0 48 48" aria-hidden="true">
          <circle cx="24" cy="24" r="22" fill="none" stroke="currentColor" stroke-width="1.5" class="text-border/60" />
          <circle cx="24" cy="24" r="22" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"
            stroke-dasharray="34 104" class="text-primary" />
        </svg>
      {/if}
    </div>

    <div class="min-w-0 flex-1 pt-0.5">
      <p class="text-ui-sm font-medium leading-snug text-foreground">{title}</p>
      {#if subtitle}
        <p class="mt-1 max-w-[68ch] text-pretty text-ui-xs leading-relaxed text-muted-foreground">{subtitle}</p>
      {/if}
      {#if timeout || hint}
        <p class="mt-2 flex items-center gap-2 text-ui-2xs text-muted-foreground">
          {#if timeout}
            <span class={cn('tabular-nums', left > 0 && left <= 60 && 'text-warning')} aria-live="off">
              {left > 0 ? `${clock} left` : 'Timing out'}
            </span>
          {/if}
          {#if timeout && hint}<span aria-hidden="true" class="size-0.5 rounded-full bg-muted-foreground/60"></span>{/if}
          {#if hint}<span class="min-w-0 truncate">{hint}</span>{/if}
        </p>
      {/if}
    </div>

    {#if action}
      <div class="shrink-0">{@render action()}</div>
    {/if}
  </div>

  {#if footer}
    <div class="flex min-h-10 flex-wrap items-center gap-x-1 gap-y-1 border-t border-border/40 bg-muted/20 px-4 py-1.5 text-ui-2xs text-muted-foreground">
      {@render footer()}
    </div>
  {/if}
</section>

<style>
  @keyframes ring-turn { to { transform: rotate(360deg); } }
  .ring-turn { animation: ring-turn 1.1s linear infinite; transform-origin: 50% 50%; }
  @media (prefers-reduced-motion: reduce) {
    .ring-turn { animation-duration: 3s; }
  }
</style>
