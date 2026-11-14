<script>
  import { onMount } from "svelte";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { cn } from "$lib/utils.js";
  import Bug from "@lucide/svelte/icons/bug";
  import Zap from "@lucide/svelte/icons/zap";
  import MessageSquare from "@lucide/svelte/icons/message-square";
  import Copy from "@lucide/svelte/icons/copy";
  import Check from "@lucide/svelte/icons/check";
  import { Button } from "$lib/components/ui/button/index.js";
  import ExternalLink from "@lucide/svelte/icons/external-link";

  let {
    open = $bindable(false),
    /** Pre-selected report type, e.g. 'crash' when opened from the error screen. */
    initialType = /** @type {'bug'|'crash'|'feature'} */ ("bug"),
    /** Pre-filled title. */
    initialTitle = "",
    /** Pre-filled description. */
    initialBody = "",
    /** Verbatim technical detail (stack trace, …) appended as a code block. */
    details = "",
  } = $props();

  /** @type {'bug'|'crash'|'feature'} */
  let issueType = $state("bug");
  let title = $state("");
  let body = $state("");
  let copied = $state(false);

  // Seed the form once per open so a caller-supplied report (the crash screen)
  // arrives submit-ready, without clobbering edits while the dialog is open.
  let seeded = false;
  $effect(() => {
    if (!open) {
      seeded = false;
      return;
    }
    if (seeded) return;
    seeded = true;
    issueType = initialType;
    if (initialTitle) title = initialTitle;
    if (initialBody) body = initialBody;
  });

  let appVersion = $state("dev");
  let platform = $state("");

  onMount(async () => {
    try {
      const { getVersion } = await import("@tauri-apps/api/app");
      appVersion = await getVersion();
    } catch {}
    const ua = navigator?.userAgent ?? "";
    if (ua.includes("Mac")) platform = "macOS";
    else if (ua.includes("Win")) platform = "Windows";
    else if (ua.includes("Linux")) platform = "Linux";
    else platform = navigator?.platform ?? "unknown";
  });

  const TYPES = [
    { id: "bug", label: "Bug report", Icon: Bug, desc: "Something is broken" },
    {
      id: "crash",
      label: "Crash report",
      Icon: Zap,
      desc: "App crashed or froze",
    },
    {
      id: "feature",
      label: "Feature",
      Icon: MessageSquare,
      desc: "Suggest an improvement",
    },
  ];

  const GITHUB_REPO = "stroke-app/stroke";

  const systemInfo = $derived(
    `**App version:** ${appVersion}\n**Platform:** ${platform || "unknown"}\n**Reported via:** in-app`,
  );

  const detailsBlock = $derived(
    details ? `\n## Error details\n\n\`\`\`\n${details.trim()}\n\`\`\`\n` : "",
  );

  const fullBody = $derived(
    issueType === "feature"
      ? `## Feature request\n\n${body || "_Describe the feature you would like..._"}\n\n---\n${systemInfo}`
      : `## Description\n\n${body || "_Describe what happened..._"}\n${detailsBlock}\n## Steps to reproduce\n\n1. \n2. \n\n## Expected behavior\n\n\n\n---\n${systemInfo}`,
  );

  function buildGithubUrl() {
    const label = issueType === "feature" ? "enhancement" : "bug";
    const params = new URLSearchParams({
      title: title || `[${issueType}] `,
      body: fullBody,
      labels: label,
      template: "",
    });
    return `https://github.com/${GITHUB_REPO}/issues/new?${params}`;
  }

  async function openOnGithub() {
    const url = buildGithubUrl();
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch {
      window.open(url, "_blank");
    }
  }

  async function copyReport() {
    const text = `**Type:** ${issueType}\n**Title:** ${title || "(untitled)"}\n\n${fullBody}`;
    await navigator.clipboard.writeText(text);
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }

  const lbl = "text-ui-xs font-medium text-foreground/90";
  const tinp =
    "field-surface w-full bg-transparent px-2.5 py-2 text-ui-sm text-foreground placeholder:text-muted-foreground placeholder:font-normal outline-none resize-none transition-colors";
</script>

<!-- Same frame as the other dialogs: title and one line under it, the form,
     then a ruled footer with one filled action. -->
<Dialog.Root bind:open>
  <Dialog.Content class="w-[min(32rem,calc(100vw-2rem))] gap-0 p-0">
    <Dialog.Header class="gap-1 px-6 pt-6 pe-12">
      <Dialog.Title class="text-ui-lg">Report an issue</Dialog.Title>
      <Dialog.Description class="text-ui-xs">
        Opens a GitHub issue with this filled in, plus Stroke <span class="font-mono tabular-nums">v{appVersion}</span> on {platform || "this computer"}{details ? " and the error details" : ""}.
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-5 px-6 pt-5 pb-6">
      <!-- Kind: a segmented control, one choice, no icons. -->
      <div role="radiogroup" aria-label="Kind of report" class="grid grid-cols-3 gap-0.5 rounded-lg bg-muted/40 p-0.5">
        {#each TYPES as t (t.id)}
          <button
            type="button"
            role="radio"
            aria-checked={issueType === t.id}
            onclick={() => (issueType = t.id)}
            class={cn(
              "h-7 min-w-0 truncate rounded-md px-2 text-ui-xs font-medium transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring",
              issueType === t.id ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground",
            )}
          >{t.label}</button>
        {/each}
      </div>

      <div class="flex flex-col gap-1.5">
        <label for="ri-title" class={lbl}>Title</label>
        <input
          id="ri-title"
          bind:value={title}
          placeholder={issueType === "feature" ? "Add support for…" : "What went wrong, in a line"}
          class={cn(tinp, "h-9 py-0")}
        />
      </div>

      <div class="flex flex-col gap-1.5">
        <label for="ri-body" class={lbl}>{issueType === "feature" ? "Details" : "What happened"}</label>
        <textarea
          id="ri-body"
          bind:value={body}
          rows="5"
          placeholder={issueType === "feature"
            ? "What you'd do with it, and why it matters."
            : "Steps to reproduce, what you expected, what happened instead."}
          class={tinp}
        ></textarea>
      </div>
    </div>

    <div class="flex items-center gap-2 border-t border-border/40 px-6 py-4">
      <Button type="button" variant="ghost" onclick={copyReport}>
        {#if copied}<Check class="size-3.5 shrink-0 text-success" />Copied{:else}<Copy class="size-3.5 shrink-0" />Copy{/if}
      </Button>
      <span class="flex-1"></span>
      <Button
        type="button"
        variant="ghost"
        onclick={async () => {
          try {
            const { openUrl } = await import("@tauri-apps/plugin-opener");
            await openUrl(`https://github.com/${GITHUB_REPO}/issues`);
          } catch {
            window.open(`https://github.com/${GITHUB_REPO}/issues`, "_blank");
          }
        }}
      >View issues</Button>
      <Button type="button" onclick={openOnGithub}>
        <ExternalLink class="size-3.5 shrink-0" />Open on GitHub
      </Button>
    </div>
  </Dialog.Content>
</Dialog.Root>
