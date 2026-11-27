<script>
  import Kbd from './Kbd.svelte'
  /**
   * SQL notebook: prose and queries in cells, saved as a .sqlnb file on disk.
   *
   * - Open a .sqlnb, or bring in a Jupyter notebook, a Markdown file (its
   *   ```sql fences become cells) or a .sql script; Import adds those cells to
   *   the notebook you are in.
   * - Export to Markdown (with result tables), Jupyter, a .sql script or a
   *   self-contained HTML report.
   * - {{name}} variables, filled from the bar under the toolbar before a run.
   * - Autosave once the notebook has a file; a recent list to come back to.
   */
  import { onDestroy, onMount, tick } from 'svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { comboTitle } from '$lib/shortcuts.js'
  import Plus from '@lucide/svelte/icons/plus'
  import Save from '@lucide/svelte/icons/save'
  import FolderOpen from '@lucide/svelte/icons/folder-open'
  import Play from '@lucide/svelte/icons/play'
  import FileCode2 from '@lucide/svelte/icons/file-code-2'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import Download from '@lucide/svelte/icons/download'
  import Upload from '@lucide/svelte/icons/upload'
  import Braces from '@lucide/svelte/icons/braces'
  import X from '@lucide/svelte/icons/x'
  import Eraser from '@lucide/svelte/icons/eraser'
  import Check from '@lucide/svelte/icons/check'
  import Clock from '@lucide/svelte/icons/clock'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { cn } from '$lib/utils.js'
  import { executeSql, readFile, saveExportAs } from '$lib/api.js'
  import { saveNotebook, saveNotebookAs } from '$lib/api.js'
  import { renderSafeMarkdown } from '$lib/markdown-safe.js'
  import {
    createSqlCell,
    createMarkdownCell,
    serializeNotebook,
    titleFromPath,
    importNotebook,
    substituteVars,
    variableNames,
    toMarkdown,
    toIpynb,
    toSqlScript,
    toHtml,
    recentNotebooks,
    rememberNotebook,
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
   *   active?: boolean,
   *   onupdate: (updates: { notebook?: Notebook, filePath?: string | null, dirty?: boolean, title?: string }) => void,
   * }}
   */
  let { notebook, filePath, dirty, onupdate, active = true } = $props()

  /** @type {Set<string>} */
  let running = $state(new Set())
  /** Jupyter-style run order, per cell, for this session. */
  let runCounter = 0
  let runCounts = $state(/** @type {Record<string, number>} */ ({}))
  let addMenuOpen = $state(false)
  let addMenuAfterIdx = $state(-1)
  let addMenuTop = $state(0)
  let addMenuLeft = $state(0)
  let recent = $state(recentNotebooks())

  const vars = $derived(notebook.variables ?? [])
  const varNames = $derived(vars.map((v) => v.name).filter(Boolean))
  /** Placeholders used in cells that no variable fills yet. */
  const undefinedVars = $derived.by(() => {
    const used = new Set(notebook.cells.filter((c) => c.type === 'sql').flatMap((c) => variableNames(c.content)))
    for (const n of varNames) used.delete(n)
    return [...used]
  })
  const sqlCount = $derived(notebook.cells.filter((c) => c.type === 'sql' && c.content.trim()).length)

  function update(/** @type {Partial<Notebook>} */ changes) {
    onupdate({ notebook: { ...notebook, ...changes }, dirty: true })
  }
  function updateCell(/** @type {string} */ id, /** @type {Partial<NotebookCell>} */ changes) {
    update({ cells: notebook.cells.map((c) => (c.id === id ? { ...c, ...changes } : c)) })
  }

  // ── Cells ────────────────────────────────────────────────────────────────
  /** @param {number} afterIndex @param {'sql' | 'markdown'} type */
  function addCell(afterIndex, type) {
    const cell = type === 'sql' ? createSqlCell() : createMarkdownCell()
    const cells = [...notebook.cells]
    cells.splice(afterIndex + 1, 0, cell)
    update({ cells })
    return cell
  }
  /** @param {string} id */
  function removeCell(id) {
    if (notebook.cells.length <= 1) return
    update({ cells: notebook.cells.filter((c) => c.id !== id) })
  }
  /** @param {string} id */
  function duplicateCell(id) {
    const i = notebook.cells.findIndex((c) => c.id === id)
    if (i < 0) return
    const src = notebook.cells[i]
    const copy = { ...src, id: crypto.randomUUID(), result: null }
    const cells = [...notebook.cells]
    cells.splice(i + 1, 0, copy)
    update({ cells })
  }
  /** @param {string} id @param {-1 | 1} dir */
  function moveCell(id, dir) {
    const idx = notebook.cells.findIndex((c) => c.id === id)
    const next = idx + dir
    if (idx < 0 || next < 0 || next >= notebook.cells.length) return
    const cells = [...notebook.cells]
    ;[cells[idx], cells[next]] = [cells[next], cells[idx]]
    update({ cells })
  }

  /** @param {string} id */
  async function runCell(id) {
    const cell = notebook.cells.find((c) => c.id === id)
    if (!cell || cell.type !== 'sql' || !cell.content.trim()) return
    const { sql, missing } = substituteVars(cell.content, vars)
    if (missing.length) {
      updateCell(id, { result: { columns: [], rows: [], queryMs: 0, message: '', ranAt: Date.now(), error: `No value for ${missing.map((m) => `{{${m}}}`).join(', ')}. Add ${missing.length === 1 ? 'it' : 'them'} in the variables bar above.` } })
      return
    }
    running = new Set([...running, id])
    runCounts = { ...runCounts, [id]: ++runCounter }
    try {
      const data = await executeSql(sql)
      updateCell(id, {
        result: { columns: data.columns ?? [], rows: data.rows ?? [], queryMs: data.queryMs ?? data.query_ms ?? 0, error: '', message: data.message ?? '', ranAt: Date.now() },
      })
    } catch (err) {
      updateCell(id, { result: { columns: [], rows: [], queryMs: 0, error: String(/** @type {Error} */ (err).message ?? err), message: '', ranAt: Date.now() } })
    } finally {
      running = new Set([...running].filter((x) => x !== id))
    }
  }

  /** Run, then on to the next cell (a new SQL cell after the last). @param {string} id */
  async function runAndAdvance(id) {
    const i = notebook.cells.findIndex((c) => c.id === id)
    void runCell(id)
    let next = notebook.cells[i + 1]
    if (!next) next = addCell(i, 'sql')
    focusCell(next.id)
  }

  /** @param {string} id */
  function focusCell(id) {
    requestAnimationFrame(() => {
      const el = document.getElementById(`nb-cell-${id}`)
      el?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
      const target = /** @type {HTMLElement | null} */ (el?.querySelector('.cm-content, textarea, [data-md-preview]'))
      target?.focus()
    })
  }

  // A new notebook is one empty SQL cell: put the caret in it, as a new SQL
  // editor does, instead of leaving focus on nothing. Its editor mounts a few
  // frames after the page, so look for it until it is there.
  onMount(() => {
    const only = notebook.cells.length === 1 ? notebook.cells[0] : null
    if (!only || only.type !== 'sql' || only.content.trim()) return
    let frames = 0
    const tryFocus = () => {
      const target = /** @type {HTMLElement | null} */ (document.getElementById(`nb-cell-${only.id}`)?.querySelector('.cm-content'))
      // Opened from the command menu, the menu hands focus back to whatever had
      // it as it closes: wait for that, then take it unless a text field has it.
      const menuOpen = !!document.querySelector('[role=dialog]')
      if (target && !menuOpen) {
        const ae = /** @type {HTMLElement | null} */ (document.activeElement)
        const typing = !!ae && (ae.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(ae.tagName))
        if (!typing) target.focus()
        return
      }
      if (++frames < 60) requestAnimationFrame(tryFocus)
    }
    requestAnimationFrame(tryFocus)
  })

  async function runAll() {
    for (const cell of notebook.cells) {
      if (cell.type === 'sql' && cell.content.trim()) {
        await runCell(cell.id)
        // Stop at the first failure: later cells usually depend on earlier ones.
        if (notebook.cells.find((c) => c.id === cell.id)?.result?.error) {
          toast.error('Run all stopped at a failing cell')
          focusCell(cell.id)
          return
        }
      }
    }
  }

  function clearOutputs() {
    update({ cells: notebook.cells.map((c) => ({ ...c, result: null })) })
    runCounts = {}
    runCounter = 0
  }

  // ── Variables ────────────────────────────────────────────────────────────
  /** @param {string} [name] */
  function addVariable(name = '') {
    const i = vars.length
    update({ variables: [...vars, { name, value: '' }] })
    // Straight to what is still missing: the value for a name a cell already
    // uses, the name for a blank one. Focus stayed on the button that vanished.
    void tick().then(() => {
      /** @type {HTMLInputElement | null | undefined} */ (varsBar?.querySelector(`[data-var="${i}"][data-part="${name ? 'value' : 'name'}"]`))?.focus()
    })
  }
  /** @type {HTMLElement | null} */
  let varsBar = $state(null)
  /** @param {number} i @param {Partial<{ name: string, value: string }>} patch */
  function setVariable(i, patch) {
    update({ variables: vars.map((v, j) => (j === i ? { ...v, ...patch } : v)) })
  }
  /** @param {number} i */
  function removeVariable(i) {
    update({ variables: vars.filter((_, j) => j !== i) })
  }

  // ── Files ────────────────────────────────────────────────────────────────
  let saving = $state(false)
  async function save() {
    if (saving) return
    saving = true
    try {
      const json = serializeNotebook(notebook)
      if (filePath) {
        await saveNotebook(filePath, json)
        onupdate({ dirty: false })
        rememberNotebook(filePath, notebook.title)
      } else {
        const path = await saveNotebookAs(json, `${notebook.title || 'notebook'}.sqlnb`)
        if (path) {
          onupdate({ filePath: path, dirty: false, title: titleFromPath(path) })
          rememberNotebook(path, titleFromPath(path))
          toast.success('Saved', { description: path })
        }
      }
      recent = recentNotebooks()
    } catch (err) {
      toast.error(`Save failed: ${/** @type {Error} */ (err).message ?? err}`)
    } finally {
      saving = false
    }
  }

  async function saveAs() {
    try {
      const path = await saveNotebookAs(serializeNotebook(notebook), `${notebook.title || 'notebook'}.sqlnb`)
      if (path) {
        onupdate({ filePath: path, dirty: false, title: titleFromPath(path) })
        rememberNotebook(path, titleFromPath(path))
        recent = recentNotebooks()
        toast.success('Saved', { description: path })
      }
    } catch (err) {
      toast.error(`Save failed: ${/** @type {Error} */ (err).message ?? err}`)
    }
  }

  // Autosave: once the notebook lives in a file, edits are written a moment
  // after you stop typing. A notebook with no file yet waits for Save.
  const AUTOSAVE_KEY = 'stroke:notebooks:autosave'
  let autosave = $state((() => { try { return localStorage.getItem(AUTOSAVE_KEY) !== '0' } catch { return true } })())
  function toggleAutosave() {
    autosave = !autosave
    try { localStorage.setItem(AUTOSAVE_KEY, autosave ? '1' : '0') } catch { /* not kept */ }
  }
  let autosaveTimer = 0
  $effect(() => {
    void notebook
    if (!autosave || !filePath || !dirty) return
    clearTimeout(autosaveTimer)
    autosaveTimer = window.setTimeout(() => void save(), 1500)
    return () => clearTimeout(autosaveTimer)
  })
  onDestroy(() => clearTimeout(autosaveTimer))

  const FORMATS = [{ name: 'Notebooks and scripts', extensions: ['sqlnb', 'ipynb', 'md', 'markdown', 'sql'] }]

  /** @returns {Promise<string | null>} */
  async function pickFile() {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const path = await open({ filters: FORMATS, multiple: false })
    return typeof path === 'string' ? path : null
  }

  /** Open a notebook in place of this one. @param {string} [given] */
  async function openFile(given) {
    try {
      const path = given ?? (await pickFile())
      if (!path) return
      const nb = importNotebook(path, await readFile(path))
      const native = /\.sqlnb$/i.test(path)
      // Another format opens as a new, unsaved notebook: Save writes a .sqlnb
      // rather than overwriting the .md or .ipynb it came from.
      onupdate({ notebook: nb, filePath: native ? path : null, dirty: !native, title: nb.title })
      if (native) rememberNotebook(path, nb.title)
      recent = recentNotebooks()
      runCounts = {}
    } catch (err) {
      toast.error(`Could not open: ${/** @type {Error} */ (err).message ?? err}`)
      if (given) {
        // A recent file that moved or was deleted: drop it from the list.
        try { localStorage.setItem('stroke:notebooks:recent', JSON.stringify(recentNotebooks().filter((r) => r.path !== given))) } catch { /* ignore */ }
        recent = recentNotebooks()
      }
    }
  }

  /** Add another file's cells (and variables) at the end of this notebook. */
  async function importFile() {
    try {
      const path = await pickFile()
      if (!path) return
      const nb = importNotebook(path, await readFile(path))
      const names = new Set(varNames)
      update({
        cells: [...notebook.cells, ...nb.cells],
        variables: [...vars, ...(nb.variables ?? []).filter((v) => !names.has(v.name))],
      })
      toast.success(`Imported ${nb.cells.length} cell${nb.cells.length === 1 ? '' : 's'}`, { description: titleFromPath(path) })
    } catch (err) {
      toast.error(`Could not import: ${/** @type {Error} */ (err).message ?? err}`)
    }
  }

  /** @param {'md' | 'ipynb' | 'sql' | 'html'} kind */
  async function exportAs(kind) {
    const base = (notebook.title || 'notebook').replace(/[\\/:*?"<>|]+/g, '_')
    const spec = {
      md: { body: () => toMarkdown(notebook), name: 'Markdown', ext: 'md' },
      ipynb: { body: () => toIpynb(notebook), name: 'Jupyter notebook', ext: 'ipynb' },
      sql: { body: () => toSqlScript(notebook), name: 'SQL script', ext: 'sql' },
      html: { body: () => toHtml(notebook, renderSafeMarkdown), name: 'HTML report', ext: 'html' },
    }[kind]
    try {
      const path = await saveExportAs(spec.body(), `${base}.${spec.ext}`, { name: spec.name, extensions: [spec.ext] })
      if (path) toast.success(`Exported as ${spec.name}`, { description: path })
    } catch (err) {
      toast.error(`Export failed: ${/** @type {Error} */ (err).message ?? err}`)
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
    if (!active) return // keep-alive on other tabs; only the visible notebook answers
    const mod = e.ctrlKey || e.metaKey
    if (mod && e.key.toLowerCase() === 's') {
      e.preventDefault()
      void (e.shiftKey ? saveAs() : save())
    } else if (mod && e.shiftKey && e.key === 'Enter') {
      e.preventDefault()
      void runAll()
    }
  }

  /** @param {number} afterIdx @param {MouseEvent} e */
  function openAddMenu(afterIdx, e) {
    const rect = /** @type {HTMLElement} */ (e.currentTarget).getBoundingClientRect()
    addMenuAfterIdx = afterIdx
    addMenuTop = rect.bottom + 4
    addMenuLeft = rect.left
    addMenuOpen = true
  }

  const fileName = $derived(filePath ? filePath.split(/[/\\]/).pop() : '')
  const toolBtn = 'flex h-7 items-center gap-1.5 rounded-md px-2.5 text-ui-xs text-muted-foreground hover:bg-muted hover:text-foreground data-[state=open]:bg-muted data-[state=open]:text-foreground'
</script>

<svelte:window onkeydown={onGlobalKeydown} />

<div class="flex h-full min-h-0 flex-col overflow-hidden bg-background">
  <!-- ── Toolbar ─────────────────────────────────────────────────────────── -->
  <div class="flex h-11 shrink-0 items-center gap-2 border-b border-border/50 px-4">
    <FileCode2 class="size-4 shrink-0 text-muted-foreground" />
    <input
      value={notebook.title}
      oninput={onTitleInput}
      class="no-field-frame -mx-1.5 h-7 min-w-0 flex-1 rounded-md bg-transparent px-1.5 text-ui-sm font-medium text-foreground outline-none transition-colors placeholder:text-muted-foreground hover:bg-muted/40 focus:bg-muted/50"
      placeholder="Untitled notebook"
      spellcheck="false"
      aria-label="Notebook title"
    />
    <span class="flex shrink-0 items-center gap-1.5 text-ui-3xs text-muted-foreground" title={filePath ?? 'Not saved to a file yet'}>
      {#if saving}
        Saving…
      {:else if !filePath}
        Not saved
      {:else if dirty}
        <span class="size-1.5 rounded-full bg-warning"></span>{fileName}
      {:else}
        <Check class="size-3 shrink-0 text-success" />{fileName}
      {/if}
    </span>

    <div class="ml-2 flex shrink-0 items-center gap-0.5">
      <DropdownMenu.Root>
        <DropdownMenu.Trigger class={toolBtn}><FolderOpen class="size-3.5 shrink-0" />Open<ChevronDown class="size-3 shrink-0 opacity-60" /></DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="min-w-72">
          <DropdownMenu.Item onSelect={() => void openFile()}>
            <FolderOpen class="size-3.5 shrink-0 text-muted-foreground" />Open a file…
            <span class="ml-auto text-ui-3xs text-muted-foreground">.sqlnb .ipynb .md .sql</span>
          </DropdownMenu.Item>
          {#if recent.length}
            <DropdownMenu.Separator />
            <DropdownMenu.Group>
              <DropdownMenu.GroupHeading>Recent</DropdownMenu.GroupHeading>
              {#each recent.slice(0, 8) as r (r.path)}
                <DropdownMenu.Item onSelect={() => void openFile(r.path)} title={r.path}>
                  <Clock class="size-3.5 shrink-0 text-muted-foreground" /><span class="min-w-0 truncate">{r.title}</span>
                </DropdownMenu.Item>
              {/each}
            </DropdownMenu.Group>
          {/if}
        </DropdownMenu.Content>
      </DropdownMenu.Root>

      <button type="button" class={toolBtn} onclick={() => void importFile()} title="Add the cells of a .sqlnb, Jupyter, Markdown or .sql file to this notebook">
        <Upload class="size-3.5 shrink-0" />Import
      </button>

      <DropdownMenu.Root>
        <DropdownMenu.Trigger class={toolBtn}><Download class="size-3.5 shrink-0" />Export<ChevronDown class="size-3 shrink-0 opacity-60" /></DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="min-w-60">
          <DropdownMenu.Item onSelect={() => void exportAs('html')}>HTML report<span class="ml-auto text-ui-3xs text-muted-foreground">with results</span></DropdownMenu.Item>
          <DropdownMenu.Item onSelect={() => void exportAs('md')}>Markdown<span class="ml-auto text-ui-3xs text-muted-foreground">with results</span></DropdownMenu.Item>
          <DropdownMenu.Item onSelect={() => void exportAs('ipynb')}>Jupyter notebook<span class="ml-auto text-ui-3xs text-muted-foreground">%%sql</span></DropdownMenu.Item>
          <DropdownMenu.Item onSelect={() => void exportAs('sql')}>SQL script<span class="ml-auto text-ui-3xs text-muted-foreground">-- %% cells</span></DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Root>

      <DropdownMenu.Root>
        <DropdownMenu.Trigger class={toolBtn} title={`Save (${comboTitle('Mod+S')})`}><Save class="size-3.5 shrink-0" />Save<ChevronDown class="size-3 shrink-0 opacity-60" /></DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="min-w-56">
          <DropdownMenu.Item onSelect={() => void save()}>Save<span class="ml-auto text-ui-3xs text-muted-foreground">{comboTitle('Mod+S')}</span></DropdownMenu.Item>
          <DropdownMenu.Item onSelect={() => void saveAs()}>Save as…<span class="ml-auto text-ui-3xs text-muted-foreground">{comboTitle('Mod+Shift+S')}</span></DropdownMenu.Item>
          <DropdownMenu.Separator />
          <DropdownMenu.CheckboxItem checked={autosave} onCheckedChange={toggleAutosave}>Autosave to the file</DropdownMenu.CheckboxItem>
        </DropdownMenu.Content>
      </DropdownMenu.Root>

      <button type="button" class={toolBtn} onclick={clearOutputs} title="Clear every cell's output"><Eraser class="size-3.5 shrink-0" /></button>
      <Button onclick={() => void runAll()} class="ml-1" size="sm" disabled={sqlCount === 0} title={`Run every SQL cell in order (${comboTitle('Mod+Shift+Enter')})`}>
        <Play class="size-3 shrink-0" />Run all
      </Button>
    </div>
  </div>

  <!-- ── Variables ───────────────────────────────────────────────────────── -->
  {#if vars.length || undefinedVars.length}
    <div bind:this={varsBar} class="flex shrink-0 flex-wrap items-center gap-1.5 border-b border-border/40 px-4 py-1.5">
      <span class="flex items-center gap-1 text-ui-3xs font-medium text-muted-foreground"><Braces class="size-3 shrink-0" />Variables</span>
      {#each vars as v, i (i)}
        <span class="flex h-6 items-center gap-1 rounded-md border border-border/60 bg-background pr-0.5 pl-1.5 font-mono text-ui-2xs">
          <input class="no-focus-ring w-20 bg-transparent text-foreground outline-none placeholder:text-muted-foreground" value={v.name} placeholder="name" aria-label="Variable name" data-var={i} data-part="name" oninput={(e) => setVariable(i, { name: /** @type {HTMLInputElement} */ (e.target).value.replace(/[^\w]/g, '') })} />
          <span class="text-muted-foreground">=</span>
          <input class="no-focus-ring w-28 bg-transparent text-foreground outline-none placeholder:text-muted-foreground" value={v.value} placeholder="value" aria-label="Value of {v.name || 'variable'}" data-var={i} data-part="value" oninput={(e) => setVariable(i, { value: /** @type {HTMLInputElement} */ (e.target).value })} />
          <button type="button" class="hit-area inline-flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Remove variable" onclick={() => removeVariable(i)}><X class="size-3 shrink-0" /></button>
        </span>
      {/each}
      {#each undefinedVars as n (n)}
        <button type="button" class="flex h-6 items-center gap-1 rounded-md border border-dashed border-warning/50 px-1.5 font-mono text-ui-2xs text-warning hover:bg-warning/10" title="Used in a cell, not defined yet" onclick={() => addVariable(n)}>
          <Plus class="size-3 shrink-0" />{n}
        </button>
      {/each}
      <button type="button" class="flex h-6 items-center gap-1 rounded-md px-1.5 text-ui-2xs text-muted-foreground hover:bg-muted hover:text-foreground" onclick={() => addVariable()}><Plus class="size-3 shrink-0" />Add</button>
    </div>
  {/if}

  <!-- ── Cells ───────────────────────────────────────────────────────────── -->
  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if notebook.cells.length === 0}
      <div class="flex flex-col items-center gap-3 py-24 text-muted-foreground">
        <FileCode2 class="size-8 shrink-0 opacity-20" />
        <p class="text-ui-sm">Empty notebook</p>
        <div class="flex gap-2">
          <button type="button" onclick={() => update({ cells: [createSqlCell()] })} class="flex items-center gap-2 rounded-md border border-dashed border-border px-4 py-2 text-ui-sm hover:bg-muted"><Plus class="size-4 shrink-0" />SQL cell</button>
          <button type="button" onclick={() => update({ cells: [createMarkdownCell()] })} class="flex items-center gap-2 rounded-md border border-dashed border-border px-4 py-2 text-ui-sm hover:bg-muted"><Plus class="size-4 shrink-0" />Text cell</button>
        </div>
      </div>
    {:else}
      <div class="mx-auto w-full max-w-6xl">
        {@render Adder(-1)}
        {#each notebook.cells as cell, idx (cell.id)}
          <div
            id="nb-cell-{cell.id}"
            class={cn(
              'relative border-t border-border/20 border-l-2 border-l-transparent',
              cell.type === 'sql' ? 'focus-within:border-l-info/40 hover:border-l-info/15' : 'focus-within:border-l-success/40 hover:border-l-success/15',
            )}
          >
            {#if cell.type === 'sql'}
              <SqlCell
                cellIndex={idx}
                content={cell.content}
                result={cell.result}
                running={running.has(cell.id)}
                runCount={runCounts[cell.id] ?? null}
                variables={varNames}
                onchange={(v) => updateCell(cell.id, { content: v })}
                onrun={() => void runCell(cell.id)}
                onrunadvance={() => void runAndAdvance(cell.id)}
                onduplicate={() => duplicateCell(cell.id)}
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
                onadvance={() => { const n = notebook.cells[idx + 1] ?? addCell(idx, 'sql'); focusCell(n.id) }}
                onmoveup={() => moveCell(cell.id, -1)}
                onmovedown={() => moveCell(cell.id, 1)}
                onremove={() => removeCell(cell.id)}
                canmoveup={idx > 0}
                canmovedown={idx < notebook.cells.length - 1}
                canremove={notebook.cells.length > 1}
              />
            {/if}
          </div>
          {@render Adder(idx)}
        {/each}
        <div class="flex justify-center gap-2 py-6">
          <button type="button" onclick={() => { const c = addCell(notebook.cells.length - 1, 'sql'); focusCell(c.id) }} class="flex h-7 items-center gap-1.5 rounded-md border border-border/60 px-3 text-ui-xs text-muted-foreground hover:bg-muted hover:text-foreground"><Plus class="size-3.5 shrink-0" />SQL</button>
          <button type="button" onclick={() => { const c = addCell(notebook.cells.length - 1, 'markdown'); focusCell(c.id) }} class="flex h-7 items-center gap-1.5 rounded-md border border-border/60 px-3 text-ui-xs text-muted-foreground hover:bg-muted hover:text-foreground"><Plus class="size-3.5 shrink-0" />Text</button>
        </div>
        <!-- Keycaps, as the SQL editor's hints are: a 10px run of "Ctrl+Enter run ·
             Shift+Enter run and next ·" read as one sentence of key names. -->
        <div class="flex flex-wrap items-center justify-center gap-x-4 gap-y-1.5 pb-16 text-ui-xs text-muted-foreground">
          <span class="inline-flex items-center gap-1.5"><Kbd combo="Mod+Enter" /> runs a cell</span>
          <span class="inline-flex items-center gap-1.5"><Kbd combo="Shift+Enter" /> runs it and moves on</span>
          <span class="inline-flex items-center gap-1.5"><Kbd combo="Mod+Shift+Enter" /> runs them all</span>
          <span><span class="font-mono text-foreground">{'{{name}}'}</span> in a query asks for a value</span>
        </div>
      </div>
    {/if}
  </div>
</div>

<!-- Add cell type -->
{#if addMenuOpen}
  <div class="fixed inset-0 z-40" onclick={() => { addMenuOpen = false }} role="presentation"></div>
  <div class="fixed z-50 min-w-36 overflow-hidden rounded-lg border border-border/60 bg-popover p-1 elevate-2-rim" style="top:{addMenuTop}px; left:{addMenuLeft}px">
    <button type="button" onclick={() => { const c = addCell(addMenuAfterIdx, 'sql'); addMenuOpen = false; focusCell(c.id) }} class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-ui-sm hover:bg-accent hover:text-foreground">
      <span class="font-mono font-semibold text-info">SQL</span><span class="text-muted-foreground">Query cell</span>
    </button>
    <button type="button" onclick={() => { const c = addCell(addMenuAfterIdx, 'markdown'); addMenuOpen = false; focusCell(c.id) }} class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-ui-sm hover:bg-accent hover:text-foreground">
      <span class="font-mono font-semibold text-success">MD</span><span class="text-muted-foreground">Text cell</span>
    </button>
  </div>
{/if}

{#snippet Adder(/** @type {number} */ idx)}
  <div class="group/adder flex h-5 items-center px-4 opacity-0 transition-opacity hover:opacity-100 focus-within:opacity-100">
    <div class="h-px flex-1 bg-border/30 transition-colors group-hover/adder:bg-border/60"></div>
    <button type="button" onclick={(e) => openAddMenu(idx, e)} class="mx-2 flex items-center gap-1 rounded px-1.5 py-0.5 text-ui-3xs text-muted-foreground hover:bg-muted hover:text-foreground">
      <Plus class="size-3 shrink-0" />Add cell<ChevronDown class="size-3 shrink-0 opacity-60" />
    </button>
    <div class="h-px flex-1 bg-border/30 transition-colors group-hover/adder:bg-border/60"></div>
  </div>
{/snippet}
