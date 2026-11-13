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
  import * as Select from '$lib/components/ui/select/index.js'
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
<Select.Root
  type="single"
  value={String(value ?? '')}
  onValueChange={(v) => {
    if (v == null) return
    value = v
    onchange(v)
  }}
>
  <Select.Trigger {size} class={cn(mono && 'font-mono', className)} {...rest}>
    <span class={cn('truncate', !label && 'text-muted-foreground')}>{label || placeholder}</span>
  </Select.Trigger>
  <Select.Content class={cn('z-[130] max-h-[18rem] min-w-[10rem]', contentClass)} sideOffset={4}>
    {#each options as opt (opt.value)}
      <Select.Item
        value={String(opt.value)}
        label={opt.label ?? String(opt.value)}
        disabled={opt.disabled}
        class={cn(mono && 'font-mono')}
      >{opt.label ?? opt.value}</Select.Item>
    {/each}
  </Select.Content>
</Select.Root>
{/if}
