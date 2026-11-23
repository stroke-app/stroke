<script>
  /**
   * Settings ▸ Keybindings: every app shortcut, searchable, and every command's
   * key remappable in place.
   *
   * Rows come from `$lib/shortcuts.js`: COMMANDS (remappable, with their
   * defaults) and FIXED_SHORTCUTS (keys a component owns itself - the SQL
   * editor, the grid - listed so the page answers "what does this key do"
   * without pretending they can be changed). Overrides live in
   * `$lib/stores/keybindings.js`; every hotkey reads them through
   * `bindingFor`, so a change here applies the moment it is made.
   *
   * Recording listens on window in the capture phase and stops the key there,
   * so the combo being typed does not also run whatever it is bound to. A bare
   * modifier records nothing: the combo is taken on the first non-modifier key.
   */
  import { onDestroy, tick } from "svelte";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import Search from "@lucide/svelte/icons/search";
  import { Button } from "$lib/components/ui/button/index.js";
  import Kbd from "./Kbd.svelte";
  import { cn } from "$lib/utils.js";
  import { COMMANDS, FIXED_SHORTCUTS, SHORTCUT_GROUPS, comboText, comboTitle } from "$lib/shortcuts.js";
  import {
    keybindings,
    effectiveCombo,
    isOverridden,
    setBinding,
    unbind,
    resetBinding,
    resetAllBindings,
    assignReplacing,
    findConflict,
    recordKeydown,
    setRecordingShortcut,
  } from "$lib/stores/keybindings.js";

  /**
   * @typedef {{ key: string, id: string | null, label: string, group: string, combo: string, fixed: boolean, changed: boolean }} Row
   * @typedef {{ id: string, combo: string, conflict: import('$lib/stores/keybindings.js').Conflict }} Pending
   */

  let query = $state("");
  /** Commands (remappable), Built-in (owned by the editor and grid), or the ones changed here. */
  let tab = $state(/** @type {'commands' | 'builtin' | 'changed'} */ ("commands"));
  /** Numbered runs ("Switch to saved connection 1..9") that are expanded. */
  let openSeries = $state(/** @type {Set<string>} */ (new Set()));

  /** @type {Row[]} */
  const rows = $derived.by(() => {
    const overrides = $keybindings;
    /** @type {Row[]} */
    const out = COMMANDS.map((c) => ({
      key: c.id,
      id: c.id,
      label: c.label,
      group: c.group,
      combo: effectiveCombo(overrides, c.id),
      fixed: false,
      changed: isOverridden(overrides, c.id),
    }));
    FIXED_SHORTCUTS.forEach((s, i) =>
      out.push({ key: `fixed:${i}`, id: null, label: s.label, group: s.group, combo: s.combo, fixed: true, changed: false }),
    );
    return out;
  });

  const overrideCount = $derived(Object.keys($keybindings).length);

  const tabRows = $derived(
    tab === "builtin" ? rows.filter((r) => r.fixed)
      : tab === "changed" ? rows.filter((r) => r.changed)
      : rows.filter((r) => !r.fixed),
  );

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tabRows;
    return tabRows.filter(
      (r) =>
        r.label.toLowerCase().includes(q) ||
        r.group.toLowerCase().includes(q) ||
        (r.combo && comboText(r.combo).includes(q)) ||
        (!r.combo && "unbound".includes(q)),
    );
  });

  /**
   * @typedef {{ kind: 'row', row: Row } | { kind: 'series', key: string, label: string, rows: Row[] }} Item
   */
  /**
   * A group's rows, with runs of numbered commands folded into one item:
   * nine "Switch to saved connection N" rows read as one thing, and pushed
   * everything after them off the screen. A search shows them unfolded.
   * @param {Row[]} list
   * @returns {Item[]}
   */
  function fold(list) {
    if (query.trim()) return list.map((row) => ({ kind: "row", row }));
    /** @type {Item[]} */
    const out = [];
    for (let i = 0; i < list.length; ) {
      const m = /^(.*\D) (\d+)$/.exec(list[i].label);
      let j = i + 1;
      if (m) while (j < list.length && list[j].label.startsWith(m[1] + " ") && /^\d+$/.test(list[j].label.slice(m[1].length + 1))) j++;
      if (m && j - i >= 4) {
        const run = list.slice(i, j);
        out.push({ kind: "series", key: `${list[i].group}:${m[1]}`, label: `${m[1]} ${run[0].label.slice(m[1].length + 1)} to ${run[run.length - 1].label.slice(m[1].length + 1)}`, rows: run });
      } else {
        for (let k = i; k < j; k++) out.push({ kind: "row", row: list[k] });
      }
      i = j;
    }
    return out;
  }

  /** In SHORTCUT_GROUPS order. */
  const grouped = $derived(
    SHORTCUT_GROUPS.map((g) => ({
      label: g.label,
      items: fold(visible.filter((r) => r.group === g.label)),
    })).filter((g) => g.items.length > 0),
  );

  const builtinCount = $derived(rows.filter((r) => r.fixed).length);
  const commandCount = $derived(rows.length - builtinCount);

  // ── Recording ─────────────────────────────────────────────────────────────

  /** @type {string | null} */
  let recordingId = $state(null);
  /** @type {Pending | null} */
  let pending = $state(null);
  /** @type {Record<string, HTMLButtonElement>} */
  const editButtons = {};

  /** @param {KeyboardEvent} e */
  function onRecordKey(e) {
    if (!recordingId) return;
    // Ours, entirely: no hotkey, dialog Escape or menu accelerator sees it.
    e.preventDefault();
    e.stopPropagation();
    e.stopImmediatePropagation();
    if (e.repeat) return;
    const result = recordKeydown(e);
    if (!result) return; // a modifier on its own - keep waiting for the key
    const id = recordingId;
    stopRecording();
    if (result.type === "cancel") return;
    if (result.type === "clear") { unbind(id); return; }
    commit(id, result.combo);
  }

  /** @param {string} id */
  function startRecording(id) {
    if (recordingId) stopRecording();
    pending = null;
    recordingId = id;
    setRecordingShortcut(true);
    window.addEventListener("keydown", onRecordKey, true);
  }

  function stopRecording() {
    const id = recordingId;
    recordingId = null;
    setRecordingShortcut(false);
    window.removeEventListener("keydown", onRecordKey, true);
    if (id) void tick().then(() => editButtons[id]?.focus());
  }

  /** @param {string} id @param {string} combo */
  function commit(id, combo) {
    if (combo === effectiveCombo($keybindings, id)) return;
    const conflict = findConflict($keybindings, combo, id);
    if (conflict) { pending = { id, combo, conflict }; return; }
    setBinding(id, combo);
  }

  function confirmPending() {
    if (!pending) return;
    if (pending.conflict.fixed) setBinding(pending.id, pending.combo);
    else assignReplacing(pending.id, pending.combo, pending.conflict.id);
    pending = null;
  }

  onDestroy(() => {
    if (recordingId) {
      setRecordingShortcut(false);
      window.removeEventListener("keydown", onRecordKey, true);
    }
  });
</script>

<div class="flex flex-col gap-4">
  <!-- Tabs and search on one line; Reset all only when something changed. -->
  <div class="flex flex-wrap items-center gap-2">
    <div role="tablist" aria-label="Shortcuts" class="flex shrink-0 gap-0.5 rounded-full bg-muted/60 p-0.5">
      {#each [
        { v: "commands", l: "Commands", n: commandCount },
        { v: "builtin", l: "Built-in", n: builtinCount },
        { v: "changed", l: "Changed", n: overrideCount },
      ] as t (t.v)}
        <button
          type="button"
          role="tab"
          aria-selected={tab === t.v}
          onclick={() => (tab = /** @type {'commands' | 'builtin' | 'changed'} */ (t.v))}
          class={cn(
            "inline-flex h-7 items-center gap-1.5 rounded-full px-3 text-ui-xs font-medium transition-colors",
            tab === t.v ? "bg-background text-foreground shadow-xs" : "text-muted-foreground hover:text-foreground",
          )}
        >
          {t.l}
          <span class="text-ui-3xs text-muted-foreground tabular-nums">{t.n}</span>
        </button>
      {/each}
    </div>
    <div class="relative min-w-40 flex-1">
      <Search class="pointer-events-none absolute left-2.5 top-1/2 size-3.5 shrink-0 -translate-y-1/2 text-muted-foreground" />
      <input
        bind:value={query}
        type="search"
        placeholder="Search by command or key"
        aria-label="Search keybindings"
        class="search-box no-field-frame h-8 w-full pl-8 pr-3 text-ui-xs"
      />
    </div>
    {#if overrideCount > 0}
      <Button variant="ghost" size="sm" class="gap-1.5 text-muted-foreground" onclick={() => { stopRecording(); pending = null; resetAllBindings(); }}>
        <RotateCcw class="size-3.5 shrink-0" />
        Reset all
      </Button>
    {/if}
  </div>

  {#if tab === "builtin"}
    <p class="-mt-1 text-ui-2xs leading-relaxed text-muted-foreground">Keys the SQL editor and the grid handle themselves. Listed so you can look them up; they can't be changed.</p>
  {/if}

  {#each grouped as g (g.label)}
    <section aria-label={g.label}>
      <p class="mb-1 px-2 text-ui-2xs font-medium text-muted-foreground">{g.label}</p>
      <ul class="flex flex-col">
        {#each g.items as it (it.kind === "row" ? it.row.key : it.key)}
          {#if it.kind === "row"}
            {@render row(it.row, false)}
          {:else}
            {@const open = openSeries.has(it.key)}
            <li>
              <button
                type="button"
                class="flex h-9 w-full items-center gap-2 rounded-md px-2 text-left transition-colors hover:bg-muted/40"
                aria-expanded={open}
                onclick={() => { const n = new Set(openSeries); if (open) n.delete(it.key); else n.add(it.key); openSeries = n; }}
              >
                <ChevronRight class={cn("size-3.5 shrink-0 text-muted-foreground transition-transform duration-150", open && "rotate-90")} />
                <span class="min-w-0 flex-1 truncate text-ui-sm text-foreground">{it.label}</span>
                {#if it.rows.some((r) => r.changed)}<span class="size-1.5 shrink-0 rounded-full bg-primary" title="Some are changed"></span>{/if}
                <span class="flex shrink-0 items-center gap-1 text-ui-2xs text-muted-foreground">
                  {#if it.rows[0].combo}<Kbd combo={it.rows[0].combo} />{/if}
                  <span>to</span>
                  {#if it.rows[it.rows.length - 1].combo}<Kbd combo={it.rows[it.rows.length - 1].combo} />{/if}
                </span>
                <span class="w-7 shrink-0" aria-hidden="true"></span>
              </button>
              {#if open}
                <ul class="ms-5 flex flex-col border-s border-border/50 ps-1">
                  {#each it.rows as r (r.key)}{@render row(r, true)}{/each}
                </ul>
              {/if}
            </li>
          {/if}
        {/each}
      </ul>
    </section>
  {:else}
    <div class="flex flex-col items-center gap-1.5 py-12 text-center">
      <p class="text-ui-sm text-foreground">{query ? `No shortcut matches "${query}"` : tab === "changed" ? "No changed shortcuts" : "Nothing here"}</p>
      {#if tab === "changed" && !query}<p class="text-ui-xs text-muted-foreground">Click a shortcut under Commands to change it.</p>{/if}
    </div>
  {/each}
</div>

{#snippet row(/** @type {Row} */ r, /** @type {boolean} */ nested)}
  {@const isRecording = recordingId === r.id && !!r.id}
  {@const isPending = !!pending && pending.id === r.id}
  <li>
    <div class={cn("group/kb flex h-9 items-center gap-2 rounded-md px-2 transition-colors hover:bg-muted/40", isRecording && "bg-primary/8 hover:bg-primary/8", nested && "h-8")}>
      <span class={cn("size-1.5 shrink-0 rounded-full", r.changed ? "bg-primary" : "bg-transparent")} title={r.changed ? "Changed from the default" : undefined}></span>
      <span class={cn("min-w-0 flex-1 truncate text-ui-sm", r.fixed ? "text-muted-foreground" : "text-foreground")}>{r.label}</span>

      {#if r.fixed || !r.id}
        <!-- Built-in: shown, not editable. -->
        {#if r.combo}<Kbd combo={r.combo} />{:else}<span class="text-ui-xs text-muted-foreground">Unbound</span>{/if}
        <span class="w-7 shrink-0" aria-hidden="true"></span>
      {:else}
        {@const id = r.id}
        <!-- The keys are the control: click them to record new ones. -->
        <button
          bind:this={editButtons[id]}
          type="button"
          class={cn(
            "inline-flex h-7 min-w-16 shrink-0 items-center justify-end gap-1 rounded-md px-1.5 transition-colors hover:bg-muted focus-visible:outline-2 focus-visible:outline-ring",
            isRecording && "bg-background",
          )}
          aria-label={isRecording ? `Stop recording a shortcut for ${r.label}` : `Change the shortcut for ${r.label}`}
          aria-pressed={isRecording}
          title={isRecording ? "Esc cancels · Backspace unbinds" : "Click to change"}
          onclick={() => (isRecording ? stopRecording() : startRecording(id))}
        >
          {#if isRecording}
            <span class="text-ui-xs font-medium text-primary" aria-live="assertive">Press keys…</span>
          {:else if r.combo}
            <Kbd combo={r.combo} />
          {:else}
            <span class="text-ui-xs text-muted-foreground">Unbound</span>
          {/if}
        </button>
        {#if r.changed && !isRecording}
          <button
            type="button"
            class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-[opacity,background-color,color] group-hover/kb:opacity-100 hover:bg-muted hover:text-foreground focus-visible:opacity-100"
            aria-label={`Reset the shortcut for ${r.label}`}
            title="Reset to default"
            onclick={() => { if (pending?.id === id) pending = null; resetBinding(id); }}
          >
            <RotateCcw class="size-3.5 shrink-0" />
          </button>
        {:else}
          <span class="w-7 shrink-0" aria-hidden="true"></span>
        {/if}
      {/if}
    </div>

    {#if isPending && pending}
      <div class="mx-2 mb-2 flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg bg-warning/10 px-3 py-2" role="alert">
        <p class="min-w-0 flex-1 text-ui-xs leading-relaxed text-foreground">
          {#if pending.conflict.fixed}
            {comboTitle(pending.combo)} is also a built-in key for "{pending.conflict.label}" ({pending.conflict.group}). Where that applies it may win.
          {:else}
            {comboTitle(pending.combo)} is already "{pending.conflict.label}" ({pending.conflict.group}). Replacing leaves that one unbound.
          {/if}
        </p>
        <div class="flex shrink-0 items-center gap-1.5">
          <Button variant="ghost" size="sm" onclick={() => (pending = null)}>Cancel</Button>
          <Button size="sm" onclick={confirmPending}>{pending.conflict.fixed ? "Assign anyway" : "Replace"}</Button>
        </div>
      </div>
    {/if}
  </li>
{/snippet}
