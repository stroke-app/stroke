<script>
  import Tags from '@lucide/svelte/icons/tags'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import { Button } from '$lib/components/ui/button/index.js'

  let {
    /** @type {{ name: string, values: string[], usedInTables: string[] }[]} */
    enums = [],
    loading = false,
    onrefresh = () => {},
  } = $props()
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  <!-- Toolbar -->
  <div class="studio-chrome flex h-9 shrink-0 items-center gap-2 border-b border-border bg-panel px-3" data-studio-chrome>
    <Tags class="size-4 shrink-0 text-muted-foreground" />
    <span class="font-mono text-ui-sm font-medium">Enums</span>
    {#if enums.length > 0}
      <span class="font-mono text-ui-xs text-muted-foreground">({enums.length})</span>
    {/if}
    <div class="ml-auto flex items-center gap-1">
      <Button
        type="button"
        variant="ghost"
        size="sm"
        class="h-7 gap-1.5 px-2 text-muted-foreground hover:text-foreground"
        disabled={loading}
        title="Refresh enums"
        onclick={onrefresh}
      >
        <RefreshCw class={loading ? 'size-3.5 animate-spin' : 'size-3.5'} />
        <span class="text-ui-xs">Refresh</span>
      </Button>
    </div>
  </div>

  <!-- Content -->
  <div class="app-scroll min-h-0 flex-1 overflow-y-auto [will-change:transform]">
    {#if enums.length === 0}
      <div class="flex h-full flex-col items-center justify-center gap-3 text-center">
        <Tags class="size-8 shrink-0 text-muted-foreground" />
        <div>
          <p class="font-mono text-ui-sm text-muted-foreground">No enum types</p>
          <p class="mt-0.5 text-ui-xs text-muted-foreground">
            Enum types defined in this schema will appear here
          </p>
        </div>
      </div>
    {:else}
      <table class="w-full text-ui-xs">
        <thead class="studio-chrome sticky top-0 z-10 bg-panel text-left">
          <tr class="border-b border-border/50">
            <th class="w-[200px] px-3 py-2 font-mono font-normal text-muted-foreground">Name</th>
            <th class="px-3 py-2 font-mono font-normal text-muted-foreground">Values</th>
            <th class="w-52 px-3 py-2 font-mono font-normal text-muted-foreground">Used by</th>
            <th class="w-10 px-3 py-2 text-right font-mono font-normal text-muted-foreground">n</th>
          </tr>
        </thead>
        <tbody>
          {#each enums as e (e.name)}
            <tr class="border-b border-border/25 align-top hover:bg-accent/15">
              <td class="px-3 py-2.5 font-mono text-ui-xs font-medium text-foreground/90">
                {e.name}
              </td>
              <td class="px-3 py-2.5">
                {#if e.values.length > 0}
                  <div class="flex flex-wrap gap-x-2 gap-y-1">
                    {#each e.values as val (val)}
                      <span class="font-mono text-ui-xs text-muted-foreground">{val}</span>
                    {/each}
                  </div>
                {:else}
                  <span class="font-mono text-ui-xs text-muted-foreground">—</span>
                {/if}
              </td>
              <td class="px-3 py-2.5">
                {#if (e.usedInTables ?? []).length > 0}
                  <div class="flex flex-wrap gap-1">
                    {#each e.usedInTables as tbl (tbl)}
                      <span class="rounded bg-muted/40 px-1.5 py-px font-mono text-ui-3xs text-foreground/60">{tbl}</span>
                    {/each}
                  </div>
                {:else}
                  <span class="font-mono text-ui-xs text-muted-foreground">unused</span>
                {/if}
              </td>
              <td class="px-3 py-2.5 text-right font-mono text-ui-xs tabular-nums text-muted-foreground">
                {e.values.length}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
</div>
