<script>
  // Side panel for the selected Redis key: its value, paged, with edits staged
  // into the page's pending list (applied together with the table's edits).
  import { redisKeyValue } from '$lib/api.js'
  import { cn } from '$lib/utils.js'
  import { LIST_TOMBSTONE, humanBytes, stagedFor, ttlLabel } from '$lib/redis-staging.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import X from '@lucide/svelte/icons/x'
  import Copy from '@lucide/svelte/icons/copy'
  import Check from '@lucide/svelte/icons/check'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Undo2 from '@lucide/svelte/icons/undo-2'
  import Plus from '@lucide/svelte/icons/plus'
  import Braces from '@lucide/svelte/icons/braces'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import { onDestroy } from 'svelte'

  /**
   * @typedef {import('$lib/api.js').RedisKeyRow} RedisKeyRow
   * @typedef {import('$lib/api.js').RedisKeyValue} RedisKeyValue
   * @typedef {import('$lib/api.js').RedisValueEntry} RedisValueEntry
   * @typedef {import('$lib/redis-staging.js').Staged} Staged
   */

  /** @type {{
   *   row: RedisKeyRow, meta?: { icon: any, color: string, label: string },
   *   staged: Staged[], now: number, reloadToken?: number, readOnly?: boolean,
   *   onstage: (item: Omit<Staged, 'id'>) => void,
   *   onunstage: (key: string, cell: string) => void,
   *   onclose: () => void,
   * }} */
  let { row, meta, staged, now, reloadToken = 0, readOnly = false, onstage, onunstage, onclose } = $props()

  const PAGE = 200
  let data = $state(/** @type {RedisKeyValue | null} */ (null))
  let entries = $state.raw(/** @type {RedisValueEntry[]} */ ([]))
  let loading = $state(false)
  let loadingMore = $state(false)
  let error = $state('')
  let fetchedAt = $state(0)
  let loadSeq = 0

  const ref = $derived({ key: row.key, binary: row.binary })
  const keyDeleted = $derived(!!stagedFor(staged, row.key, 'row'))

  async function load() {
    const seq = ++loadSeq
    loading = true
    error = ''
    try {
      const v = await redisKeyValue(ref, null, PAGE)
      if (seq !== loadSeq) return
      data = v
      entries = v.entries
      fetchedAt = Date.now()
      textDraft = null
    } catch (e) {
      if (seq !== loadSeq) return
      data = null
      entries = []
      error = String(e).replace(/^Error:\s*/, '')
    } finally {
      if (seq === loadSeq) loading = false
    }
  }

  async function loadMore() {
    if (!data?.next || loadingMore) return
    loadingMore = true
    const seq = loadSeq
    try {
      const v = await redisKeyValue(ref, data.next, PAGE)
      if (seq !== loadSeq) return
      data = { ...data, next: v.next }
      entries = [...entries, ...v.entries]
    } catch (e) {
      error = String(e).replace(/^Error:\s*/, '')
    } finally {
      loadingMore = false
    }
  }

  // Reload when the key changes or the page applied changes.
  $effect(() => {
    void row.key
    void reloadToken
    void load()
  })

  const type = $derived(data?.type ?? row.type)

  // ── Entry identity and staging ────────────────────────────────────────────
  /** @param {RedisValueEntry} e */
  function cellOf(e) {
    if (type === 'hash') return `e:f:${e.field}`
    if (type === 'list') return `e:i:${e.index}`
    if (type === 'stream') return `e:id:${e.field}`
    return `e:m:${e.value}`
  }

  /** @param {RedisValueEntry} e */
  function stageDelete(e) {
    const k = row.key
    const cell = cellOf(e)
    /** @type {string[][]} */
    let ops
    if (type === 'hash') ops = [['HDEL', k, String(e.field)]]
    else if (type === 'set') ops = [['SREM', k, e.value]]
    else if (type === 'zset') ops = [['ZREM', k, e.value]]
    else if (type === 'stream') ops = [['XDEL', k, String(e.field)]]
    else if (type === 'list') {
      // Tombstone by index now; one LREM after all edits removes them, so the
      // other staged indexes stay valid.
      ops = [['LSET', k, String(e.index), LIST_TOMBSTONE]]
      onstage({ key: k, binary: row.binary, cell: 'post:list-tombstones', label: 'remove deleted elements', ops: [['LREM', k, '0', LIST_TOMBSTONE]] })
    } else return
    onstage({ key: k, binary: row.binary, cell, label: `delete ${entryName(e)}`, deleted: true, ops })
  }

  /** @param {RedisValueEntry} e @param {string} next @param {'value'|'score'} what */
  function stageEdit(e, next, what) {
    const k = row.key
    const cell = cellOf(e)
    const original = what === 'score' ? e.score ?? '' : e.value
    if (next === original) {
      onunstage(k, cell)
      return
    }
    /** @type {string[][]} */
    let ops
    if (type === 'hash') ops = [['HSET', k, String(e.field), next]]
    else if (type === 'list') ops = [['LSET', k, String(e.index), next]]
    else if (type === 'set') ops = [['SREM', k, e.value], ['SADD', k, next]]
    else if (type === 'zset') ops = what === 'score' ? [['ZADD', k, next, e.value]] : [['ZREM', k, e.value], ['ZADD', k, e.score ?? '0', next]]
    else return
    onstage({ key: k, binary: row.binary, cell, label: `edit ${entryName(e)}`, display: next, ops })
  }

  /** @param {RedisValueEntry} e */
  function entryName(e) {
    if (type === 'hash') return `field ${e.field}`
    if (type === 'list') return `element #${e.index}`
    if (type === 'stream') return `entry ${e.field}`
    return `member ${e.value}`
  }

  // Entries added in this session (staged, not yet in Redis).
  const added = $derived(staged.filter((s) => s.key === row.key && s.cell.startsWith('e:add:')))
  let addSeq = 0
  let addField = $state('')
  let addValue = $state('')
  let addScore = $state('')

  function stageAdd() {
    const k = row.key
    const v = addValue
    if (type === 'hash' && !addField.trim()) return
    /** @type {string[][]} */
    let ops
    let label
    if (type === 'hash') { ops = [['HSET', k, addField, v]]; label = `${addField}: ${v}` }
    else if (type === 'list') { ops = [['RPUSH', k, v]]; label = v }
    else if (type === 'set') { ops = [['SADD', k, v]]; label = v }
    else if (type === 'zset') { ops = [['ZADD', k, addScore.trim() || '0', v]]; label = `${v} (${addScore.trim() || '0'})` }
    else return
    onstage({ key: k, binary: row.binary, cell: `e:add:${Date.now()}-${addSeq++}`, label: `add ${label}`, display: label, ops })
    addField = ''
    addValue = ''
    addScore = ''
  }

  // ── Inline entry editing ──────────────────────────────────────────────────
  let editing = $state(/** @type {{ cell: string, what: 'value'|'score' } | null} */ (null))
  let draft = $state('')

  /** @param {RedisValueEntry} e @param {'value'|'score'} what */
  function startEdit(e, what) {
    if (readOnly || keyDeleted || type === 'stream') return
    const s = stagedFor(staged, row.key, cellOf(e))
    if (s?.deleted) return
    editing = { cell: cellOf(e), what }
    draft = what === 'score' ? (s && type === 'zset' && s.ops[0][0] === 'ZADD' && s.ops.length === 1 ? s.ops[0][2] : e.score ?? '') : s?.display ?? e.value
  }

  /** @param {RedisValueEntry} e */
  function commitEdit(e) {
    if (!editing) return
    stageEdit(e, draft, editing.what)
    editing = null
  }

  /** @param {KeyboardEvent} ev @param {RedisValueEntry} e */
  function onEditKey(ev, e) {
    if (ev.key === 'Enter' && !ev.shiftKey) {
      ev.preventDefault()
      commitEdit(e)
    } else if (ev.key === 'Escape') {
      ev.preventDefault()
      ev.stopPropagation()
      editing = null
    }
  }

  /** @param {HTMLElement} node */
  function focusOnMount(node) {
    node.focus()
    if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement) node.select()
  }

  // ── String value ──────────────────────────────────────────────────────────
  let textDraft = $state(/** @type {string | null} */ (null))
  let pretty = $state(false)
  const stagedText = $derived(stagedFor(staged, row.key, 'value'))
  const currentText = $derived(stagedText?.display ?? data?.text ?? '')
  const parsedJson = $derived.by(() => {
    const t = currentText.trim()
    if (!t || (t[0] !== '{' && t[0] !== '[')) return null
    try { return JSON.parse(t) } catch { return null }
  })
  const shownText = $derived(textDraft ?? (pretty && parsedJson != null ? JSON.stringify(parsedJson, null, 2) : currentText))
  const textEditable = $derived(!readOnly && !keyDeleted && !data?.text_truncated)

  function stageText() {
    if (textDraft == null) return
    let next = textDraft
    // Pretty-printed JSON goes back compact, the way it came.
    if (pretty && parsedJson != null) {
      try { next = JSON.stringify(JSON.parse(next)) } catch { /* keep as typed */ }
    }
    if (next === (data?.text ?? '')) onunstage(row.key, 'value')
    else onstage({ key: row.key, binary: row.binary, cell: 'value', label: 'set value', display: next, ops: [['SET', row.key, next]] })
    textDraft = null
  }

  // ── Copy ──────────────────────────────────────────────────────────────────
  let copied = $state('')
  /** @type {ReturnType<typeof setTimeout> | null} */
  let copiedTimer = null
  onDestroy(() => { if (copiedTimer) clearTimeout(copiedTimer) })
  /** @param {string} text @param {string} id */
  function copy(text, id) {
    navigator.clipboard.writeText(text).then(() => {
      copied = id
      if (copiedTimer) clearTimeout(copiedTimer)
      copiedTimer = setTimeout(() => (copied = ''), 1200)
    }).catch(() => {})
  }

  const ttl = $derived(data ? ttlLabel(data.ttl_ms, fetchedAt, now) : '')
  const countLabel = $derived.by(() => {
    if (!data || data.length == null) return ''
    const unit = { string: 'chars', hash: 'fields', list: 'elements', set: 'members', zset: 'members', stream: 'entries' }[type] ?? 'items'
    return `${data.length.toLocaleString()} ${unit}`
  })
  const cols = $derived(
    type === 'hash' ? 'grid-cols-[minmax(80px,1fr)_minmax(0,2fr)_48px]'
    : type === 'list' ? 'grid-cols-[44px_minmax(0,1fr)_48px]'
    : type === 'zset' ? 'grid-cols-[minmax(0,1fr)_88px_48px]'
    : type === 'stream' ? 'grid-cols-[150px_minmax(0,1fr)_48px]'
    : 'grid-cols-[minmax(0,1fr)_48px]',
  )
  const Glyph = $derived(meta?.icon)
</script>

<aside class="flex h-full min-w-0 flex-col bg-panel" aria-label="Key value">
  <header class="flex h-11 shrink-0 items-center gap-2 pl-4 pr-2">
    {#if Glyph}<Glyph class={cn('size-4 shrink-0', meta?.color)} />{/if}
    <span class={cn('min-w-0 flex-1 truncate font-mono text-ui-sm text-foreground', keyDeleted && 'text-destructive line-through')} title={row.key}>{row.key}</span>
    <button type="button" class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" title="Copy key name" aria-label="Copy key name" onclick={() => copy(row.key, 'key')}>
      {#if copied === 'key'}<Check class="size-3.5 shrink-0 text-success" />{:else}<Copy class="size-3.5 shrink-0" />{/if}
    </button>
    <button type="button" class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" title="Close panel" aria-label="Close panel" onclick={onclose}>
      <X class="size-3.5 shrink-0" />
    </button>
  </header>

  <!-- Facts as one quiet line of pills: type, size, memory, encoding, expiry. -->
  <div class="flex shrink-0 flex-wrap items-center gap-1.5 border-b border-border/30 px-4 pb-3 text-ui-xs text-muted-foreground">
    <span class="inline-flex h-6 items-center gap-1.5 rounded-full bg-muted/50 px-2.5 text-foreground/85">
      <span class={cn('size-1.5 rounded-full bg-current', meta?.color)}></span>{(meta?.label ?? type).replace(/^./, (c) => c.toUpperCase())}
    </span>
    {#if countLabel}<span class="inline-flex h-6 items-center rounded-full bg-muted/30 px-2.5 tabular-nums">{countLabel}</span>{/if}
    {#if data?.memory != null}<span class="inline-flex h-6 items-center rounded-full bg-muted/30 px-2.5 tabular-nums">{humanBytes(data.memory)}</span>{/if}
    {#if data?.encoding}<span class="inline-flex h-6 items-center rounded-full bg-muted/30 px-2.5" title="Redis internal encoding">{data.encoding}</span>{/if}
    <span class={cn('inline-flex h-6 items-center rounded-full px-2.5 tabular-nums', ttl ? 'bg-warning/10 text-warning' : 'bg-muted/30')}>{ttl ? `Expires in ${ttl}` : 'No expiry'}</span>
  </div>

  <div class="min-h-0 flex-1 overflow-hidden">
    {#if loading && !data}
      <div class="flex h-full items-center justify-center"><LoaderCircle class="size-4 shrink-0 animate-spin text-muted-foreground" /></div>
    {:else if error}
      <div class="px-4 py-10 text-center">
        <p class="text-ui-xs text-destructive">{error}</p>
        <Button variant="ghost" size="sm" class="mt-2" onclick={() => void load()}>Retry</Button>
      </div>
    {:else if type === 'string'}
      <div class="group/val relative flex h-full min-h-0 flex-col">
        {#if parsedJson != null || stagedText}
        <div class="flex h-10 shrink-0 items-center gap-1.5 pl-3 pr-12">
          {#if parsedJson != null}
            <Button variant={pretty ? 'secondary' : 'ghost'} size="xs" aria-pressed={pretty} onclick={() => { pretty = !pretty; textDraft = null }}>
              <Braces /> Format JSON
            </Button>
          {/if}
          {#if stagedText}<span class="text-ui-2xs text-warning">Edited, not applied</span>{/if}
          <span class="flex-1"></span>
          {#if stagedText && textDraft == null}
            <Button variant="ghost" size="xs" onclick={() => onunstage(row.key, 'value')}><Undo2 /> Revert</Button>
          {/if}
        </div>
        {/if}
        <!-- Copy floats over the value's corner on hover instead of owning a toolbar row. -->
        <button type="button" class="absolute right-3 top-1.5 z-10 inline-flex size-7 items-center justify-center rounded-md bg-panel/90 text-muted-foreground opacity-0 transition-[opacity,color] group-hover/val:opacity-100 hover:bg-accent hover:text-foreground focus-visible:opacity-100" title="Copy value" aria-label="Copy value" onclick={() => copy(currentText, 'value')}>
          {#if copied === 'value'}<Check class="size-3.5 shrink-0 text-success" />{:else}<Copy class="size-3.5 shrink-0" />{/if}
        </button>
        {#if data?.text_truncated}
          <p class="px-4 pt-3 text-ui-xs text-muted-foreground">Showing the first 512 KB. Values this large are read-only here; use the console to change them.</p>
        {/if}
        <textarea
          class={cn('app-scroll no-focus-ring no-field-frame min-h-0 flex-1 resize-none border-0 bg-transparent px-4 py-3 font-mono text-ui-sm leading-relaxed text-foreground shadow-none outline-none', stagedText && textDraft == null && 'bg-warning/5')}
          value={shownText}
          readonly={!textEditable}
          spellcheck="false"
          aria-label="String value"
          oninput={(e) => (textDraft = e.currentTarget.value)}
          onkeydown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && textDraft != null) { e.preventDefault(); e.stopPropagation(); stageText() }
            else if (e.key === 'Escape' && textDraft != null) { e.preventDefault(); e.stopPropagation(); textDraft = null }
          }}
        ></textarea>
        {#if textDraft != null}
          <!-- Editing: the actions sit under the text, where the eye already is. -->
          <div class="flex h-12 shrink-0 items-center gap-2 border-t border-border/30 px-4">
            <span class="min-w-0 flex-1 truncate text-ui-2xs text-muted-foreground">Staged changes apply with Apply (⌘S)</span>
            <Button variant="ghost" size="sm" class="rounded-md" onclick={() => (textDraft = null)}>Cancel</Button>
            <Button size="sm" class="rounded-md" onclick={stageText} title="Stage this change (⌘↵)">Stage change</Button>
          </div>
        {/if}
      </div>
    {:else if ['hash', 'list', 'set', 'zset', 'stream'].includes(type)}
      <div class="app-scroll h-full overflow-auto" data-studio-selectable="text">
        <div class={cn('sticky top-0 z-10 grid h-8 items-center gap-2 border-b border-border/30 bg-panel px-4 text-ui-xs font-medium text-muted-foreground', cols)}>
          {#if type === 'hash'}<span>Field</span><span>Value</span>
          {:else if type === 'list'}<span>#</span><span>Element</span>
          {:else if type === 'zset'}<span>Member</span><span class="text-right">Score</span>
          {:else if type === 'stream'}<span>ID</span><span>Fields</span>
          {:else}<span>Member</span>{/if}
          <span></span>
        </div>

        {#each entries as e (cellOf(e))}
          {@const s = stagedFor(staged, row.key, cellOf(e))}
          {@const isEditing = editing?.cell === cellOf(e)}
          <div
            class={cn(
              'group/entry grid min-h-8 items-start gap-2 border-b border-border/20 px-4 py-1.5 font-mono text-ui-xs',
              cols,
              s?.deleted ? 'bg-destructive/5 text-destructive line-through' : s ? 'bg-warning/10' : 'hover:bg-accent/40',
            )}
          >
            {#if type === 'hash'}
              <span class="truncate pt-px text-muted-foreground" title={e.field ?? ''}>{e.field}</span>
            {:else if type === 'list'}
              <span class="pt-px tabular-nums text-muted-foreground">{e.index}</span>
            {:else if type === 'stream'}
              <span class="truncate pt-px text-muted-foreground">{e.field}</span>
            {/if}

            {#if isEditing && editing?.what === 'value'}
              <textarea
                use:focusOnMount
                bind:value={draft}
                rows={Math.min(8, Math.max(1, draft.split('\n').length))}
                class="field-surface w-full resize-y bg-input/30 px-1.5 py-0.5 font-mono text-ui-xs text-foreground"
                aria-label="Edit value"
                onkeydown={(ev) => onEditKey(ev, e)}
                onblur={() => commitEdit(e)}
              ></textarea>
            {:else}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span
                class={cn('min-w-0 whitespace-pre-wrap break-all pt-px text-foreground', s?.deleted && 'text-destructive')}
                ondblclick={() => startEdit(e, 'value')}
              >{s && !s.deleted && type !== 'zset' ? s.display : e.value}</span>
            {/if}

            {#if type === 'zset'}
              {#if isEditing && editing?.what === 'score'}
                <input
                  use:focusOnMount
                  bind:value={draft}
                  class="field-surface h-6 w-full bg-input/30 px-1.5 text-right font-mono text-ui-xs tabular-nums text-foreground"
                  aria-label="Edit score"
                  onkeydown={(ev) => onEditKey(ev, e)}
                  onblur={() => commitEdit(e)}
                />
              {:else}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <span class="pt-px text-right tabular-nums text-muted-foreground" ondblclick={() => startEdit(e, 'score')}>
                  {s && !s.deleted && s.ops.length === 1 ? s.ops[0][2] : e.score}
                </span>
              {/if}
            {/if}

            <span class="flex justify-end gap-0.5 opacity-0 transition-opacity group-hover/entry:opacity-100 focus-within:opacity-100">
              {#if s}
                <button type="button" class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" title="Undo" aria-label="Undo change" onclick={() => onunstage(row.key, cellOf(e))}>
                  <Undo2 class="size-3.5 shrink-0" />
                </button>
              {:else if !readOnly && !keyDeleted}
                <button type="button" class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive" title="Delete" aria-label={`Delete ${entryName(e)}`} onclick={() => stageDelete(e)}>
                  <Trash2 class="size-3.5 shrink-0" />
                </button>
              {/if}
            </span>
          </div>
        {/each}

        {#each added as a (a.cell)}
          <div class={cn('grid min-h-7 items-center gap-2 border-b border-border/40 bg-success/10 px-3 py-1 font-mono text-ui-xs text-foreground', 'grid-cols-[minmax(0,1fr)_48px]')}>
            <span class="min-w-0 truncate">+ {a.display}</span>
            <span class="flex justify-end">
              <button type="button" class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" title="Remove" aria-label="Remove staged entry" onclick={() => onunstage(row.key, a.cell)}>
                <X class="size-3.5 shrink-0" />
              </button>
            </span>
          </div>
        {/each}

        {#if data?.next}
          <div class="flex justify-center py-2">
            <Button variant="ghost" size="xs" disabled={loadingMore} onclick={() => void loadMore()}>
              {#if loadingMore}<LoaderCircle class="animate-spin" />{/if}
              Load more ({entries.length.toLocaleString()} of {data.length?.toLocaleString() ?? '?'})
            </Button>
          </div>
        {:else if entries.length === 0 && added.length === 0}
          <p class="px-3 py-6 text-center text-ui-xs text-muted-foreground">Empty</p>
        {/if}
      </div>
    {:else}
      <p class="px-4 py-10 text-center text-ui-xs text-muted-foreground">
        {type} values can't be shown here yet. Use the console to read them.
      </p>
    {/if}
  </div>

  {#if !readOnly && !keyDeleted && ['hash', 'list', 'set', 'zset'].includes(type)}
    <form
      class="flex shrink-0 items-center gap-1.5 border-t border-border px-3 py-2"
      onsubmit={(ev) => { ev.preventDefault(); stageAdd() }}
    >
      {#if type === 'hash'}
        <input bind:value={addField} placeholder="field" aria-label="New field name" spellcheck="false" autocomplete="off" class="field-surface shadow-none h-7 w-28 min-w-0 px-2 font-mono text-ui-xs text-foreground placeholder:text-muted-foreground" />
      {/if}
      <input bind:value={addValue} placeholder={type === 'hash' ? 'value' : type === 'list' ? 'append element' : 'member'} aria-label="New value" spellcheck="false" autocomplete="off" class="field-surface shadow-none h-7 min-w-0 flex-1 px-2 font-mono text-ui-xs text-foreground placeholder:text-muted-foreground" />
      {#if type === 'zset'}
        <input bind:value={addScore} placeholder="score" aria-label="Score" inputmode="decimal" spellcheck="false" autocomplete="off" class="field-surface shadow-none h-7 w-20 min-w-0 px-2 text-right font-mono text-ui-xs tabular-nums text-foreground placeholder:text-muted-foreground" />
      {/if}
      <Button type="submit" variant="outline" size="icon-sm" aria-label="Stage new entry" title="Add (staged)" disabled={type === 'hash' && !addField.trim()}>
        <Plus />
      </Button>
    </form>
  {/if}
</aside>
