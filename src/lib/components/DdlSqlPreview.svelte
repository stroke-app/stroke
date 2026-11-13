<script>
  /**
   * The live SQL preview the Create trigger / sequence / table dialogs share:
   * collapsible, with a Copy action, highlighted by the app's lazily loaded
   * Shiki highlighter (the same one GenerateSqlDialog's ShikiBlock uses).
   * Placeholders (`‹label›`, see ddl-builders.js) render in muted italics so
   * what is still missing reads differently from what was typed.
   */
  import { untrack } from 'svelte'
  import { cn } from '$lib/utils.js'
  import { appThemeId } from '$lib/stores/settings.js'
  import { escapeHtml } from '$lib/json-inspector.js'
  import { PLACEHOLDER_RE } from '$lib/ddl-builders.js'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import Copy from '@lucide/svelte/icons/copy'
  import Check from '@lucide/svelte/icons/check'

  let {
    sql = '',
    expanded = $bindable(true),
    class: className = '',
  } = $props()

  let html = $state('')
  let copied = $state(false)
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let copyTimer

  const theme = $derived($appThemeId)

  /**
   * Swap each placeholder for a plain word token Shiki keeps whole, so it can
   * be put back as a styled span after highlighting.
   * @param {string} src
   */
  function mask(src) {
    /** @type {string[]} */
    const labels = []
    const masked = src.replace(PLACEHOLDER_RE, (_, label) => `zqph${labels.push(label) - 1}zq`)
    return { masked, labels }
  }

  /** @param {string} out @param {string[]} labels */
  const unmask = (out, labels) =>
    out.replace(/zqph(\d+)zq/g, (_, i) => `<span class="ddl-ph">${escapeHtml(labels[Number(i)] ?? '')}</span>`)

  $effect(() => {
    const src = sql
    const t = theme
    const { masked, labels } = mask(src)
    // Plain text first (and as the fallback), so typing never waits on Shiki.
    if (!untrack(() => html)) html = `<pre>${unmask(escapeHtml(masked), labels)}</pre>`
    let cancelled = false
    import('$lib/shiki-highlighter.js')
      .then(({ highlightCode }) => highlightCode(masked, 'sql', t))
      .then((out) => { if (!cancelled) html = unmask(out, labels) })
      .catch(() => { if (!cancelled) html = `<pre>${unmask(escapeHtml(masked), labels)}</pre>` })
    return () => { cancelled = true }
  })

  async function copy() {
    await navigator.clipboard.writeText(sql).catch(() => {})
    copied = true
    clearTimeout(copyTimer)
    copyTimer = setTimeout(() => (copied = false), 1500)
  }

  $effect(() => () => clearTimeout(copyTimer))
</script>

<div class={cn('flex flex-col gap-1.5', className)}>
  <div class="flex items-center gap-2">
    <button
      type="button"
      class="-ml-1 flex h-7 items-center gap-1 rounded-md px-1 text-ui-xs text-muted-foreground transition-colors hover:text-foreground"
      aria-expanded={expanded}
      onclick={() => (expanded = !expanded)}
    >
      <ChevronRight class={cn('size-3.5 shrink-0 transition-transform duration-150', expanded && 'rotate-90')} />
      SQL preview
    </button>
    <button
      type="button"
      class="ml-auto flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      onclick={copy}
      aria-label="Copy SQL"
    >
      {#if copied}
        <Check class="size-3.5 shrink-0 text-success" />Copied
      {:else}
        <Copy class="size-3.5 shrink-0" />Copy
      {/if}
    </button>
  </div>
  {#if expanded}
    <div
      data-studio-selectable="text"
      class="app-scroll max-h-64 overflow-auto rounded-lg bg-muted/40 [&_.ddl-ph]:text-muted-foreground [&_.ddl-ph]:italic [&_pre]:m-0 [&_pre]:bg-transparent! [&_pre]:px-3.5 [&_pre]:py-3 [&_pre]:font-mono [&_pre]:text-ui-xs [&_pre]:leading-relaxed [&_pre]:whitespace-pre"
    >
      {@html html}
    </div>
  {/if}
</div>
