<script>
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import ArrowRight from '@lucide/svelte/icons/arrow-right'
  import { appSidebarStyle } from '$lib/stores/settings.js'
  import ReleaseNotes from './ReleaseNotes.svelte'
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

  /** "2.7.8" from "2.7.8-nightly.202611141611" or "v2.7.8". @param {string} v */
  const short = (v) => String(v ?? '').replace(/^v/, '').replace(/-nightly\.\d+$/, '')
  /** @param {string} v */
  const isNightly = (v) => /-nightly\./.test(String(v ?? ''))

  /** @param {import('$lib/whats-new.js').HighlightAction} id */
  function act(id) {
    open = false
    onaction(id)
  }

</script>


<Dialog.Root bind:open>
  <Dialog.Content class="max-w-[32rem] gap-0 p-0">
    <!-- The title, then the exact build in mono (the one to quote in a bug
         report), then where it came from. No mark: the window is the app. -->
    <Dialog.Header class="gap-1 px-6 pt-6 pe-12">
      <Dialog.Title class="text-ui-xl font-semibold tracking-tight">What's new</Dialog.Title>
      <Dialog.Description class="flex min-w-0 flex-wrap items-baseline gap-x-1.5 text-ui-sm text-muted-foreground">
        <span>{isNightly(current) ? 'Stroke Nightly' : 'Stroke'}</span>
        <span class="min-w-0 truncate font-mono text-ui-xs tabular-nums" title={current}>{current}</span>
      </Dialog.Description>
      {#if previous}
        <p class="mt-2 text-ui-xs text-muted-foreground">Updated from <span class="font-mono tabular-nums">{short(previous)}</span>.</p>
      {/if}
    </Dialog.Header>

    <div class="mt-5 max-h-[min(30rem,62vh)] overflow-y-auto border-t border-border/40 px-6 pt-4 pb-2 [scrollbar-gutter:stable]">
      {#if hero}
        <!-- The lead feature: the only filled button in the dialog. -->
        <section class="flex flex-col gap-1.5" aria-labelledby="whats-new-hero">
          <div class="flex items-center gap-2">
            <h3 id="whats-new-hero" class="text-ui-lg font-semibold text-foreground">{hero.title}</h3>
            {#if hero.beta}
              <span class="rounded-full border border-border/70 px-1.5 py-px text-ui-3xs font-medium leading-4 text-muted-foreground">Labs</span>
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
          <div class="pb-4 {highlights.length ? '' : 'pt-1'}"><ReleaseNotes {releases} {omitted} compact /></div>
        {/if}
      {/if}
    </div>

    <!-- Summary left, actions right, like a confirm dialog's footer. -->
    <Dialog.Footer class="mx-0 mb-0 flex-row items-center gap-3 border-t border-border/40 px-6 py-4 sm:justify-between">
      <p class="min-w-0 truncate text-ui-xs text-muted-foreground">
        {#if changeCount}{`${changeCount} ${changeCount === 1 ? 'change' : 'changes'}${releases.length > 1 ? ` across ${releases.length} versions` : ''}`}{/if}
      </p>
      <div class="flex shrink-0 items-center gap-2">
        <Button variant="outline" onclick={onfullchangelog}>
          Full changelog<ExternalLink class="size-3.5 shrink-0" />
        </Button>
        <Button onclick={() => (open = false)}>Done</Button>
      </div>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
