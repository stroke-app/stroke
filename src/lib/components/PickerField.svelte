<script>
  /**
   * A form field that opens a searchable list (SearchableMenu) - for pickers
   * too long to scan in a plain select: functions, tables, column types.
   */
  import SearchableMenu from './SearchableMenu.svelte'
  import { cn } from '$lib/utils.js'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import Check from '@lucide/svelte/icons/check'

  let {
    value = $bindable(''),
    /** @type {Array<{ value: string, label?: string, hint?: string, group?: string, keywords?: string[] }>} */
    items = [],
    placeholder = 'Select…',
    search = 'Search…',
    empty = 'No results',
    mono = false,
    id = undefined,
    invalid = false,
    /** 'sm' = h-7 frameless cell, 'default' = h-8 field. */
    size = 'default',
    class: className = '',
    contentClass = 'w-[var(--bits-popover-anchor-width)] min-w-60',
    'aria-label': ariaLabel = undefined,
    /** @type {(v: string) => void} */
    onchange = () => {},
    /** optional snippet({ close }) under the list */
    menuFooter = undefined,
  } = $props()

  let open = $state(false)
  const label = $derived(items.find((i) => i.value === value)?.label ?? value)
</script>

<SearchableMenu
  bind:open
  {items}
  placeholder={search}
  {empty}
  {contentClass}
  onselect={(it) => { value = it.value; onchange(it.value) }}
>
  {#snippet trigger(props)}
    <button
      {...props}
      {id}
      type="button"
      aria-label={ariaLabel}
      data-invalid={invalid || undefined}
      class={cn(
        'flex w-full min-w-0 items-center gap-1.5 text-left text-ui-sm outline-none',
        size === 'sm'
          ? 'h-7 rounded-md px-2 hover:bg-accent focus-visible:bg-accent data-[state=open]:bg-accent'
          : 'field-surface h-8 pr-2 pl-3 hover:border-foreground/30 data-invalid:border-destructive',
        className,
      )}
    >
      <span class={cn('min-w-0 flex-1 truncate', mono && value && 'font-mono', !value && 'text-muted-foreground')}>
        {value ? label : placeholder}
      </span>
      <ChevronDown class="size-3.5 shrink-0 text-muted-foreground" />
    </button>
  {/snippet}
  {#snippet item(it)}
    <span data-slot="menu-label" class={cn('min-w-0 truncate', mono && 'font-mono')}>{it.label ?? it.value}</span>
    {#if it.hint}<span class="ml-auto min-w-0 shrink truncate pl-3 font-mono text-ui-2xs text-muted-foreground">{it.hint}</span>{/if}
    {#if it.value === value}<Check class={cn('size-3.5 shrink-0 text-primary', !it.hint && 'ml-auto')} />{/if}
  {/snippet}
  {#snippet footer()}
    {#if menuFooter}
      <div class="border-t border-border/60 p-1">{@render menuFooter({ close: () => (open = false) })}</div>
    {/if}
  {/snippet}
</SearchableMenu>
