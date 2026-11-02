<script>
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import Lock from '@lucide/svelte/icons/lock'
  import PenLine from '@lucide/svelte/icons/pen-line'
  import Wrench from '@lucide/svelte/icons/wrench'
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert'
  import Gauge from '@lucide/svelte/icons/gauge'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { cn } from '$lib/utils.js'
  import { agentPrefs, EFFORTS, AGENTS_WITH_EFFORT } from '$lib/local-agents.js'
  import { readOnlyMode } from '$lib/stores/read-only.js'

  /** The composer's controls for a local agent: effort and access, beside the model. */
  let {
    /** @type {string} */
    agent = '',
  } = $props()

  /** @type {Record<string, string>} */
  const EFFORT_NOTES = {
    low: 'Fast replies, lighter reasoning.',
    medium: 'Balances speed and depth.',
    high: 'More reasoning for harder questions.',
    xhigh: 'Extra depth for complex analysis.',
    max: 'The most reasoning the model allows.',
  }

  // Enforced by Stroke's MCP server (src-tauri/src/mcp/access.rs), not left
  // to the model: a statement above the level is refused.
  const ACCESS = [
    { id: 'read', label: 'Read only', note: 'SELECT and EXPLAIN. Changes come back as SQL for you to run.', icon: Lock },
    { id: 'write', label: 'Read & write', note: 'Inserts, and updates or deletes with a WHERE clause.', icon: PenLine },
    { id: 'modify', label: 'Modify schema', note: 'Also CREATE, ALTER and indexes. Nothing dropped.', icon: Wrench },
    { id: 'dangerous', label: 'Dangerous', note: 'Anything, including DROP, TRUNCATE and changes without WHERE.', icon: TriangleAlert },
  ]

  const effortLabel = $derived(EFFORTS.find((e) => e.id === $agentPrefs.effort)?.label ?? 'Medium')
  // A connection opened read-only overrides the chat's setting.
  const access = $derived($readOnlyMode ? ACCESS[0] : ACCESS.find((a) => a.id === $agentPrefs.access) ?? ACCESS[0])

  const chip = "inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground select-none data-[state=open]:bg-accent data-[state=open]:text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-60"
</script>

{#if AGENTS_WITH_EFFORT.has(agent)}
  <span class="h-3.5 w-px shrink-0 bg-border/70" aria-hidden="true"></span>
  <DropdownMenu.Root>
    <DropdownMenu.Trigger class={chip} title="Reasoning effort">
      <span>{effortLabel}</span>
      <ChevronDown class="size-3 shrink-0 opacity-40" />
    </DropdownMenu.Trigger>
    <DropdownMenu.Content side="top" align="start" class="w-64 p-1">
      <DropdownMenu.RadioGroup
        value={$agentPrefs.effort}
        onValueChange={(v) => agentPrefs.update((p) => ({ ...p, effort: /** @type {any} */ (v) }))}
      >
        {#each EFFORTS as e (e.id)}
          <DropdownMenu.RadioItem value={e.id} class="flex items-start gap-2 px-2.5 py-1.5">
            <Gauge class="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
            <div class="flex min-w-0 flex-col">
              <span class="text-ui-xs font-medium text-foreground">{e.label}</span>
              <span class="text-ui-3xs text-muted-foreground">{EFFORT_NOTES[e.id]}</span>
            </div>
          </DropdownMenu.RadioItem>
        {/each}
      </DropdownMenu.RadioGroup>
    </DropdownMenu.Content>
  </DropdownMenu.Root>
{/if}

<span class="h-3.5 w-px shrink-0 bg-border/70" aria-hidden="true"></span>
<DropdownMenu.Root>
  <DropdownMenu.Trigger
    class={cn(chip, access.id === 'dangerous' ? "text-destructive hover:text-destructive" : access.id !== 'read' && "text-foreground")}
    title="What the agent may run"
  >
    <access.icon class="size-3.5 shrink-0" />
    <span>{access.label}</span>
    <ChevronDown class="size-3 shrink-0 opacity-40" />
  </DropdownMenu.Trigger>
  <DropdownMenu.Content side="top" align="start" class="w-80 p-1">
    {#if $readOnlyMode}
      <p class="flex items-start gap-2 rounded-md bg-muted/40 px-2.5 py-2 text-ui-3xs text-muted-foreground">
        <Lock class="mt-0.5 size-3 shrink-0" />
        This connection is open read-only, so the agent is too. Turn read-only off for the connection to allow more.
      </p>
    {/if}
    <DropdownMenu.RadioGroup
      value={access.id}
      onValueChange={(v) => agentPrefs.update((p) => ({ ...p, access: /** @type {any} */ (ACCESS.some((a) => a.id === v) ? v : 'read') }))}
    >
      {#each ACCESS as a (a.id)}
        <DropdownMenu.RadioItem value={a.id} disabled={$readOnlyMode && a.id !== 'read'} class="flex items-start gap-2 px-2.5 py-1.5">
          <a.icon class={cn("mt-0.5 size-3.5 shrink-0", a.id === 'dangerous' ? "text-destructive" : "text-muted-foreground")} />
          <div class="flex min-w-0 flex-col">
            <span class={cn("text-ui-xs font-medium", a.id === 'dangerous' ? "text-destructive" : "text-foreground")}>{a.label}</span>
            <span class="text-ui-3xs text-muted-foreground">{a.note}</span>
          </div>
        </DropdownMenu.RadioItem>
      {/each}
    </DropdownMenu.RadioGroup>
  </DropdownMenu.Content>
</DropdownMenu.Root>
