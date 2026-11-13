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
  import Pencil from "@lucide/svelte/icons/pencil";
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

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return rows;
    return rows.filter(
      (r) =>
        r.label.toLowerCase().includes(q) ||
        r.group.toLowerCase().includes(q) ||
        (r.combo && comboText(r.combo).includes(q)) ||
        (!r.combo && "unbound".includes(q)),
    );
  });

  /** In SHORTCUT_GROUPS order, remappable rows first within a group. */
  const grouped = $derived(
    SHORTCUT_GROUPS.map((g) => ({
      label: g.label,
      rows: visible.filter((r) => r.group === g.label).sort((a, b) => Number(a.fixed) - Number(b.fixed)),
    })).filter((g) => g.rows.length > 0),
  );

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

<div class="flex flex-col gap-3">
  <div class="flex items-center justify-between gap-3">
    <p class="text-ui-xs text-muted-foreground" aria-live="polite">
      Showing {visible.length} {visible.length === 1 ? "binding" : "bindings"}.
    </p>
    <Button variant="ghost" size="sm" disabled={overrideCount === 0} onclick={() => { stopRecording(); pending = null; resetAllBindings(); }}>
      <RotateCcw class="size-3.5 shrink-0" />
      Reset all
    </Button>
  </div>

  <div class="relative">
    <Search class="pointer-events-none absolute left-2.5 top-1/2 size-3.5 shrink-0 -translate-y-1/2 text-muted-foreground" />
    <input
      bind:value={query}
      type="search"
      placeholder="Search by command or key…"
      aria-label="Search keybindings"
      class="field-surface h-8 w-full bg-transparent pl-8 pr-2.5 text-ui-xs text-foreground outline-none placeholder:text-muted-foreground"
    />
  </div>

  {#each grouped as g (g.label)}
    <section aria-label={g.label}>
      <p class="mt-4 mb-1 text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground">{g.label}</p>
      <ul class="flex flex-col">
        {#each g.rows as r (r.key)}
          {@const isRecording = recordingId === r.id && !!r.id}
          {@const isPending = !!pending && pending.id === r.id}
          <li class="border-t border-border/25 first:border-t-0">
            <div class="flex min-h-9 items-center gap-3 py-1">
              <div class="flex min-w-0 flex-1 items-center gap-2">
                <span class={cn("min-w-0 truncate text-ui-sm", r.fixed ? "text-muted-foreground" : "text-foreground")}>{r.label}</span>
                {#if r.changed}
                  <span class="shrink-0 rounded-full bg-foreground/[0.08] px-1.5 text-ui-3xs text-muted-foreground">changed</span>
                {/if}
              </div>

              <div class="flex shrink-0 items-center gap-1">
                {#if isRecording}
                  <span class="text-ui-xs text-foreground" aria-live="assertive">Press a key combo · Esc cancels · ⌫ unbinds</span>
                {:else if r.combo}
                  <Kbd combo={r.combo} />
                {:else}
                  <span class="text-ui-xs text-muted-foreground">Unbound</span>
                {/if}

                {#if r.fixed}
                  <span class="w-14 shrink-0 text-right text-ui-3xs text-muted-foreground" title="Handled inside the editor or grid itself, so it can't be remapped">built-in</span>
                {:else if r.id}
                  {@const id = r.id}
                  <button
                    bind:this={editButtons[id]}
                    type="button"
                    class={cn(
                      "ml-1 inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
                      isRecording && "bg-accent text-foreground",
                    )}
                    aria-label={isRecording ? `Stop recording a shortcut for ${r.label}` : `Change the shortcut for ${r.label}`}
                    aria-pressed={isRecording}
                    title={isRecording ? "Recording - press Esc to cancel" : "Change shortcut"}
                    onclick={() => (isRecording ? stopRecording() : startRecording(id))}
                  >
                    <Pencil class="size-3.5 shrink-0" />
                  </button>
                  {#if r.changed}
                    <button
                      type="button"
                      class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                      aria-label={`Reset the shortcut for ${r.label}`}
                      title="Reset to default"
                      onclick={() => { if (pending?.id === id) pending = null; resetBinding(id); }}
                    >
                      <RotateCcw class="size-3.5 shrink-0" />
                    </button>
                  {:else}
                    <span class="size-7 shrink-0" aria-hidden="true"></span>
                  {/if}
                {/if}
              </div>
            </div>

            {#if isPending && pending}
              <div class="mb-2 flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg bg-muted/40 px-3 py-2" role="alert">
                <p class="min-w-0 flex-1 text-ui-xs leading-relaxed text-foreground">
                  {#if pending.conflict.fixed}
                    {comboTitle(pending.combo)} is also a built-in key for “{pending.conflict.label}” ({pending.conflict.group}). Where that applies it may win.
                  {:else}
                    {comboTitle(pending.combo)} is already bound to “{pending.conflict.label}” ({pending.conflict.group}). Replacing it leaves that command unbound.
                  {/if}
                </p>
                <div class="flex shrink-0 items-center gap-1.5">
                  <Button variant="ghost" size="sm" onclick={() => (pending = null)}>Cancel</Button>
                  <Button size="sm" onclick={confirmPending}>{pending.conflict.fixed ? "Assign anyway" : "Replace"}</Button>
                </div>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {:else}
    <p class="py-6 text-center text-ui-xs text-muted-foreground">No shortcut matches “{query}”.</p>
  {/each}
</div>
