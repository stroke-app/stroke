<script>
  import { tick } from "svelte";
  import Copy from "@lucide/svelte/icons/copy";
  import { toast } from "$lib/components/ui/sonner/toast.svelte.js";
  import { formatJsonValue } from "$lib/row-inspector.js";
  import { valueForJsonViewer } from "$lib/cell-expand.js";
  import { highlightJson, linkifyJsonInElement } from "$lib/json-inspector.js";
  import { cn } from "$lib/utils.js";

  let {
    value,
    /** Max height before scrolling (CSS length) */
    maxHeight = "min(40vh, 14rem)",
    class: className = "",
  } = $props();

  /** @type {HTMLDivElement | null} */
  let rootEl = $state(null);

  const jsonText = $derived(formatJsonValue(valueForJsonViewer(value)));
  const html = $derived(highlightJson(jsonText));

  $effect(() => {
    if (!html || !rootEl) return;
    const source = jsonText;
    void tick().then(() => {
      const pre = rootEl?.querySelector("pre");
      if (pre instanceof HTMLElement) linkifyJsonInElement(pre, source);
    });
  });

  async function copyJson() {
    try {
      await navigator.clipboard.writeText(jsonText);
      toast.success("Copied JSON");
    } catch {
      toast.error("Could not copy to clipboard");
    }
  }
</script>

<div
  class={cn(
    "mini-json-viewer group/mini relative isolate w-full min-w-0 max-w-[50vw] overflow-hidden rounded-md border border-border/70 bg-[var(--editor-surface)] shadow-sm",
    className,
  )}
>
  <button
    type="button"
    class="absolute top-1.5 left-1.5 z-10 inline-flex size-6 items-center justify-center rounded-md border border-transparent bg-background text-muted-foreground opacity-0 shadow-sm transition-opacity hover:border-border hover:text-foreground group-hover/mini:opacity-100 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    title="Copy JSON"
    onclick={copyJson}
  >
    <Copy class="size-3 shrink-0" />
  </button>
  <div
    bind:this={rootEl}
    data-studio-selectable="text"
    class="app-scroll max-w-full overflow-x-auto overflow-y-auto px-2.5 py-2 pr-9"
    style:max-height={maxHeight}
  >
    <div
      class="mini-json-shiki min-w-0 max-w-full [&_pre]:m-0 [&_pre]:bg-transparent! [&_pre]:p-0 [&_pre]:font-mono [&_pre]:text-ui-2xs [&_pre]:leading-relaxed [&_pre]:whitespace-pre [&_.json-inspector-url]:text-link [&_.json-inspector-url]:hover:underline [&_.json-inspector-url]:underline-offset-2"
    >
      {@html html}
    </div>
  </div>
</div>
