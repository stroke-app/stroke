<script>
  /**
   * The search field for a page or a dialog: a filled box with the search icon,
   * the `/` keycap that focuses it (slash-focus.js), and a clear button once
   * there is text. One component, so every page's search looks and works alike.
   */
  import Icon from './Icon.svelte'
  import Kbd from './Kbd.svelte'
  import { cn } from '$lib/utils.js'

  /** @type {{ [attr: string]: any, value?: string, placeholder?: string, label?: string, rank?: number | null, class?: string, inputClass?: string, ref?: HTMLInputElement | null, onkeydown?: (e: KeyboardEvent) => void, oninput?: (e: Event) => void, title?: string }} */
  let {
    value = $bindable(''),
    placeholder = 'Search',
    label = '',
    /** slash-focus rank: pages use 25, dialogs 30, the sidebar 10. Null: `/` does not focus it, and no keycap shows. */
    rank = 25,
    class: className = '',
    /** Extra classes for the input itself (e.g. a taller field). */
    inputClass = '',
    ref = $bindable(null),
    onkeydown = () => {},
    oninput = () => {},
    title = undefined,
    /** Anything else (tabindex, autocomplete, aria-*) goes to the input. */
    ...rest
  } = $props()

  let focused = $state(false)
</script>

<div class={cn('relative', className)}>
  <Icon name="search" class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
  <input
    bind:this={ref}
    bind:value
    type="text"
    data-slash-focus={rank ?? undefined}
    aria-keyshortcuts={rank == null ? undefined : "/"}
    {title}
    {oninput}
    {...rest}
    aria-label={label || placeholder}
    {placeholder}
    onfocus={() => (focused = true)}
    onblur={() => (focused = false)}
    onkeydown={(e) => {
      if (e.key === 'Escape' && value) { e.stopPropagation(); value = '' }
      onkeydown(e)
    }}
    class={cn(
      // An outlined box, not a shadow and not a bare fill: a 1px border reads
      // as a field on every platform, where the fill alone was lost on light
      // themes and shadows rendered muddy on Linux and Windows.
      'search-box no-field-frame w-full pr-9 pl-8 text-ui-xs',
      inputClass,
    )}
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
  {:else if !focused && rank != null}
    <Kbd combo="/" class="pointer-events-none absolute top-1/2 right-1.5 -translate-y-1/2" aria-hidden="true" />
  {/if}
</div>
