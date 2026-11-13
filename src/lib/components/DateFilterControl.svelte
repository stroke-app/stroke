<script>
  import { Button } from '$lib/components/ui/button/index.js'
  import FieldSelect from './FieldSelect.svelte';
  /**
   * Unified date/timestamp filter control for the filter bar. One popover with:
   *  - a relative-preset column (Today, Last 7 days, This month, "In the last N…")
   *  - a calendar (single, or a 2-month range when the op is "between")
   * Emits { op, value } so a preset can set both the operator and the value at
   * once. Day-granularity values are bare YYYY-MM-DD (the backend treats them as
   * the whole day on timestamp columns); hour presets emit a full timestamp.
   */
  import { Popover, PopoverTrigger, PopoverContent } from "$lib/components/ui/popover/index.js";
  import { Calendar, RangeCalendar } from "$lib/components/ui/calendar/index.js";
  import { CalendarDate } from "@internationalized/date";
  import CalendarIcon from "@lucide/svelte/icons/calendar";
  import Minus from "@lucide/svelte/icons/minus";
  import Plus from "@lucide/svelte/icons/plus";
  import { cn } from "$lib/utils.js";

  /** @type {{ op: string, value: string, onchange: (d: { op: string, value: string }) => void, class?: string }} */
  let { op = "gte", value = "", onchange, class: className } = $props();

  let open = $state(false);
  let lastN = $state(7);
  let lastUnit = $state("days");

  const pad = (n) => String(n).padStart(2, "0");
  const ymd = (d) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  const ymdhms = (d) => `${ymd(d)} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  function addDays(base, n) { const d = new Date(base); d.setDate(d.getDate() + n); return d; }

  const isBetween = $derived(op === "between");

  /** @param {string} s */
  function parseCD(s) {
    const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(s || "");
    if (!m) return undefined;
    try { return new CalendarDate(+m[1], +m[2], +m[3]); } catch { return undefined; }
  }
  function fmtCD(d) {
    if (!d) return "";
    return `${String(d.year).padStart(4, "0")}-${pad(d.month)}-${pad(d.day)}`;
  }

  const singleValue = $derived(parseCD(value));
  const rangeValue = $derived.by(() => {
    const [f, t] = (value || "").split(",");
    return { start: parseCD(f), end: parseCD(t) };
  });

  const OP_PREFIX = { eq: "", neq: "≠ ", gt: "after ", gte: "from ", lt: "before ", lte: "until " };
  const label = $derived.by(() => {
    if (op === "between") {
      const [f, t] = (value || "").split(",");
      if (f && t) return `${f} - ${t}`;
      if (f) return `${f} - …`;
      return "Pick range…";
    }
    if (op === "is_not_null") return "any date";
    if (!value) return "Pick date…";
    // Show just the date portion of a full timestamp preset in the trigger.
    const shown = value.length > 10 ? value.slice(0, 16) : value;
    return (OP_PREFIX[op] ?? "") + shown;
  });

  function emit(nextOp, nextValue) { onchange({ op: nextOp, value: nextValue }); }

  const PRESETS = [
    { id: "today", label: "Today" },
    { id: "yesterday", label: "Yesterday" },
    { id: "last7", label: "Last 7 days" },
    { id: "last30", label: "Last 30 days" },
    { id: "last90", label: "Last 90 days" },
    { id: "thisMonth", label: "This month" },
    { id: "ytd", label: "Year to date" },
    { id: "all", label: "All time" },
  ];

  function applyPreset(id) {
    const now = new Date();
    switch (id) {
      case "today":     emit("eq", ymd(now)); break;
      case "yesterday": emit("eq", ymd(addDays(now, -1))); break;
      case "last7":     emit("gte", ymd(addDays(now, -7))); break;
      case "last30":    emit("gte", ymd(addDays(now, -30))); break;
      case "last90":    emit("gte", ymd(addDays(now, -90))); break;
      case "thisMonth": emit("between", `${ymd(new Date(now.getFullYear(), now.getMonth(), 1))},${ymd(now)}`); break;
      case "ytd":       emit("between", `${ymd(new Date(now.getFullYear(), 0, 1))},${ymd(now)}`); break;
      case "all":       emit("is_not_null", ""); break;
    }
    open = false;
  }

  function applyLastN() {
    const now = new Date();
    const n = Math.max(1, Number(lastN) || 1);
    let from;
    if (lastUnit === "hours") { from = new Date(now.getTime() - n * 3600_000); emit("gte", ymdhms(from)); }
    else if (lastUnit === "weeks") { emit("gte", ymd(addDays(now, -n * 7))); }
    else if (lastUnit === "months") { const d = new Date(now); d.setMonth(d.getMonth() - n); emit("gte", ymd(d)); }
    else { emit("gte", ymd(addDays(now, -n))); }
    open = false;
  }

  function pickSingle(d) {
    if (!d) return;
    emit(op === "between" ? "eq" : op, fmtCD(d));
    open = false;
  }
  function pickRange(r) {
    if (!r) return;
    const f = fmtCD(r.start), t = fmtCD(r.end);
    emit("between", `${f},${t}`);
    if (f && t) open = false;
  }
</script>

<Popover bind:open>
  <PopoverTrigger
    class={cn(
"field-surface inline-flex h-7 min-w-[10rem] items-center gap-1.5 bg-input/30 px-2 text-ui-sm text-foreground transition-colors hover:bg-accent focus:outline-none",
      !value && op !== "is_not_null" && "text-muted-foreground",
      className,
    )}
  >
    <CalendarIcon class="size-3 shrink-0 text-muted-foreground" />
    <span class="truncate">{label}</span>
  </PopoverTrigger>
  <PopoverContent class="flex w-auto p-0" align="start">
    <!-- Presets -->
    <div class="flex w-40 shrink-0 flex-col gap-0.5 border-r border-border/50 p-1.5">
      {#each PRESETS as p (p.id)}
        <button
          type="button"
          class="flex items-center rounded-md px-2.5 py-1.5 text-left text-ui-sm text-foreground/80 transition-colors hover:bg-accent hover:text-foreground active:scale-[0.98]"
          onclick={() => applyPreset(p.id)}
        >{p.label}</button>
      {/each}
      <!-- In the last N … -->
      <div class="mt-1 flex flex-col gap-1.5 border-t border-border/40 px-1 pt-2">
        <span class="text-ui-2xs font-medium uppercase tracking-[0.06em] text-muted-foreground">In the last</span>
        <div class="flex items-center gap-1">
          <button type="button" class="grid size-6 shrink-0 place-items-center rounded-md border border-border/60 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground active:scale-[0.96]" aria-label="Decrease" onclick={() => (lastN = Math.max(1, lastN - 1))}>
            <Minus class="size-3 shrink-0" />
          </button>
          <input
          class= "field-surface h-6 w-9 min-w-0 bg-input/30 text-center font-mono text-ui-xs tabular-nums outline-none"
            inputmode="numeric"
            value={lastN}
            oninput={(e) => (lastN = Math.max(1, parseInt(e.currentTarget.value.replace(/\D/g, "")) || 1))}
          />
          <button type="button" class="grid size-6 shrink-0 place-items-center rounded-md border border-border/60 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground active:scale-[0.96]" aria-label="Increase" onclick={() => (lastN = lastN + 1)}>
            <Plus class="size-3 shrink-0" />
          </button>
        </div>
        <FieldSelect
          size="sm"
          class="h-6 w-full bg-input/30 px-1.5 text-ui-xs"
          bind:value={lastUnit}
          options={[
            { value: 'hours', label: 'hours' },
            { value: 'days', label: 'days' },
            { value: 'weeks', label: 'weeks' },
            { value: 'months', label: 'months' },
          ]}
        />
        <Button
          class="mt-0.5" size="sm"
          onclick={applyLastN}
>Apply</Button>
      </div>
    </div>

    <!-- Calendar -->
    <div class="p-0">
      {#if isBetween}
        <RangeCalendar value={rangeValue} onValueChange={pickRange} numberOfMonths={2} />
      {:else}
        <Calendar value={singleValue} onValueChange={pickSingle} />
      {/if}
    </div>
  </PopoverContent>
</Popover>
