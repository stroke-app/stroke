<script>
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import ExternalLink from '@lucide/svelte/icons/external-link'
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

  // An update that brings new highlights leads with them: the first at full
  // size with the one filled button, the rest a quiet list, then the notes.
  // Opened on demand (no `previous`), the highlights are the current major's
  // standing features, not news: the notes lead and they follow as a list.
  // That used to open on four Stroke 3.0 cards with the changes folded away.
  const leadWithHighlights = $derived(!!previous && highlights.length > 0)
  const hero = $derived(leadWithHighlights ? (highlights[0] ?? null) : null)
  const rest = $derived(leadWithHighlights ? highlights.slice(1) : highlights)
  /** @param {import('$lib/whats-new.js').Highlight} h */
  const isOn = (h) => h.action?.id === 'enable-tree-sidebar' && $appSidebarStyle === 'tree'
  const changeCount = $derived(releases.reduce((n, r) => n + r.sections.reduce((m, s) => m + s.items.length, 0), 0))

  /** "2.7.8" from "2.7.8-nightly.202611141611" or "v2.7.8". @param {string} v */
  const short = (v) => String(v ?? '').replace(/^v/, '').replace(/-nightly\.\d+$/, '')
  /** @param {string} v */
  const isNightly = (v) => /-nightly\./.test(String(v ?? ''))

  /** @type {HTMLButtonElement | null} */
  let doneEl = $state(null)

  /** @param {import('$lib/whats-new.js').HighlightAction} id */
  function act(id) {
    open = false
    onaction(id)
  }

</script>


<Dialog.Root bind:open>
  <!-- Focus lands on Done without scrolling: the dialog focused its first
       button, which is now the "Worth trying" list at the bottom, and opened
       scrolled past every change. -->
  <Dialog.Content
    class="max-w-[32rem] gap-0 p-0"
    onOpenAutoFocus={(e) => { e.preventDefault(); doneEl?.focus({ preventScroll: true }) }}
  >
    <!-- The title, then the version people read (3.6.6) with the channel as a
         badge; the exact build, the one to quote in a bug report, is its
         tooltip. "Stroke Nightly 3.6.2-nightly.2026..." said Nightly twice. -->
    <Dialog.Header class="gap-1.5 px-6 pt-6 pe-12">
      <Dialog.Title class="text-ui-xl font-semibold tracking-tight">What's new</Dialog.Title>
      <Dialog.Description class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-ui-sm text-muted-foreground">
        <span title={current}>Stroke <span class="font-mono text-ui-xs tabular-nums text-foreground">{short(current)}</span></span>
        {#if isNightly(current)}
          <span class="rounded-full border border-border/70 px-1.5 py-px text-ui-3xs font-medium leading-4 text-muted-foreground">Nightly</span>
        {/if}
        {#if previous}
          <span class="text-ui-xs">· updated from <span class="font-mono tabular-nums">{short(previous)}</span></span>
        {/if}
      </Dialog.Description>
    </Dialog.Header>

    <div class="mt-5 max-h-[min(32rem,64vh)] overflow-y-auto border-t border-border/40 px-6 pt-5 pb-5 [scrollbar-gutter:stable]">
      {#if hero}
        <!-- The lead feature of this update: the only filled button here. -->
        <section class="flex flex-col gap-1.5 pb-5" aria-labelledby="whats-new-hero">
          <div class="flex items-center gap-2">
            <h3 id="whats-new-hero" class="text-ui-lg font-semibold text-foreground">{hero.title}</h3>
            {#if hero.beta}
              <span class="rounded-full border border-border/70 px-1.5 py-px text-ui-3xs font-medium leading-4 text-muted-foreground">Labs</span>
            {/if}
          </div>
          <p class="text-ui-sm leading-relaxed text-pretty text-muted-foreground">{hero.body}</p>
          {#if hero.action}
            <div class="pt-2.5">
              <Button size="sm" disabled={isOn(hero)} onclick={() => act(hero.action.id)}>
                {isOn(hero) ? 'Already on' : hero.action.label}
              </Button>
            </div>
          {/if}
        </section>
      {/if}

      {#snippet featureList(/** @type {import('$lib/whats-new.js').Highlight[]} */ items)}
        <!-- Grouped by space, not boxes: each row is a title, one line, and its action. -->
        <ul class="flex flex-col">
          {#each items as h (h.id)}
            <li class="flex items-start gap-4 border-t border-border/40 py-3 first:border-t-0 first:pt-0">
              <div class="min-w-0 flex-1">
                <p class="flex items-center gap-2 text-ui-sm font-medium text-foreground">
                  {h.title}
                  {#if h.beta}<span class="rounded-full border border-border/70 px-1.5 py-px text-ui-3xs font-medium leading-4 text-muted-foreground">Labs</span>{/if}
                </p>
                <p class="mt-0.5 text-ui-xs leading-relaxed text-pretty text-muted-foreground">{h.body}</p>
              </div>
              {#if h.action}
                <Button variant="ghost" size="sm" class="-me-2 shrink-0 gap-1 text-muted-foreground hover:text-foreground" disabled={isOn(h)} onclick={() => act(h.action.id)}>
                  {isOn(h) ? 'Already on' : h.action.label}<ArrowRight class="size-3.5 shrink-0" />
                </Button>
              {/if}
            </li>
          {/each}
        </ul>
      {/snippet}

      {#if leadWithHighlights && rest.length}
        <div class="border-t border-border/40 pt-3 pb-5">{@render featureList(rest)}</div>
      {/if}

      {#if releases.length}
        {#if leadWithHighlights}
          <h3 class="border-t border-border/40 pt-4 pb-3 text-ui-sm font-semibold text-foreground">All changes</h3>
        {/if}
        <ReleaseNotes {releases} {omitted} compact />
      {/if}

      {#if !leadWithHighlights && rest.length}
        <h3 class="mt-7 border-t border-border/40 pt-4 pb-3 text-ui-sm font-semibold text-foreground">Worth trying</h3>
        {@render featureList(rest)}
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
        <Button bind:ref={doneEl} onclick={() => (open = false)}>Done</Button>
      </div>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
