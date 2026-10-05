<script>
  import { comboTitle } from '$lib/shortcuts.js'
  import Plus from '@lucide/svelte/icons/plus'
  import Save from '@lucide/svelte/icons/save'
  import FolderOpen from '@lucide/svelte/icons/folder-open'
  import Play from '@lucide/svelte/icons/play'
  import FileCode2 from '@lucide/svelte/icons/file-code-2'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { cn } from '$lib/utils.js'
  import { executeSql } from '$lib/api.js'
  import { saveNotebook, saveNotebookAs, openNotebookFile } from '$lib/api.js'
  import {
    createSqlCell,
    createMarkdownCell,
    serializeNotebook,
    deserializeNotebook,
    titleFromPath,
  } from '$lib/notebook.js'
  import SqlCell from './notebook/SqlCell.svelte'
  import MarkdownCell from './notebook/MarkdownCell.svelte'

  /** @typedef {import('$lib/notebook.js').Notebook} Notebook */
  /** @typedef {import('$lib/notebook.js').NotebookCell} NotebookCell */

  /**
   * @type {{
   *   notebook: Notebook,
   *   filePath: string | null,
   *   dirty: boolean,
   *   onupdate: (updates: { notebook?: Notebook, filePath?: string | null, dirty?: boolean, title?: string }) => void,
   * }}
   */
  let { notebook, filePath, dirty, onupdate, active = true } = $props()

  /** @type {Set<string>} */
  let running = $state(new Set())
  let addMenuOpen = $state(false)
  let addMenuAfterIdx = $state(-1)
  let addMenuTop = $state(0)
  let addMenuLeft = $state(0)

  function update(/** @type {Partial<Notebook>} */ changes) {
    onupdate({ notebook: { ...notebook, ...changes }, dirty: true })
  }

  function updateCell(id, /** @type {Partial<NotebookCell>} */ changes) {
    update({ cells: notebook.cells.map((c) => (c.id === id ? { ...c, ...changes } : c)) })
  }

  /** @param {number} afterIndex @param {'sql' | 'markdown'} type */
  function addCell(afterIndex, type) {
    const newCell = type === 'sql' ? createSqlCell() : createMarkdownCell()
    const cells = [...notebook.cells]
    cells.splice(afterIndex + 1, 0, newCell)
    update({ cells })
  }

  /** @param {string} id */
  function removeCell(id) {
    if (notebook.cells.length <= 1) return
    update({ cells: notebook.cells.filter((c) => c.id !== id) })
  }

  /** @param {string} id @param {-1 | 1} dir */
  function moveCell(id, dir) {
    const idx = notebook.cells.findIndex((c) => c.id === id)
    if (idx < 0) return
    const next = idx + dir
    if (next < 0 || next >= notebook.cells.length) return
    const cells = [...notebook.cells]
    ;[cells[idx], cells[next]] = [cells[next], cells[idx]]
    update({ cells })
  }

  /** @param {string} id */
  async function runCell(id) {
    const cell = notebook.cells.find((c) => c.id === id)
    if (!cell || cell.type !== 'sql' || !cell.content.trim()) return
    running = new Set([...running, id])
    try {
      const data = await executeSql(cell.content)
      updateCell(id, {
        result: {
          columns: data.columns ?? [],
          rows: data.rows ?? [],
          queryMs: data.queryMs ?? data.query_ms ?? 0,
          error: '',
          message: data.message ?? '',
        },
      })
    } catch (err) {
      updateCell(id, {
        result: {
          columns: [], rows: [], queryMs: 0,
          error: String(/** @type {Error} */ (err).message ?? err),
          message: '',
        },
      })
    } finally {
      running = new Set([...running].filter((x) => x !== id))
    }
  }

  async function runAll() {
    for (const cell of notebook.cells) {
      if (cell.type === 'sql' && cell.content.trim()) await runCell(cell.id)
    }
  }

  async function save() {
    try {
      const json = serializeNotebook(notebook)
      if (filePath) {
        await saveNotebook(filePath, json)
        onupdate({ dirty: false })
        toast.success('Saved')
      } else {
        const path = await saveNotebookAs(json, `${notebook.title || 'notebook'}.sqlnb`)
        if (path) {
          onupdate({ filePath: path, dirty: false, title: titleFromPath(path) })
          toast.success('Saved')
        }
      }
    } catch (err) {
      toast.error(`Save failed: ${/** @type {Error} */ (err).message}`)
    }
  }

  async function openFile() {
    try {
      const result = await openNotebookFile()
      if (!result) return
      const nb = deserializeNotebook(result.content)
      onupdate({ notebook: nb, filePath: result.path, dirty: false, title: titleFromPath(result.path) })
    } catch (err) {
      toast.error(`Failed to open: ${/** @type {Error} */ (err).message}`)
    }
  }

  async function saveAs() {
    try {
      const json = serializeNotebook(notebook)
      const path = await saveNotebookAs(json, `${notebook.title || 'notebook'}.sqlnb`)
      if (path) {
        onupdate({ filePath: path, dirty: false, title: titleFromPath(path) })
        toast.success('Saved')
      }
    } catch (err) {
      toast.error(`Save failed: ${/** @type {Error} */ (err).message}`)
    }
  }

  /** @param {Event} e */
  function onTitleInput(e) {
    const title = /** @type {HTMLInputElement} */ (e.target).value
    update({ title })
    onupdate({ title })
  }

  /** @param {KeyboardEvent} e */
  function onGlobalKeydown(e) {
    if (!active) return // keep-alive on other tabs; only the visible notebook saves
    if ((e.ctrlKey || e.metaKey) && e.key === 's' && !e.shiftKey) {
      e.preventDefault()
      void save()
    }
  }

  /**
   * @param {number} afterIdx
   * @param {MouseEvent} e
   */
  function openAddMenu(afterIdx, e) {
    const rect = /** @type {HTMLElement} */ (e.currentTarget).getBoundingClientRect()
    addMenuAfterIdx = afterIdx
    addMenuTop = rect.bottom + 4
    addMenuLeft = rect.left
    addMenuOpen = true
  }
</script>

<svelte:window onkeydown={onGlobalKeydown} />

<div class="flex h-full min-h-0 flex-col overflow-hidden bg-background">

  <!-- ── Toolbar ──────────────────────────────────────────────────────────── -->
  <div
    class="flex shrink-0 items-center gap-0 border-b border-border/50 px-4 py-0"
    style="height: 38px;"
  >
    <FileCode2 class="size-3.5 shrink-0 text-muted-foreground" />
    <input
      value={notebook.title}
      oninput={onTitleInput}
      class="ml-2 min-w-0 flex-1 bg-transparent text-ui-sm font-medium text-foreground/80 outline-none placeholder:text-muted-foreground"
      placeholder="Untitled Notebook"
      spellcheck="false"
    />
    {#if dirty}
      <span class="mr-3 shrink-0 text-ui-3xs text-muted-foreground">●</span>
    {/if}
    {#if filePath}
      <span
        class="mr-3 max-w-[180px] shrink-0 truncate text-ui-3xs text-muted-foreground"
        title={filePath}
      >
        {filePath.split(/[/\\]/).pop()}
      </span>
    {/if}

    <div class="ml-auto flex shrink-0 items-center gap-0.5">
      <button
        onclick={openFile}
        class="flex items-center gap-1.5 rounded px-2.5 py-1 text-ui-xs text-muted-foreground hover:bg-muted hover:text-foreground"
        title="Open notebook"
      >
        <FolderOpen class="size-3.5" />
        Open
      </button>
      <button
        onclick={save}
        class="flex items-center gap-1.5 rounded px-2.5 py-1 text-ui-xs text-muted-foreground hover:bg-muted hover:text-foreground"
        title={`Save (${comboTitle('Mod+S')})`}
      >
        <Save class="size-3.5" />
        Save
      </button>
      <button
        onclick={runAll}
        class="ml-1 flex items-center gap-1.5 rounded-md bg-primary px-2.5 py-1 text-ui-xs font-medium text-primary-foreground hover:opacity-90"
        title="Run all SQL cells"
      >
        <Play class="size-3" />
        Run all
      </button>
    </div>
  </div>

  <!-- ── Cells ────────────────────────────────────────────────────────────── -->
  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if notebook.cells.length === 0}
      <div class="flex flex-col items-center gap-3 py-24 text-muted-foreground">
        <FileCode2 class="size-8 opacity-20" />
        <p class="text-ui-sm">Empty notebook</p>
        <button
          onclick={() => { update({ cells: [createSqlCell()] }) }}
          class="flex items-center gap-2 rounded-md border border-dashed border-border px-4 py-2 text-ui-sm hover:bg-muted"
        >
          <Plus class="size-4" />
          Add first cell
        </button>
      </div>
    {:else}
      <!-- Top adder -->
      {@render Adder(-1)}

      {#each notebook.cells as cell, idx (cell.id)}
        <!-- ── Cell wrapper ────────────────────────────────────────────── -->
        <div
          class={cn(
            'relative border-t border-border/20',
            cell.type === 'sql'
              ? 'border-l-[2px] border-l-transparent focus-within:border-l-info/30 hover:border-l-info/15'
              : 'border-l-[2px] border-l-transparent focus-within:border-l-success/30 hover:border-l-success/15',
          )}
        >
          {#if cell.type === 'sql'}
            <SqlCell
              cellIndex={idx}
              content={cell.content}
              result={cell.result}
              running={running.has(cell.id)}
              onchange={(v) => updateCell(cell.id, { content: v })}
              onrun={() => runCell(cell.id)}
              onmoveup={() => moveCell(cell.id, -1)}
              onmovedown={() => moveCell(cell.id, 1)}
              onremove={() => removeCell(cell.id)}
              canmoveup={idx > 0}
              canmovedown={idx < notebook.cells.length - 1}
              canremove={notebook.cells.length > 1}
            />
          {:else}
            <MarkdownCell
              cellIndex={idx}
              content={cell.content}
              onchange={(v) => updateCell(cell.id, { content: v })}
              onmoveup={() => moveCell(cell.id, -1)}
              onmovedown={() => moveCell(cell.id, 1)}
              onremove={() => removeCell(cell.id)}
              canmoveup={idx > 0}
              canmovedown={idx < notebook.cells.length - 1}
              canremove={notebook.cells.length > 1}
            />
          {/if}
        </div>

        <!-- Adder between cells -->
        {@render Adder(idx)}
      {/each}
    {/if}

    <!-- Bottom breathing room -->
    <div class="h-24"></div>
  </div>
</div>

<!-- Add cell type dropdown -->
{#if addMenuOpen}
  <div
    class="fixed inset-0 z-40"
    onclick={() => { addMenuOpen = false }}
    role="presentation"
  ></div>
  <div
    class="fixed z-50 min-w-36 overflow-hidden rounded-[10px] border border-border/60 bg-popover p-1 elevate-2-rim"
    style="top:{addMenuTop}px; left:{addMenuLeft}px"
  >
    <button
      onclick={() => { addCell(addMenuAfterIdx, 'sql'); addMenuOpen = false }}
      class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-ui-sm hover:bg-accent hover:text-foreground"
    >
      <span class="font-mono font-semibold text-info">SQL</span>
      <span class="text-muted-foreground">SQL cell</span>
    </button>
    <button
      onclick={() => { addCell(addMenuAfterIdx, 'markdown'); addMenuOpen = false }}
      class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-ui-sm hover:bg-accent hover:text-foreground"
    >
      <span class="font-mono font-semibold text-success">MD</span>
      <span class="text-muted-foreground">Markdown cell</span>
    </button>
  </div>
{/if}

<!-- ── Snippets ──────────────────────────────────────────────────────────── -->

{#snippet Adder(idx)}
  <div
    class="group/adder flex h-5 items-center px-4 opacity-0 transition-opacity hover:opacity-100 focus-within:opacity-100"
  >
    <div class="h-px flex-1 bg-border/30 transition-colors group-hover/adder:bg-border/60"></div>
    <button
      onclick={(e) => openAddMenu(idx, e)}
      class="mx-2 flex items-center gap-1 rounded px-1.5 py-0.5 text-ui-3xs text-muted-foreground hover:bg-muted hover:text-foreground"
    >
      <Plus class="size-3" />
      Add cell
      <ChevronDown class="size-3 opacity-60" />
    </button>
    <div class="h-px flex-1 bg-border/30 transition-colors group-hover/adder:bg-border/60"></div>
  </div>
{/snippet}
