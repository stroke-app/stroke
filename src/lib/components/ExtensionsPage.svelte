<script>
  import { untrack } from "svelte";
  import { cn } from "$lib/utils.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { revealInFolder } from "$lib/api.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import SearchableMenu from "./SearchableMenu.svelte";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Check from "@lucide/svelte/icons/check";
  import { EXTENSIONS } from "$lib/plugins/registry.js";
  import {
    pluginState,
    pluginEnabledIn,
    setPluginEnabled,
    setPluginConfig,
  } from "$lib/stores/plugins.js";
  import { TIMEZONE_OPTIONS, PRECISION_OPTIONS, timeZoneOffsetLabel } from "$lib/plugins/extensions/better-time.js";
  import { CURRENCIES } from "$lib/plugins/extensions/money-format.js";
  import { BOOLEAN_STYLES } from "$lib/plugins/extensions/boolean-glyph.js";
  import { DEFAULT_RULES } from "$lib/plugins/extensions/linkify.js";
  import Clock from "@lucide/svelte/icons/clock";
  import Hash from "@lucide/svelte/icons/hash";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Wand2 from "@lucide/svelte/icons/wand-2";
  import Blocks from "@lucide/svelte/icons/blocks";
  import Search from "@lucide/svelte/icons/search";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import DollarSign from "@lucide/svelte/icons/dollar-sign";
  import Timer from "@lucide/svelte/icons/timer";
  import Tag from "@lucide/svelte/icons/tag";
  import ToggleLeft from "@lucide/svelte/icons/toggle-left";
  import Palette from "@lucide/svelte/icons/palette";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import Globe from "@lucide/svelte/icons/globe";
  import Flame from "@lucide/svelte/icons/flame";
  import ShieldAlert from "@lucide/svelte/icons/shield-alert";
  import Link2 from "@lucide/svelte/icons/link-2";
  import BarChart3 from "@lucide/svelte/icons/bar-chart-3";
  import Thermometer from "@lucide/svelte/icons/thermometer";
  import CircleSlash from "@lucide/svelte/icons/circle-slash";
  import Dices from "@lucide/svelte/icons/dices";
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Bookmark from "@lucide/svelte/icons/bookmark";
  import Replace from "@lucide/svelte/icons/replace";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Loader2 from "@lucide/svelte/icons/loader-2";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import Fingerprint from "@lucide/svelte/icons/fingerprint";
  import HardDrive from "@lucide/svelte/icons/hard-drive";
  import Network from "@lucide/svelte/icons/network";
  import Copy from "@lucide/svelte/icons/copy";
  import TrendingUp from "@lucide/svelte/icons/trending-up";
  import MapPin from "@lucide/svelte/icons/map-pin";
  import Link from "@lucide/svelte/icons/link";
  import { toast } from "$lib/components/ui/sonner/toast.svelte.js";
  import {
    externalPlugins,
    externalPluginErrors,
    refreshExternalPlugins,
    setExternalEnabled,
    installExternalPlugin,
    uninstallExternalPlugin,
    reloadExternalPlugin,
    externalPluginsDir,
    pluginKey,
  } from "$lib/plugins/external/host.js";

  const ICONS = {
    "better-time": Clock,
    "money-format": DollarSign,
    "number-format": Hash,
    "duration-format": Timer,
    "status-badge": Tag,
    "boolean-glyph": ToggleLeft,
    "color-swatch": Palette,
    "mask-sensitive": EyeOff,
    "smart-text": Globe,
    heatmap: Flame,
    validators: ShieldAlert,
    linkify: Link2,
    "column-annotator": BarChart3,
    "id-generators": Sparkles,
    "cell-transforms": Wand2,
    "saved-views": Bookmark,
    "find-replace": Replace,
    // Without these three the grid drew the generic block icon for Freshness
    // Heat, Empty & NULL Markers and Data Generator - three different tools
    // wearing the "unknown extension" mark.
    freshness: Thermometer,
    "nullish-values": CircleSlash,
    "data-gen": Dices,
    "fk-labels": Link,
    "id-decoder": Fingerprint,
    "token-decoder": KeyRound,
    "byte-size": HardDrive,
    "ip-address": Network,
    duplicates: Copy,
    "row-change": TrendingUp,
    "geo-pair": MapPin,
  };

  const SECTIONS = [
    { title: "Workflow", kinds: ["workflow"] },
    { title: "Formatters", kinds: ["formatter"] },
    { title: "Links & annotations", kinds: ["linkify", "annotator"] },
    { title: "Cell tools", kinds: ["generators", "transforms"] },
  ];

  const KIND_LABEL = {
    formatter: "Display formatter",
    linkify: "Linkifier",
    annotator: "Column annotator",
    generators: "Value generator",
    transforms: "Cell transform",
    workflow: "Workflow feature",
  };

  // ── Plugins installed from a folder ────────────────────────────────────
  // Third-party formatters. They have no config UI of their own yet, so their
  // cards are not drill-in: a card is the plugin, its switch, and the two things
  // you can do to a folder on disk.
  let installing = $state(false);
  let pluginsDir = $state("");

  refreshExternalPlugins();
  externalPluginsDir().then((d) => (pluginsDir = d));

  async function installPlugin() {
    installing = true;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({ directory: true, multiple: false, title: "Pick a plugin folder" });
      if (typeof picked !== "string") return;
      const installed = await installExternalPlugin(picked);
      toast.success(`Installed ${installed.name}`, { description: "Turn it on to start using it." });
    } catch (e) {
      toast.error("Could not install that folder", { description: String(e) });
    } finally {
      installing = false;
    }
  }

  /** @param {import('$lib/plugins/external/types.js').ExternalPluginInfo} p */
  async function reloadPlugin(p) {
    const next = await reloadExternalPlugin(p.id);
    if (next?.loadable) toast.success(`Reloaded ${next.name}`);
    else toast.error(`${p.name} did not load`, { description: next?.error || "Check its manifest and entry file." });
  }

  /** @param {import('$lib/plugins/external/types.js').ExternalPluginInfo} p */
  async function removePlugin(p) {
    try {
      await uninstallExternalPlugin(p.id);
      toast.success(`Removed ${p.name}`);
    } catch (e) {
      toast.error(`Could not remove ${p.name}`, { description: String(e) });
    }
  }

  /**
   * What each formatter is shown working on.
   *
   * The detail page described an extension and then asked you to go and open a
   * table to find out what it does. These are the values the extension is for,
   * run through the extension's own `format()` with the settings currently set,
   * so the preview below answers "what will my data look like" without leaving
   * the page - and moves the moment a setting is changed.
   *
   * `type` is what the formatter's `appliesTo()` is given, so each sample has to
   * carry the column type it would really arrive with.
   * @type {Record<string, { type: string, values: unknown[] }>}
   */
  const PREVIEW_SAMPLES = {
    "better-time": {
      type: "timestamptz",
      values: [
        new Date(Date.now() - 45 * 1000).toISOString().replace("T", " ").replace("Z", "+00"),
        new Date(Date.now() - 3 * 3600 * 1000).toISOString().replace("T", " ").replace("Z", "+00"),
        "2024-01-15 10:30:00.492+00",
        "1999-12-31 23:59:59+00",
      ],
    },
    "number-format": { type: "int8", values: [1234000, 987, -45600, 0.5] },
    "money-format": { type: "numeric", values: [1999, 250000, -3450, 0] },
    "duration-format": { type: "int4", values: [45, 3725, 86400, 950400] },
    "boolean-glyph": { type: "bool", values: [true, false, null] },
    "mask-sensitive": { type: "text", values: ["ada@example.com", "+1 415 555 0132", "4242 4242 4242 4242"] },
    "linkify": { type: "text", values: ["https://stroke.sh/docs", "ada@example.com", "not a link"] },
    "color-swatch": { type: "text", values: ["#3b82f6", "rgb(34 197 94)", "#f59e0b"] },
    "status-badge": { type: "text", values: ["active", "pending", "failed", "archived"] },
    "nullish-values": { type: "text", values: [null, "", "  "] },
    "smart-text": { type: "text", values: ["  padded  ", "MIXED Case Text", "a-very-long-slug-that-keeps-going-and-going"] },
    "freshness": { type: "timestamptz", values: [new Date(Date.now() - 120 * 1000).toISOString(), "2024-01-15 10:30:00+00"] },
    "heatmap": { type: "int4", values: [12, 480, 1290] },
    "validators": { type: "text", values: ["ada@example.com", "not-an-email", "550e8400-e29b-41d4-a716-446655440000"] },
    "byte-size": { type: "int8", values: [512, 4404019, 1073741824, 3298534883328] },
    "id-decoder": { type: "uuid", values: ["01890a5d-ac96-774b-bcce-b302099a8057", "01ARZ3NDEKTSV4RRFFQ69G5FAV", "507f1f77bcf86cd799439011"] },
    "token-decoder": { type: "text", values: ["eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZGEiLCJleHAiOjE3MDAwMDAwMDB9.x"] },
    "ip-address": { type: "inet", values: ["192.168.1.20", "8.8.8.8", "::1", "fd12:3456::1"] },
  };

  /**
   * Run one sample through an extension, exactly as the grid does.
   * @param {any} ext @param {unknown} value @param {string} type
   */
  function previewOf(ext, value, type) {
    if (typeof ext?.format !== "function") return null;
    try {
      if (typeof ext.appliesTo === "function" && !ext.appliesTo(type)) return null;
      return ext.format(value, type, cfg(ext.id, ext.defaultConfig ?? {})) ?? null;
    } catch {
      // A formatter that throws on a sample is a bug in the formatter, not a
      // reason to take the page down with it.
      return null;
    }
  }

  /** `NULL` and an empty string have to be distinguishable in the raw column. */
  function rawText(/** @type {unknown} */ v) {
    if (v === null || v === undefined) return "NULL";
    if (v === "") return "''";
    return String(v);
  }

  const CONFIGURABLE = new Set([
    "better-time", "number-format", "money-format", "duration-format",
    "boolean-glyph", "mask-sensitive", "heatmap", "linkify",
    "fk-labels", "id-decoder", "byte-size", "row-change", "geo-pair",
  ]);

  /**
   * Settings of the extensions that only need a choice or a switch, as data:
   * one row each, drawn by the same two controls.
   * @type {Record<string, Array<{ key: string, label: string, hint: string, def: string | boolean, options?: { value: string, label: string }[] }>>}
   */
  const SIMPLE_SETTINGS = {
    "fk-labels": [
      { key: "style", label: "Show as", hint: "Keep the id and add the label, or show only the label", def: "trail",
        options: [{ value: "trail", label: "8 · Electronics" }, { value: "replace", label: "Electronics" }] },
    ],
    "id-decoder": [
      { key: "inline", label: "Age in the cell", hint: "Show how old the id is after it, not only on hover", def: false },
      { key: "snowflake", label: "Snowflake ids", hint: "Numeric ids are only decoded with their service's epoch", def: "none",
        options: [{ value: "none", label: "Don't decode" }, { value: "twitter", label: "Twitter / X" }, { value: "discord", label: "Discord" }, { value: "instagram", label: "Instagram" }] },
    ],
    "byte-size": [
      { key: "units", label: "Units", hint: "Binary (1 KiB = 1024 bytes) or decimal (1 KB = 1000)", def: "binary",
        options: [{ value: "binary", label: "Binary · KiB, MiB" }, { value: "decimal", label: "Decimal · KB, MB" }] },
    ],
    "row-change": [
      { key: "mode", label: "Change as", hint: "Percent of the row above, or the difference itself", def: "percent",
        options: [{ value: "percent", label: "Percent · ▲ 12%" }, { value: "absolute", label: "Difference · ▲ 120" }] },
    ],
    "geo-pair": [
      { key: "provider", label: "Open in", hint: "The map a coordinate opens in", def: "openstreetmap",
        options: [{ value: "openstreetmap", label: "OpenStreetMap" }, { value: "google", label: "Google Maps" }, { value: "apple", label: "Apple Maps" }] },
    ],
  };

  // Short "how to use" steps, keyed by extension kind.
  const USAGE = {
    formatter: [
      "Toggle it on.",
      "Open any table, matching cells restyle automatically.",
      "Data is never changed; hover a cell to see the raw value.",
    ],
    generators: [
      "Open an editable table (one with a primary key).",
      "Right-click a cell → Insert generated value.",
      "Pick a generator, then Save the staged edit.",
    ],
    transforms: [
      "Right-click any cell → Transform.",
      "Choose a conversion that fits the value.",
      "The result is copied to your clipboard.",
    ],
    linkify: [
      "Add pattern → URL rules below.",
      "Open a table; matching IDs become links.",
      "Click a linked cell to open its URL.",
    ],
    annotator: [
      "Toggle it on.",
      "A distribution strip appears under each column header.",
    ],
  };

  // Per-extension usage overrides (workflow features differ too much to share).
  const USAGE_BY_ID = {
    "saved-views": [
      "Open a table and set up filters, sort, search, hidden columns or a view mode.",
      "Click the bookmark icon in the table toolbar → name it → Save.",
      "Switch views from the same menu, each table keeps its own list.",
    ],
    "find-replace": [
      "Open a table, then choose Find & replace… from the toolbar's ⋯ menu.",
      "Pick a column, a match mode (contains / exact / regex) and a replacement.",
      "Review the full before → after preview, then apply, nothing is written blind.",
    ],
  };

  let { initialExtensionId = "" } = $props();

  /** null → grid overview; an id → drilled into that extension's detail page.
   *  Seeded once from the prop (a fresh instance mounts per detail tab). */
  /** Overview filter: free text over name and description, and All / On. */
  let query = $state("");
  let show = $state(/** @type {'all' | 'on'} */ ("all"));
  /** @param {{ id: string, name: string, description?: string }} e */
  function matches(e) {
    if (show === "on" && !isOn(e.id)) return false;
    const q = query.trim().toLowerCase();
    return !q || e.name.toLowerCase().includes(q) || (e.description ?? "").toLowerCase().includes(q);
  }
  let selectedId = $state(/** @type {string | null} */ (untrack(() => initialExtensionId) || null));
  const selected = $derived(selectedId ? (EXTENSIONS.find((e) => e.id === selectedId) ?? null) : null);
  const enabledCount = $derived(EXTENSIONS.filter((e) => pluginEnabledIn($pluginState, e.id)).length);

  /** @param {string} id */
  function isOn(id) {
    return pluginEnabledIn($pluginState, id);
  }
  /** @param {string} id @param {Record<string, unknown>} defaults */
  function cfg(id, defaults) {
    return { ...defaults, ...($pluginState.config[id] ?? {}) };
  }

  const tzLabel = (id) =>
    TIMEZONE_OPTIONS.find((t) => t.value === cfg(id, { timeZone: "local" }).timeZone)?.label ??
    "Local (system)";

  const DURATION_UNITS = [
    { value: "auto", label: "Auto (by name)" },
    { value: "ms", label: "Milliseconds" },
    { value: "s", label: "Seconds" },
  ];
  const HEAT_PALETTES = [
    { value: "blue", label: "Blue" },
    { value: "heat", label: "Heat (green→red)" },
    { value: "green", label: "Green" },
  ];

  // ── Linkifier rules editor ──────────────────────────────────────────────
  function rulesOf() {
    const r = $pluginState.config["linkify"]?.rules;
    return Array.isArray(r) ? r : DEFAULT_RULES;
  }
  function setRules(rules) { setPluginConfig("linkify", { rules }); }
  function updateRule(i, patch) { setRules(rulesOf().map((r, idx) => (idx === i ? { ...r, ...patch } : r))); }
  function addRule() { setRules([...rulesOf(), { pattern: "", template: "" }]); }
  function removeRule(i) { setRules(rulesOf().filter((_, idx) => idx !== i)); }

  const selTrigger =
    "h-7 w-[12rem] justify-between gap-2 border-border/70 bg-background px-2.5 text-ui-xs font-normal shadow-none";
  const ruleInput =
"field-surface h-7 min-w-0 bg-background px-2.5 font-mono text-ui-xs text-foreground outline-none";
</script>

<!-- Compact Linear/Resend-style toggle -->
{#snippet toggle(on, onclick, label)}
  <button
    type="button"
    role="switch"
    aria-label={label}
    aria-checked={on}
    {onclick}
    class={cn(
      // before: expands the hit area beyond the 18px visual without moving neighbors
      "group/toggle relative inline-flex h-[18px] w-8 shrink-0 cursor-pointer items-center rounded-full before:absolute before:-inset-1 before:content-[''] focus-visible:ring-2 focus-visible:ring-ring/18 focus-visible:outline-none",
      "transition-[background-color] duration-150",
      // Inset rim defines the pill edge on dark surfaces; inner shadow gives the
      // trough depth so the knob reads as sitting *in* the track, not on it.
      "shadow-[inset_0_0_0_1px_rgba(255,255,255,0.07),inset_0_1px_2px_rgba(0,0,0,0.2)]",
      // The theme's own accent, not `success`. Green is this app's word for "that
      // operation worked"; twenty switches, twenty icons and a count badge all
      // wearing it made the page read as a status board and left the theme's
      // accent unused on the one page built entirely out of on/off. The knob
      // swaps to `primary-foreground`, which is the token guaranteed to contrast
      // with `primary` - that pairing is what the near-white Studio accent broke
      // when the knob was hard-coded white.
      on ? "bg-primary" : "bg-muted-foreground/25 hover:bg-muted-foreground/35",
    )}
  >
    <span
      class={cn(
        // iOS-style press feedback: the knob stretches along the travel axis
        // while staying anchored to its end of the track.
        "pointer-events-none block h-3.5 w-3.5 rounded-full",
        on ? "bg-primary-foreground" : "bg-white",
        "shadow-[0_1px_2px_rgba(0,0,0,0.28),0_0_1px_rgba(0,0,0,0.16)]",
        "transition-[translate,width] duration-[180ms] ease-[cubic-bezier(0.23,1,0.32,1)]",
        "group-active/toggle:w-4",
        on ? "translate-x-4 group-active/toggle:translate-x-3.5" : "translate-x-0.5",
      )}
    ></span>
  </button>
{/snippet}

{#snippet settingRow(label, hint, control)}
  <div class="flex items-center justify-between gap-4 py-2.5">
    <div class="flex min-w-0 flex-col">
      <span class="text-ui-sm text-foreground">{label}</span>
      {#if hint}<span class="mt-0.5 text-ui-2xs text-muted-foreground">{hint}</span>{/if}
    </div>
    {@render control()}
  </div>
{/snippet}

<!-- Group label + hairline-bordered list -->
{#snippet sectionLabel(text)}
  <h3 class="mb-2 px-0.5 text-ui-2xs font-medium uppercase tracking-[0.06em] text-muted-foreground">{text}</h3>
{/snippet}

<div class="app-scroll min-h-0 flex-1 overflow-y-auto bg-background">
  {#if !selected}
    <!-- ── Grid overview ─────────────────────────────────────────────────── -->
    <!-- 72rem, not 52. At 52 a 1,600px window spent 380px of empty gutter on
         each side to show three cards per row; the column now earns the space and
         the grid resolves to four or five. `mx-auto` + equal `px` is what keeps
         the two gutters identical at every width. -->
    <div class="mx-auto w-full max-w-[72rem] px-8 py-8">
      <!-- Header: what this page is, how many are on, and a way to find one. -->
      <div class="flex items-start gap-3">
        <span class="grid size-9 shrink-0 place-items-center rounded-lg border border-border/60 bg-muted/40 text-foreground/80">
          <Blocks class="size-4 shrink-0" />
        </span>
        <div class="min-w-0 flex-1">
          <h1 class="text-ui-xl font-semibold tracking-tight text-foreground">Extensions</h1>
          <p class="mt-0.5 text-ui-sm text-muted-foreground text-pretty">
            Formatters, links and cell tools for the data grid. Open one to configure it.
          </p>
        </div>
      </div>

      <div class="mt-6 flex flex-wrap items-center gap-2">
        <div class="relative min-w-0 flex-1 basis-56">
          <Search class="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <input
            type="search"
            bind:value={query}
            placeholder="Search {EXTENSIONS.length} extensions"
            aria-label="Search extensions"
            class="field-surface h-8 w-full bg-transparent pl-8 pr-3 text-ui-sm text-foreground placeholder:text-muted-foreground"
          />
        </div>
        <!-- All / On: a segmented control, the current option raised. -->
        <div class="flex h-8 shrink-0 items-center gap-0.5 rounded-md bg-muted/50 p-0.5" role="group" aria-label="Show">
          {#each [["all", `All`, EXTENSIONS.length], ["on", "On", enabledCount]] as [v, label, n] (v)}
            <button
              type="button"
              aria-pressed={show === v}
              class={cn(
                "inline-flex h-full items-center gap-1.5 rounded-sm px-2.5 text-ui-xs transition-colors",
                show === v ? "bg-background text-foreground shadow-sm ring-1 ring-border/50" : "text-muted-foreground hover:text-foreground",
              )}
              onclick={() => (show = /** @type {'all' | 'on'} */ (v))}
            >
              {label}<span class="tabular-nums text-muted-foreground">{n}</span>
            </button>
          {/each}
        </div>
      </div>

      <!-- ── Installed from a folder ─────────────────────────────────────────
           Plugins loaded off disk. Each runs in its own Worker with the network
           globals removed, so a broken one stops formatting and nothing else. -->
      {#if !query.trim() && show === "all"}
        <section class="mt-8" aria-labelledby="ext-installed">
          <div class="mb-2.5 flex items-center gap-2">
            <h2 id="ext-installed" class="text-ui-sm font-medium text-foreground">Installed</h2>
            {#if $externalPlugins.length > 0}
              <span class="text-ui-xs tabular-nums text-muted-foreground">{$externalPlugins.length}</span>
            {/if}
            <span class="flex-1"></span>
            <button
              type="button"
              class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              title="Rescan the plugins folder"
              aria-label="Rescan the plugins folder"
              onclick={() => void refreshExternalPlugins()}
            >
              <RefreshCw class="size-3.5 shrink-0" />
            </button>
            <Button variant="outline" size="sm" onclick={installPlugin} disabled={installing}>
              {#if installing}<Loader2 class="size-3.5 shrink-0 animate-spin" />{:else}<Plus class="size-3.5 shrink-0" />{/if}
              Install from folder
            </Button>
          </div>

          {#if $externalPlugins.length === 0}
            <div class="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-lg bg-muted/25 px-4 py-3">
              <p class="min-w-0 flex-1 text-ui-xs text-muted-foreground text-pretty">
                No plugins yet. A plugin is a folder with a <code class="font-mono text-foreground/80">manifest.json</code> and one script; see
                <code class="font-mono text-foreground/80">docs/PLUGIN_API.md</code>.
              </p>
              {#if pluginsDir}
                <button
                  type="button"
                  class="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                  title={pluginsDir}
                  onclick={() => void revealInFolder(pluginsDir).catch(() => {})}
                >
                  <FolderOpen class="size-3.5 shrink-0" /> Open plugins folder
                </button>
              {/if}
            </div>
          {:else}
        <div class="mt-2.5 grid grid-cols-[repeat(auto-fill,minmax(13.5rem,1fr))] gap-2">
              {#each $externalPlugins as p (p.id)}
                {@const on = pluginEnabledIn($pluginState, pluginKey(p.id))}
                {@const err = $externalPluginErrors[p.id] ?? ""}
                <div class="group relative flex h-full flex-col gap-2 rounded-lg border border-border/60 bg-card py-2 pl-2.5 pr-12">
                  <div class="flex min-w-0 items-center gap-2.5">
                    <Blocks class={cn("size-4 shrink-0", on && p.loadable ? "text-foreground" : "text-muted-foreground")} />
                    <span class="flex min-w-0 flex-1 flex-col">
                      <span class="truncate text-ui-xs font-medium leading-tight text-foreground" title={p.description || p.name}>{p.name}</span>
                      <span class="truncate text-ui-2xs leading-tight text-muted-foreground">
                        {p.version ? `v${p.version}` : p.id}{p.author ? ` · ${p.author}` : ""}
                      </span>
                    </span>
                  </div>
                  {#if p.error || err}
                    <p class="text-ui-2xs leading-snug text-destructive">{p.error || err}</p>
                  {/if}
                  <div class="mt-auto flex items-center gap-1">
                    <button
                      type="button"
                      class="hit-area inline-flex size-5 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground"
                      title="Reload from disk"
                      aria-label="Reload {p.name}"
                      onclick={() => void reloadPlugin(p)}
                    >
                      <RefreshCw class="size-3 shrink-0" />
                    </button>
                    <button
                      type="button"
                      class="hit-area inline-flex size-5 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-destructive"
                      title="Remove, deleting its folder"
                      aria-label="Remove {p.name}"
                      onclick={() => void removePlugin(p)}
                    >
                      <Trash2 class="size-3 shrink-0" />
                    </button>
                    {#if p.permissions.length > 0}
                      <span class="ml-auto truncate text-ui-2xs text-muted-foreground" title="Permissions: {p.permissions.join(', ')}">
                        {p.permissions.length} permission{p.permissions.length === 1 ? "" : "s"}
                      </span>
                    {/if}
                  </div>
                  <div class="absolute right-3 top-2.5 flex h-[18px] items-center">
                    {#if p.loadable}
                      {@render toggle(on, () => void setExternalEnabled(p.id, !on), `Toggle ${p.name}`)}
                    {:else}
                      <span class="rounded bg-destructive/10 px-1.5 py-0.5 text-ui-2xs font-medium text-destructive">Broken</span>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </section>
      {/if}

      {#each SECTIONS as section (section.title)}
        {@const items = EXTENSIONS.filter((e) => section.kinds.includes(e.kind) && matches(e))}
        {#if items.length}
          <section class="mt-8" aria-labelledby="ext-{section.title}">
            <div class="mb-2.5 flex items-baseline gap-2">
              <h2 id="ext-{section.title}" class="text-ui-sm font-medium text-foreground">{section.title}</h2>
              <span class="text-ui-xs tabular-nums text-muted-foreground">{items.length}</span>
            </div>
            <!-- Columns come from the width, so the last row never stretches. Each
                 card says what the extension does, not its kind: the section
                 heading already names the kind. -->
            <div class="grid grid-cols-[repeat(auto-fill,minmax(17rem,1fr))] gap-2">
              {#each items as ext (ext.id)}
                {@const Icon = ICONS[ext.id] ?? Blocks}
                {@const on = isOn(ext.id)}
                <div class="relative">
                  <button
                    type="button"
                    onclick={() => (selectedId = ext.id)}
                    title="Configure {ext.name}"
                    class="group flex h-full w-full items-start gap-3 rounded-lg bg-muted/25 p-3 pr-14 text-left transition-[background-color,scale] duration-150 ease-out hover:bg-accent/50 active:scale-[0.99] focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
                  >
                    <span
                      class={cn(
                        "grid size-8 shrink-0 place-items-center rounded-md transition-colors",
                        on ? "bg-primary/15 text-primary" : "bg-muted/60 text-muted-foreground group-hover:text-foreground",
                      )}
                    >
                      <Icon class="size-4 shrink-0" />
                    </span>
                    <span class="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span class="truncate text-ui-sm font-medium text-foreground">{ext.name}</span>
                      <span class="line-clamp-2 text-ui-xs leading-snug text-muted-foreground">{ext.description ?? KIND_LABEL[ext.kind] ?? "Extension"}</span>
                    </span>
                  </button>
                  <!-- Layered over the card so it is not a button inside a button. -->
                  <div class="pointer-events-none absolute right-3 top-3 flex h-8 items-center">
                    <span class="pointer-events-auto">
                      {@render toggle(on, () => setPluginEnabled(ext.id, !on), `Toggle ${ext.name}`)}
                    </span>
                  </div>
                </div>
              {/each}
            </div>
          </section>
        {/if}
      {/each}

      {#if !SECTIONS.some((sec) => EXTENSIONS.some((e) => sec.kinds.includes(e.kind) && matches(e)))}
        <div class="mt-10 flex flex-col items-center gap-2 text-center">
          <p class="text-ui-sm text-foreground">{show === "on" && !query.trim() ? "No extensions are on" : "No extension matches"}</p>
          <p class="text-ui-xs text-muted-foreground">
            {show === "on" && !query.trim() ? "Turn one on below to change how the grid shows data." : "Try a different word, or clear the search."}
          </p>
          <Button variant="ghost" size="sm" onclick={() => { query = ""; show = "all" }}>Show all</Button>
        </div>
      {/if}
    </div>
  {:else}
    <!-- ── Detail (drill-in) ─────────────────────────────────────────────── -->
    {#key selected.id}
      {@const Icon = ICONS[selected.id]}
      {@const on = isOn(selected.id)}
      <!-- Same outer column as the overview, with the content left-aligned
           inside it rather than centred on its own: a 42rem block centred in a
           wide window starts hundreds of pixels right of the grid you just came
           from, so going into an extension moved the whole page sideways. -->
      <div class="mx-auto w-full max-w-[72rem] px-8 py-6">
      <div class="w-full max-w-[42rem]">
        <button
          type="button"
          class="-ml-1.5 mb-4 flex items-center gap-1.5 rounded-md px-1.5 py-1 text-ui-xs text-muted-foreground transition-[background-color,color] hover:bg-muted/50 hover:text-foreground"
          onclick={() => (selectedId = null)}
        >
          <ArrowLeft class="size-3.5 shrink-0" />
          All extensions
        </button>
        <!-- No card. The page is already a panel and this is its heading, so a
             bordered, tinted box around the title drew a frame that says nothing;
             the icon tile carries the on/off state and the rule underneath does
             the separating. -->
        <div class="border-b border-border/50 pb-4">
          <div class="flex items-start gap-3.5">
            <!-- One surface, whatever the state. The tile used to turn into a
                 blue-washed, blue-bordered, blue-iconed square when the
                 extension was on - three carriers of one bit, in the accent
                 colour that everywhere else in the app means "interactive",
                 sitting next to the toggle that already says on or off. The
                 icon is an identifier, not a status light. -->
            <span class="grid size-10 shrink-0 place-items-center rounded-lg border border-border/60 bg-muted/30 text-foreground/70">
              {#if Icon}<Icon class="size-5 shrink-0" />{/if}
            </span>
            <div class="min-w-0 flex-1 pt-0.5">
              <h2 class="text-ui-lg font-semibold tracking-tight text-foreground">{selected.name}</h2>
              <p class="mt-0.5 text-ui-2xs text-muted-foreground">{KIND_LABEL[selected.kind] ?? "Extension"}</p>
            </div>
            {@render toggle(on, () => setPluginEnabled(selected.id, !on), `Toggle ${selected.name}`)}
          </div>
          <p class="mt-3 text-ui-sm leading-relaxed text-muted-foreground">{selected.description}</p>
        </div>

        <!-- How to use -->
        {#if USAGE_BY_ID[selected.id] ?? USAGE[selected.kind]}
          <div class="mt-7">
            {@render sectionLabel("How to use")}
            <ol class="space-y-2">
              {#each USAGE_BY_ID[selected.id] ?? USAGE[selected.kind] as step, i (i)}
                <li class="flex items-start gap-2.5">
                  <span class="mt-px grid size-4 shrink-0 place-items-center rounded-full border border-border/60 text-ui-3xs font-semibold text-muted-foreground">{i + 1}</span>
                  <span class="text-ui-xs leading-relaxed text-foreground/75">{step}</span>
                </li>
              {/each}
            </ol>
          </div>
        {/if}

        <!-- Before → after, as the grid would draw it -->
        {#if PREVIEW_SAMPLES[selected.id] && typeof selected.format === "function"}
          {@const sample = PREVIEW_SAMPLES[selected.id]}
          <div class="mt-7">
            {@render sectionLabel("Before & after")}
            <!-- A real two-column table at the grid's own metrics - mono, 28px
                 rows, hairline rules - because the question this answers is
                 "what will my column look like", and a prose example in a
                 different typeface cannot answer it. It re-renders from the
                 settings below, so a zone or a precision change shows up here
                 before you go and open a table. -->
            <div class="overflow-hidden rounded-lg border border-border/50">
              <table class="w-full table-fixed border-collapse font-mono text-ui-2xs">
                <thead>
                  <tr class="bg-muted/25 text-left">
                    <th class="w-1/2 border-b border-border/50 px-3 py-1.5 font-[530] text-muted-foreground">Stored value</th>
                    <th class="w-1/2 border-b border-border/50 px-3 py-1.5 font-[530] text-muted-foreground">Rendered</th>
                  </tr>
                </thead>
                <tbody>
                  {#each sample.values as v, i (i)}
                    {@const out = previewOf(selected, v, sample.type)}
                    <tr>
                      <td
                        class={cn(
                          "h-7 overflow-hidden px-3 align-middle text-ellipsis whitespace-nowrap text-muted-foreground",
                          i > 0 && "border-t border-border/25",
                          (v === null || v === undefined) && "italic",
                        )}
                        title={rawText(v)}
                      >{rawText(v)}</td>
                      <td
                        class={cn(
                          "h-7 overflow-hidden px-3 align-middle text-ellipsis whitespace-nowrap text-foreground",
                          i > 0 && "border-t border-border/25",
                        )}
                        title={out?.title ?? out?.display ?? ""}
                      >
                        {#if !out}
                          <span class="text-muted-foreground/60">—</span>
                        {:else}
                          <span class="flex min-w-0 items-center gap-1.5">
                            <!-- The three non-text shapes a formatter can return,
                                 drawn the way the canvas draws them. -->
                            {#if out.swatch}
                              <span class="size-3 shrink-0 rounded-[3px] border border-border/60" style="background:{out.swatch}"></span>
                            {:else if out.dot}
                              <span class="size-2 shrink-0 rounded-full" style="background:{out.dot}"></span>
                            {/if}
                            {#if out.badge}
                              <span
                                class="shrink-0 rounded-full px-2 py-px text-ui-3xs"
                                style="background:{out.badge.bg ?? 'transparent'};color:{out.badge.fg ?? 'inherit'}"
                              >{out.display}</span>
                            {:else if out.display}
                              <span class="min-w-0 truncate">{out.display}</span>
                            {/if}
                          </span>
                        {/if}
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
            <p class="mt-1.5 px-0.5 text-ui-3xs text-muted-foreground">
              Sample <span class="font-mono">{sample.type}</span> values · the stored value is never changed
            </p>
          </div>
        {/if}

        <!-- Available generators / conversions -->
        {#if selected.kind === "generators" || selected.kind === "transforms"}
          {@const list = selected.kind === "generators" ? selected.generators : selected.transforms}
          <div class="mt-7">
            {@render sectionLabel(selected.kind === "generators" ? "Generators" : "Conversions")}
            <ul class="overflow-hidden rounded-lg border border-border/50">
              {#each list as it, i (it.id)}
                <li class="flex items-center gap-3 px-3 py-2 {i > 0 ? 'border-t border-border/40' : ''}">
                  <span class="min-w-0 shrink-0 text-ui-sm text-foreground/85">{it.label}</span>
                  {#if it.hint}<span class="min-w-0 flex-1 truncate text-ui-2xs text-muted-foreground">{it.hint}</span>{:else}<span class="flex-1"></span>{/if}
                  <span class="shrink-0 rounded border border-border/50 px-1.5 py-0.5 text-ui-3xs text-muted-foreground">{selected.kind === "generators" ? "Insert" : "Copy"}</span>
                </li>
              {/each}
            </ul>
          </div>
        {/if}

        <!-- Settings -->
        {#if CONFIGURABLE.has(selected.id)}
          <div class="mt-7">
            {@render sectionLabel("Settings")}
            <div class="rounded-lg border border-border/50 px-3.5 [&>*]:border-border/40 [&>*+*]:border-t">
              {#if selected.id === "better-time"}
                {@const c = cfg(selected.id, { mode: "absolute", timeZone: "local" })}
                <!-- Searchable, because the list is now every zone the platform
                     knows - four hundred of them. A scroll-only dropdown is fine
                     for nine cities and useless for four hundred, which is what
                     made a zone that was there look like a zone that was not. -->
                {#snippet tzControl()}
                  <SearchableMenu
                    items={TIMEZONE_OPTIONS}
                    placeholder="Search city or country…"
                    contentClass="z-[120] w-[20rem]"
                    align="end"
                    onselect={(it) => setPluginConfig(selected.id, { timeZone: it.value })}
                  >
                    {#snippet trigger(props)}
                      <button {...props} type="button" class={cn(selTrigger, "inline-flex items-center gap-1.5 border")} aria-label="Timezone">
                        <span class="min-w-0 flex-1 truncate text-left">{tzLabel(selected.id)}</span>
                        <span class="shrink-0 font-mono text-ui-3xs text-muted-foreground">{timeZoneOffsetLabel(c.timeZone ?? "local")}</span>
                        <ChevronDown class="size-3 shrink-0 text-muted-foreground" />
                      </button>
                    {/snippet}
                    {#snippet item(it)}
                      <span class="min-w-0 flex-1 truncate">{it.label}</span>
                      {#if it.country && it.country !== it.label}
                        <span class="shrink-0 truncate text-ui-3xs text-muted-foreground">{it.country}</span>
                      {/if}
                      <span class="ml-1 shrink-0 font-mono text-ui-3xs text-muted-foreground/80">{timeZoneOffsetLabel(it.value)}</span>
                      {#if it.value === (c.timeZone ?? "local")}<Check class="size-3.5 shrink-0 text-primary" />{/if}
                    {/snippet}
                  </SearchableMenu>
                {/snippet}
                {#snippet relControl()}{@render toggle(c.mode === "relative", () => setPluginConfig(selected.id, { mode: c.mode === "relative" ? "absolute" : "relative" }), "Toggle relative time")}{/snippet}
                {#snippet precisionControl()}
                  <Select.Root type="single" value={c.precision ?? "seconds"} onValueChange={(v) => v && setPluginConfig(selected.id, { precision: v })}>
                    <Select.Trigger size="sm" class={selTrigger} aria-label="Time precision">
                      <span class="truncate">{PRECISION_OPTIONS.find((o) => o.value === (c.precision ?? "seconds"))?.label ?? "Seconds"}</span>
                    </Select.Trigger>
                    <Select.Content class="z-[120] min-w-[10rem] p-1" sideOffset={6}>
                      {#each PRECISION_OPTIONS as o (o.value)}<Select.Item value={o.value} label={o.label} class="py-1.5 pl-2 text-ui-xs">{o.label}</Select.Item>{/each}
                    </Select.Content>
                  </Select.Root>
                {/snippet}
                <!-- The row reads "24-hour clock", so ON is `hour12: false`. -->
                {#snippet hour12Control()}{@render toggle(c.hour12 !== true, () => setPluginConfig(selected.id, { hour12: c.hour12 !== true }), "Toggle 24-hour clock")}{/snippet}
                {#snippet zoneNameControl()}{@render toggle(c.showZone === true, () => setPluginConfig(selected.id, { showZone: !(c.showZone === true) }), "Toggle timezone suffix")}{/snippet}
                {@render settingRow("Timezone", `Render timestamps in this zone · ${timeZoneOffsetLabel(c.timeZone ?? "local")}`, tzControl)}
                {@render settingRow("Relative time", 'Show "3 hours ago" instead of a date', relControl)}
                {@render settingRow("Precision", "How much of the time to print - milliseconds matter when rows are ordered by it", precisionControl)}
                {@render settingRow("24-hour clock", "14:30 rather than 2:30 PM", hour12Control)}
                {@render settingRow("Show timezone", "Append the zone, so a value cannot be read in the wrong one", zoneNameControl)}
              {:else if selected.id === "number-format"}
                {@const c = cfg(selected.id, { mode: "thousands" })}
                {#snippet ctl()}{@render toggle(c.mode === "compact", () => setPluginConfig(selected.id, { mode: c.mode === "compact" ? "thousands" : "compact" }), "Toggle compact")}{/snippet}
                {@render settingRow("Compact notation", "1,234,000 → 1.2M", ctl)}
              {:else if selected.id === "money-format"}
                {@const c = cfg(selected.id, { currency: "USD", minorUnits: false })}
                {#snippet curCtl()}
                  <Select.Root type="single" value={c.currency} onValueChange={(v) => v && setPluginConfig(selected.id, { currency: v })}>
                    <Select.Trigger size="sm" class={selTrigger} aria-label="Currency"><span class="truncate">{c.currency}</span></Select.Trigger>
                    <Select.Content class="z-[120] max-h-[18rem] min-w-[8rem] p-1" sideOffset={6}>
                      {#each CURRENCIES as cur (cur)}<Select.Item value={cur} label={cur} class="py-1.5 pl-2 text-ui-xs">{cur}</Select.Item>{/each}
                    </Select.Content>
                  </Select.Root>
                {/snippet}
                {#snippet centsCtl()}{@render toggle(c.minorUnits === true, () => setPluginConfig(selected.id, { minorUnits: !c.minorUnits }), "Toggle minor units")}{/snippet}
                {@render settingRow("Currency", null, curCtl)}
                {@render settingRow("Stored as cents", "Divide by 100 before formatting", centsCtl)}
              {:else if selected.id === "duration-format"}
                {@const c = cfg(selected.id, { unit: "auto" })}
                {#snippet ctl()}
                  <Select.Root type="single" value={c.unit} onValueChange={(v) => v && setPluginConfig(selected.id, { unit: v })}>
                    <Select.Trigger size="sm" class={selTrigger} aria-label="Duration unit"><span class="truncate">{DURATION_UNITS.find((u) => u.value === c.unit)?.label ?? "Auto"}</span></Select.Trigger>
                    <Select.Content class="z-[120] min-w-[12rem] p-1" sideOffset={6}>
                      {#each DURATION_UNITS as u (u.value)}<Select.Item value={u.value} label={u.label} class="py-1.5 pl-2 text-ui-xs">{u.label}</Select.Item>{/each}
                    </Select.Content>
                  </Select.Root>
                {/snippet}
                {@render settingRow("Source unit", "How the stored number is interpreted", ctl)}
              {:else if selected.id === "boolean-glyph"}
                {@const c = cfg(selected.id, { style: "dot" })}
                {#snippet ctl()}
                  <Select.Root type="single" value={c.style} onValueChange={(v) => v && setPluginConfig(selected.id, { style: v })}>
                    <Select.Trigger size="sm" class={selTrigger} aria-label="Boolean style">
                      <span class="truncate">{BOOLEAN_STYLES.find((s) => s.value === c.style)?.label ?? "Dot + text"}</span>
                    </Select.Trigger>
                    <Select.Content class="z-[120] max-h-[18rem] min-w-[10rem] p-1" sideOffset={6}>
                      {#each BOOLEAN_STYLES as st (st.value)}<Select.Item value={st.value} label={st.label} class="py-1.5 pl-2 text-ui-xs">{st.label}</Select.Item>{/each}
                    </Select.Content>
                  </Select.Root>
                {/snippet}
                {@render settingRow("Style", "How true and false are drawn", ctl)}
              {:else if selected.id === "mask-sensitive"}
                {@const c = cfg(selected.id, { revealOnHover: true })}
                {#snippet ctl()}{@render toggle(c.revealOnHover !== false, () => setPluginConfig(selected.id, { revealOnHover: c.revealOnHover === false }), "Toggle reveal on hover")}{/snippet}
                {@render settingRow("Reveal on hover", "Show the real value while hovering the cell", ctl)}
              {:else if SIMPLE_SETTINGS[selected.id]}
                {@const defs = SIMPLE_SETTINGS[selected.id]}
                {@const c = cfg(selected.id, Object.fromEntries(defs.map((d) => [d.key, d.def])))}
                {#each defs as d (d.key)}
                  {#snippet ctl()}
                    {#if d.options}
                      <Select.Root type="single" value={String(c[d.key] ?? d.def)} onValueChange={(v) => v !== undefined && setPluginConfig(selected.id, { [d.key]: v })}>
                        <Select.Trigger size="sm" class={selTrigger} aria-label={d.label}><span class="truncate">{d.options.find((o) => o.value === String(c[d.key] ?? d.def))?.label ?? d.options[0].label}</span></Select.Trigger>
                        <Select.Content class="z-[120] min-w-[12rem] p-1" sideOffset={6}>
                          {#each d.options as o (o.value)}<Select.Item value={o.value} label={o.label} class="py-1.5 pl-2 text-ui-xs">{o.label}</Select.Item>{/each}
                        </Select.Content>
                      </Select.Root>
                    {:else}
                      {@render toggle(c[d.key] === true, () => setPluginConfig(selected.id, { [d.key]: !(c[d.key] === true) }), d.label)}
                    {/if}
                  {/snippet}
                  {@render settingRow(d.label, d.hint, ctl)}
                {/each}
              {:else if selected.id === "heatmap"}
                {@const c = cfg(selected.id, { palette: "blue" })}
                {#snippet ctl()}
                  <Select.Root type="single" value={c.palette} onValueChange={(v) => v && setPluginConfig(selected.id, { palette: v })}>
                    <Select.Trigger size="sm" class={selTrigger} aria-label="Heatmap palette"><span class="truncate">{HEAT_PALETTES.find((p) => p.value === c.palette)?.label ?? "Blue"}</span></Select.Trigger>
                    <Select.Content class="z-[120] min-w-[12rem] p-1" sideOffset={6}>
                      {#each HEAT_PALETTES as p (p.value)}<Select.Item value={p.value} label={p.label} class="py-1.5 pl-2 text-ui-xs">{p.label}</Select.Item>{/each}
                    </Select.Content>
                  </Select.Root>
                {/snippet}
                {@render settingRow("Palette", "Color scale for the value gradient", ctl)}
              {:else if selected.id === "linkify"}
                <div class="py-3">
                  <p class="text-ui-xs leading-relaxed text-muted-foreground">
                    When a cell matches a <span class="font-mono text-foreground/80">pattern</span> (regex), clicking it opens the
                    <span class="font-mono text-foreground/80">template</span>. Use <span class="font-mono text-foreground/80">{"{value}"}</span> for the cell value.
                  </p>
                  <div class="mt-3 space-y-2">
                    {#each rulesOf() as rule, i (i)}
                      <div class="grid grid-cols-[9rem_1fr_auto] items-center gap-2">
                        <input class={ruleInput} placeholder="^cus_" value={rule.pattern} oninput={(e) => updateRule(i, { pattern: e.currentTarget.value })} />
                        <input class={ruleInput} placeholder={"https://…/{value}"} value={rule.template} oninput={(e) => updateRule(i, { template: e.currentTarget.value })} />
                        <button type="button" class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground" aria-label="Remove rule" onclick={() => removeRule(i)}>
                          <X class="size-3.5 shrink-0" />
                        </button>
                      </div>
                    {/each}
                  </div>
                  <button type="button" class="mt-2.5 flex items-center gap-1.5 rounded-md px-1.5 py-1 text-ui-xs text-muted-foreground hover:text-foreground" onclick={addRule}>
                    <Plus class="size-3.5 shrink-0" /> Add rule
                  </button>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </div>
      </div>
    {/key}
  {/if}
</div>
