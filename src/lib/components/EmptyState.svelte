<script>
  /**
   * The empty state: what will appear here, and the one step that fills it.
   *
   * DESIGN_SYSTEM.md §10a, as one component. Eleven pages drew their own: an
   * icon in a tinted well at 40, 44, 48 or 64px, title and text 8, 12 or 16px
   * apart, so no two empty screens matched. Here the glyph stands alone, muted,
   * the title is one weight above the text, and the spacing is fixed: 12px to
   * the title, 4px to the description, 16px to the action, so the description
   * reads with its title and the action stands apart from both.
   *
   * @typedef {Object} Props
   * @property {any} [icon] a Lucide component
   * @property {string} [iconName] or an icon-registry name, for `Icon`
   * @property {string} [title]
   * @property {import('svelte').Snippet} [heading] a title with markup
   * @property {string} [description]
   * @property {import('svelte').Snippet} [body] a description with markup
   * @property {import('svelte').Snippet} [children] the action(s)
   * @property {string} [class]
   */
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'

  /** @type {Props} */
  let { icon: IconComponent, iconName = '', title = '', heading, description = '', body, children, class: className = '' } = $props()
</script>

<div class={cn('flex flex-col items-center px-6 text-center', className)}>
  {#if IconComponent}
    <IconComponent class="mb-3 size-6 shrink-0 text-muted-foreground" strokeWidth={1.5} aria-hidden="true" />
  {:else if iconName}
    <Icon name={iconName} class="mb-3 size-6 shrink-0 text-muted-foreground" aria-hidden="true" />
  {/if}
  <div class="flex max-w-sm flex-col gap-1">
    <p class="text-ui-sm font-medium text-balance text-foreground">
      {#if heading}{@render heading()}{:else}{title}{/if}
    </p>
    {#if body || description}
      <p class="text-ui-xs leading-relaxed text-pretty text-muted-foreground">
        {#if body}{@render body()}{:else}{description}{/if}
      </p>
    {/if}
  </div>
  {#if children}
    <div class="mt-4 flex flex-wrap items-center justify-center gap-2">{@render children()}</div>
  {/if}
</div>
