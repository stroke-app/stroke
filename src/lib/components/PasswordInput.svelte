<script>
  // A password field with a reveal toggle. Masked input plus no way to check
  // what you typed is how a wrong character survives three failed connects, so
  // every secret field in the app gets the eye.
  //
  // Drop-in for <Input type="password" />: same props, same classes.
  import { Input } from '$lib/components/ui/input/index.js'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'

  let {
    value = $bindable(''),
    class: className = '',
    /** Hides the toggle for fields that are rendered read-only. */
    disabled = false,
    ...rest
  } = $props()

  let revealed = $state(false)
</script>

<div class="relative w-full">
  <Input
    bind:value
    type={revealed ? 'text' : 'password'}
    {disabled}
    class={cn(className, 'pr-8')}
    {...rest}
  />
  <button
    type="button"
    {disabled}
    onclick={() => (revealed = !revealed)}
    aria-label={revealed ? 'Hide password' : 'Show password'}
    aria-pressed={revealed}
    title={revealed ? 'Hide' : 'Show'}
    tabindex="-1"
    class="absolute right-2 top-1/2 -translate-y-1/2 rounded text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:text-foreground disabled:pointer-events-none disabled:opacity-40"
  >
    <Icon name={revealed ? 'eye-off' : 'eye'} class="size-3.5" />
  </button>
</div>
