<script>
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import ListTree from '@lucide/svelte/icons/list-tree'
  import Palette from '@lucide/svelte/icons/palette'
  import Keyboard from '@lucide/svelte/icons/keyboard'
  import Database from '@lucide/svelte/icons/database'
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

  const ICON = { tree: ListTree, themes: Palette, keyboard: Keyboard, database: Database }

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

<Dialog.Root bind:open>
  <Dialog.Content class="max-w-[32rem] gap-0">
    <Dialog.Header class="gap-1 pr-6">
      <Dialog.Title class="text-ui-xl">What's new in Stroke {current}</Dialog.Title>
      {#if previous}
        <Dialog.Description class="text-ui-xs">Updated from v{previous}</Dialog.Description>
      {/if}
    </Dialog.Header>

    <div class="-mx-5 mt-4 max-h-[min(30rem,65vh)] overflow-y-auto border-t border-border/40 px-5 py-4">
      <div class="flex flex-col gap-5">
        {#if highlights.length}
          <!-- What to try first, each with the button that does it. -->
          <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
            {#each highlights as h (h.id)}
              {@const Ico = ICON[h.icon]}
              {@const done = h.action?.id === 'enable-tree-sidebar' && $appSidebarStyle === 'tree'}
              <div class="flex flex-col gap-2 rounded-lg border border-border/60 bg-panel p-3">
                <div class="flex items-center gap-2">
                  <span class="flex size-7 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
                    <Ico class="size-3.5 shrink-0" />
                  </span>
                  <p class="min-w-0 truncate text-ui-sm font-medium text-foreground">{h.title}</p>
                  {#if h.beta}
                    <span class="shrink-0 rounded-full border border-border/70 px-1.5 py-px text-ui-3xs font-medium leading-4 text-muted-foreground">Beta</span>
                  {/if}
                </div>
                <p class="flex-1 text-ui-xs leading-relaxed text-muted-foreground text-pretty">{h.body}</p>
                {#if h.action}
                  <Button variant="outline" size="sm" class="self-start" disabled={done} onclick={() => act(h.action.id)}>
                    {done ? 'On' : h.action.label}
                  </Button>
                {/if}
              </div>
            {/each}
          </div>
          {#if releases.length}
            <h3 class="-mb-2 text-ui-xs font-medium text-muted-foreground">Everything in this update</h3>
          {/if}
        {/if}
        {#each releases as release (release.version)}
          <section class="flex flex-col gap-3">
            {#if releases.length > 1}
              <div class="flex items-baseline gap-2">
                <span class="font-mono text-ui-xs font-medium text-foreground">v{release.version}</span>
                {#if release.date}
                  <span class="text-ui-2xs text-muted-foreground">{release.date}</span>
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
                  <h4 class="pl-3.5 text-ui-xs font-medium text-muted-foreground">{section.title}</h4>
                {/if}
                <ul class="flex flex-col gap-1 pl-3.5">
                  {#each section.items as item, j (j)}
                    <li class="relative pl-3 text-ui-sm leading-relaxed text-foreground/90 before:absolute before:top-[0.6em] before:left-0 before:size-1 before:rounded-full before:bg-muted-foreground/60">
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
    </div>

    <Dialog.Footer>
      <Button variant="outline" onclick={onfullchangelog}>
        <ExternalLink class="size-3.5 shrink-0" />
        Full changelog
      </Button>
      <Button onclick={() => (open = false)}>Got it</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
