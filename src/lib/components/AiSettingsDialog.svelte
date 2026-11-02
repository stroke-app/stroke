<script>
  import { profileAgent } from "$lib/local-agents.js";
  import { onDestroy } from "svelte";
  import { fetchOllamaRegistry, formatModelSize } from "$lib/ollama-registry.js";
  import Check from "@lucide/svelte/icons/check";
  import Plus from "@lucide/svelte/icons/plus";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Loader2 from "@lucide/svelte/icons/loader-2";
  import AlertTriangle from "@lucide/svelte/icons/alert-triangle";
  import Eye from "@lucide/svelte/icons/eye";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Copy from "@lucide/svelte/icons/copy";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Route from "@lucide/svelte/icons/route";
  import HardDrive from "@lucide/svelte/icons/hard-drive";
  import Waypoints from "@lucide/svelte/icons/waypoints";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import BrandIcon from "$lib/components/BrandIcon.svelte";
  import { hasBrand } from "$lib/brand-icons.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import { cn } from "$lib/utils.js";
  import { chatCompletionRaw, fetchModelIds } from "$lib/ai.js";
  import { omniRouteEnv, omniRouteInstall, omniRouteStart, omniRouteRunning } from "$lib/api.js";
  import {
    aiProfiles,
    activeProfileId,
    saveProfile,
    deleteProfile,
    setActiveProfile,
    PROVIDERS,
    PROVIDER_MODELS,
  } from "$lib/stores/ai-settings.js";
  import { invoke } from "@tauri-apps/api/core";
  import CopilotLogin from "$lib/components/CopilotLogin.svelte";
  import { fetchCopilotModels } from "$lib/copilot.js";

  let { open = $bindable(false) } = $props();

  /** @param {string} url */
  async function openExternal(url) {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch {
      window.open(url, "_blank", "noopener,noreferrer");
    }
  }

  /** @type {'list' | 'form'} */
  let view = $state("list");
  /** @type {string | null} */
  let editingId = $state(null);

  // Stepper: 0 = provider, 1 = model, 2 = credentials
  let step = $state(0);
  const STEPS = ["Provider", "Model", "Credentials"];

  // Selection styling shared by the provider and model grids, so the two steps read
  // as one system. An idle card is a hairline that firms up on hover; a selected one
  // is a soft tinted surface with an inset rim rather than a saturated stroke around
  // a flat fill - the stroke was the loudest thing in the dialog.
  const CARD_BASE =
    "flex w-full items-center gap-2.5 rounded-lg border text-left outline-none transition-[background-color,border-color,box-shadow] duration-150 focus-visible:ring-2 focus-visible:ring-ring/40";
  const CARD_SELECTED =
    "border-primary/35 bg-primary/[0.07] text-foreground shadow-[inset_0_1px_0_rgba(255,255,255,0.05),0_1px_2px_rgba(0,0,0,0.14)]";
  const CARD_IDLE =
    "border-border/40 text-foreground/85 hover:border-border/70 hover:bg-muted/30 hover:text-foreground";

  /** Marks for the providers brand-icons.js has no logo for. */
  const PROVIDER_ICON_FALLBACK = /** @type {Record<string, any>} */ ({
    stroke: Sparkles,
    openrouter: Route,
    ollama: HardDrive,
    omniroute: Waypoints,
    custom: SlidersHorizontal,
  });

  let formProvider = $state("openrouter");
  let formModel = $state("");
  let formName = $state("");
  let formBaseUrl = $state("");
  let formApiKey = $state("");
  let showKey = $state(false);
  let showAdvanced = $state(false);

  /** @type {'idle' | 'testing' | 'ok' | 'error'} */
  let testState = $state("idle");
  let testMsg = $state("");
  let saving = $state(false);

  const provider = $derived(PROVIDERS.find((p) => p.id === formProvider) ?? PROVIDERS[0]);
  const modelPresets = $derived(PROVIDER_MODELS[formProvider] ?? []);
  /** Show only no-cost models. OpenRouter marks them with a `:free` suffix on the
   *  id; our own presets carry a `free` tag. Off by default so paid models are
   *  not hidden from someone who is paying for them. */
  let freeOnly = $state(false);
  /** @param {{model?: string, tag?: string}} m */
  const isFreeModel = (m) => m.tag === 'free' || /:free\b/.test(m.model ?? '');
  const visiblePresets = $derived(freeOnly ? modelPresets.filter(isFreeModel) : modelPresets);
  const visibleLocalModels = $derived(freeOnly ? localModels.filter((id) => isFreeModel({ model: id })) : localModels);
  /** Any free model to filter down to? Hides the toggle where it would do nothing. */
  const hasFreeModels = $derived(
    modelPresets.some(isFreeModel) || localModels.some((id) => isFreeModel({ model: id })),
  );
  const isOllama = $derived(formProvider === "ollama");
  const isOmniroute = $derived(formProvider === "omniroute");
  /** Our own free gateway: authenticated by device id, so there is no key to enter. */
  const isStroke = $derived(formProvider === "stroke");
  const isCustom = $derived(formProvider === "custom");
  const isCopilot = $derived(formProvider === "copilot");
  const needsKey = $derived(!formApiKey && !isOllama && !isCustom && !isCopilot && !isStroke);

  /** Models discovered from a local OpenAI-compatible server (Ollama, LM Studio). */
  let localModels = $state(/** @type {string[]} */ ([]));
  let localModelsLoading = $state(false);
  let localModelsError = $state("");
  /** The server answered and has nothing installed - an empty state, not a
   *  failure. Holds the command that fixes it, or "" when not applicable. */
  let localModelsEmpty = $state("");
  let emptyCopied = $state("");
  /** What ollama.com currently offers. Fetched only when the empty state is on
   *  screen - the user is configuring Ollama at that moment - never at startup. */
  let ollamaSuggestions = $state(/** @type {{ local: any[], cloud: any[] }} */ ({ local: [], cloud: [] }));
  let suggestionsLoading = $state(false);
  let cloudOpen = $state(false);
  let suggestionsTried = $state(false);
  /** @type {ReturnType<typeof setTimeout> | null} */
  let emptyCopyTimer = null;

  const isLocalEndpoint = $derived(
    isOllama || isOmniroute || (isCustom && /localhost|127\.0\.0\.1/.test(formBaseUrl)),
  );

  // ── OmniRoute setup ────────────────────────────────────────────────────────
  // The proxy is a global npm package the user would otherwise install and run
  // in a terminal. Offer to do it here, but never silently: each step is a
  // click, and a missing prerequisite names itself.
  let omniEnv = $state(/** @type {{node: string|null, npm: string|null, omniroute: string|null} | null} */ (null));
  let omniBusy = $state(/** @type {'' | 'checking' | 'installing' | 'starting'} */ (''));
  let omniError = $state("");
  let omniServing = $state(false);
  /** Last line the backend printed. `npm i -g` can run for a minute; a spinner
   *  with no output is indistinguishable from a hang, which is what it looked
   *  like. The Rust side has been streaming these all along - nothing listened. */
  let omniLog = $state("");
  /** @type {(() => void) | null} */
  let omniUnlisten = null;

  async function watchOmniLog() {
    if (omniUnlisten) return;
    try {
      const { listen } = await import("@tauri-apps/api/event");
      omniUnlisten = await listen("omniroute-log", (/** @type {any} */ e) => {
        const line = String(e.payload?.line ?? "").trim();
        if (line) omniLog = line.slice(0, 160);
      });
    } catch { /* dev / non-Tauri: the spinner simply has no detail line */ }
  }

  /** @param {string} text */
  function copyCommand(text) {
    void navigator.clipboard.writeText(text);
    emptyCopied = text;
    if (emptyCopyTimer) clearTimeout(emptyCopyTimer);
    emptyCopyTimer = setTimeout(() => (emptyCopied = ""), 1200);
  }

  onDestroy(() => {
    omniUnlisten?.();
    omniUnlisten = null;
    if (emptyCopyTimer) clearTimeout(emptyCopyTimer);
  });
  /** Port comes from the provider's own URL (20128), never a literal. Hardcoding
   *  4000 meant the status line probed a port OmniRoute never uses, so whatever
   *  else was listening there showed a green "Running" next to a failed fetch. */
  const OMNI_PORT = $derived.by(() => {
    const m = (formBaseUrl || provider?.url || '').match(/:(\d{2,5})(?:\/|$)/);
    return m ? Number(m[1]) : 20128;
  });

  async function refreshOmni() {
    omniBusy = 'checking'; omniError = "";
    try {
      omniEnv = await omniRouteEnv();
      omniServing = await omniRouteRunning(OMNI_PORT);
    } catch (e) {
      omniError = String(/** @type {any} */ (e)?.message ?? e);
    } finally { omniBusy = ''; }
  }

  async function installOmni() {
    omniBusy = 'installing'; omniError = ""; omniLog = "resolving package…";
    void watchOmniLog();
    try {
      await omniRouteInstall();
      omniEnv = await omniRouteEnv();
    } catch (e) {
      omniError = String(/** @type {any} */ (e)?.message ?? e);
    } finally { omniBusy = ''; omniLog = ""; }
  }

  async function startOmni() {
    omniBusy = 'starting'; omniError = ""; omniLog = "starting gateway…";
    void watchOmniLog();
    try {
      const url = await omniRouteStart(OMNI_PORT);
      formBaseUrl = `${url}/v1`;
      omniServing = true;
      resetLocalModels();
      await loadLocalModels();
    } catch (e) {
      omniError = String(/** @type {any} */ (e)?.message ?? e);
    } finally { omniBusy = ''; omniLog = ""; }
  }

  // Latched, not guarded on the result. A failed fetch leaves the lists empty,
  // so a guard on emptiness re-enters the moment it settles and the spinner
  // never stops - the same loop the Copilot model fetch had, a few lines up.
  // One attempt per dialog session; Recheck is the retry.
  //
  // Runs for any Ollama setup rather than only the empty state: /v1/models
  // lists what is on disk, so a cloud model can never appear there, and without
  // this there is no path to one at all.
  $effect(() => {
    if (!isOllama || step !== 1 || suggestionsTried) return;
    suggestionsTried = true;
    suggestionsLoading = true;
    void fetchOllamaRegistry()
      .then((r) => { ollamaSuggestions = r; })
      .finally(() => { suggestionsLoading = false; });
  });

  // Probe once the user lands on the OmniRoute steps, so the panel opens with
  // the real state instead of an empty shell.
  $effect(() => {
    if (isOmniroute && omniEnv === null && omniBusy === '') void refreshOmni();
  });

  function resetLocalModels() {
    localModels = [];
    localModelsLoading = false;
    localModelsError = "";
    localModelsEmpty = "";
  }

  /**
   * Ask the local server which models it actually has. Ollama rejects anything but
   * an exact installed tag, so the list has to come from the server, not a preset.
   */
  async function loadLocalModels() {
    localModelsLoading = true;
    localModelsError = "";
    localModelsEmpty = "";
    try {
      const ids = await fetchModelIds(formBaseUrl || (provider?.url ?? ""), formApiKey);
      localModels = ids;
      if (ids.length === 0) {
        // Answered, with nothing to offer. That is not a failure and must not be
        // dressed as one - telling someone to start a server that just replied
        // is worse than saying nothing.
        // A marker, not a command: the suggestions below come from Ollama's
        // registry, and a model name written down here would be wrong within
        // months - llama3.1:8b, which used to live on this line, is no longer
        // on the registry at all.
        localModelsEmpty = isOllama ? "ollama" : isOmniroute ? "omniroute" : "server";
      } else if (!ids.includes(formModel)) {
        formModel = ids[0];
      }
    } catch (e) {
      localModels = [];
      localModelsError = String(/** @type {any} */ (e)?.message ?? e).slice(0, 200);
    } finally {
      localModelsLoading = false;
    }
  }

  /** Discover local models when entering step 1; retry is manual after that. */
  $effect(() => {
    if (isLocalEndpoint && step === 1 && localModels.length === 0 && !localModelsLoading && !localModelsError && !localModelsEmpty) {
      void loadLocalModels();
    }
  });

  /** Dynamic models fetched from Copilot API after login */
  let copilotModels = $state(/** @type {{id:string,name:string}[]} */ ([]));
  let copilotModelsLoading = $state(false);
  let copilotModelsError = $state("");

  /** Called when CopilotLogin completes with a fresh model list */
  function onCopilotConnect(models) {
    copilotModels = models;
    copilotModelsError = "";
    if (models.length > 0 && !formModel) formModel = models[0].id;
    testState = "ok";
    testMsg = "Connected to GitHub Copilot";
  }

  /** Load dynamic Copilot models when entering step 1 for the Copilot provider.
   *  The error latch matters: fetch fails instantly when not logged in, and
   *  without it the effect would refetch in a hot loop. Retry is manual. */
  $effect(() => {
    if (isCopilot && step === 1 && copilotModels.length === 0 && !copilotModelsLoading && !copilotModelsError) {
      copilotModelsLoading = true;
      fetchCopilotModels()
        .then((m) => { copilotModels = m; if (!formModel && m.length > 0) formModel = m[0].id; })
        .catch((e) => { copilotModelsError = String(/** @type {any} */ (e)?.message ?? e); /* fallback to static presets */ })
        .finally(() => { copilotModelsLoading = false; });
    }
  });

  const stepCanProceed = $derived(
    step === 0 ? !!formProvider : !!formModel.trim()
  );

  function resetForm(pid = "openrouter") {
    formProvider = pid;
    const p = PROVIDERS.find((x) => x.id === pid) ?? PROVIDERS[0];
    formBaseUrl = p.url;
    formModel = (PROVIDER_MODELS[pid] ?? [])[0]?.model ?? "";
    formName = "";
    formApiKey = "";
    showKey = false;
    showAdvanced = false;
    testState = "idle";
    testMsg = "";
    step = 0;
    resetLocalModels();
    copilotModels = [];
    copilotModelsError = "";
    freeOnly = false;
    // Drop cached probe results so each open re-checks the real state instead
    // of showing the previous session's env/serving status.
    omniEnv = null;
    omniServing = false;
    suggestionsTried = false;
    ollamaSuggestions = { local: [], cloud: [] };
    omniError = "";
  }

  function handleOpenChange(/** @type {boolean} */ next) {
    if (next) { view = "list"; editingId = null; resetForm(); }
  }

  function startAdd() { editingId = null; resetForm(); view = "form"; }

  /** @param {import('$lib/stores/ai-settings.js').ModelProfile} profile */
  async function startEdit(profile) {
    editingId = profile.id;
    formProvider = profile.provider;
    formBaseUrl = profile.baseUrl;
    formModel = profile.model;
    formName = profile.name;
    showKey = false;
    showAdvanced = profile.provider === "custom";
    testState = "idle";
    testMsg = "";
    step = 0;
    resetLocalModels();
    try { formApiKey = await invoke("ai_load_key", { profileId: profile.id }); }
    catch { formApiKey = ""; }
    view = "form";
  }

  /** @param {string} pid */
  function selectProvider(pid) {
    formProvider = pid;
    const p = PROVIDERS.find((x) => x.id === pid) ?? PROVIDERS[0];
    formBaseUrl = p.url;
    const presets = PROVIDER_MODELS[pid] ?? [];
    formModel = presets[0]?.model ?? "";
    testState = "idle";
    resetLocalModels();
    copilotModelsError = "";
  }

  function nextStep() { if (step < STEPS.length - 1) step++; }
  function prevStep() { if (step > 0) step--; else { view = "list"; editingId = null; } }

  async function save() {
    if (!formModel.trim()) return;
    saving = true;
    try {
      const name = formName.trim() || modelShortName(formModel);
      const isNew = !editingId;
      const id = await saveProfile({ id: editingId ?? crypto.randomUUID(), name, provider: formProvider, baseUrl: formBaseUrl.trim() || (provider?.url ?? ""), model: formModel.trim() }, formApiKey);
      await setActiveProfile(id);
      view = "list";
      // Adding a model is a finished errand - the new model is already active,
      // so bouncing back to the list just asks for a second dismissal.
      if (isNew) open = false;
    } catch (e) {
      // Stay on the form and surface the failure in the step-2 banner -
      // otherwise the spinner just stops and the click appears to do nothing.
      testState = "error";
      testMsg = String(/** @type {any} */ (e)?.message ?? e).slice(0, 200);
    } finally { saving = false; }
  }

  /** @param {string | null} id */
  async function handleDelete(id) {
    if (!id) return;
    await deleteProfile(id);
    if (editingId === id) view = "list";
  }

  async function testConnection() {
    if (testState === "testing") return;
    testState = "testing"; testMsg = "";
    try {
      const res = await chatCompletionRaw(
        { baseUrl: formBaseUrl || (provider?.url ?? ""), apiKey: formApiKey, model: formModel },
        [{ role: "user", content: "Reply with the single word: ok" }], null,
      );
      if (res.content == null) throw new Error("Empty response");
      testState = "ok"; testMsg = `Connected · ${formModel}`;
    } catch (e) { testState = "error"; testMsg = String(/** @type {any} */ (e)?.message ?? e).slice(0, 200); }
  }

  /** @param {string} model */
  function modelShortName(model) { return model.split("/").pop()?.split(":")[0] ?? model; }

  // Local agents are picked from the model menu and have nothing to edit here.
  const listedProfiles = $derived($aiProfiles.filter((p) => !profileAgent(p)));
</script>

<!-- Selected mark: a filled badge reads as a deliberate state, where a bare tick
     floating in the row read as decoration. -->
{#snippet freeToggle()}
  <!-- Only offered when there is something to filter to, so it never reads as a
       toggle that does nothing. -->
  {#if hasFreeModels}
    <button
      type="button"
      class={cn(
        "mb-2 flex items-center gap-1.5 self-start rounded-md border px-2 py-1 text-ui-3xs transition-colors",
        freeOnly
          ? "border-primary/35 bg-primary/[0.07] text-foreground"
          : "border-border/40 text-muted-foreground hover:border-border/70 hover:text-foreground",
      )}
      onclick={() => (freeOnly = !freeOnly)}
    >
      <Check class={cn("size-3 shrink-0", !freeOnly && "invisible")} />
      Free models only
    </button>
  {/if}
{/snippet}

{#snippet checkBadge()}
  <span class="flex size-4 shrink-0 items-center justify-center rounded-full bg-primary text-primary-foreground shadow-[0_1px_2px_rgba(0,0,0,0.25)]">
    <Check class="size-3" strokeWidth={3.5} />
  </span>
{/snippet}

<!-- One row of the model suggestion list: pick it, or copy the command
     that makes it available. Shared by the empty state and the cloud section. -->
{#snippet suggestionRow(/** @type {any} */ m)}
  {@const chosen = formModel === m.id}
  <div class={cn(
    "flex items-center gap-2 rounded-md border px-2.5 py-1.5 transition-colors",
    chosen ? "border-primary/50 bg-primary/5" : "border-border/40 bg-background/60",
  )}>
    <button
      type="button"
      class="flex min-w-0 flex-1 items-center gap-2 text-left"
      onclick={() => { formModel = m.id; testState = "idle"; }}
    >
      <code class="min-w-0 truncate font-mono text-ui-2xs text-foreground/90">{m.id}</code>
      {#if formatModelSize(m.bytes)}
        <span class="shrink-0 font-mono text-ui-3xs tabular-nums text-muted-foreground">{formatModelSize(m.bytes)}</span>
      {/if}
    </button>
    <button
      type="button"
      class="inline-flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
      aria-label="Copy {m.pull}"
      title={m.pull}
      onclick={() => copyCommand(m.pull)}
    >
      {#if emptyCopied === m.pull}<Check class="size-3 text-success" />{:else}<Copy class="size-3" />{/if}
    </button>
  </div>
{/snippet}

<Dialog.Root bind:open onOpenChange={handleOpenChange}>
  <!-- The list is a short menu of models, the form is a multi-step wizard - one
       width cannot serve both, so the list stays tight and the form gets room. -->
  <Dialog.Content
    showCloseButton={false}
    class={cn(
      "sm:max-w-none gap-0 overflow-hidden p-0 transition-[width]",
      view === "list" ? "w-[min(27.5rem,calc(100vw-2rem))]" : "w-[min(36.25rem,calc(100vw-2rem))]",
    )}
  >

    <!-- ══ LIST VIEW ═════════════════════════════════════════════════════ -->
    {#if view === "list"}
      <div class="flex items-center gap-2 border-b border-border/25 px-4 py-3">
        <div class="flex-1">
          <Dialog.Title class="text-ui-xs font-semibold text-foreground">AI Models</Dialog.Title>
          <Dialog.Description class="mt-0.5 text-ui-3xs text-muted-foreground">
            Right-click a model to edit or delete it.
          </Dialog.Description>
        </div>
        <Dialog.Close class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/50 hover:text-muted-foreground focus-visible:outline-none" />
      </div>

      <div class="app-scroll max-h-[min(60vh,26rem)] overflow-y-auto px-2 py-1.5">
        {#if listedProfiles.length === 0}
          <p class="py-6 text-center text-ui-xs text-muted-foreground">No models configured.</p>
        {:else}
          <div class="flex flex-col">
            {#each listedProfiles as profile (profile.id)}
              {@const isActive = profile.id === $activeProfileId}
              <ContextMenu.Root>
                <ContextMenu.Trigger>
                  <div
                    class={cn(
                      "group flex cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 transition-colors",
                      isActive ? "bg-muted/40" : "hover:bg-muted/25",
                    )}
                    role="button"
                    tabindex="0"
                    onclick={() => void setActiveProfile(profile.id)}
                    onkeydown={(e) => e.key === "Enter" && void setActiveProfile(profile.id)}
                  >
                    {#if hasBrand(profile.provider)}
                      <BrandIcon
                        name={profile.provider}
                        class={cn("size-3.5 shrink-0", isActive ? "text-foreground" : "text-muted-foreground")}
                      />
                    {:else}
                      {@const Fallback = PROVIDER_ICON_FALLBACK[profile.provider] ?? Sparkles}
                      <Fallback class="size-3.5 shrink-0 text-muted-foreground" />
                    {/if}
                    <!-- Name and model id on one line: the id is the detail, not a
                         second heading, so it trails in mono and truncates first. -->
                    <div class="flex min-w-0 flex-1 items-baseline gap-2">
                      <span class={cn("shrink-0 truncate text-ui-xs", isActive ? "font-medium text-foreground" : "text-foreground/80")}>{profile.name}</span>
                      <span class="min-w-0 truncate font-mono text-ui-3xs text-muted-foreground">{profile.model}</span>
                    </div>
                    <div class="flex shrink-0 items-center gap-1">
                      <button
                        class="rounded px-1.5 py-0.5 text-ui-3xs text-muted-foreground opacity-0 transition-colors hover:text-foreground group-hover:opacity-100"
                        onclick={(e) => { e.stopPropagation(); void startEdit(profile) }}
                      >Edit</button>
                      <!-- A check marks the active model; the old uppercase pill
                           was the loudest thing in a list of quiet rows. -->
                      <Check class={cn("size-3.5 shrink-0 text-primary", !isActive && "invisible")} />
                    </div>
                  </div>
                </ContextMenu.Trigger>
                <ContextMenu.Content class="min-w-40">
                  {#if !isActive}
                    <ContextMenu.Item onSelect={() => void setActiveProfile(profile.id)}>
                      <Check />
                      Set as active
                    </ContextMenu.Item>
                  {/if}
                  <ContextMenu.Item onSelect={() => void startEdit(profile)}>
                    <SlidersHorizontal />
                    Edit
                  </ContextMenu.Item>
                  <ContextMenu.Separator />
                  <ContextMenu.Item variant="destructive" onSelect={() => void handleDelete(profile.id)}>
                    <Trash2 />
                    Delete
                  </ContextMenu.Item>
                </ContextMenu.Content>
              </ContextMenu.Root>
            {/each}
          </div>
        {/if}
      </div>

      <div class="border-t border-border/25 p-1.5">
        <button type="button"
          class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-ui-xs text-muted-foreground transition-colors hover:bg-muted/30 hover:text-foreground"
          onclick={startAdd}>
          <Plus class="size-3.5 shrink-0" />
          Add model
        </button>
      </div>

    <!-- ══ FORM VIEW ═════════════════════════════════════════════════════ -->
    {:else}
      <!-- Header -->
      <div class="border-b border-border/25 px-4 py-3.5">
        <div class="flex items-center gap-2">
          <button type="button"
            class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
            onclick={prevStep}
            title={step === 0 ? "Cancel" : "Back"}
          >
            <ChevronLeft class="size-3.5" />
          </button>
          <Dialog.Title class="flex-1 text-center text-ui-sm font-semibold text-foreground">
            {editingId ? "Edit model" : "Add model"}
          </Dialog.Title>
          <Dialog.Close class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/50 hover:text-muted-foreground focus-visible:outline-none" />
        </div>

        <!-- Progress. Numbered bubbles joined by connector lines are a lot of
             chrome for three steps; a named step plus a segmented rail says the
             same thing in one line and never wraps. -->
        <div class="mt-3 flex items-center gap-2">
          <span class="text-ui-xs font-medium text-foreground">{STEPS[step]}</span>
          <span class="text-ui-2xs tabular-nums text-muted-foreground"
            >{step + 1}/{STEPS.length}</span
          >
          <div class="ml-auto flex items-center gap-1" aria-hidden="true">
            {#each STEPS as _, i}
              <span
                class={cn(
                  "h-0.5 w-5 rounded-full transition-colors",
                  i <= step ? "bg-primary" : "bg-border",
                )}
              ></span>
            {/each}
          </div>
        </div>
      </div>

      <!-- Step body -->
      <div class="app-scroll max-h-[min(60vh,34rem)] overflow-y-auto px-4 py-3">

        <!-- Step 0 · Provider ─────────────────────────────────────────── -->
        {#if step === 0}
          <!-- Every row carries a mark: providers are recognised by their logo
               long before the name is read, and a grid of bare labels was the
               least identifiable version of this list. -->
          <div class="grid grid-cols-2 gap-1.5">
            {#each PROVIDERS as p (p.id)}
              {@const selected = formProvider === p.id}
              {@const Fallback = PROVIDER_ICON_FALLBACK[p.id] ?? Sparkles}
              <button
                type="button"
                class={cn(CARD_BASE, "h-9 px-2.5", selected ? CARD_SELECTED : CARD_IDLE)}
                onclick={() => selectProvider(p.id)}
              >
                {#if hasBrand(p.id)}
                  <BrandIcon name={p.id} class={cn("size-4 shrink-0", !selected && "opacity-70")} />
                {:else}
                  <Fallback class={cn("size-4 shrink-0", !selected && "opacity-60")} />
                {/if}
                <span class="min-w-0 flex-1 truncate text-ui-sm font-medium">{p.label}</span>
                {#if selected}{@render checkBadge()}{/if}
              </button>
            {/each}
          </div>

        <!-- Step 1 · Model ────────────────────────────────────────────── -->
        {:else if step === 1}
          {#if isCopilot}
            <!-- Copilot: show dynamic models fetched from the API -->
            {#if copilotModelsLoading}
              <div class="flex items-center justify-center gap-2 py-4 text-ui-xs text-muted-foreground">
                <Loader2 class="size-3.5 animate-spin" />Fetching available models…
              </div>
            {:else}
              {@const list = copilotModels.length > 0 ? copilotModels.map(m => ({ label: m.name, model: m.id, tag: '' })) : modelPresets}
              <div class="grid grid-cols-2 gap-1.5">
                {#each list as preset (preset.model)}
                  {@const selected = formModel === preset.model}
                  <button
                    type="button"
                    class={cn(CARD_BASE, "px-3 py-2", selected ? CARD_SELECTED : CARD_IDLE)}
                    onclick={() => { formModel = preset.model; testState = "idle"; }}
                  >
                    <div class="min-w-0 flex-1">
                      <p class="text-ui-sm font-medium text-foreground">{preset.label}</p>
                      {#if preset.tag}<p class="mt-0.5 font-mono text-ui-3xs text-muted-foreground">{preset.tag}</p>{/if}
                    </div>
                    {#if selected}{@render checkBadge()}{/if}
                  </button>
                {/each}
              </div>
            {/if}
          {:else if isLocalEndpoint}
            <!-- OmniRoute setup: Node → package → server, each step named so a
                 missing prerequisite says which one it is. -->
            {#if isOmniroute}
              <div class="mb-3 flex flex-col gap-2 rounded-lg border border-border/40 bg-muted/20 px-3 py-2.5">
                <div class="flex items-center gap-2">
                  <span class={cn("size-1.5 shrink-0 rounded-full", omniServing ? "bg-success" : "bg-muted-foreground/30")}></span>
                  <p class="text-ui-xs font-medium text-foreground">
                    {omniServing ? `Running on port ${OMNI_PORT}` : "Not running"}
                  </p>
                  <button
                    type="button"
                    class="ml-auto inline-flex items-center gap-1 text-ui-3xs text-muted-foreground transition-colors hover:text-foreground"
                    onclick={() => void refreshOmni()}
                    disabled={omniBusy !== ''}
                  >
                    <RefreshCw class={cn("size-3", omniBusy === 'checking' && "animate-spin")} />
                    Recheck
                  </button>
                </div>

                <!-- min-h holds the row at one line whether or not the probe
                     has answered, so Recheck doesn't jog everything below it. -->
                <div class="flex min-h-4 flex-wrap items-center gap-x-3 gap-y-1 font-mono text-ui-3xs text-muted-foreground">
                  <span>node {omniEnv?.node ?? "—"}</span>
                  <span>npm {omniEnv?.npm ?? "—"}</span>
                  <span>omniroute {omniEnv?.omniroute ?? "—"}</span>
                </div>

                {#if omniEnv && !omniEnv.node}
                  <p class="flex items-start gap-2 text-ui-3xs text-destructive">
                    <AlertTriangle class="mt-0.5 size-3 shrink-0" />
                    <span>Node.js is not installed. OmniRoute runs on Node - install it from nodejs.org, then Recheck.</span>
                  </p>
                  <button
                    type="button"
                    class="self-start text-ui-3xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
                    onclick={() => void openExternal("https://nodejs.org")}
                  >Open nodejs.org</button>
                {:else if omniEnv && !omniEnv.omniroute}
                  <button
                    type="button"
                    class= "field-surface flex h-8 items-center justify-center gap-1.5 bg-background/60 text-ui-xs text-foreground transition-colors hover:bg-muted/40 disabled:opacity-50"
                    onclick={() => void installOmni()}
                    disabled={omniBusy !== ''}
                  >
                    {#if omniBusy === 'installing'}
                      <Loader2 class="size-3.5 animate-spin" />Installing…
                    {:else}
                      <!-- Not `-g`: Stroke installs its own copy under the app
                           data directory, so this never needs elevation. -->
                      <Plus class="size-3.5" />Install OmniRoute
                    {/if}
                  </button>
                {:else if omniEnv && !omniServing}
                  <button
                    type="button"
                    class= "field-surface flex h-8 items-center justify-center gap-1.5 bg-background/60 text-ui-xs text-foreground transition-colors hover:bg-muted/40 disabled:opacity-50"
                    onclick={() => void startOmni()}
                    disabled={omniBusy !== ''}
                  >
                    {#if omniBusy === 'starting'}
                      <Loader2 class="size-3.5 animate-spin" />Starting…
                    {:else}
                      <Waypoints class="size-3.5" />Start the gateway
                    {/if}
                  </button>
                {/if}

                <!-- `npm i -g` runs for tens of seconds. A spinner alone is
                     indistinguishable from a hang - which is exactly what it
                     looked like - so show what the process is actually saying.
                     Reserved height, so lines arriving don't push the dialog. -->
                {#if omniBusy === 'installing' || omniBusy === 'starting'}
                  <p class="min-h-4 truncate font-mono text-ui-3xs text-muted-foreground" title={omniLog}>
                    {omniLog || "working…"}
                  </p>
                {/if}

                {#if omniError}
                  <p class="flex items-start gap-2 text-ui-3xs text-destructive">
                    <AlertTriangle class="mt-0.5 size-3 shrink-0" /><span class="break-words">{omniError}</span>
                  </p>
                {/if}
              </div>
            {/if}

            <!-- Local server: list what is actually installed, never a guessed tag -->
            {#if localModelsLoading}
              <div class="flex items-center justify-center gap-2 py-4 text-ui-xs text-muted-foreground">
                <Loader2 class="size-3.5 animate-spin" />Looking for installed models…
              </div>
            {:else if localModels.length > 0}
              {@render freeToggle()}
              <div class="grid grid-cols-2 gap-1.5">
                {#each visibleLocalModels as id (id)}
                  {@const selected = formModel === id}
                  <button
                    type="button"
                    class={cn(CARD_BASE, "px-3 py-2", selected ? CARD_SELECTED : CARD_IDLE)}
                    onclick={() => { formModel = id; testState = "idle"; }}
                  >
                    <div class="min-w-0 flex-1">
                      <p class="truncate font-mono text-ui-xs text-foreground">{id}</p>
                    </div>
                    {#if selected}{@render checkBadge()}{/if}
                  </button>
                {/each}
              </div>
            {/if}

            <!-- Reachable-but-empty is a different fact from unreachable, and
                 conflating them is what produced "is the server running?" under
                 a server that had just answered. Empty is neutral and carries
                 the one command that fixes it; only unreachable is destructive. -->
            {#if localModelsEmpty}
              <div class="flex flex-col gap-2.5 rounded-lg border border-border/40 bg-muted/20 px-3 py-2.5">
                <div class="flex items-center gap-2">
                  <span class="size-1.5 shrink-0 rounded-full bg-success/70"></span>
                  <p class="min-w-0 flex-1 text-ui-xs text-foreground/80">
                    {isOllama ? "Ollama is running, with no models yet" : "Connected, with no models yet"}
                  </p>
                  <button
                    type="button"
                    class= "field-surface inline-flex h-6 shrink-0 items-center gap-1 px-2 text-ui-3xs text-muted-foreground transition-colors hover:text-foreground"
                    onclick={() => { suggestionsTried = false; void loadLocalModels(); }}
                  ><RefreshCw class={cn("size-3", (localModelsLoading || suggestionsLoading) && "animate-spin")} />Recheck</button>
                </div>
                {#if isOmniroute}
                  <p class="text-ui-3xs text-muted-foreground">
                    Connect a provider in the OmniRoute dashboard, then recheck.
                  </p>
                {:else}
                  <!-- Suggestions come from ollama.com's own registry, not a
                       literal: the lineup turns over fast enough that a name in
                       the source is wrong within months. Picking one fills the
                       Model ID below, so Continue works without retyping it. -->

                  {#if suggestionsLoading}
                    <div class="flex items-center gap-2 py-1 text-ui-3xs text-muted-foreground">
                      <Loader2 class="size-3 animate-spin" />Loading what Ollama offers…
                    </div>
                  {:else if ollamaSuggestions.local.length || ollamaSuggestions.cloud.length}
                    {#if ollamaSuggestions.local.length}
                      <p class="text-ui-3xs uppercase tracking-wider text-muted-foreground">Runs on this machine</p>
                      <div class="flex flex-col gap-1">
                        {#each ollamaSuggestions.local.slice(0, 3) as m (m.id)}{@render suggestionRow(m)}{/each}
                      </div>
                    {/if}
                    {#if ollamaSuggestions.cloud.length}
                      <p class="mt-0.5 text-ui-3xs uppercase tracking-wider text-muted-foreground">
                        Ollama Cloud · runs on their hardware
                      </p>
                      <div class="flex flex-col gap-1">
                        {#each ollamaSuggestions.cloud.slice(0, 5) as m (m.id)}{@render suggestionRow(m)}{/each}
                      </div>
                      <p class="text-ui-3xs text-muted-foreground">
                        Cloud models need <code class="font-mono">ollama signin</code> once - no download, and they are far
                        larger than anything local.
                      </p>
                    {/if}
                  {:else}
                    <!-- Registry unreachable. Rather than name a model that may
                         not exist any more, point at the list that is always right. -->
                    <p class="text-ui-3xs text-muted-foreground">
                      Couldn't reach Ollama's model list. Pick one at
                      <button
                        type="button"
                        class="underline underline-offset-2 hover:text-foreground"
                        onclick={() => void openExternal("https://ollama.com/library")}
                      >ollama.com/library</button>, pull it, then Recheck.
                    </p>
                  {/if}
                  {#if ollamaSuggestions.local.length || ollamaSuggestions.cloud.length}
                    <p class="text-ui-3xs text-muted-foreground">Copy a command, run it in a terminal, then Recheck.</p>
                  {/if}
                {/if}
              </div>
            {/if}
            <!-- Cloud models never appear in /v1/models - Ollama only lists what
                 is on disk - so without this there is no path to one from the
                 picker at all, however many local models exist. -->
            {#if isOllama && !localModelsEmpty && ollamaSuggestions.cloud.length > 0}
              <div class="mt-2 flex flex-col gap-1.5">
                <button
                  type="button"
                  class="flex items-center gap-1.5 self-start text-ui-3xs uppercase tracking-wider text-muted-foreground transition-colors hover:text-foreground"
                  onclick={() => (cloudOpen = !cloudOpen)}
                >
                  <ChevronRight class={cn("size-3 transition-transform duration-150", cloudOpen && "rotate-90")} />
                  Ollama Cloud · no download
                </button>
                {#if cloudOpen}
                  <div class="flex flex-col gap-1">
                    {#each ollamaSuggestions.cloud.slice(0, 6) as m (m.id)}{@render suggestionRow(m)}{/each}
                  </div>
                  <p class="text-ui-3xs text-muted-foreground">
                    Runs on Ollama's hardware. Needs <code class="font-mono">ollama signin</code> once; some models need a paid plan.
                  </p>
                {/if}
              </div>
            {/if}

            {#if localModelsError}
              <!-- One line, then the fix. A wall of red with the raw fetch error
                   pasted in reads as a crash; what the user needs is which server
                   is not answering and the button that starts it. -->
              <div class="flex flex-col gap-2 rounded-lg border border-destructive/25 bg-destructive/5 px-3 py-2.5">
                <div class="flex items-center gap-2">
                  <span class="size-1.5 shrink-0 rounded-full bg-destructive/70"></span>
                  <p class="min-w-0 flex-1 truncate text-ui-xs text-foreground/80" title={localModelsError}>
                    {isOllama ? "Ollama is not answering" : isOmniroute ? "The gateway is not answering" : "The server is not answering"}
                  </p>
                  {#if isOmniroute && omniEnv?.omniroute && !omniServing}
                    <button
                      type="button"
                      class="inline-flex h-6 shrink-0 items-center gap-1 rounded-md bg-primary px-2 text-ui-3xs font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-50"
                      onclick={() => void startOmni()}
                      disabled={omniBusy !== ''}
                    >
                      {#if omniBusy === 'starting'}<Loader2 class="size-3 animate-spin" />Starting{:else}<Waypoints class="size-3" />Start{/if}
                    </button>
                  {:else}
                    <button
                      type="button"
                      class= "field-surface inline-flex h-6 shrink-0 items-center gap-1 px-2 text-ui-3xs text-muted-foreground transition-colors hover:text-foreground"
                      onclick={() => void loadLocalModels()}
                    ><RefreshCw class="size-3" />Retry</button>
                  {/if}
                </div>
                <p class="text-ui-3xs text-muted-foreground">
                  {#if isOllama}
                    Start it with <code class="font-mono">ollama serve</code>, or point the URL elsewhere.
                  {:else}
                    Start it below, or point the URL at an instance you run yourself.
                  {/if}
                </p>
                <Input
                  class="h-8 font-mono text-ui-xs"
                  placeholder={provider?.url ?? ""}
                  bind:value={formBaseUrl}
                />
              </div>
            {/if}

            {#if !localModelsLoading}
              <button
                type="button"
                class="mt-2 flex items-center gap-1.5 self-start text-ui-xs text-muted-foreground hover:text-foreground"
                onclick={() => void loadLocalModels()}
              >
                <RefreshCw class="size-3 shrink-0" />
                {localModels.length > 0 ? `${localModels.length} installed · refresh` : "Retry"}
              </button>
            {/if}

          {:else if modelPresets.length > 0}
            {@render freeToggle()}
            <div class="grid grid-cols-2 gap-1.5">
              {#each visiblePresets as preset (preset.model)}
                {@const selected = formModel === preset.model}
                <button
                  type="button"
                  class={cn(CARD_BASE, "px-3 py-2", selected ? CARD_SELECTED : CARD_IDLE)}
                  onclick={() => { formModel = preset.model; testState = "idle"; }}
                >
                  <div class="min-w-0 flex-1">
                    <p class="text-ui-sm font-medium text-foreground">{preset.label}</p>
                    {#if preset.tag}<p class="mt-0.5 font-mono text-ui-3xs text-muted-foreground">{preset.tag}</p>{/if}
                  </div>
                  {#if selected}{@render checkBadge()}{/if}
                </button>
              {/each}
            </div>
          {/if}

          <div class="mt-3 flex flex-col gap-1.5">
            <label for="form-model" class="text-ui-xs font-medium text-foreground">
              Model ID
              {#if modelPresets.length > 0}
                <span class="font-normal text-muted-foreground">(editable)</span>
              {/if}
            </label>
            <Input
              id="form-model"
              class="h-8 font-mono text-ui-xs"
              placeholder={isOmniroute
                ? "auto"
                : isLocalEndpoint
                  ? (ollamaSuggestions.local[0]?.id ?? ollamaSuggestions.cloud[0]?.id ?? "model:tag")
                  : "provider/model-name"}
              bind:value={formModel}
            />
          </div>

        <!-- Step 2 · Credentials ──────────────────────────────────────── -->
        {:else}
          <div class="flex flex-col gap-3">
            <div class="flex flex-col gap-1.5">
              <label for="form-name" class="text-ui-xs font-medium text-foreground">Display name</label>
              <Input id="form-name" class="h-8 text-ui-sm" placeholder={formModel ? modelShortName(formModel) : "My model"} bind:value={formName} />
            </div>

            {#if isCopilot}
              <!-- GitHub Copilot: OAuth device flow instead of API key -->
              <CopilotLogin onconnect={onCopilotConnect} ondisconnect={() => { testState = "idle"; testMsg = "" }} />
            {:else if isStroke}
              <p class="rounded-lg border border-border/40 bg-muted/20 px-3 py-2.5 text-ui-xs text-muted-foreground">
                No key needed - this is Stroke's own free tier, with a daily limit
                per device. Add your own provider any time for unlimited use.
              </p>
            {:else if !isOllama}
              <div class="flex flex-col gap-1.5">
                <div class="flex items-baseline gap-1.5">
                  <label for="form-key" class="text-ui-xs font-medium text-foreground">API key</label>
                  <span class="text-ui-3xs text-muted-foreground">stored securely</span>
                </div>
                <div class="relative">
                  <Input
                    id="form-key"
                    class="h-8 pr-8 font-mono text-ui-xs"
                    type={showKey ? "text" : "password"}
                    placeholder="sk-…"
                    bind:value={formApiKey}
                  />
                  <button
                    type="button"
                    class="absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                    onclick={() => (showKey = !showKey)}
                    tabindex="-1"
                  >
                    {#if showKey}<EyeOff class="size-3.5" />{:else}<Eye class="size-3.5" />{/if}
                  </button>
                </div>
                {#if needsKey}
                  <p class="flex items-center gap-1.5 text-ui-xs text-muted-foreground">
                    <AlertTriangle class="size-3 shrink-0" />
                    Required for {provider?.label}.
                    {#if provider?.keysUrl}
                      <button type="button" class="underline underline-offset-2" onclick={() => openExternal(provider.keysUrl)}>
                        Get one →
                      </button>
                    {/if}
                  </p>
                {/if}
              </div>
            {/if}

            {#if isCustom || modelPresets.length === 0}
              <div class="flex flex-col gap-1.5">
                <label for="form-endpoint" class="text-ui-xs font-medium text-foreground">Endpoint URL</label>
                <Input id="form-endpoint" class="h-8 font-mono text-ui-xs" placeholder="https://…/v1" bind:value={formBaseUrl} />
              </div>
            {:else if !isOllama}
              <button
                type="button"
                class="flex items-center gap-1 self-start text-ui-xs text-muted-foreground hover:text-foreground"
                onclick={() => (showAdvanced = !showAdvanced)}
              >
                <ChevronRight class={cn("size-3.5 transition-transform", showAdvanced && "rotate-90")} />
                Custom endpoint
              </button>
              {#if showAdvanced}
                <Input class="h-8 font-mono text-ui-xs" placeholder={provider?.url ?? ""} bind:value={formBaseUrl} />
              {/if}
            {/if}

            {#if testState === "ok"}
              <div class="flex items-center gap-2 rounded-lg border border-success/20 bg-success/[0.08] px-3 py-2.5 text-ui-xs text-success">
                <Check class="size-3.5 shrink-0" />{testMsg}
              </div>
            {:else if testState === "error"}
              <div class="flex items-start gap-2 rounded-lg border border-destructive/20 bg-destructive/[0.08] px-3 py-2.5 text-ui-xs text-destructive">
                <AlertTriangle class="mt-0.5 size-3.5 shrink-0" /><span class="break-words">{testMsg}</span>
              </div>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Footer -->
      <div class="flex items-center justify-between gap-2 border-t border-border/25 px-4 py-3">
        <div class="flex items-center gap-1.5">
          {#if step === 2}
            <button type="button"
            class= "field-surface inline-flex h-7 items-center gap-1.5 px-3 text-ui-xs text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground disabled:opacity-40"
              disabled={testState === "testing" || !formModel.trim()}
              onclick={testConnection}>
              {#if testState === "testing"}<Loader2 class="size-3 animate-spin" />Testing…{:else}Test{/if}
            </button>
          {/if}
          {#if editingId}
            <button type="button"
              class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/40 hover:text-destructive"
              onclick={() => void handleDelete(editingId)}>
              <Trash2 class="size-3.5" />
            </button>
          {/if}
        </div>

        {#if step < STEPS.length - 1}
          <button type="button"
            class="inline-flex h-7 items-center gap-1 rounded-md bg-primary px-3 text-ui-xs font-medium text-primary-foreground elevate-1 transition-[background-color,transform] duration-150 hover:bg-primary/90 active:scale-[0.97] disabled:opacity-40"
            disabled={!stepCanProceed}
            onclick={nextStep}>
            Continue <ChevronRight class="size-3" />
          </button>
        {:else}
          <button type="button"
            class="inline-flex h-7 items-center gap-1.5 rounded-md bg-primary px-3 text-ui-xs font-medium text-primary-foreground elevate-1 transition-[background-color,transform] duration-150 hover:bg-primary/90 active:scale-[0.97] disabled:opacity-40"
            disabled={saving || !formModel.trim()}
            onclick={() => void save()}>
            {#if saving}<Loader2 class="size-3 animate-spin" />{/if}
            {editingId ? "Save" : "Add model"}
          </button>
        {/if}
      </div>
    {/if}

  </Dialog.Content>
</Dialog.Root>
