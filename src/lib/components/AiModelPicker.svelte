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
  let highlight = $state(0)
  let searchEl = $state(/** @type {HTMLInputElement | null} */ (null))
  /** The agent whose setup command was just copied. */
  let copied = $state('')

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

  /** Per provider, rows shown before "Show N more". */
  const GROUP_CAP = 6
  /** Groups expanded past the cap. */
  let expanded = $state(/** @type {Set<string>} */ (new Set()))

  /**
   * What the list shows: favourites first (when any), then one section per
   * provider in the order they were added. A search flattens everything into
   * one section of matches.
   * @typedef {{ key: string, label: string, brand: string, agent?: import('$lib/api.js').AgentInfo, rows: Entry[], more: number }} Section
   */
  const sections = $derived.by(() => {
    const q = query.trim().toLowerCase()
    if (q) {
      const rows = entries.filter((e) => `${e.label} ${e.provider} ${e.tag} ${e.note}`.toLowerCase().includes(q))
      return /** @type {Section[]} */ ([{ key: 'search', label: '', brand: '', rows, more: 0 }])
    }
    /** @type {Section[]} */
    const out = []
    const favs = entries.filter((e) => favorites.has(e.key))
    if (favs.length) out.push({ key: 'fav', label: 'Favourites', brand: '', rows: favs, more: 0 })
    for (const g of groups) {
      const all = entries.filter((e) => e.group === g.key)
      if (!all.length && !(g.agent && g.agent.status !== 'ready')) continue
      // The active model always shows, even past the cap.
      const open = expanded.has(g.key) || all.length <= GROUP_CAP + 1
      let rows = open ? all : all.slice(0, GROUP_CAP)
      const act = all.find((e) => e.active)
      if (act && !rows.includes(act)) rows = [...rows, act]
      out.push({ key: g.key, label: g.label, brand: g.brand, agent: g.agent, rows, more: all.length - rows.length })
    }
    // Agents that can't run yet go last: one line each, below what can be picked.
    const ready = (/** @type {Section} */ x) => !x.agent || x.agent.status === 'ready'
    return [...out.filter(ready), ...out.filter((x) => !ready(x))]
  })
  /** Every row in order, for the keyboard. */
  const flat = $derived(sections.flatMap((s) => s.rows))

  // Effort belongs to the model, so it is picked beside it (T3's Reasoning column).
  const hasEffort = $derived(!!activeAgent && AGENTS_WITH_EFFORT.has(activeAgent))
  const effortLabel = $derived(EFFORTS.find((e) => e.id === $agentPrefs.effort)?.label ?? 'Medium')
  /** Short enough for one row of five. */
  const EFFORT_SHORT = /** @type {Record<string, string>} */ ({ low: 'Low', medium: 'Med', high: 'High', xhigh: 'X-High', max: 'Max' })
  /** @param {string} id */
  function setEffort(id) {
    agentPrefs.update((p) => ({ ...p, effort: /** @type {any} */ (id) }))
  }
  // Shown in the footer: what the agent may run. Read-only connection wins.
  const accessLabel = $derived(
    $readOnlyMode || !activeAgent ? ($readOnlyMode ? 'Read only' : '')
      : ({ read: 'Read only', write: 'Read & write', modify: 'Modify schema', dangerous: 'Full access' })[$agentPrefs.access] ?? 'Read only'
  )

  function openPicker() {
    query = ''
    copied = ''
    expanded = new Set()
    highlight = Math.max(0, flat.findIndex((e) => e.active))
    void refreshLocalAgents()
    queueMicrotask(() => {
      searchEl?.focus()
      document.getElementById(`ai-model-opt-${highlight}`)?.scrollIntoView({ block: 'nearest' })
    })
  }

  /** @param {Entry} e */
  function choose(e) {
    e.pick()
    open = false
  }

  /** @param {number} i */
  function moveTo(i) {
    highlight = i
    document.getElementById(`ai-model-opt-${i}`)?.scrollIntoView({ block: 'nearest' })
  }

  /** @param {KeyboardEvent} e */
  function onSearchKey(e) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      moveTo(Math.min(flat.length - 1, highlight + 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      moveTo(Math.max(0, highlight - 1))
    } else if (e.key === 'Enter' && flat[highlight]) {
      e.preventDefault()
      choose(flat[highlight])
    }
  }

  /** @param {string} id @param {string} command */
  async function copyCommand(id, command) {
    try {
      await navigator.clipboard.writeText(command)
      copied = id
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

  <Popover.PopoverContent side="top" align="end" sideOffset={8} class="flex max-h-[min(30rem,70vh)] w-[22rem] flex-col overflow-hidden p-0">
    <label data-slot="menu-search" class="search-shell m-1.5 flex h-8 shrink-0 items-center gap-2 px-2.5">
      <Search class="size-3.5 shrink-0 text-muted-foreground" />
      <input
        bind:this={searchEl}
        bind:value={query}
        oninput={() => (highlight = 0)}
        onkeydown={onSearchKey}
        class="no-focus-ring min-w-0 flex-1 bg-transparent text-ui-xs text-foreground outline-none placeholder:text-muted-foreground"
        placeholder="Search models"
        aria-label="Search models"
        role="combobox"
        aria-expanded="true"
        aria-controls="ai-model-list"
        aria-autocomplete="list"
        aria-activedescendant={flat.length ? `ai-model-opt-${highlight}` : undefined}
      />
    </label>

    <div id="ai-model-list" class="min-h-0 flex-1 overflow-y-auto px-1.5 pb-1.5" role="listbox" aria-label="Models">
      {#if flat.length === 0 && !sections.some((s) => s.agent && s.agent.status !== 'ready')}
        <p class="px-2 py-6 text-center text-ui-2xs text-muted-foreground">
          {query.trim() ? 'No model matches.' : 'No models yet. Add one in Manage models.'}
        </p>
      {/if}
      {#each sections as sec (sec.key)}
        {#if sec.label}
          <div class="flex h-7 items-center gap-1.5 px-2 pt-1.5 text-ui-3xs font-medium text-muted-foreground">
            {#if sec.key === 'fav'}
              <Star class="size-3 shrink-0 fill-current" />
            {:else}
              {@render mark(sec.brand, 'size-3 shrink-0')}
            {/if}
            <span class="truncate">{sec.label}</span>
            {#if sec.agent && sec.agent.status !== 'ready'}
              {@const agent = sec.agent}
              <span class="ml-auto shrink-0 font-normal">{agent.status === 'missing' ? 'Not installed' : 'Sign in first'}</span>
              {#if agent.setup}
                <button
                  type="button"
                  class="flex h-5 shrink-0 items-center gap-1 rounded px-1.5 font-normal text-foreground/90 hover:bg-accent"
                  title={`${agent.setup.command}\nRun it in a terminal, then reopen this menu.`}
                  onclick={() => agent.setup && copyCommand(agent.id, agent.setup.command)}
                >
                  {#if copied === agent.id}<Check class="size-3 shrink-0" />Copied{:else}<Copy class="size-3 shrink-0" />{agent.status === 'missing' ? 'Copy install' : 'Copy sign-in'}{/if}
                </button>
              {/if}
            {/if}
          </div>
        {/if}
        {#each sec.rows as e (sec.key + e.key)}
          {@const i = flat.indexOf(e, sections.slice(0, sections.indexOf(sec)).reduce((n, x) => n + x.rows.length, 0))}
          <div
            id="ai-model-opt-{i}"
            role="option"
            tabindex="-1"
            aria-selected={i === highlight}
            aria-current={e.active ? 'true' : undefined}
            title={[e.provider, e.note || e.tag].filter(Boolean).join(' · ')}
            class={cn(
              "group flex h-8 w-full min-w-0 cursor-pointer items-center gap-2 rounded-md px-2",
              i === highlight && "bg-accent",
            )}
            onmouseenter={() => (highlight = i)}
            onclick={() => choose(e)}
            onkeydown={(ev) => ev.key === 'Enter' && choose(e)}
          >
            {#if query.trim() || sec.key === 'fav'}
              {@render mark(e.brand, 'size-3.5 shrink-0 text-muted-foreground')}
            {/if}
            <span class={cn("min-w-0 truncate text-ui-xs", e.active ? "font-medium text-foreground" : "text-foreground/90")}>{e.label}</span>
            {#if (e.tag || e.note) && !e.label.toLowerCase().includes((e.tag || e.note).toLowerCase())}
              <span class="min-w-0 shrink truncate text-ui-3xs text-muted-foreground">{e.tag || e.note}</span>
            {/if}
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
              <Star class={cn("size-3 shrink-0", favorites.has(e.key) && "fill-current text-foreground")} />
            </button>
            <Check class={cn("size-3.5 shrink-0 text-primary", !e.active && "invisible")} aria-hidden="true" />
          </div>
        {/each}
        {#if sec.more > 0}
          <button
            type="button"
            class="flex h-7 w-full items-center gap-1.5 rounded-md px-2 text-left text-ui-2xs text-muted-foreground hover:bg-accent hover:text-foreground"
            onclick={() => (expanded = new Set([...expanded, sec.key]))}
          >
            <ChevronDown class="size-3.5 shrink-0" />
            {sec.more} more
          </button>
        {/if}
      {/each}
    </div>

    {#if hasEffort}
      <!-- Reasoning belongs to the chosen model: one row under the list. -->
      <div class="flex shrink-0 items-center gap-2 border-t border-border/50 px-2.5 py-1.5">
        <span class="shrink-0 text-ui-2xs text-muted-foreground">Reasoning</span>
        <div class="ml-auto flex items-center gap-0.5 rounded-md bg-muted/40 p-0.5" role="radiogroup" aria-label="Reasoning effort">
          {#each EFFORTS as ef (ef.id)}
            {@const on = $agentPrefs.effort === ef.id}
            <button
              type="button"
              role="radio"
              aria-checked={on}
              title={ef.label}
              class={cn("h-6 rounded px-1.5 text-ui-3xs transition-colors", on ? "bg-background font-medium text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground")}
              onclick={() => setEffort(ef.id)}
            >{EFFORT_SHORT[ef.id] ?? ef.label}</button>
          {/each}
        </div>
      </div>
    {/if}

    <div class="flex h-8 shrink-0 items-center gap-2 border-t border-border/50 px-1.5">
      <button
        type="button"
        class="flex h-6 items-center gap-1.5 rounded-md px-1.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={() => { open = false; onopenSettings() }}
      >
        <Settings2 class="size-3.5 shrink-0" />
        <span>Manage models</span>
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
