<script>
  /**
   * The search field for a page or a dialog: a filled box with the search icon,
   * the `/` keycap that focuses it (slash-focus.js), and a clear button once
   * there is text. One component, so every page's search looks and works alike.
   */
  import Icon from './Icon.svelte'
  import Kbd from './Kbd.svelte'
  import { cn } from '$lib/utils.js'

  /** @type {{ value?: string, placeholder?: string, label?: string, rank?: number, class?: string, ref?: HTMLInputElement | null, onkeydown?: (e: KeyboardEvent) => void }} */
  let {
    value = $bindable(''),
    placeholder = 'Search',
    label = '',
    /** slash-focus rank: pages use 25, dialogs 30, the sidebar 10. */
    rank = 25,
    class: className = '',
    ref = $bindable(null),
    onkeydown = () => {},
  } = $props()

  let focused = $state(false)
</script>

<div class={cn('relative', className)}>
  <Icon name="search" class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
  <input
    bind:this={ref}
    bind:value
    type="text"
    data-slash-focus={rank}
    aria-keyshortcuts="/"
    aria-label={label || placeholder}
    {placeholder}
    onfocus={() => (focused = true)}
    onblur={() => (focused = false)}
    onkeydown={(e) => {
      if (e.key === 'Escape' && value) { e.stopPropagation(); value = '' }
      onkeydown(e)
    }}
    class="no-field-frame h-8 w-full rounded-lg border-0 bg-muted/55 pr-8 pl-8 text-ui-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground hover:bg-muted/75 focus:bg-muted/80 focus-visible:ring-1 focus-visible:ring-ring/60"
  />
  {#if value}
    <button
      type="button"
      aria-label="Clear search"
      class="absolute top-1/2 right-1.5 inline-flex size-5 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
      onclick={() => { value = ''; ref?.focus() }}
    >
      <Icon name="x" class="size-3 shrink-0" />
    </button>
  {:else if !focused}
    <Kbd combo="/" class="pointer-events-none absolute top-1/2 right-1.5 -translate-y-1/2" aria-hidden="true" />
  {/if}
</div>
