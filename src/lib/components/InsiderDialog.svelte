<script>
  import Shuffle from '@lucide/svelte/icons/shuffle'
  import Lightbulb from '@lucide/svelte/icons/lightbulb'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { insiderTips } from '$lib/insider-tips.js'

  let { open = $bindable(false) } = $props()

  const all = insiderTips

  const categories = [...new Set(all.map((f) => f.category))]

  /** @type {typeof all} */
  let shown = $state(pick())

  function pick() {
    // Shuffle and return up to 2 per category
    const shuffled = [...all].sort(() => Math.random() - 0.5)
    /** @type {Record<string, number>} */
    const seen = {}
    return shuffled.filter((f) => {
      seen[f.category] = (seen[f.category] ?? 0) + 1
      return seen[f.category] <= 2
    })
  }

  $effect(() => {
    if (open) shown = pick()
  })

  const grouped = $derived(
    categories
      .map((cat) => ({ cat, items: shown.filter((f) => f.category === cat) }))
      .filter((g) => g.items.length > 0),
  )
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="flex max-h-[80vh] max-w-lg flex-col gap-0 overflow-hidden p-0">
    <Dialog.Header class="shrink-0 border-b border-border px-5 py-4">
      <div class="flex items-center justify-between gap-3">
        <div class="flex items-center gap-3">
          <div class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted">
            <Lightbulb class="size-3.5 shrink-0 text-muted-foreground" />
          </div>
          <div class="flex flex-col gap-0.5">
            <Dialog.Title class="text-ui-sm font-semibold">SQL Insider</Dialog.Title>
            <Dialog.Description class="text-ui-xs text-muted-foreground">
              Things worth knowing about SQL and databases.
            </Dialog.Description>
          </div>
        </div>
        <button
          type="button"
          onclick={() => (shown = pick())}
          title="Shuffle tips"
          class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        >
          <Shuffle class="size-3.5 shrink-0" />
        </button>
      </div>
    </Dialog.Header>

    <div class="min-h-0 flex-1 overflow-y-auto">
      <div class="flex flex-col gap-0 divide-y divide-border">
        {#each grouped as group (group.cat)}
          <div class="px-5 py-4">
            <p class="mb-3 text-ui-3xs font-semibold tracking-[0.06em] text-muted-foreground uppercase">
              {group.cat}
            </p>
            <ul class="flex flex-col gap-3">
              {#each group.items as fact (fact.label)}
                <li class="flex flex-col gap-0.5">
                  <span class="font-mono text-ui-2xs font-medium text-foreground/80">{fact.label}</span>
                  <span class="text-ui-sm text-muted-foreground">{fact.text}</span>
                </li>
              {/each}
            </ul>
          </div>
        {/each}
      </div>
    </div>
  </Dialog.Content>
</Dialog.Root>
