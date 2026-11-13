<script>
  import { onMount } from "svelte";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { cn } from "$lib/utils.js";
  import Bug from "@lucide/svelte/icons/bug";
  import Zap from "@lucide/svelte/icons/zap";
  import MessageSquare from "@lucide/svelte/icons/message-square";
  import Copy from "@lucide/svelte/icons/copy";
  import Check from "@lucide/svelte/icons/check";
  import X from "@lucide/svelte/icons/x";
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

  const lbl = "mb-1.5 block text-ui-2xs font-medium text-muted-foreground";
  const tinp =
    "field-surface w-full bg-transparent px-2.5 py-2 text-ui-sm text-foreground placeholder:text-muted-foreground placeholder:font-normal outline-none resize-none transition-colors";
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    showCloseButton={false}
    class="@container/ri flex w-[min(34rem,calc(100vw-2rem))] flex-col gap-0 overflow-hidden rounded-2xl border border-border/60 bg-background p-0 elevate-3-rim"
  >
    <!-- Header -->
    <div
      class="flex items-center justify-between border-b border-border/15 px-5 py-3.5"
    >
      <div class="flex items-center gap-2">
        <p class="text-ui-sm font-semibold text-foreground">Report an issue</p>
        <span class="font-mono text-ui-3xs text-muted-foreground"
          >v{appVersion}</span
        >
      </div>
      <Dialog.Close
        class="inline-flex size-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground focus-visible:outline-none"
      >
        <X class="size-3.5 shrink-0" />
      </Dialog.Close>
    </div>

    <div class="flex flex-col gap-3.5 px-5 py-4">
      <!-- Type -->
      <div>
        <p class={lbl}>Type</p>
        <div class="grid grid-cols-1 gap-1.5 @sm/ri:grid-cols-3">
          {#each TYPES as t (t.id)}
            <button
              type="button"
              onclick={() => (issueType = t.id)}
              class={cn(
                "field-surface inline-flex h-8 min-w-0 items-center justify-center gap-1.5 px-2 text-ui-2xs font-medium transition-colors",
                issueType === t.id
                  ? "bg-muted/60 text-foreground"
                  : "text-muted-foreground hover:bg-muted/30 hover:text-foreground",
              )}
            >
              <t.Icon class="size-3 shrink-0" />
              <span class="truncate">{t.label}</span>
            </button>
          {/each}
        </div>
      </div>

      <!-- Title -->
      <div>
        <label for="ri-title" class={lbl}>Title</label>
        <input
          id="ri-title"
          bind:value={title}
          placeholder={issueType === "feature"
            ? "Add support for..."
            : "Brief description of the issue"}
          class={tinp}
          style="height:30px"
        />
      </div>

      <!-- Description -->
      <div>
        <label for="ri-body" class={lbl}>
          {issueType === "feature" ? "Details" : "What happened?"}
        </label>
        <textarea
          id="ri-body"
          bind:value={body}
          rows="4"
          placeholder={issueType === "feature"
            ? "Describe the feature and why it would be useful..."
            : "Steps to reproduce, expected vs actual behavior..."}
          class={tinp}
        ></textarea>
      </div>

      <!-- System info -->
      <div class="flex flex-col gap-0.5 rounded-md bg-muted/20 px-2.5 py-2">
        <span class="text-ui-2xs text-muted-foreground">Auto-included</span>
        <span class="font-mono text-ui-2xs text-foreground"
          >v{appVersion} · {platform || "unknown"}{details
            ? " · error details"
            : ""}</span
        >
      </div>
    </div>

    <!-- Footer -->
    <div
      class="flex flex-wrap items-center justify-between gap-2 border-t border-border/15 px-5 py-3"
    >
      <button
        type="button"
        onclick={copyReport}
        class="inline-flex h-8 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-[var(--radius-field)] px-3 text-ui-2xs text-muted-foreground transition-colors hover:bg-muted/30 hover:text-foreground"
      >
        {#if copied}
          <Check class="size-3 shrink-0 text-success" />
          <span class="text-success">Copied</span>
        {:else}
          <Copy class="size-3 shrink-0" />
          Copy
        {/if}
      </button>

      <div class="flex shrink-0 items-center gap-1.5">
        <button
          type="button"
          onclick={async () => {
            try {
              const { openUrl } = await import("@tauri-apps/plugin-opener");
              await openUrl(`https://github.com/${GITHUB_REPO}/issues`);
            } catch {
              window.open(`https://github.com/${GITHUB_REPO}/issues`, "_blank");
            }
          }}
          class= "field-surface inline-flex h-8 shrink-0 items-center gap-1.5 whitespace-nowrap px-3 text-ui-2xs text-muted-foreground transition-colors hover:bg-muted/30 hover:text-foreground"
        >
          <svg
            class="size-3 shrink-0"
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden="true"
            ><path
              d="M12 2C6.477 2 2 6.477 2 12c0 4.418 2.865 8.166 6.839 9.489.5.092.682-.217.682-.482 0-.237-.009-.868-.014-1.703-2.782.604-3.369-1.34-3.369-1.34-.454-1.156-1.11-1.463-1.11-1.463-.908-.62.069-.608.069-.608 1.003.07 1.531 1.03 1.531 1.03.892 1.529 2.341 1.087 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.11-4.555-4.943 0-1.091.39-1.984 1.029-2.683-.103-.253-.446-1.27.098-2.647 0 0 .84-.269 2.75 1.025A9.578 9.578 0 0 1 12 6.836a9.59 9.59 0 0 1 2.504.337c1.909-1.294 2.747-1.025 2.747-1.025.546 1.377.202 2.394.1 2.647.64.699 1.028 1.592 1.028 2.683 0 3.842-2.339 4.687-4.566 4.935.359.309.678.919.678 1.852 0 1.336-.012 2.415-.012 2.743 0 .267.18.578.688.48C19.138 20.163 22 16.418 22 12c0-5.523-4.477-10-10-10z"
            /></svg
          >
          issues
        </button>
        <button
          type="button"
          onclick={openOnGithub}
          class="inline-flex h-8 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md bg-foreground px-3.5 text-ui-xs font-medium text-background transition-colors hover:bg-foreground/85"
        >
          <ExternalLink class="size-3 shrink-0" />
          Open on GitHub
        </button>
      </div>
    </div>
  </Dialog.Content>
</Dialog.Root>
