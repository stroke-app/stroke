<script>
  import { Button } from '$lib/components/ui/button/index.js'
  import { virtualColumnsStore } from '$lib/stores/virtual-columns.js'
  import { templateRefs, bindExpr, FN_SNIPPETS } from '$lib/virtual-column.js'
  import ResizeHandle from './ResizeHandle.svelte'
  import { getAppScale } from '$lib/app-zoom.js'
  import Plus from '@lucide/svelte/icons/plus'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Pencil from '@lucide/svelte/icons/pencil'
  import Check from '@lucide/svelte/icons/check'
  import X from '@lucide/svelte/icons/x'
  import Eye from '@lucide/svelte/icons/eye'
  import EyeOff from '@lucide/svelte/icons/eye-off'
  import Search from '@lucide/svelte/icons/search'
  import FunctionSquare from '@lucide/svelte/icons/function-square'
  import { cn } from '$lib/utils.js'

  /**
   * @type {{
   *   schema: string,
   *   tableName: string,
   *   columns?: {name: string, dataType?: string}[],
   *   rows?: unknown[][],
   *   nameToIdx?: Map<string,number>,
   *   onclose: () => void
   * }}
   */
  let { schema, tableName, columns = [], rows = [], nameToIdx, onclose } = $props()

  // ── Resizable width ────────────────────────────────────────────────────────
  const MIN_W = 260
  const MAX_W = 520
  const DEFAULT_W = 320

  function loadWidth() {
    try { return Math.min(MAX_W, Math.max(MIN_W, Number(localStorage.getItem('stroke:vcol-panel-w') || DEFAULT_W))) }
    catch { return DEFAULT_W }
  }
  let panelWidth = $state(loadWidth())
  let resizeStartWidth = 0
  /** App scale sampled at drag start - `dx` is screen px, `panelWidth` is px at 100%. */
  let resizeScale = 1

  function saveWidth(w) {
    try { localStorage.setItem('stroke:vcol-panel-w', String(w)) } catch {}
  }

  const tableKey = $derived(`${schema}.${tableName}`)
  const vcols = $derived($virtualColumnsStore[tableKey] ?? [])
  const fnGroups = $derived(
    ['String', 'Math', 'Logic'].map(g => ({
      label: g,
      fns: FN_SNIPPETS.filter(f => f.group === g),
    }))
  )

  /** @type {string|null} */
  let editId = $state(null)
  let formOpen = $state(false)
  let draftName = $state('')
  let draftExpr = $state('')
  let colSearch = $state('')

  const filteredColumns = $derived(
    colSearch.trim()
      ? columns.filter(c => c.name.toLowerCase().includes(colSearch.toLowerCase()))
      : columns
  )

  const draftRefs = $derived(templateRefs(draftExpr))
  const badRefs = $derived(draftRefs.filter(r => !columns.some(c => c.name === r)))
  const canSave = $derived(draftName.trim().length > 0 && draftExpr.trim().length > 0 && badRefs.length === 0)

  function preview(expr, row) {
    if (!row || !nameToIdx) return '—'
    try {
      const fn = bindExpr(expr, nameToIdx)
      const val = fn(row)
      return val === '' ? '(empty)' : val
    } catch { return '—' }
  }

  const previewRows = $derived(rows.slice(0, 2))

  function openAdd()  { editId = null; draftName = ''; draftExpr = ''; colSearch = ''; formOpen = true }
  function openEdit(col) { editId = col.id; draftName = col.name; draftExpr = col.expression; colSearch = ''; formOpen = true }
  function save() {
    if (!canSave) return
    const name = draftName.trim(), expression = draftExpr.trim()
    if (editId) virtualColumnsStore.patch(tableKey, editId, { name, expression })
    else        virtualColumnsStore.add(tableKey, { name, expression, enabled: true })
    formOpen = false; editId = null
  }
  function cancelForm() { formOpen = false; editId = null }

  /** @type {HTMLTextAreaElement|null} */
  let exprEl = $state(null)

  function insertAt(text, cursor) {
    if (!exprEl) { draftExpr += text; return }
    const start = exprEl.selectionStart ?? draftExpr.length
    const end = exprEl.selectionEnd   ?? draftExpr.length
    draftExpr = draftExpr.slice(0, start) + text + draftExpr.slice(end)
    const pos = cursor !== null ? start + cursor : start + text.length
    requestAnimationFrame(() => { exprEl?.setSelectionRange(pos, pos); exprEl?.focus() })
  }

  function insertCol(colName) { insertAt(`{${colName}}`, null) }
  function insertFn(snip)     { insertAt(snip.insert, snip.cursor) }
</script>

<ResizeHandle
  axis="x"
  edge="start"
  onresizestart={() => { resizeStartWidth = panelWidth; resizeScale = getAppScale() }}
  onresize={(dx) => { panelWidth = Math.min(MAX_W, Math.max(MIN_W, resizeStartWidth + dx / resizeScale)) }}
  onresizeend={() => saveWidth(panelWidth)}
/>

<div
  class="flex h-full shrink-0 flex-col border-l border-border/40 bg-panel text-ui-sm"
  style="width: calc({panelWidth}px * var(--app-scale, 1))"
  role="none"
  onkeydown={(e) => {
    if (e.key !== 'Escape') return
    if (formOpen) { e.stopPropagation(); cancelForm() }
  }}
>

  <!-- Header -->
  <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3">
    <FunctionSquare class="size-3.5 shrink-0 text-primary" />
    <span class="shrink-0 whitespace-nowrap text-ui-sm font-semibold tracking-tight">Virtual columns</span>
    <span
      title={tableName}
      class="ml-auto min-w-0 truncate rounded bg-muted/60 px-1.5 py-0.5 font-mono text-ui-2xs text-muted-foreground"
    >{tableName}</span>
    <button
      type="button"
      class="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
      onclick={onclose}
    ><X class="size-3 shrink-0" /></button>
  </div>

  <div class="flex min-h-0 flex-1 flex-col overflow-hidden">

    {#if formOpen}
      <!-- ── Form ────────────────────────────────────────────────────────── -->
      <div class="flex min-h-0 flex-1 flex-col overflow-y-auto">

        <!-- Section title -->
        <div class="flex h-8 shrink-0 items-center border-b border-border/50 px-3">
          <span class="text-ui-xs font-semibold text-foreground/70">
            {editId ? 'Edit column' : 'New column'}
          </span>
        </div>

        <!-- Name ─────────────────────────────────────────────────────────── -->
        <div class="border-b border-border/40 px-3 py-3">
          <label class="block">
            <span class="mb-1.5 block text-ui-xs font-medium text-foreground/60">Name</span>
            <input
              type="text"
              bind:value={draftName}
              placeholder="full_url"
              class="w-full rounded-lg border-2 border-border bg-muted/30 px-2.5 py-1.5 font-mono text-ui-sm text-foreground outline-none placeholder:text-muted-foreground focus:border-primary/50 focus:bg-background"
            />
          </label>
        </div>

        <!-- Expression ───────────────────────────────────────────────────── -->
        <div class="border-b border-border/40 px-3 py-3">
          <div class="mb-1.5 flex items-center justify-between">
            <span class="text-ui-xs font-medium text-foreground/60">Expression</span>
            <span class="rounded bg-muted/50 px-1.5 font-mono text-ui-2xs text-muted-foreground">{'{ col }'}</span>
          </div>
          <textarea
            bind:this={exprEl}
            bind:value={draftExpr}
            rows={3}
            spellcheck={false}
            placeholder={"https://cdn.com/{file_key}"}
            class="w-full resize-none rounded-lg border-2 border-border bg-muted/30 px-2.5 py-1.5 font-mono text-ui-sm leading-relaxed text-foreground outline-none placeholder:text-muted-foreground focus:border-primary/50 focus:bg-background"
          ></textarea>
          {#if badRefs.length > 0}
            <p class="mt-1.5 text-ui-2xs text-destructive">Unknown: {badRefs.map(r => `{${r}}`).join(', ')}</p>
          {/if}
        </div>

        <!-- Columns ──────────────────────────────────────────────────────── -->
        {#if columns.length > 0}
          <div class="border-b border-border/40 px-3 py-3">
            <p class="mb-2 text-ui-xs font-medium text-foreground/60">Columns</p>
            <div class="mb-1.5 flex items-center gap-1.5 rounded border border-border/50 bg-muted/30 px-2 py-1">
              <Search class="size-3.5 shrink-0 text-muted-foreground" />
              <input
                type="text"
                bind:value={colSearch}
                placeholder="Search columns…"
                class="no-focus-ring min-w-0 flex-1 bg-transparent text-ui-xs text-foreground outline-none placeholder:text-muted-foreground"
                autocomplete="off"
                spellcheck="false"
              />
              {#if colSearch}
                <button
                  type="button"
                  class="flex size-3.5 items-center justify-center rounded-md text-muted-foreground hover:text-foreground"
                  onclick={() => { colSearch = '' }}
                ><X class="size-3 shrink-0" /></button>
              {/if}
            </div>
            <div class="max-h-40 overflow-y-auto rounded border border-border/40 bg-background/60">
              {#if filteredColumns.length === 0}
                <p class="px-2.5 py-2.5 text-ui-2xs text-muted-foreground">No columns match</p>
              {:else}
                {#each filteredColumns as col (col.name)}
                  <button
                    type="button"
                    class={cn(
                      "flex w-full items-center gap-2 border-b border-border/30 px-2.5 py-1.5 text-left last:border-0 transition-colors hover:bg-accent/30",
                      draftRefs.includes(col.name) ? "text-primary" : "text-foreground/80"
                    )}
                    onclick={() => insertCol(col.name)}
                  >
                    <span class="min-w-0 flex-1 truncate font-mono text-ui-xs">{col.name}</span>
                    {#if draftRefs.includes(col.name)}
                      <Check class="size-3 shrink-0 text-primary" />
                    {/if}
                  </button>
                {/each}
              {/if}
            </div>
          </div>
        {/if}

        <!-- Functions ────────────────────────────────────────────────────── -->
        <div class="border-b border-border/40 px-3 py-3">
          <p class="mb-3 text-ui-xs font-medium text-foreground/60">Functions</p>
          <div class="flex flex-col gap-3">
            {#each fnGroups as group}
              <div>
                <p class="mb-1.5 text-ui-2xs font-semibold uppercase tracking-[0.06em] text-muted-foreground">{group.label}</p>
                <div class="flex flex-wrap gap-1">
                  {#each group.fns as fn}
                    <button
                      type="button"
                      class="inline-flex items-center justify-center rounded-md border border-border/60 bg-muted/50 px-2 py-1 font-mono text-ui-xs font-medium text-foreground/70 transition-colors hover:border-primary/40 hover:bg-primary/10 hover:text-primary"
                      onclick={() => insertFn(fn)}
                    >{fn.label}</button>
                  {/each}
                </div>
              </div>
            {/each}
          </div>
        </div>

        <!-- Preview ──────────────────────────────────────────────────────── -->
        <div class="border-b border-border/40 px-3 py-3">
          <p class="mb-2 text-ui-xs font-medium text-foreground/60">Preview</p>
          {#if draftExpr.trim() && previewRows.length > 0}
            <div class="overflow-hidden rounded border border-border/40 bg-muted/20">
              {#each previewRows as row, i}
                {@const val = preview(draftExpr, row)}
                <div class="flex items-center gap-2.5 px-2.5 py-1.5 {i > 0 ? 'border-t border-border/30' : ''}">
                  <span class="shrink-0 font-mono text-ui-2xs tabular-nums text-muted-foreground">#{i+1}</span>
                  <span class={cn(
                    "min-w-0 flex-1 truncate font-mono text-ui-xs",
                    val === '(empty)' || val === '—' ? "italic text-muted-foreground" : "text-foreground/80"
                  )}>{val}</span>
                </div>
              {/each}
            </div>
          {:else}
            <div class="rounded border border-border/30 bg-muted/15 px-2.5 py-2 text-ui-xs italic text-muted-foreground">
              Enter an expression above to preview
            </div>
          {/if}
        </div>

        <!-- Actions ──────────────────────────────────────────────────────── -->
        <div class="shrink-0 border-t border-border/50 px-3 py-3">
          <div class="flex gap-2">
            <Button
              onclick={save}
              disabled={!canSave}
              class="flex-1"
>
              <Check class="size-3.5 shrink-0" /> Save
            </Button>
            <button
              type="button"
              onclick={cancelForm}
              class="inline-flex h-8 items-center justify-center rounded-md border border-border/50 px-3.5 text-ui-xs font-medium text-foreground/65 transition-colors hover:bg-muted hover:text-foreground"
            >Cancel</button>
          </div>
        </div>

      </div>

    {:else}
      <!-- ── Column list ──────────────────────────────────────────────────── -->
      <div class="flex-1 overflow-y-auto">
        {#if vcols.length === 0}
          <div class="px-3 py-6">
            <div class="flex items-start gap-2.5">
              <span class="flex size-7 shrink-0 items-center justify-center rounded-md border border-border/30 bg-muted/20">
                <FunctionSquare class="size-3.5 shrink-0 text-muted-foreground" />
              </span>
              <div class="min-w-0">
                <p class="text-ui-sm font-medium text-foreground/60">No virtual columns yet</p>
                <p class="mt-1 text-ui-2xs leading-relaxed text-muted-foreground">
                  Build a column from the ones you already have - join a name, format a date, do the arithmetic once.
                </p>
              </div>
            </div>
            <button
              type="button"
              onclick={openAdd}
              class="mt-4 flex w-full items-center justify-center gap-1.5 rounded-md border border-dashed border-border/50 py-2 text-ui-xs font-medium text-muted-foreground transition-colors hover:border-primary/40 hover:bg-primary/5 hover:text-primary"
            >
              <Plus class="size-3 shrink-0" /> Add virtual column
            </button>
          </div>
        {:else}
          <div class="py-1">
            {#each vcols as col (col.id)}
              {@const firstRowVal = rows[0] ? preview(col.expression, rows[0]) : null}
              <div class={cn(
                "group relative flex items-center gap-2 border-b border-border/40 px-3 py-2.5 last:border-0 transition-colors hover:bg-muted/20",
                !col.enabled && "opacity-40"
              )}>
                <button
                  type="button"
                  class={cn(
                    "hit-area flex size-5 shrink-0 items-center justify-center rounded-md transition-colors",
                    col.enabled
                      ? "text-foreground/50 hover:bg-muted hover:text-foreground"
                      : "text-muted-foreground hover:bg-muted hover:text-foreground"
                  )}
                  title={col.enabled ? 'Disable' : 'Enable'}
                  onclick={() => virtualColumnsStore.patch(tableKey, col.id, { enabled: !col.enabled })}
                >
                  {#if col.enabled}<Eye class="size-3.5 shrink-0" />{:else}<EyeOff class="size-3.5 shrink-0" />{/if}
                </button>
                <div class="min-w-0 flex-1 overflow-hidden">
                  <div class="flex items-baseline gap-1.5">
                    <span class="shrink-0 font-mono text-ui-sm font-medium text-foreground/85">{col.name}</span>
                    {#if firstRowVal && col.enabled && firstRowVal !== '—'}
                      <span class="min-w-0 truncate font-mono text-ui-2xs text-muted-foreground">{firstRowVal}</span>
                    {/if}
                  </div>
                  <p class="truncate font-mono text-ui-2xs text-muted-foreground">{col.expression}</p>
                </div>
                <div class="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
                  <button
                    type="button"
                    class="flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
                    onclick={() => openEdit(col)}
                  ><Pencil class="size-3 shrink-0" /></button>
                  <button
                    type="button"
                    class="flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                    onclick={() => virtualColumnsStore.remove(tableKey, col.id)}
                  ><Trash2 class="size-3 shrink-0" /></button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      {#if vcols.length > 0}
        <!-- Footer: only when there is a list to add to - in the empty state the
             same button already sits under the explanation. -->
        <div class="shrink-0 border-t border-border/50 px-3 py-2.5">
          <button
            type="button"
            onclick={openAdd}
            class="flex w-full items-center justify-center gap-1.5 rounded-md border border-dashed border-border/50 py-2 text-ui-xs font-medium text-muted-foreground transition-colors hover:border-primary/40 hover:bg-primary/5 hover:text-primary"
          >
            <Plus class="size-3 shrink-0" /> Add virtual column
          </button>
        </div>
      {/if}
    {/if}

  </div>
</div>
