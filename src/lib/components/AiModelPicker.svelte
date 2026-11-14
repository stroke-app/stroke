<script>
  import Settings2 from '@lucide/svelte/icons/settings-2'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import Sparkles from '@lucide/svelte/icons/sparkles'
  import SquareTerminal from '@lucide/svelte/icons/square-terminal'
  import Search from '@lucide/svelte/icons/search'
  import Star from '@lucide/svelte/icons/star'
  import Copy from '@lucide/svelte/icons/copy'
  import Route from '@lucide/svelte/icons/route'
  import HardDrive from '@lucide/svelte/icons/hard-drive'
  import Waypoints from '@lucide/svelte/icons/waypoints'
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal'
  import Check from '@lucide/svelte/icons/check'
  import Lock from '@lucide/svelte/icons/lock'
  import * as Popover from '$lib/components/ui/popover/index.js'
  import BrandIcon from '$lib/components/BrandIcon.svelte'
  import { hasBrand } from '$lib/brand-icons.js'
  import { cn } from '$lib/utils.js'
  import {
    aiProfiles,
    activeProfileId,
    setActiveProfile,
    useAgentProfile,
    PROVIDERS,
    PROVIDER_MODELS,
  } from '$lib/stores/ai-settings.js'
  import { localAgents, refreshLocalAgents, profileAgent, agentPrefs, EFFORTS, AGENTS_WITH_EFFORT } from '$lib/local-agents.js'
  import { readOnlyMode } from '$lib/stores/read-only.js'
  import { IS_MAC } from '$lib/shortcuts.js'

  let {
    /** @type {() => void} */
    onopenSettings = () => {},
  } = $props()

  /**
   * One pickable model, whether a saved HTTP profile or an agent CLI's model.
   * @typedef {{ key: string, group: string, label: string, provider: string, brand: string, tag: string, note: string, active: boolean, pick: () => void }} Entry
   * @typedef {{ key: string, label: string, brand: string, agent?: import('$lib/api.js').AgentInfo }} Group
   */

  const FAVORITES_KEY = 'stroke:ai-favorite-models'

  /** @returns {Set<string>} */
  function loadFavorites() {
    try {
      const raw = JSON.parse(localStorage.getItem(FAVORITES_KEY) ?? '[]')
      return new Set(Array.isArray(raw) ? raw.map(String) : [])
    } catch {
      return new Set()
    }
  }

  let open = $state(false)
  let query = $state('')
  let favorites = $state(loadFavorites())
  let railKey = $state('')
  let highlight = $state(0)
  let searchEl = $state(/** @type {HTMLInputElement | null} */ (null))
  let copied = $state(false)
  let showAll = $state(false)
  /** Rows shown before "Show more". */
  const LIST_CAP = 8

  /** @param {string} key */
  function toggleFavorite(key) {
    const next = new Set(favorites)
    if (next.has(key)) next.delete(key)
    else next.add(key)
    favorites = next
    try { localStorage.setItem(FAVORITES_KEY, JSON.stringify([...next])) } catch { /* ignore */ }
  }

  const activeProfile = $derived(
    $aiProfiles.find((p) => p.id === $activeProfileId) ?? $aiProfiles[0]
  )
  const activeAgent = $derived(profileAgent(activeProfile))

  /** A mark for providers without a brand one, so no two rail icons look alike. */
  const FALLBACK_ICONS = /** @type {Record<string, any>} */ ({
    openrouter: Route,
    ollama: HardDrive,
    omniroute: Waypoints,
    custom: SlidersHorizontal,
  })

  /** Show a provider's models. Hover does it too, but never over a search. @param {string} key */
  function showGroup(key) {
    if (query.trim() || railKey === key) return
    railKey = key
    highlight = 0
  }

  /** The brand mark for a provider; agents borrow their maker's. @param {string} provider */
  function brandOf(provider) {
    const agent = profileAgent({ provider })
    if (agent === 'claude') return 'claude'
    if (agent === 'codex') return 'openai'
    return provider
  }

  // The chip names the model; the mark beside it already says whose it is.
  const displayName = $derived(
    !activeProfile ? 'No model'
      : activeAgent ? activeProfile.name.split(' · ').pop() ?? activeProfile.name
      : activeProfile.name
  )

  /** @param {string} provider @param {string} model */
  function tagFor(provider, model) {
    return (PROVIDER_MODELS[provider] ?? []).find((m) => m.model === model)?.tag ?? ''
  }

  /** @type {Group[]} */
  const groups = $derived.by(() => {
    /** @type {Group[]} */
    const out = []
    for (const p of $aiProfiles) {
      if (profileAgent(p) || out.some((g) => g.key === `p:${p.provider}`)) continue
      out.push({ key: `p:${p.provider}`, label: PROVIDERS.find((x) => x.id === p.provider)?.label ?? p.provider, brand: p.provider })
    }
    for (const a of $localAgents) {
      out.push({ key: `a:${a.id}`, label: a.name, brand: brandOf(`agent:${a.id}`), agent: a })
    }
    return out
  })

  /** @type {Entry[]} */
  const entries = $derived.by(() => {
    /** @type {Entry[]} */
    const out = []
    for (const p of $aiProfiles) {
      if (profileAgent(p)) continue
      const group = groups.find((g) => g.key === `p:${p.provider}`)
      out.push({
        key: `profile:${p.id}`,
        group: `p:${p.provider}`,
        label: p.name,
        provider: group?.label ?? p.provider,
        brand: p.provider,
        tag: tagFor(p.provider, p.model),
        note: '',
        active: p.id === activeProfile?.id,
        pick: () => void setActiveProfile(p.id),
      })
    }
    for (const a of $localAgents) {
      if (a.status !== 'ready') continue
      for (const m of a.models) {
        out.push({
          key: `agent:${a.id}:${m.id}`,
          group: `a:${a.id}`,
          label: m.label,
          provider: a.name,
          brand: brandOf(`agent:${a.id}`),
          tag: '',
          note: m.note,
          active: activeAgent === a.id && activeProfile?.model === m.id,
          pick: () => void useAgentProfile(a.id, a.name, m),
        })
      }
    }
    return out
  })

  const matching = $derived.by(() => {
    const q = query.trim().toLowerCase()
    if (q) return entries.filter((e) => `${e.label} ${e.provider} ${e.tag} ${e.note}`.toLowerCase().includes(q))
    if (railKey === 'fav') return entries.filter((e) => favorites.has(e.key))
    return entries.filter((e) => e.group === railKey)
  })
  const visible = $derived(showAll || query.trim() ? matching : matching.slice(0, LIST_CAP))
  const hiddenCount = $derived(matching.length - visible.length)

  // Effort belongs to the model, so it is picked beside it (T3's Reasoning column).
  const hasEffort = $derived(!!activeAgent && AGENTS_WITH_EFFORT.has(activeAgent))
  const effortLabel = $derived(EFFORTS.find((e) => e.id === $agentPrefs.effort)?.label ?? 'Medium')
  /** @param {string} id */
  function setEffort(id) {
    agentPrefs.update((p) => ({ ...p, effort: /** @type {any} */ (id) }))
  }
  // Shown in the footer: what the agent may run. Read-only connection wins.
  const accessLabel = $derived(
    $readOnlyMode || !activeAgent ? ($readOnlyMode ? 'Read only' : '')
      : ({ read: 'Read only', write: 'Read & write', modify: 'Modify schema', dangerous: 'Full access' })[$agentPrefs.access] ?? 'Read only'
  )
  const modKey = IS_MAC ? '⌘' : 'Ctrl+'

  const railGroup = $derived(groups.find((g) => g.key === railKey))
  // An agent that cannot run yet gets its setup step instead of an empty list.
  const blockedAgent = $derived(
    !query.trim() && railGroup?.agent && railGroup.agent.status !== 'ready' ? railGroup.agent : null
  )

  function openPicker() {
    query = ''
    copied = false
    const current = entries.find((e) => e.active)
    railKey = current && favorites.has(current.key) ? 'fav' : current?.group ?? groups[0]?.key ?? 'fav'
    showAll = false
    highlight = Math.max(0, visible.findIndex((e) => e.active))
    void refreshLocalAgents()
    queueMicrotask(() => searchEl?.focus())
  }

  /** @param {Entry} e */
  function choose(e) {
    e.pick()
    open = false
  }

  /** @param {KeyboardEvent} e */
  function onSearchKey(e) {
    // ⌘1-⌘4 pick the first four rows, as the chips beside them say.
    if ((e.metaKey || e.ctrlKey) && /^[1-4]$/.test(e.key)) {
      const hit = visible[Number(e.key) - 1]
      if (hit) { e.preventDefault(); choose(hit) }
      return
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      highlight = Math.min(visible.length - 1, highlight + 1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      highlight = Math.max(0, highlight - 1)
    } else if (e.key === 'Enter' && visible[highlight]) {
      e.preventDefault()
      choose(visible[highlight])
    }
  }

  /** @param {string} command */
  async function copyCommand(command) {
    try {
      await navigator.clipboard.writeText(command)
      copied = true
    } catch { /* ignore */ }
  }
</script>

{#snippet mark(/** @type {string} */ brand, /** @type {string} */ cls)}
  {#if hasBrand(brand)}
    <BrandIcon name={brand} class={cls} />
  {:else if brand.startsWith('agent:')}
    <SquareTerminal class={cls} />
  {:else if FALLBACK_ICONS[brand]}
    {@const Icon = FALLBACK_ICONS[brand]}
    <Icon class={cls} />
  {:else}
    <Sparkles class={cls} />
  {/if}
{/snippet}

<Popover.Popover bind:open onOpenChange={(o) => { if (o) openPicker() }}>
  <Popover.PopoverTrigger
    class="inline-flex h-8 min-w-0 shrink items-center gap-2 rounded-full px-3 text-ui-xs text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground select-none data-[state=open]:bg-accent data-[state=open]:text-foreground outline-none focus-visible:outline-2 focus-visible:outline-ring"
    title={activeProfile ? `${activeProfile.name}${hasEffort ? ` · ${effortLabel}` : ''} · switch model` : 'Switch model'}
  >
    {@render mark(activeProfile ? brandOf(activeProfile.provider) : '', 'size-3.5 shrink-0')}
    <span class="min-w-0 max-w-48 truncate text-foreground/90">{displayName}</span>
    {#if hasEffort}<span class="shrink-0">{effortLabel}</span>{/if}
  </Popover.PopoverTrigger>

  <Popover.PopoverContent side="top" align="end" sideOffset={8} class={cn("flex h-96 flex-col overflow-hidden p-0", hasEffort ? "w-[32rem]" : "w-[24rem]")}>
    <div class="flex min-h-0 flex-1">
    <!-- Provider rail: favourites first, then every provider with a model. -->
    <div class="flex w-11 shrink-0 flex-col items-center gap-1 border-r border-border/50 py-2">
      <button
        type="button"
        class={cn(
          "relative flex size-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
          railKey === 'fav' && !query && "bg-accent text-foreground",
        )}
        title="Favourites"
        aria-label="Favourites"
        aria-pressed={railKey === 'fav' && !query}
        onfocus={() => showGroup('fav')}
        onclick={() => { railKey = 'fav'; query = ''; highlight = 0 }}
      >
        <Star class={cn("size-4 shrink-0", railKey === 'fav' && !query && "fill-current")} />
      </button>
      <span class="my-0.5 h-px w-6 bg-border/60" aria-hidden="true"></span>
      {#each groups as g (g.key)}
        <button
          type="button"
          class={cn(
            "relative flex size-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
            railKey === g.key && !query && "bg-accent text-foreground",
            g.agent && g.agent.status !== 'ready' && "opacity-40",
          )}
          title={g.agent && g.agent.status !== 'ready' ? `${g.label} (${g.agent.status === 'missing' ? 'not installed' : 'sign in first'})` : g.label}
          aria-label={g.label}
          aria-pressed={railKey === g.key && !query}
          onfocus={() => showGroup(g.key)}
          onclick={() => { railKey = g.key; query = ''; highlight = 0 }}
        >
          {@render mark(g.brand, 'size-4 shrink-0')}
          {#if railKey === g.key && !query}
            <span class="absolute -left-1.5 top-1.5 h-5 w-0.5 rounded-full bg-primary" aria-hidden="true"></span>
          {/if}
        </button>
      {/each}
    </div>

    <div class="flex min-w-0 flex-1 flex-col">
      <label data-slot="menu-search" class="flex h-10 shrink-0 items-center gap-2 px-3">
        <Search class="size-3.5 shrink-0 text-muted-foreground" />
        <input
          bind:this={searchEl}
          bind:value={query}
          oninput={() => (highlight = 0)}
          onkeydown={onSearchKey}
          class="no-focus-ring min-w-0 flex-1 bg-transparent text-ui-xs text-foreground outline-none placeholder:text-muted-foreground"
          placeholder="Search models…"
          aria-label="Search models"
          role="combobox"
          aria-expanded="true"
          aria-controls="ai-model-list"
          aria-autocomplete="list"
          aria-activedescendant={visible.length ? `ai-model-opt-${highlight}` : undefined}
        />
      </label>

      <div id="ai-model-list" class="min-h-0 flex-1 overflow-y-auto p-1.5" role="listbox" aria-label="Models">
        {#if blockedAgent}
          <div class="flex flex-col gap-2 px-2 py-3">
            <p class="text-ui-xs font-medium text-foreground">
              {blockedAgent.name} {blockedAgent.status === 'missing' ? 'is not installed' : 'needs you to sign in'}
            </p>
            {#if blockedAgent.setup}
              <button
                type="button"
                class="group flex items-center gap-2 rounded-md border border-border/60 bg-muted/30 px-2 py-1.5 text-left font-mono text-ui-3xs text-foreground hover:bg-muted/50"
                onclick={() => blockedAgent?.setup && copyCommand(blockedAgent.setup.command)}
                title="Copy"
              >
                <span class="min-w-0 flex-1 truncate">{blockedAgent.setup.command}</span>
                <Copy class="size-3 shrink-0 text-muted-foreground" />
              </button>
              <p class="text-ui-3xs text-muted-foreground">{copied ? 'Copied. Run it in a terminal, then reopen this menu.' : 'Run this in a terminal, then reopen this menu.'}</p>
            {:else if blockedAgent.hint}
              <p class="text-ui-3xs text-muted-foreground">{blockedAgent.hint}</p>
            {/if}
          </div>
        {:else if visible.length === 0}
          <p class="px-2 py-6 text-center text-ui-2xs text-muted-foreground">
            {query.trim() ? 'No model matches.' : railKey === 'fav' ? 'Star a model to keep it here.' : 'No models.'}
          </p>
        {:else}
          {#each visible as e, i (e.key)}
            <div
              id="ai-model-opt-{i}"
              role="option"
              tabindex="-1"
              aria-selected={i === highlight}
              aria-current={e.active ? 'true' : undefined}
              title={[e.provider, e.note || e.tag].filter(Boolean).join(' · ')}
              class={cn(
                "group flex min-h-7 w-full min-w-0 cursor-pointer items-center gap-2 rounded-sm px-2 py-1",
                i === highlight && "bg-accent",
              )}
              onmouseenter={() => (highlight = i)}
              onclick={() => choose(e)}
              onkeydown={(ev) => ev.key === 'Enter' && choose(e)}
            >
              {@render mark(e.brand, 'size-3.5 shrink-0 text-muted-foreground')}
              <span class={cn("min-w-0 truncate text-ui-xs", e.active ? "font-medium text-foreground" : "text-foreground/90")}>{e.label}</span>
              {#if e.active}<Check class="size-3.5 shrink-0 text-foreground" />{/if}
              <span class="flex-1"></span>
              <button
                type="button"
                class={cn(
                  "flex size-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:text-foreground",
                  favorites.has(e.key) ? "opacity-100" : "opacity-0 group-hover:opacity-100 focus-visible:opacity-100",
                )}
                aria-label={favorites.has(e.key) ? `Unstar ${e.label}` : `Star ${e.label}`}
                onclick={(ev) => { ev.stopPropagation(); toggleFavorite(e.key) }}
              >
                <Star class={cn("size-3.5 shrink-0", favorites.has(e.key) && "fill-current text-foreground")} />
              </button>
              {#if i < 4}
                <span class="shrink-0 font-mono text-ui-3xs tracking-widest text-muted-foreground">{modKey}{i + 1}</span>
              {/if}
            </div>
          {/each}
          {#if hiddenCount > 0}
            <button
              type="button"
              class="flex min-h-7 w-full items-center gap-2 rounded-sm px-2 py-1 text-left text-ui-xs text-muted-foreground hover:bg-accent hover:text-foreground"
              onclick={() => (showAll = true)}
            >
              <ChevronDown class="size-3.5 shrink-0" />
              Show more
              <span class="ml-auto font-mono text-ui-3xs tabular-nums">{hiddenCount}</span>
            </button>
          {/if}
        {/if}
      </div>
    </div>

    {#if hasEffort}
      <!-- Reasoning: the effort levels of the model above, picked beside it. -->
      <div class="flex w-36 shrink-0 flex-col border-l border-border/50">
        <div class="flex h-10 shrink-0 items-center px-3 text-ui-3xs font-medium text-muted-foreground">
          <span class="truncate">Reasoning · {displayName}</span>
        </div>
        <div class="flex flex-col p-1.5" role="radiogroup" aria-label="Reasoning effort">
          {#each EFFORTS as ef (ef.id)}
            {@const on = $agentPrefs.effort === ef.id}
            <button
              type="button"
              role="radio"
              aria-checked={on}
              class={cn("flex min-h-7 items-center gap-2 rounded-sm px-2 py-1 text-left text-ui-xs hover:bg-accent", on ? "font-medium text-foreground" : "text-foreground/90")}
              onclick={() => setEffort(ef.id)}
            >
              <span class="min-w-0 flex-1 truncate">{ef.label}</span>
              {#if on}<Check class="size-3.5 shrink-0" />{/if}
            </button>
          {/each}
        </div>
      </div>
    {/if}
    </div>

    <div class="flex h-8 shrink-0 items-center gap-2 border-t border-border/50 px-1.5">
      <button
        type="button"
        class="flex h-6 items-center gap-1.5 rounded-sm px-1.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={() => { open = false; onopenSettings() }}
      >
        <Settings2 class="size-3.5 shrink-0" />
        <span>Manage models…</span>
      </button>
      {#if accessLabel}
        <!-- What the agent may run. Full access is the one that warrants colour. -->
        <span class={cn("ml-auto flex items-center gap-1 px-1.5 text-ui-2xs", $agentPrefs.access === 'dangerous' && activeAgent && !$readOnlyMode ? "text-warning" : "text-muted-foreground")} title="What the agent may run">
          <Lock class="size-3 shrink-0" aria-hidden="true" />{accessLabel}
        </span>
      {/if}
    </div>
  </Popover.PopoverContent>
</Popover.Popover>
