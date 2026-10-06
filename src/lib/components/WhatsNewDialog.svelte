<script>
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import ExternalLink from '@lucide/svelte/icons/external-link'
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
  } = $props()

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

    <div class="-mx-5 mt-4 max-h-[min(26rem,60vh)] overflow-y-auto border-t border-border/40 px-5 py-4">
      <div class="flex flex-col gap-5">
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
