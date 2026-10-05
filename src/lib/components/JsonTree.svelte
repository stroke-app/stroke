<script>
  // Lightweight collapsible JSON tree - Monaco-style folding without Monaco's
  // weight, so it can live inline in expanded table rows. Perf model: only
  // expanded children mount, arrays/objects render at most CHILD_PAGE children
  // until "Show more" is clicked, and string leaves are display-truncated.
  // The full (untruncated) value is always available via copy / open actions.
  import { untrack } from "svelte";
  import JsonTree from "./JsonTree.svelte";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Copy from "@lucide/svelte/icons/copy";
  import Maximize2 from "@lucide/svelte/icons/maximize-2";
  import { oversizeCellInfo, formatByteSize } from "$lib/cell-value.js";
  import { splitHighlight } from "$lib/json-search.js";

  /**
   * `query` / `matchPaths` / `openPaths` come from one `searchJson` walk done by
   * the owner, not from each node asking about its own subtree - that question
   * is O(n) per node and O(n^2) over the document. A node only has to look
   * itself up in two sets.
   *
   * @type {{
   *   value: unknown,
   *   label?: string | null,
   *   depth?: number,
   *   defaultDepth?: number,
   *   oncopy?: (value: unknown) => void,
   *   onopen?: ((value: unknown, label: string) => void) | null,
   *   path?: string,
   *   query?: string,
   *   matchPaths?: Set<string> | null,
   *   openPaths?: Set<string> | null,
   *   insideMatch?: boolean,
   * }}
   */
  let {
    value,
    label = null,
    depth = 0,
    defaultDepth = 1,
    oncopy = () => {},
    onopen = null,
    path = "",
    query = "",
    matchPaths = null,
    openPaths = null,
    insideMatch = false,
  } = $props();

  const STRING_DISPLAY_LIMIT = 160;
  const CHILD_PAGE = 200;

  const searching = $derived(!!query && !!matchPaths);
  const selfMatch = $derived(searching && !!matchPaths?.has(path));
  /** Everything under a matched node is shown whole - the match is its parent. */
  const childrenInsideMatch = $derived(insideMatch || selfMatch);
  /**
   * While searching, a branch that leads to a match opens itself. Kept separate
   * from `expanded` rather than written into it, so collapsing a branch by hand
   * is not undone on the next keystroke, and clearing the query restores
   * exactly the shape you had before typing.
   */
  const forceOpen = $derived(
    searching && (childrenInsideMatch || !!openPaths?.has(path)),
  );

  const oversize = $derived(oversizeCellInfo(value));
  const isArray = $derived(Array.isArray(value));
  const isObject = $derived(
    !oversize && value !== null && typeof value === "object" && !isArray,
  );
  const isContainer = $derived(isArray || isObject);

  // Initial expansion depends on the node's fixed depth props - capture only
  // the initial value (untrack) since a node's depth never changes at runtime.
  let expanded = $state(untrack(() => depth < defaultDepth));
  let childLimit = $state(CHILD_PAGE);

  // Total child count - cheap for arrays (.length). Avoids materializing a tuple
  // for every element of a huge array/object just to show a count.
  // No JSDoc casts around a prop inside the expression: Svelte 5.55.10+ prints
  // `Object.keys(/** @type */ (value)).length` as invalid JS.
  const totalCount = $derived(
    isArray
      ? /** @type {unknown[]} */ (value)?.length ?? 0
      : isObject
        ? Object.keys(value ?? {}).length
        : 0,
  );

  // Only the currently-visible children (up to childLimit) are materialized, so
  // expanding a 100k-element array never builds 100k tuples at once.
  /**
   * While a search is running, a child is drawn only when it matches, sits on
   * the way to a match, or lives inside one that did. Everything else stays
   * out of the DOM, which is the whole point - a tree is the one shape you
   * cannot scan with your eyes.
   * @param {string} childPath
   */
  function childVisible(childPath) {
    if (!searching || childrenInsideMatch) return true;
    return !!matchPaths?.has(childPath) || !!openPaths?.has(childPath);
  }

  /** @type {[string, unknown][]} */
  const rawEntries = $derived.by(() => {
    if (isArray) {
      const arr = /** @type {unknown[]} */ (value);
      const n = Math.min(arr.length, childLimit);
      /** @type {[string, unknown][]} */
      const out = [];
      for (let i = 0; i < n; i++) out.push([String(i), arr[i]]);
      return out;
    }
    if (isObject) {
      /** @type {[string, unknown][]} */
      const out = [];
      let i = 0;
      for (const k in /** @type {Record<string, unknown>} */ (value)) {
        if (i >= childLimit) break;
        out.push([k, /** @type {Record<string, unknown>} */ (value)[k]]);
        i++;
      }
      return out;
    }
    return [];
  });

  const visibleEntries = $derived(
    searching ? rawEntries.filter(([k]) => childVisible(`${path}/${k}`)) : rawEntries,
  );

  const summary = $derived.by(() => {
    if (isArray) return totalCount === 1 ? "1 item" : `${totalCount} items`;
    if (isObject) return totalCount === 1 ? "1 key" : `${totalCount} keys`;
    return "";
  });

  /** Display text for a primitive leaf. */
  const leafText = $derived.by(() => {
    if (oversize)
      return `${oversize.dataType || "value"} · ${formatByteSize(oversize.bytes)}, truncated`;
    if (value === null || value === undefined) return "null";
    if (typeof value === "string") {
      const s =
        value.length > STRING_DISPLAY_LIMIT
          ? value.slice(0, STRING_DISPLAY_LIMIT) + "…"
          : value;
      return JSON.stringify(s);
    }
    return String(value);
  });

  const leafClass = $derived.by(() => {
    if (oversize) return "json-tok-null italic";
    if (value === null || value === undefined) return "json-tok-null";
    if (typeof value === "string") return "json-tok-str";
    if (typeof value === "number") return "json-tok-num";
    if (typeof value === "boolean") return "json-tok-bool";
    return "text-foreground";
  });

  const openable = $derived(
    !!onopen &&
      (isContainer ||
        oversize ||
        (typeof value === "string" && value.length > STRING_DISPLAY_LIMIT)),
  );

  function handleOpen() {
    onopen?.(value, label ?? (isArray ? "array" : "value"));
  }
</script>

<div class="min-w-0 font-mono text-ui-xs leading-normal">
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div class="group/jsonrow flex min-w-0 items-start gap-1">
    {#if isContainer}
      <button
        type="button"
        class="mt-[3px] flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
        aria-label={expanded || forceOpen ? "Collapse" : "Expand"}
        aria-expanded={expanded || forceOpen}
        onclick={() => (expanded = !(expanded || forceOpen))}
      >
        <ChevronRight
          class={[
            "size-3.5 transition-transform duration-100",
            expanded || forceOpen ? "rotate-90" : "",
          ].join(" ")}
        />
      </button>
    {:else}
      <span class="size-4 shrink-0"></span>
    {/if}

    <div class="min-w-0 flex-1">
      <span class="inline-flex max-w-full items-baseline gap-1">
        {#if label !== null}
          <button
            type="button"
            class={[
              "json-tok-key shrink-0 cursor-pointer select-text bg-transparent p-0 text-left",
              isContainer
                ? "hover:underline underline-offset-2"
                : "cursor-text",
            ].join(" ")}
            onclick={() => {
              if (isContainer) expanded = !expanded;
            }}
            tabindex={isContainer ? 0 : -1}
            >{#if searching}{#each splitHighlight(label ?? "", query) as run, i (i)}{#if run.hit}<mark
                    class="rounded-[2px] bg-warning/35 px-0 text-foreground">{run.t}</mark
                  >{:else}{run.t}{/if}{/each}{:else}{label}{/if}</button
          ><span class="-ms-1 text-muted-foreground">:</span>
        {/if}

        {#if isContainer}
          <button
            type="button"
            class="cursor-pointer select-none bg-transparent p-0 text-left text-muted-foreground hover:text-foreground"
            onclick={() => (expanded = !expanded)}
          >
            <span class="text-muted-foreground">{isArray ? "[" : "{"}</span
            >{#if !(expanded || forceOpen)}<span
                class="px-1 text-ui-xs text-muted-foreground">{summary}</span
              ><span class="text-muted-foreground">{isArray ? "]" : "}"}</span
              >{/if}
          </button>
        {:else}
          <span class={["break-all", leafClass].join(" ")}
            >{#if searching}{#each splitHighlight(leafText, query) as run, i (i)}{#if run.hit}<mark
                    class="rounded-[2px] bg-warning/35 px-0 text-inherit">{run.t}</mark
                  >{:else}{run.t}{/if}{/each}{:else}{leafText}{/if}</span
          >
        {/if}

        <!-- Hover actions -->
        <span
          class="opacity-0 ml-1 inline-flex shrink-0 items-center gap-0.5 group-hover/jsonrow:opacity-100"
        >
          <button
            type="button"
            class="flex size-4.5 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
            title="Copy value"
            onclick={() => oncopy(value)}
          >
            <Copy class="size-3" />
          </button>
          {#if openable}
            <button
              type="button"
              class="flex size-4.5 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
              title="Open in JSON viewer"
              onclick={handleOpen}
            >
              <Maximize2 class="size-3" />
            </button>
          {/if}
        </span>
      </span>

      {#if isContainer && (expanded || forceOpen)}
        <!-- Indent guide sits just inside the opening bracket (ml-1.5), so it
             reads as this object's own tree rail - never colliding with the
             table's gutter separator the way a chevron-aligned line did. pl-3
             indents child rows one clean step past it. -->
        <div class="ml-1.5 border-l border-border/40 pl-3">
          {#each visibleEntries as [k, v] (k)}
            <JsonTree
              value={v}
              label={k}
              depth={depth + 1}
              {defaultDepth}
              {oncopy}
              {onopen}
              path={`${path}/${k}`}
              {query}
              {matchPaths}
              {openPaths}
              insideMatch={childrenInsideMatch}
            />
          {/each}

          {#if totalCount > childLimit}
            <button
              type="button"
              class="my-0.5 ml-5 rounded bg-muted/40 px-2 py-0.5 text-ui-xs text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
              onclick={() => (childLimit += CHILD_PAGE)}
            >
              Show {Math.min(CHILD_PAGE, totalCount - childLimit)} more ({totalCount -
                childLimit} hidden)
            </button>
          {/if}
        </div>
        <!-- Closing bracket aligns under the opening one (both at flex-1 left). -->
        <span class="text-muted-foreground">{isArray ? "]" : "}"}</span>
      {/if}
    </div>
  </div>
</div>
