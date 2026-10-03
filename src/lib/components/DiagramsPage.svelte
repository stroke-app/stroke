<script>
  import { savedDiagrams, saveDiagram, updateDiagram, deleteDiagram, switchDiagramsConnection } from '$lib/stores/saved-diagrams.js'
  import MermaidViewer from './MermaidViewer.svelte'
  import CodeEditor from './CodeEditor.svelte'
  import Download from '@lucide/svelte/icons/download'
  import Copy from '@lucide/svelte/icons/copy'
  import GitBranch from '@lucide/svelte/icons/git-branch'
  import GitFork from '@lucide/svelte/icons/git-fork'
  import Grid2x2 from '@lucide/svelte/icons/grid-2x2'
  import Network from '@lucide/svelte/icons/network'
  import Map from '@lucide/svelte/icons/map'
  import Brain from '@lucide/svelte/icons/brain'
  import Plus from '@lucide/svelte/icons/plus'
  import Pencil from '@lucide/svelte/icons/pencil'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import ZoomIn from '@lucide/svelte/icons/zoom-in'
  import ZoomOut from '@lucide/svelte/icons/zoom-out'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import Maximize2 from '@lucide/svelte/icons/maximize-2'
  import X from '@lucide/svelte/icons/x'
  import Check from '@lucide/svelte/icons/check'
  import Search from '@lucide/svelte/icons/search'
  import { cn } from '$lib/utils.js'
  import { focusTrap } from '$lib/actions/focus-trap.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'

  /**
   * Run one of the viewer's exports and name the file it landed in.
   * @param {Promise<string | null> | undefined} run
   * @param {string} label
   * @param {string} filename
   */
  async function reportExport(run, label, filename) {
    try {
      const path = await run
      if (path === null) return  // no diagram, or the save dialog was cancelled
      toast.success(`Exported ${label}`, {
        description: path ? `Saved to ${path}` : `${filename} saved to your downloads`,
      })
    } catch (e) {
      toast.error('Export failed', { description: String(e) })
    }
  }

  /** Copy the rendered diagram to the clipboard as a PNG. */
  async function copyDiagram() {
    try {
      const ok = await viewerRef?.copyPng()
      if (!ok) return  // nothing rendered yet
      toast.success('Diagram copied as PNG')
    } catch (e) {
      toast.error('Copy failed', { description: String(e) })
    }
  }

  let {
    /** @type {import('$lib/stores/connections.js').SavedConnection | null} */
    connection = null,
  } = $props()

  // Reload diagrams whenever the active connection changes
  $effect(() => {
    switchDiagramsConnection(connection?.id ?? '')
  })

  const TEMPLATES = {
    flowchart: `flowchart TD\n    A[Start] --> B{Decision}\n    B -->|Yes| C[Action 1]\n    B -->|No| D[Action 2]\n    C --> E[End]\n    D --> E`,
    class: `classDiagram\n    class User {\n        +id: uuid\n        +name: string\n        +email: string\n    }\n    class Order {\n        +id: uuid\n        +userId: uuid\n        +status: string\n    }\n    User "1" --> "0..*" Order`,
    sequence: `sequenceDiagram\n    participant Client\n    participant Server\n    participant DB\n    Client->>Server: GET /api/data\n    Server->>DB: SELECT * FROM table\n    DB-->>Server: rows\n    Server-->>Client: JSON response`,
    erd: `erDiagram\n    USER {\n        uuid id PK\n        string name\n        string email\n    }\n    ORDER {\n        uuid id PK\n        uuid user_id FK\n        string status\n    }\n    USER ||--o{ ORDER : places`,
    mindmap: `mindmap\n  root((Database))\n    Tables\n      Users\n      Orders\n      Products\n    Queries\n      SELECT\n      INSERT\n      UPDATE`,
  }

  /** @param {number} ts */
  function relTime(ts) {
    const diff = Date.now() - ts
    const s = Math.floor(diff / 1000)
    if (s < 60) return 'just now'
    const m = Math.floor(s / 60)
    if (m < 60) return `${m}m ago`
    const h = Math.floor(m / 60)
    if (h < 24) return `${h}h ago`
    const d = Math.floor(h / 24)
    if (d < 7) return `${d}d ago`
    return `${Math.floor(d / 7)}w ago`
  }

  /** @param {string} code */
  function diagramIcon(code) {
    const first = code.trim().split('\n')[0].trim().toLowerCase()
    if (first.startsWith('flowchart') || first.startsWith('graph')) return GitFork
    if (first.startsWith('classdiagram')) return Grid2x2
    if (first.startsWith('sequencediagram')) return Network
    if (first.startsWith('erdiagram')) return Map
    if (first.startsWith('mindmap')) return Brain
    return GitBranch
  }

  let searchQ = $state('')
  let selectedId = $state(/** @type {string | null} */ (null))
  let editing = $state(false)
  let isNew = $state(false)
  let draftCode = $state('')
  let draftName = $state('')
  let draftGroup = $state('Default')
  let confirmingDelete = $state(false)
  let fullscreenCode = $state(/** @type {string | null} */ (null))

  const all = $derived($savedDiagrams)
  const filtered = $derived(
    searchQ ? all.filter((d) => d.name.toLowerCase().includes(searchQ.toLowerCase())) : all
  )
  const grouped = $derived.by(() => {
    /** @type {Record<string, import('$lib/stores/saved-diagrams.js').SavedDiagram[]>} */
    const groups = {}
    for (const d of filtered) {
      if (!groups[d.group]) groups[d.group] = []
      groups[d.group].push(d)
    }
    return groups
  })
  const selected = $derived(all.find((d) => d.id === selectedId) ?? null)

  function startNew() {
    editing = true
    isNew = true
    draftCode = TEMPLATES.flowchart
    draftName = 'Untitled Diagram'
    draftGroup = 'Default'
    selectedId = null
    confirmingDelete = false
  }

  function startEdit() {
    if (!selected) return
    editing = true
    isNew = false
    draftCode = selected.code
    draftName = selected.name
    draftGroup = selected.group
  }

  function saveChanges() {
    const name = draftName.trim() || 'Untitled Diagram'
    if (isNew) {
      const d = saveDiagram(name, draftCode, draftGroup || 'Default')
      selectedId = d.id
      toast.success('Diagram saved')
    } else if (selectedId) {
      updateDiagram(selectedId, { name, code: draftCode, group: draftGroup || 'Default' })
      toast.success('Diagram updated')
    }
    editing = false
    isNew = false
  }

  function cancelEdit() {
    editing = false
    isNew = false
  }

  function confirmDelete() {
    if (!selectedId) return
    deleteDiagram(selectedId)
    selectedId = null
    confirmingDelete = false
    toast.success('Diagram deleted')
  }

  /** @param {KeyboardEvent} e */
  function handleFullscreenKey(e) {
    if (e.key === 'Escape') fullscreenCode = null
  }

  /** @type {import('./MermaidViewer.svelte').default | null} */
  let viewerRef = $state(null)
  /** @type {import('./MermaidViewer.svelte').default | null} */
  let fullscreenViewerRef = $state(null)
</script>

<svelte:window onkeydown={handleFullscreenKey} />

<div class="flex h-full min-h-0 flex-col overflow-hidden bg-background">
  <!-- Page header -->
  <div class="flex shrink-0 items-center justify-between gap-3 border-b border-border/50 px-4 py-3">
    <h2 class="text-ui-sm font-medium text-foreground">Diagrams</h2>
    <button
      type="button"
      onclick={startNew}
      class="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-ui-xs font-medium text-primary-foreground transition-opacity hover:opacity-90"
    >
      <Plus class="size-3" />
      New diagram
    </button>
  </div>

  <!-- Body: sidebar + main -->
  <div class="flex min-h-0 flex-1 overflow-hidden">
    <!-- Left sidebar -->
    <div class="flex w-56 shrink-0 flex-col overflow-hidden border-r border-border/50 bg-panel">
      <!-- Search -->
      <div class="shrink-0 border-b border-border/40 px-2 py-2">
        <div class="relative flex items-center">
          <Search class="absolute left-2 size-3 text-muted-foreground" />
          <input
            type="text"
            placeholder="Search diagrams…"
            bind:value={searchQ}
            class= "field-surface h-7 w-full bg-background/50 pl-6 pr-2 text-ui-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
          />
        </div>
      </div>

      <!-- List -->
      <div class="min-h-0 flex-1 overflow-y-auto px-1 py-1">
        {#if filtered.length === 0}
          <div class="flex flex-col items-center gap-1.5 px-3 py-6 text-center">
            <GitBranch class="size-5 text-muted-foreground" />
            <p class="text-ui-2xs text-muted-foreground">
              {searchQ ? 'No results' : 'No diagrams yet'}
            </p>
          </div>
        {:else}
          {#each Object.entries(grouped) as [group, items]}
            <p class="px-2 pb-1 pt-3 text-ui-2xs font-medium uppercase tracking-wide text-muted-foreground first:pt-1">
              {group}
            </p>
            {#each items as d (d.id)}
              {@const Icon = diagramIcon(d.code)}
              <button
                type="button"
                onclick={() => { selectedId = d.id; editing = false; isNew = false; confirmingDelete = false }}
                class={cn(
                  'flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left transition-colors cursor-pointer',
                  selectedId === d.id
                    ? 'bg-sidebar-accent text-sidebar-accent-foreground'
                    : 'text-muted-foreground hover:bg-sidebar-accent/50 hover:text-foreground'
                )}
              >
                <Icon class="size-3.5 shrink-0" />
                <span class="min-w-0 flex-1 truncate font-mono text-ui-sm">{d.name}</span>
                <span class="shrink-0 text-ui-3xs tabular-nums opacity-50">{relTime(d.updatedAt)}</span>
              </button>
            {/each}
          {/each}
        {/if}
      </div>

      <!-- Footer -->
      <div class="shrink-0 border-t border-border/40 px-3 py-2">
        <p class="text-ui-3xs text-muted-foreground">
          {all.length} diagram{all.length === 1 ? '' : 's'}
        </p>
      </div>
    </div>

    <!-- Main panel -->
    <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      {#if editing}
        <!-- State C: Editor mode -->
        <div class="flex shrink-0 items-center gap-2 border-b border-border/50 px-4 py-3">
          <input
            type="text"
            bind:value={draftName}
            placeholder="Diagram name…"
            class= "field-surface h-7 min-w-0 flex-1 bg-background/50 px-2.5 text-ui-sm text-foreground placeholder:text-muted-foreground focus:outline-none"
          />
          <input
            type="text"
            bind:value={draftGroup}
            placeholder="Group"
            class= "field-surface h-7 w-24 bg-background/50 px-2.5 text-ui-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
          />
          <button
            type="button"
            onclick={saveChanges}
            class="inline-flex h-7 items-center gap-1 rounded-md bg-primary px-3 text-ui-xs font-medium text-primary-foreground hover:opacity-90"
          >
            <Check class="size-3" />
            Save
          </button>
          <button
            type="button"
            onclick={cancelEdit}
            class="inline-flex h-7 items-center rounded-md px-3 text-ui-xs text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            Cancel
          </button>
        </div>

        {#if isNew}
          <!-- Template picker -->
          <div class="flex shrink-0 items-center gap-1.5 border-b border-border/40 px-4 py-2">
            <span class="text-ui-3xs text-muted-foreground mr-1">Template:</span>
            {#each Object.entries(TEMPLATES) as [key, tpl]}
              <button
                type="button"
                onclick={() => { draftCode = tpl }}
                class={cn(
                  'rounded px-2 py-0.5 text-ui-3xs transition-colors border',
                  draftCode === tpl
                    ? 'border-border bg-accent text-foreground'
                    : 'border-border/40 text-muted-foreground hover:border-border hover:text-foreground'
                )}
              >
                {key.charAt(0).toUpperCase() + key.slice(1).replace('map', ' Map')}
              </button>
            {/each}
          </div>
        {/if}

        <!-- Editor + preview -->
        <div class="flex min-h-0 flex-1 gap-0 overflow-hidden">
          <div class="flex w-1/2 min-w-0 flex-col border-r border-border/40 p-3">
            <p class="mb-1.5 text-ui-3xs font-medium uppercase tracking-wide text-muted-foreground">Mermaid code</p>
            <div class="field-surface flex min-h-0 flex-1 flex-col overflow-hidden bg-background/40">
              <CodeEditor bind:value={draftCode} ariaLabel="Mermaid code" placeholder="flowchart TD" />
            </div>
          </div>
          <div class="flex w-1/2 min-w-0 flex-col overflow-hidden">
            <p class="shrink-0 px-3 pb-1.5 pt-3 text-ui-3xs font-medium uppercase tracking-wide text-muted-foreground">Preview</p>
            <div class="min-h-0 flex-1 overflow-hidden">
              <MermaidViewer code={draftCode} class="h-full w-full" />
            </div>
          </div>
        </div>

      {:else if selected}
        <!-- State B: View mode -->
        <div class="flex shrink-0 items-center gap-2 border-b border-border/50 px-4 py-3">
          <h3 class="min-w-0 flex-1 truncate text-ui-sm font-medium text-foreground">{selected.name}</h3>
          <span class="shrink-0 rounded border border-border/40 bg-muted/30 px-1.5 py-0.5 text-ui-3xs text-muted-foreground">{selected.group}</span>
          <div class="flex shrink-0 items-center gap-0.5 ml-1">
            <button
              type="button"
              onclick={startEdit}
              class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
              title="Edit"
            ><Pencil class="size-3.5" /></button>
            {#if confirmingDelete}
              <button
                type="button"
                onclick={confirmDelete}
                class="inline-flex h-7 items-center gap-1 rounded-md bg-destructive px-2 text-ui-3xs text-destructive-foreground hover:opacity-90"
              ><Check class="size-3" />Confirm</button>
              <button
                type="button"
                onclick={() => confirmingDelete = false}
                class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
              ><X class="size-3.5" /></button>
            {:else}
              <button
                type="button"
                onclick={() => confirmingDelete = true}
                class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-destructive"
                title="Delete"
              ><Trash2 class="size-3.5" /></button>
            {/if}
            <div class="mx-1 h-4 w-px bg-border/40"></div>
            <button
              type="button"
              onclick={() => viewerRef?.dispatch('diagram:zoomin')}
              class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
              title="Zoom in (Ctrl+scroll)"
            ><ZoomIn class="size-3.5" /></button>
            <button
              type="button"
              onclick={() => viewerRef?.dispatch('diagram:zoomout')}
              class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
              title="Zoom out"
            ><ZoomOut class="size-3.5" /></button>
            <button
              type="button"
              onclick={() => viewerRef?.dispatch('diagram:reset')}
              class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
              title="Reset view (double-click)"
            ><RefreshCw class="size-3.5" /></button>
            <div class="mx-1 h-4 w-px bg-border/40"></div>
            <button
              type="button"
              onclick={() => void copyDiagram()}
              class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-ui-3xs text-muted-foreground hover:bg-accent hover:text-foreground"
              title="Copy as PNG"
            ><Copy class="size-3" />Copy</button>
            <button
              type="button"
              onclick={() => reportExport(viewerRef?.exportSvg(`${selected.name}.svg`), 'diagram as SVG', `${selected.name}.svg`)}
              class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-ui-3xs text-muted-foreground hover:bg-accent hover:text-foreground"
              title="Export SVG"
            ><Download class="size-3" />SVG</button>
            <button
              type="button"
              onclick={() => reportExport(viewerRef?.exportPng(`${selected.name}.png`), 'diagram as PNG', `${selected.name}.png`)}
              class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-ui-3xs text-muted-foreground hover:bg-accent hover:text-foreground"
              title="Export PNG"
            ><Download class="size-3" />PNG</button>
            <div class="mx-1 h-4 w-px bg-border/40"></div>
            <button
              type="button"
              onclick={() => fullscreenCode = selected.code}
              class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
              title="Fullscreen"
            ><Maximize2 class="size-3.5" /></button>
          </div>
        </div>

        <div class="min-h-0 flex-1 overflow-hidden">
          <MermaidViewer bind:this={viewerRef} code={selected.code} class="h-full w-full" />
        </div>

      {:else}
        <!-- State A: Empty state -->
        <div class="flex flex-1 flex-col items-center justify-center gap-3 text-center">
          <GitBranch class="size-8 text-muted-foreground" />
          <div>
            <p class="text-ui-sm font-medium text-muted-foreground">No diagram selected</p>
            <p class="mt-0.5 text-ui-xs text-muted-foreground">Pick one from the list or create a new one</p>
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>

<!-- Fullscreen overlay -->
{#if fullscreenCode !== null}
  <div
    class="fixed inset-0 z-50 flex flex-col bg-background"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    use:focusTrap
  >
    <div class="flex shrink-0 items-center gap-2 border-b border-border/50 px-4 py-2.5">
      <span class="min-w-0 flex-1 truncate text-ui-sm font-medium text-foreground">{selected?.name ?? 'Diagram'}</span>
      <button type="button" onclick={() => fullscreenViewerRef?.dispatch('diagram:zoomin')} class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" title="Zoom in"><ZoomIn class="size-3.5" /></button>
      <button type="button" onclick={() => fullscreenViewerRef?.dispatch('diagram:zoomout')} class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" title="Zoom out"><ZoomOut class="size-3.5" /></button>
      <button type="button" onclick={() => fullscreenViewerRef?.dispatch('diagram:reset')} class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" title="Reset"><RefreshCw class="size-3.5" /></button>
      <div class="mx-1 h-4 w-px bg-border/40"></div>
      <button
        type="button"
        onclick={() => fullscreenCode = null}
        class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
        title="Close (Esc)"
      ><X class="size-4" /></button>
    </div>
    <div class="min-h-0 flex-1 overflow-hidden">
      <MermaidViewer bind:this={fullscreenViewerRef} code={fullscreenCode} class="h-full w-full" />
    </div>
    <p class="shrink-0 py-1.5 text-center text-ui-3xs text-muted-foreground">Drag to pan · Ctrl+scroll to zoom · Double-click to reset · Esc to close</p>
  </div>
{/if}
