<script>
  /**
   * Drop-in replacement for a native `<select>`.
   *
   * The OS popup a native select opens cannot be styled, so on Linux/WebKitGTK it
   * renders as a grey system list in the middle of the app's own theme. This wraps
   * the bits-ui Select in the same shape a native select had - `bind:value` plus a
   * flat options array - so converting a call site is a one-line change rather
   * than fifteen lines of Root/Trigger/Content boilerplate.
   */
  import SearchableMenu from './SearchableMenu.svelte'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import Check from '@lucide/svelte/icons/check'
  import { cn } from '$lib/utils.js'

  let {
    value = $bindable(''),
    /** @type {Array<{ value: string, label?: string, disabled?: boolean }>} */
    options = [],
    placeholder = 'Select…',
    disabled = false,
    /** 'sm' = h-7 compact, 'default' = h-8. */
    size = 'default',
    class: className = '',
    contentClass = '',
    /** Mono for identifier values (column types, encodings); the UI font otherwise. */
    mono = false,
    /** Shown instead of the raw value, e.g. to add a unit. */
    format = /** @type {((v: string) => string) | undefined} */ (undefined),
    onchange = /** @type {(v: string) => void} */ (() => {}),
    ...rest
  } = $props()

  const selected = $derived(options.find((o) => String(o.value) === String(value)))
  const label = $derived(
    format ? format(String(value ?? '')) : (selected?.label ?? (value ? String(value) : '')),
  )
</script>

{#if disabled}
  <!-- A disabled trigger still painted its chevron, so a read-only grid (a view's
       structure, a primary-key row) advertised a dropdown on every line that did
       nothing when clicked. Render the value as plain text instead: no chevron, no
       hover state, no affordance the control can't honour. Height matches the
       trigger so swapping between states never shifts the row. -->
  <span
    class={cn(
      'inline-flex max-w-full items-center truncate',
      mono && 'font-mono',
      size === 'sm' ? 'h-7' : 'h-8',
      className,
      !label && 'text-muted-foreground',
    )}
    {...rest}
  >{label || placeholder}</span>
{:else}
<!-- The searchable menu every other select uses: a field on top (a column
     type list is long), the same rows and check. -->
<SearchableMenu
  items={options.map((o) => ({ value: String(o.value), label: o.label ?? String(o.value), disabled: o.disabled, keywords: [String(o.value)] }))}
  align="start"
  contentClass={cn('z-[130] w-[var(--bits-popover-anchor-width)] min-w-[10rem]', contentClass)}
  onselect={(it) => {
    if (!it || it.disabled) return
    value = it.value
    onchange(it.value)
  }}
>
  {#snippet trigger(props)}
    <button
      {...props}
      {...rest}
      type="button"
      data-size={size}
      class={cn(
        'field-surface flex w-fit items-center justify-between gap-1.5 whitespace-nowrap py-2 pr-2 pl-2.5 text-ui-sm outline-none transition-colors select-none hover:bg-muted/50 focus-visible:outline-2 focus-visible:outline-ring data-[state=open]:bg-muted/50',
        size === 'sm' ? 'h-7' : 'h-8',
        mono && 'font-mono',
        className,
      )}
    >
      <span class={cn('truncate', !label && 'text-muted-foreground')}>{label || placeholder}</span>
      <ChevronDown class="size-4 shrink-0 text-muted-foreground" />
    </button>
  {/snippet}
  {#snippet item(it)}
    <span class={cn('min-w-0 flex-1 truncate', mono && 'font-mono', it.disabled && 'opacity-40')}>{it.label}</span>
    {#if String(it.value) === String(value)}<Check class="size-3.5 shrink-0 text-primary" />{/if}
  {/snippet}
</SearchableMenu>
{/if}
