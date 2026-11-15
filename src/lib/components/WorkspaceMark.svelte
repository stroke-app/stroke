<script>
  // A workspace's mark: a solid square in its colour with its initial, the way
  // desktop apps show a space or vault. One component so every place matches.
  import { colorOf } from '$lib/stores/workspaces.js'
  import { cn } from '$lib/utils.js'

  /**
   * `solid` is the filled tile (menus, manager); `soft` is a tinted tile with a
   * coloured letter, quiet enough for window chrome beside the avatar.
   * @type {{ workspace: { name?: string, color?: string } | null | undefined, size?: 'xs' | 'sm' | 'md' | 'lg' | 'chrome', tone?: 'solid' | 'soft', class?: string }}
   */
  let { workspace, size = 'sm', tone = 'solid', class: className = '' } = $props()
  const c = $derived(colorOf(workspace?.color))
  /** The workspace colour as a CSS value: `bg-[oklch(0.55_0.19_25)]` → `oklch(0.55 0.19 25)`. */
  const raw = $derived(
    c?.dot?.startsWith('bg-[') ? c.dot.slice(4, -1).replaceAll('_', ' ') : c?.style ? 'var(--cc)' : 'var(--border)',
  )
  /** Soft tone: a hairline ring in the workspace colour. */
  const softStyle = $derived(`background-color: color-mix(in oklch, ${raw} 24%, transparent); box-shadow: inset 0 0 0 1px color-mix(in oklch, ${raw} 55%, transparent);`)

  const initial = $derived((workspace?.name?.trim()?.[0] ?? '?').toUpperCase())
  // About 28% of the side at every size: a continuous, app-icon corner.
  const SIZES = { xs: 'size-4 rounded-[4.5px] text-ui-3xs', sm: 'size-5 rounded-[5.5px] text-ui-3xs', md: 'size-7 rounded-[8px] text-ui-xs', lg: 'size-9 rounded-[10px] text-ui-sm', chrome: 'size-[calc(20px*var(--app-scale))] rounded-[calc(5.5px*var(--app-scale))] text-ui-3xs' }
</script>

<span
  class={cn(
    'flex shrink-0 select-none items-center justify-center font-semibold leading-none',
    SIZES[size],
    tone === 'soft'
      ? 'text-foreground'
      : cn('mark-grain', c?.dot ?? 'bg-zinc-600', 'text-white [text-shadow:0_1px_1px_rgb(0_0_0/0.35)] shadow-[inset_0_1px_0_rgb(255_255_255/0.3),inset_0_0_0_1px_rgb(255_255_255/0.12),inset_0_-1px_0_rgb(0_0_0/0.25),0_1px_2px_rgb(0_0_0/0.3),0_3px_8px_-3px_rgb(0_0_0/0.35)]'),
    className,
  )}
  style={[c?.style, tone === 'soft' ? softStyle : ''].filter(Boolean).join(';')}
  aria-hidden="true"
>{initial}</span>
