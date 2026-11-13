<script>
  // Extensions sidebar panel - VSCode-style switchable panel listing the
  // built-in extensions with an inline Install / Installed toggle, plus whatever
  // has been installed from a folder. For a built-in there is no install step:
  // "Install" = enable it. An installed plugin is a folder on disk, so it also
  // gets Reload (re-read the file) and Remove (delete the folder).
  import Icon from "./Icon.svelte";
  import { cn } from "$lib/utils.js";
  import { EXTENSIONS } from "$lib/plugins/registry.js";
  import { pluginState, pluginEnabledIn, setPluginEnabled } from "$lib/stores/plugins.js";
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
  // Per-extension glyphs - mirror ExtensionsPage's ICONS map so each extension
  // shows its own icon (not one shared per-kind glyph).
  import Clock from "@lucide/svelte/icons/clock";
  import DollarSign from "@lucide/svelte/icons/dollar-sign";
  import Hash from "@lucide/svelte/icons/hash";
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
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Wand2 from "@lucide/svelte/icons/wand-2";
  import Bookmark from "@lucide/svelte/icons/bookmark";
  import Replace from "@lucide/svelte/icons/replace";
  import Blocks from "@lucide/svelte/icons/blocks";

  let {
    /** @type {(ext: { id: string, name: string, description?: string, kind: string }) => void} */
    onopendetail = (ext) => {},
  } = $props();

  let filter = $state("");
  let installing = $state(false);
  /** Folder installed plugins live in, shown in the empty state. */
  let pluginsDir = $state("");

  refreshExternalPlugins()
  externalPluginsDir().then((d) => (pluginsDir = d))

  const filteredExternal = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return $externalPlugins;
    return $externalPlugins.filter(
      (p) => p.name.toLowerCase().includes(q) || p.id.includes(q) || (p.description ?? "").toLowerCase().includes(q),
    );
  });

  /** Pick a folder holding a manifest.json and copy it into the plugins directory. */
  async function install() {
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
  async function reload(p) {
    const next = await reloadExternalPlugin(p.id);
    if (next?.loadable) toast.success(`Reloaded ${next.name}`);
    else toast.error(`${p.name} did not load`, { description: next?.error || "See the plugin's error below." });
  }

  /** @param {import('$lib/plugins/external/types.js').ExternalPluginInfo} p */
  async function remove(p) {
    try {
      await uninstallExternalPlugin(p.id);
      toast.success(`Removed ${p.name}`);
    } catch (e) {
      toast.error(`Could not remove ${p.name}`, { description: String(e) });
    }
  }

  const filtered = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return EXTENSIONS;
    return EXTENSIONS.filter(
      (e) =>
        e.name.toLowerCase().includes(q) ||
        (e.description ?? "").toLowerCase().includes(q) ||
        (e.kind ?? "").toLowerCase().includes(q),
    );
  });

  /** Per-extension icon component, keyed by extension id (mirrors ExtensionsPage). */
  const EXT_ICONS = /** @type {Record<string, any>} */ ({
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
    freshness: Thermometer,
    "nullish-values": CircleSlash,
    "data-gen": Dices,
  });

  /** Human-readable kind label (mirrors ExtensionsPage KIND_LABEL). */
  const KIND_LABEL = /** @type {Record<string, string>} */ ({
    formatter: "Display formatter",
    linkify: "Linkifier",
    annotator: "Column annotator",
    generators: "Value generator",
    transforms: "Cell transform",
    workflow: "Workflow feature",
  });
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <!-- Sticky header -->
  <div class="flex h-9 shrink-0 items-center gap-1 px-2.5">
    <span class="min-w-0 flex-1 truncate text-ui-3xs font-medium uppercase tracking-[0.06em] text-muted-foreground">
      Extensions
    </span>
    <button
      type="button"
      class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40"
      title="Install a plugin from a folder"
      aria-label="Install a plugin from a folder"
      onclick={install}
      disabled={installing}
    >
      <Icon name={installing ? "loader-2" : "plus"} class={cn("size-3.5", installing && "animate-spin")} />
    </button>
    <button
      type="button"
      class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      title="Rescan the plugins folder"
      aria-label="Rescan the plugins folder"
      onclick={() => void refreshExternalPlugins()}
    >
      <Icon name="refresh-cw" class="size-3.5 shrink-0" />
    </button>
  </div>

  <!-- Filter -->
  <div class="shrink-0 px-2 pb-1.5">
    <div class="relative">
      <Icon
        name="search"
        class="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground"
      />
      <input
        type="text"
        bind:value={filter}
        placeholder="Search extensions"
        aria-label="Search extensions"
        class= "field-surface h-7 w-full min-w-0 bg-background/40 pl-8 pr-2.5 text-ui-sm text-foreground shadow-none outline-none transition-colors hover:bg-background/55 focus-visible:border-ring/55 focus-visible:ring-1 focus-visible:ring-ring"
      />
    </div>
  </div>

  <!-- List -->
  <div class="app-scroll min-h-0 w-full flex-1 overflow-y-auto overscroll-y-contain px-1.5 pb-2">

    <!-- ── Installed from a folder ──────────────────────────────────────────
         Third-party plugins. Each runs in its own Worker with the network
         globals removed, so the worst a broken one can do is stop formatting. -->
    {#if filteredExternal.length > 0}
      <p class="px-2.5 pt-1 pb-1 text-ui-3xs font-medium uppercase tracking-[0.06em] text-muted-foreground">
        Installed
      </p>
      {#each filteredExternal as p (p.id)}
        {@const on = pluginEnabledIn($pluginState, pluginKey(p.id))}
        {@const err = $externalPluginErrors[p.id] ?? ""}
        <div class="group/ext min-w-0 rounded-md transition-colors hover:bg-accent/20">
          <div class="flex min-w-0 items-center">
            <div class="flex min-w-0 flex-1 items-center gap-2 px-2.5 py-1.5">
              <Icon name="blocks" class="size-4 shrink-0 text-muted-foreground" />
              <span class="flex min-w-0 flex-1 flex-col">
                <span class="truncate text-ui-sm font-medium text-sidebar-foreground">{p.name}</span>
                <span class="truncate text-ui-2xs text-muted-foreground">
                  {p.version ? `v${p.version} · ` : ""}{KIND_LABEL[p.kind] ?? "Extension"}{p.author ? ` · ${p.author}` : ""}
                </span>
              </span>
            </div>
            <div class="flex shrink-0 items-center gap-1 pr-1.5">
              <button
                type="button"
                title="Reload from disk"
                aria-label="Reload {p.name}"
                onclick={() => void reload(p)}
                class="hit-area inline-flex size-5 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity hover:text-foreground group-hover/ext:opacity-100"
              >
                <Icon name="refresh-cw" class="size-3 shrink-0" />
              </button>
              <button
                type="button"
                title="Remove, deleting its folder"
                aria-label="Remove {p.name}"
                onclick={() => void remove(p)}
                class="hit-area inline-flex size-5 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity hover:text-destructive group-hover/ext:opacity-100"
              >
                <Icon name="trash-2" class="size-3 shrink-0" />
              </button>
              {#if !p.loadable}
                <span class="rounded-md bg-destructive/10 px-2 py-0.5 text-ui-2xs font-medium text-destructive">Broken</span>
              {:else if on}
                <button
                  type="button"
                  title="On, click to turn off"
                  onclick={() => void setExternalEnabled(p.id, false)}
                  class="flex items-center gap-1 rounded-md bg-primary/10 px-2 py-0.5 text-ui-2xs font-medium text-primary transition-colors hover:bg-primary/20"
                >
                  <Icon name="check" class="size-3 shrink-0" />
                  On
                </button>
              {:else}
                <button
                  type="button"
                  title="Turn on {p.name}"
                  onclick={() => void setExternalEnabled(p.id, true)}
                  class="rounded-md border border-border bg-background/60 px-2 py-0.5 text-ui-2xs font-medium text-foreground transition-colors hover:bg-accent"
                >
                  Turn on
                </button>
              {/if}
            </div>
          </div>
          {#if p.error || err}
            <p class="px-2.5 pb-1.5 text-ui-2xs text-destructive">{p.error || err}</p>
          {:else if p.permissions.length > 0}
            <p class="px-2.5 pb-1.5 text-ui-2xs text-muted-foreground">Permissions: {p.permissions.join(", ")}</p>
          {/if}
        </div>
      {/each}
      <div class="my-1.5 h-px bg-border/40"></div>
    {:else if !filter}
      <div class="mx-1 my-1.5 rounded-md border border-dashed border-border/60 px-2.5 py-2">
        <p class="text-ui-2xs text-muted-foreground">No plugins installed.</p>
        <p class="mt-0.5 text-ui-2xs text-muted-foreground">
          A plugin is a folder with a manifest.json and one .js file. Install one with +, or drop it in
          {pluginsDir || "the plugins folder"}.
        </p>
      </div>
    {/if}

    <p class="px-2.5 pt-1 pb-1 text-ui-3xs font-medium uppercase tracking-[0.06em] text-muted-foreground">
      Built in
    </p>
    {#if filtered.length === 0}
      <div class="flex flex-col items-center gap-2 px-4 py-10 text-center">
        <Icon name="blocks" class="size-5 shrink-0 text-muted-foreground" />
        <p class="text-ui-xs text-muted-foreground">No extensions match "{filter}"</p>
      </div>
    {:else}
      {#each filtered as ext (ext.id)}
        {@const on = pluginEnabledIn($pluginState, ext.id)}
        {@const ExtIcon = EXT_ICONS[ext.id] ?? Blocks}
        <div class="group/ext flex min-w-0 items-center rounded-md transition-colors hover:bg-accent/20">
          <button
            type="button"
            title={ext.description || ext.name}
            onclick={() => onopendetail(ext)}
            class="flex min-w-0 flex-1 items-center gap-2 px-2.5 py-1.5 text-left"
          >
            <ExtIcon class="size-4 shrink-0 text-muted-foreground" />
            <span class="flex min-w-0 flex-1 flex-col">
              <span class="truncate text-ui-sm font-medium text-sidebar-foreground">{ext.name}</span>
              <span class="truncate text-ui-2xs text-muted-foreground">{KIND_LABEL[ext.kind] ?? "Extension"}</span>
            </span>
          </button>
          <div class="flex shrink-0 items-center pr-1.5">
            {#if on}
              <button
                type="button"
                title="Installed, click to disable"
                aria-label="Uninstall {ext.name}"
                onclick={(e) => {
                  e.stopPropagation();
                  setPluginEnabled(ext.id, false);
                }}
                class="flex items-center gap-1 rounded-md bg-primary/10 px-2 py-0.5 text-ui-2xs font-medium text-primary transition-colors hover:bg-primary/20"
              >
                <Icon name="check" class="size-3 shrink-0" />
                Installed
              </button>
            {:else}
              <button
                type="button"
                title="Install {ext.name}"
                aria-label="Install {ext.name}"
                onclick={(e) => {
                  e.stopPropagation();
                  setPluginEnabled(ext.id, true);
                }}
                class="rounded-md border border-border bg-background/60 px-2 py-0.5 text-ui-2xs font-medium text-foreground opacity-0 transition-opacity hover:bg-accent hover:text-foreground group-hover/ext:opacity-100"
              >
                Install
              </button>
            {/if}
          </div>
        </div>
      {/each}
    {/if}
  </div>
</div>
