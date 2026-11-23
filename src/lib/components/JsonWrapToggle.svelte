<script>
  // The Wrap switch shared by every JSON viewer.
  //
  // One component rather than five copies: the app shows JSON in five places
  // (SQL output, ORM output, the JSON tab, a JSON cell, the row detail panel)
  // and a preference that means "wrap JSON" should not be five preferences.
  // It writes the app setting, so flipping it here also moves the switch in
  // Settings, and every other open viewer reflows with it.
  import WrapText from '@lucide/svelte/icons/wrap-text'
  import { appJsonWordWrap } from '$lib/stores/settings.js'
  import { loadSettings, saveSettings } from '$lib/stores/settings.js'
  import { cn } from '$lib/utils.js'

  let {
    /** Compact renders the glyph alone, for toolbars with no room for a label. */
    compact = false,
    class: className = '',
  } = $props()

  function toggle() {
    const next = !$appJsonWordWrap
    appJsonWordWrap.set(next)
    saveSettings({ ...loadSettings(), jsonWordWrap: next })
  }
</script>

<button
  type="button"
  title={$appJsonWordWrap ? 'Wrap long values: on' : 'Wrap long values: off'}
  aria-label="Wrap long values"
  aria-pressed={$appJsonWordWrap}
  onclick={toggle}
  class={cn(
    'inline-flex shrink-0 items-center gap-1.5 rounded-md font-mono text-ui-2xs transition-colors',
    compact ? 'size-6 justify-center' : 'px-2 py-1',
    $appJsonWordWrap
      ? 'bg-muted text-foreground'
      : 'text-muted-foreground hover:bg-muted hover:text-foreground',
    className,
  )}
>
  <WrapText class="size-3 shrink-0" />
  {#if !compact}<span class="@max-[34rem]/jv:sr-only">Wrap</span>{/if}
</button>
