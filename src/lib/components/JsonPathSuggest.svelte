<script>
  // The JSONPath suggestion list, shared by every JSON viewer.
  //
  // There were three of these. The JSON tab had a good one - a colour dot per
  // kind, the typed fragment highlighted, the type and a value preview. The SQL
  // and table views had a much poorer one that called `getCompletions`, which
  // throws away `kind`, `detail` and `preview`, and then rebuilt a worse version
  // from the insert string alone: every row printed the same identifier twice
  // and emphasised none of it. This is the good one, made shared, so a fix to
  // the widget is a fix everywhere rather than in whichever copy was noticed.
  import { cn } from '$lib/utils.js'
  import { splitPath } from '$lib/jsonpath.js'
  import Kbd from './Kbd.svelte'

  let {
    /** @type {import('$lib/jsonpath.js').CompletionItem[]} */
    items = [],
    /** The path as typed, for highlighting the fragment being completed. */
    query = '',
    /** Index of the armed row. Bindable so the parent's keyboard handler drives it. */
    activeIdx = $bindable(-1),
    /** @type {(insert: string) => void} */
    onpick = () => {},
  } = $props()

  /** @type {HTMLElement | null} */
  let listEl = $state(null)

  /** What has been typed since the last separator. */
  const token = $derived(splitPath(query).token.replace(/^[.[]/, '').toLowerCase())

  /**
   * Split a label around the typed fragment. Matches anywhere, not just at the
   * start: filtering already accepts an infix match, so highlighting only
   * prefixes left the reason a row was offered invisible.
   * @param {string} label
   */
  function parts(label) {
    if (!token) return { head: label, hit: '', tail: '' }
    const at = label.toLowerCase().indexOf(token)
    if (at === -1) return { head: label, hit: '', tail: '' }
    return { head: label.slice(0, at), hit: label.slice(at, at + token.length), tail: label.slice(at + token.length) }
  }

  /**
   * A small glyph per kind, coloured with the app's JSON tokens (the same
   * colours the JSON view paints values in), and the kind's name.
   * @param {string} kind
   */
  function kindMeta(kind) {
    switch (kind) {
      case 'object':   return { glyph: '{}', tone: 'text-[var(--json-key)]',     label: 'object' }
      case 'array':    return { glyph: '[]', tone: 'text-[var(--json-key)]',     label: 'array' }
      case 'string':   return { glyph: '""', tone: 'text-[var(--json-string)]',  label: 'string' }
      case 'number':   return { glyph: '#',  tone: 'text-[var(--json-number)]',  label: 'number' }
      case 'boolean':  return { glyph: '?',  tone: 'text-[var(--json-boolean)]', label: 'boolean' }
      case 'null':     return { glyph: '∅',  tone: 'text-[var(--json-null)]',    label: 'null' }
      case 'wildcard': return { glyph: '*',  tone: 'text-muted-foreground',      label: 'every item' }
      case 'index':    return { glyph: '0',  tone: 'text-muted-foreground',      label: 'index' }
      case 'filter':   return { glyph: '?()', tone: 'text-muted-foreground',     label: 'filter' }
      case 'slice':    return { glyph: ':',  tone: 'text-muted-foreground',      label: 'slice' }
      default:         return { glyph: '·',  tone: 'text-muted-foreground',      label: '' }
    }
  }

  // Keep the armed row in view once the list outgrows its box.
  $effect(() => {
    if (activeIdx < 0 || !listEl) return
    const row = listEl.querySelector(`[data-idx="${activeIdx}"]`)
    if (row instanceof HTMLElement) row.scrollIntoView({ block: 'nearest' })
  })

  // Is there more list below the fold? The box caps at max-h-64 and simply cut
  // the last visible row off flush, so a 12-item list looked like an 8-item one
  // and the only way to find out otherwise was to press Down and watch it move.
  let moreBelow = $state(false)
  function syncOverflow() {
    const el = listEl
    if (!el) { moreBelow = false; return }
    // 1px of slack: fractional row heights make scrollTop + clientHeight land
    // just shy of scrollHeight at the true bottom.
    moreBelow = el.scrollHeight - el.scrollTop - el.clientHeight > 1
  }
  // Re-measured when the list changes as well as when it scrolls - filtering to
  // fewer items must drop the cue, not leave it pointing at nothing.
  $effect(() => { void items.length; void activeIdx; syncOverflow() })
</script>

<!-- The mousedown handler is not an interaction, it is the opposite of one:
     pressing inside the panel would blur the path input and close the list
     before the click could land, so the default is swallowed. The interactive
     elements are the option buttons inside; this wrapper is scaffolding. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="absolute left-0 top-full z-50 mt-1 min-w-64 max-w-[min(30rem,calc(100vw-3rem))] overflow-hidden rounded-[10px] border border-border/60 bg-popover elevate-2-rim"
  onmousedown={(e) => e.preventDefault()}
>
  <div class="relative">
    <ul bind:this={listEl} onscroll={syncOverflow} class="max-h-64 overflow-y-auto p-1" role="listbox" aria-label="Path suggestions">
      {#each items as item, i (item.insert)}
        {@const on = i === activeIdx}
        {@const p = parts(item.label)}
        {@const meta = kindMeta(item.kind)}
        <li>
          <button
            type="button"
            data-idx={i}
            role="option"
            aria-selected={on}
            class={cn('flex w-full min-w-0 items-center gap-1.5 rounded-sm px-1.5 py-1 text-left', on && 'bg-accent')}
            onmousemove={() => (activeIdx = i)}
            onclick={() => onpick(item.insert)}
          >
            <span class={cn('w-5 shrink-0 text-center font-mono text-ui-3xs font-medium', meta.tone)} aria-hidden="true">{meta.glyph}</span>
            <!-- Typed fragment in full foreground, the rest muted: the reason a
                 row is offered, without a loud accent colour. -->
            <span class={cn('min-w-0 truncate font-mono text-ui-xs', p.hit ? 'text-muted-foreground' : 'text-foreground')}>
              {p.head}<span class="text-foreground">{p.hit}</span>{p.tail}
            </span>
            <span class="ml-auto flex shrink-0 items-center gap-1.5 pl-3 text-ui-3xs text-muted-foreground">
              {#if on && item.preview}
                <span class="max-w-36 truncate font-mono">{item.preview}</span>
                <span class="text-border">·</span>
              {/if}
              <!-- Whether a preview is representative: "5 values" says the key
                   varies across elements; a single value says it doesn't. -->
              {#if item.spread}<span class="font-mono tabular-nums">{item.spread}</span><span class="text-border">·</span>{/if}
              <span>{item.detail || meta.label}</span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
    <!-- The "there is more" cue; purely decorative, the footer states the count. -->
    {#if moreBelow}
      <div class="pointer-events-none absolute inset-x-0 bottom-0 h-6 bg-gradient-to-t from-popover to-transparent"></div>
    {/if}
  </div>

  <!-- A list that simply appears teaches none of its keys. -->
  <div class="flex items-center gap-3 border-t border-border/60 px-2.5 py-1.5 text-ui-3xs text-muted-foreground">
    <span class="flex items-center gap-1"><Kbd keys={['↑', '↓']} /> navigate</span>
    <span class="flex items-center gap-1"><Kbd keys={['↵']} /> accept</span>
    <span class="flex items-center gap-1"><Kbd keys={['esc']} /> close</span>
    <!-- Position, not just the total: answers "is there more?" from the footer. -->
    <span class="ms-auto font-mono tabular-nums">{#if activeIdx >= 0}{activeIdx + 1} / {/if}{items.length}</span>
  </div>
</div>
