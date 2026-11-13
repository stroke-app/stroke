<script>
  // Create or edit a workspace: its name and colour, and (not the default one)
  // delete it. Deleting moves its connections to the default workspace.
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import ColorSwatches from './ColorSwatches.svelte'
  import { createWorkspace, updateWorkspace, deleteWorkspace, setActiveWorkspace, DEFAULT_WORKSPACE_ID } from '$lib/stores/workspaces.js'
  import { releaseWorkspace } from '$lib/stores/connections.js'

  /**
   * @type {{
   *   open?: boolean,
   *   workspace?: import('$lib/stores/workspaces.js').Workspace | null,
   *   onchanged?: () => void,
   * }}
   */
  let { open = $bindable(false), workspace = null, onchanged = () => {} } = $props()

  let name = $state('')
  let color = $state(/** @type {import('$lib/stores/workspaces.js').ColorId} */ ('blue'))
  let confirmDelete = $state(false)

  $effect(() => {
    if (!open) return
    name = workspace?.name ?? ''
    color = workspace?.color ?? 'green'
    confirmDelete = false
  })

  function submit() {
    if (!name.trim()) return
    if (workspace) {
      updateWorkspace(workspace.id, { name, color })
    } else {
      const w = createWorkspace(name, color)
      setActiveWorkspace(w.id)
    }
    open = false
    onchanged()
  }

  function remove() {
    if (!workspace || workspace.id === DEFAULT_WORKSPACE_ID) return
    if (!confirmDelete) {
      confirmDelete = true
      return
    }
    deleteWorkspace(workspace.id, releaseWorkspace)
    open = false
    onchanged()
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content showCloseButton={false} class="w-[min(26rem,calc(100vw-2rem))] gap-0 overflow-hidden p-0 sm:max-w-none">
    <form onsubmit={(e) => { e.preventDefault(); submit() }}>
      <div class="flex flex-col gap-4 px-5 pt-5 pb-4">
        <div>
          <Dialog.Title class="text-ui-sm font-semibold text-foreground">{workspace ? 'Edit workspace' : 'New workspace'}</Dialog.Title>
          <Dialog.Description class="mt-1 text-ui-xs text-muted-foreground">
            A workspace keeps a set of connections together, like Home, Office or a client.
          </Dialog.Description>
        </div>
        <label class="flex flex-col gap-1.5">
          <span class="text-ui-xs font-medium text-foreground/75">Name</span>
          <Input bind:value={name} placeholder="Office" autofocus />
        </label>
        <div class="flex flex-col gap-2">
          <span class="text-ui-xs font-medium text-foreground/75">Colour</span>
          <ColorSwatches value={color} onchange={(c) => (color = /** @type {any} */ (c ?? 'gray'))} />
        </div>
      </div>
      <div class="flex items-center gap-2 bg-muted/20 px-5 py-3">
        {#if workspace && workspace.id !== DEFAULT_WORKSPACE_ID}
          <Button type="button" variant={confirmDelete ? 'destructive' : 'ghost'} size="sm" class={confirmDelete ? '' : 'text-muted-foreground hover:text-destructive'} onclick={remove}>
            {confirmDelete ? 'Delete; its connections move to Personal' : 'Delete'}
          </Button>
        {/if}
        <span class="flex-1"></span>
        <Button type="button" variant="ghost" size="sm" onclick={() => (open = false)}>Cancel</Button>
        <Button type="submit" size="sm" disabled={!name.trim()}>{workspace ? 'Save' : 'Create'}</Button>
      </div>
    </form>
  </Dialog.Content>
</Dialog.Root>
