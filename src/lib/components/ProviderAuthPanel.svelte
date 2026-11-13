<script>
  /**
   * The frame the provider sign-in flows live in.
   *
   * Cloudflare and the OAuth providers each used to draw their own idle CTA,
   * waiting card, loading row and error card. Four shapes per provider, none of
   * them the same size, so every step of a sign-in resized the pane under the
   * cursor.
   *
   * This is one block with four tones, and deliberately NOT a card: a bordered
   * box floating above the form it belongs to reads as an advert, not as the
   * first step of the form. It is a section - icon, copy, one action, a rule -
   * flush with the fields below it and the same width as them. The rule doubles
   * as the progress track, so a long wait can show life without a bar appearing
   * out of nowhere and pushing everything down.
   */
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import { cn } from '$lib/utils.js'

  let {
    /** 'idle' waits on the user · 'busy' waits on the network · 'error' failed. */
    tone = /** @type {'idle' | 'busy' | 'error'} */ ('idle'),
    title = '',
    subtitle = '',
    /** Quiet third line - the reassurance, not the instruction. */
    hint = '',
    /** Animate the rule, for a wait with no percentage to report. */
    progress = false,
    /** The provider logo, in a fixed well so every provider aligns. */
    mark = undefined,
    /** The one button for this step. */
    action = undefined,
  } = $props()
</script>

<section class="flex flex-col gap-3.5">
  <div class="flex items-start gap-3">
    <div
      class={cn(
        'relative mt-px flex size-8 shrink-0 items-center justify-center rounded-lg',
        tone === 'error' ? 'bg-destructive/10' : 'bg-muted/50',
      )}
    >
      {#if tone === 'busy'}
        <!-- Expanding ring, so waiting reads as progress and not as a stall. -->
        <span class="pulse-ring pointer-events-none absolute inset-0 rounded-lg ring-1 ring-primary/40"></span>
      {/if}
      {#if mark}{@render mark()}{:else}<Loader2 class="size-4 shrink-0 animate-spin text-muted-foreground" />{/if}
    </div>

    <div class="min-w-0 flex-1">
      <p class="text-ui-sm font-medium leading-snug text-foreground">{title}</p>
      {#if subtitle}
        <!-- Capped at a readable measure: the pane is as wide as the manual form,
             and a sentence run across all of it is a line you lose your place in. -->
        <p class="mt-1 max-w-[68ch] text-pretty text-ui-xs leading-relaxed text-muted-foreground">
          {subtitle}
        </p>
      {/if}
      {#if hint}
        <p class="mt-1.5 text-ui-2xs text-muted-foreground">{hint}</p>
      {/if}
    </div>

    {#if action}
      <div class="shrink-0">{@render action()}</div>
    {/if}
  </div>

  <div class="h-px w-full overflow-hidden bg-border/40">
    {#if progress}
      <span class="progress-slide block h-px w-1/4 bg-primary/70"></span>
    {/if}
  </div>
</section>

<style>
  @keyframes pulse-ring {
    0%   { opacity: 0.55; transform: scale(1); }
    100% { opacity: 0;    transform: scale(1.35); }
  }
  .pulse-ring { animation: pulse-ring 1.6s cubic-bezier(0.4, 0, 0.6, 1) infinite; }

  @keyframes progress-slide {
    0%   { transform: translateX(-120%); }
    100% { transform: translateX(420%); }
  }
  .progress-slide { animation: progress-slide 1.3s ease-in-out infinite; }

  @media (prefers-reduced-motion: reduce) {
    .pulse-ring { animation: none; opacity: 0.4; }
    .progress-slide { animation: none; width: 100%; opacity: 0.5; }
  }
</style>
