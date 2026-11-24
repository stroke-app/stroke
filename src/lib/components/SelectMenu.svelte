<script>
  // Canonical dropdown/select for the app: a data-driven Select built on
  // SearchableMenu, so every dropdown looks identical and carries a search box
  // regardless of how many items it holds (see `searchThreshold` there).
  // Drop-in replacement for bits-ui <Select> in the common case.
  //
  //   <SelectMenu value={x} onValueChange={setX} items={[{value,label,icon?,hint?,desc?,font?}]} />
  //
  // `desc` is muted text after the label (a row's detail); `font` draws the
  // label in that CSS font-family, as font pickers show each face in itself.
  //
  // For custom leading visuals (font preview, variable-weight icon, brand icon)
  // pass a `lead` snippet - it receives the full item and renders both the
  // trigger's and each row's leading glyph.
  import SearchableMenu from './SearchableMenu.svelte'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'

  let {
    /** Current value (compared with String() so numbers/ids both work). */
    value = undefined,
    /** @type {(v: any) => void} */
    onValueChange = () => {},
    /** @type {Array<{ value: any, label: string, icon?: string, hint?: string, keywords?: string[], [k: string]: any }>} */
    items = [],
    placeholder = 'Select…',
    searchPlaceholder = 'Search…',
    empty = 'No results',
    ariaLabel = undefined,
    /** Extra classes for the trigger button (size/width live here). */
    triggerClass = 'h-8 w-56 shrink-0',
    contentClass = 'w-[var(--bits-popover-anchor-width)] min-w-[13rem]',
    searchThreshold = -1,
    align = 'start',
    side = 'bottom',
    disabled = false,
    /** snippet(item) - custom leading visual for the trigger + rows. */
    lead,
    open = $bindable(false),
  } = $props()

  const selected = $derived(items.find((i) => String(i.value) === String(value)))
</script>

<SearchableMenu
  bind:open
  {items}
  {align}
  {side}
  {contentClass}
  {searchThreshold}
  {empty}
  placeholder={searchPlaceholder}
  onselect={(it) => { if (it && it.value !== undefined) onValueChange(it.value) }}
>
  {#snippet trigger(props)}
    <button
      {...props}
      type="button"
      {disabled}
      aria-label={ariaLabel}
      class={cn(
        'inline-flex items-center justify-between gap-2 rounded-[10px] border-2 border-border/70 bg-background px-2.5 text-ui-xs font-normal transition-colors hover:bg-muted/40 focus-visible:border-ring/55 focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none data-[state=open]:border-ring disabled:pointer-events-none disabled:opacity-50',
        triggerClass,
      )}
    >
      <span class="flex min-w-0 items-center gap-2">
        {#if lead && selected}{@render lead(selected)}{:else if selected?.icon}<Icon name={selected.icon} class="size-3.5 shrink-0 text-muted-foreground" />{/if}
        <span class="truncate font-medium" style:font-family={selected?.font}>{selected?.label ?? placeholder}</span>
      </span>
      <Icon name="chevron-down" class="size-3.5 shrink-0 text-muted-foreground" />
    </button>
  {/snippet}
  {#snippet item(it)}
    {#if lead}{@render lead(it)}{:else if it.icon}<Icon name={it.icon} class="size-4 shrink-0 text-muted-foreground" />{/if}
    <!-- Fixed line height, label top-aligned: a face with tall metrics drawn
         in its own font would otherwise make its row taller than the rest. -->
    <span class="min-w-0 flex-1 truncate leading-5"><span class="align-top" style:font-family={it.font}>{it.label}</span>{#if it.desc}<span class="ml-2 text-muted-foreground">{it.desc}</span>{/if}</span>
    {#if it.hint}<span class="shrink-0 text-ui-3xs text-muted-foreground">{it.hint}</span>{/if}
    {#if String(it.value) === String(value)}<Icon name="check" class="ml-auto size-3.5 shrink-0 text-primary" />{/if}
  {/snippet}
</SearchableMenu>
