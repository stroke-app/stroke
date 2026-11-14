<script>
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import ArrowRight from '@lucide/svelte/icons/arrow-right'
  import { appSidebarStyle } from '$lib/stores/settings.js'
  import { splitInlineCode } from '$lib/changelog.js'
  import { cn } from '$lib/utils.js'

  /**
   * First launch after an update: what changed between the version the user
   * was on and this one, read from the CHANGELOG.md bundled with this build.
   */
  let {
    open = $bindable(false),
    current = '',
    previous = '',
    /** @type {import('$lib/changelog.js').ChangelogRelease[]} */
    releases = [],
    /** Older releases left out of `releases`. */
    omitted = 0,
    onfullchangelog = /** @type {() => void} */ (() => {}),
    /** A few features to try, above the notes (see $lib/whats-new.js). @type {import('$lib/whats-new.js').Highlight[]} */
    highlights = [],
    /** A highlight's button. @type {(id: import('$lib/whats-new.js').HighlightAction) => void} */
    onaction = () => {},
  } = $props()

  // The first highlight leads, at full size with the one filled button; the
  // rest are a quiet list. Four equal boxed cards had no hierarchy and
  // truncated their titles.
  const hero = $derived(highlights[0] ?? null)
  const rest = $derived(highlights.slice(1))
  const heroOn = $derived(hero?.action?.id === 'enable-tree-sidebar' && $appSidebarStyle === 'tree')

  /** The full notes stay folded when there are highlights to read first. */
  let notesOpen = $state(false)
  $effect(() => {
    if (open) notesOpen = highlights.length === 0
  })
  const changeCount = $derived(releases.reduce((n, r) => n + r.sections.reduce((m, s) => m + s.items.length, 0), 0))

  /** @param {import('$lib/whats-new.js').HighlightAction} id */
  function act(id) {
    open = false
    onaction(id)
  }

  /** @type {Record<import('$lib/changelog.js').ChangeType, string>} */
  const DOT = {
    feature: 'bg-primary',
    fix: 'bg-success',
    change: 'bg-muted-foreground',
    other: 'bg-muted-foreground',
  }
</script>

{#snippet notes()}
  <div class="flex flex-col gap-5">
    {#each releases as release (release.version)}
      <section class="flex flex-col gap-3">
        {#if releases.length > 1}
          <div class="flex items-baseline gap-2">
            <span class="font-mono text-ui-xs font-medium tabular-nums text-foreground">v{release.version}</span>
            {#if release.date}
              <span class="text-ui-2xs tabular-nums text-muted-foreground">{release.date}</span>
            {/if}
          </div>
        {/if}
        {#each release.sections as section, i (i)}
          {@const newGroup = i === 0 || release.sections[i - 1].group !== section.group}
          <div class="flex flex-col gap-1.5">
            {#if newGroup}
              <h3 class="flex items-center gap-2 text-ui-sm font-semibold text-foreground">
                <span class={cn('size-1.5 shrink-0 rounded-full', DOT[section.type])} aria-hidden="true"></span>
                {section.group}
              </h3>
            {/if}
            {#if section.title !== section.group}
              <h4 class="ps-3.5 text-ui-xs font-medium text-muted-foreground">{section.title}</h4>
            {/if}
            <ul class="flex flex-col gap-1 ps-3.5">
              {#each section.items as item, j (j)}
                <li class="relative ps-3 text-ui-sm leading-relaxed text-pretty text-foreground/90 before:absolute before:top-[0.6em] before:left-0 before:size-1 before:rounded-full before:bg-muted-foreground/60">
                  {#each splitInlineCode(item) as part, k (k)}
                    {#if part.code}
                      <code class="rounded-md bg-muted/60 px-1 py-px font-mono text-ui-xs text-foreground">{part.text}</code>
                    {:else}
                      {part.text}
                    {/if}
                  {/each}
                </li>
              {/each}
            </ul>
          </div>
        {/each}
      </section>
    {/each}
    {#if omitted > 0}
      <p class="text-ui-xs text-muted-foreground">
        And {omitted} earlier {omitted === 1 ? 'release' : 'releases'} in the full changelog.
      </p>
    {/if}
  </div>
{/snippet}

<Dialog.Root bind:open>
  <Dialog.Content class="max-w-[30rem] gap-0 p-0">
    <Dialog.Header class="gap-1 px-6 pt-6 pe-12">
      <Dialog.Title class="text-ui-xl text-balance">What's new in Stroke {current}</Dialog.Title>
      {#if previous}
        <Dialog.Description class="text-ui-xs tabular-nums">Updated from v{previous}</Dialog.Description>
      {/if}
    </Dialog.Header>

    <div class="max-h-[min(32rem,68vh)] overflow-y-auto px-6 pt-5 pb-2 [scrollbar-gutter:stable]">
      {#if hero}
        <!-- The lead feature: the only filled button in the dialog. -->
        <section class="flex flex-col gap-1.5" aria-labelledby="whats-new-hero">
          <div class="flex items-center gap-2">
            <h3 id="whats-new-hero" class="text-ui-lg font-semibold text-foreground">{hero.title}</h3>
            {#if hero.beta}
              <span class="rounded-full border border-border/70 px-1.5 py-px text-ui-3xs font-medium leading-4 text-muted-foreground">Beta</span>
            {/if}
          </div>
          <p class="text-ui-sm leading-relaxed text-pretty text-muted-foreground">{hero.body}</p>
          {#if hero.action}
            <div class="pt-2.5">
              <Button size="sm" disabled={heroOn} onclick={() => act(hero.action.id)}>
                {heroOn ? 'Already on' : hero.action.label}
              </Button>
            </div>
          {/if}
        </section>
      {/if}

      {#if rest.length}
        <!-- Grouped by space, not boxes: each row is a title, one line, and its action. -->
        <ul class="mt-6 flex flex-col">
          {#each rest as h (h.id)}
            <li class="flex items-start gap-4 border-t border-border/40 py-3">
              <div class="min-w-0 flex-1">
                <p class="text-ui-sm font-medium text-foreground">{h.title}</p>
                <p class="mt-0.5 text-ui-xs leading-relaxed text-pretty text-muted-foreground">{h.body}</p>
              </div>
              {#if h.action}
                <Button variant="ghost" size="sm" class="-me-2 shrink-0 gap-1 text-muted-foreground hover:text-foreground" onclick={() => act(h.action.id)}>
                  {h.action.label}<ArrowRight class="size-3.5 shrink-0" />
                </Button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      {#if releases.length}
        {#if highlights.length}
          <!-- Folded instead of half-visible: a clipped first line read as broken. -->
          <button
            type="button"
            class="mt-1 flex w-full items-center gap-1.5 border-t border-border/40 py-3 text-start text-ui-sm font-medium text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
            aria-expanded={notesOpen}
            onclick={() => (notesOpen = !notesOpen)}
          >
            <ChevronRight class={cn('size-3.5 shrink-0 text-muted-foreground transition-transform duration-150', notesOpen && 'rotate-90')} />
            {notesOpen ? 'Hide' : 'Show'} all {changeCount} changes
          </button>
        {/if}
        {#if notesOpen}
          <div class="pb-4 {highlights.length ? 'ps-5' : 'pt-1'}">{@render notes()}</div>
        {/if}
      {/if}
    </div>

    <Dialog.Footer class="mx-0 mb-0 border-t border-border/40 px-6 py-4">
      <Button variant="ghost" onclick={onfullchangelog}>
        <ExternalLink class="size-3.5 shrink-0" />
        Full changelog
      </Button>
      <Button variant={hero ? 'outline' : 'default'} onclick={() => (open = false)}>Done</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
