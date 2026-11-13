<script>
  import { Button } from '$lib/components/ui/button/index.js'
  /**
   * A popover-based date/time picker for insert-row inputs.
   * Accepts/emits an ISO-ish string (YYYY-MM-DDTHH:mm:ss).
   * @typedef {Object} Props
   * @property {string} value
   * @property {(v: string) => void} onchange
   * @property {boolean} [showTime]
   * @property {boolean} [disabled]
   * @property {string} [colName]
   * @property {() => void} [onfocus]
   * @property {(e: KeyboardEvent) => void} [onkeydown]
   * @property {HTMLInputElement | null} [inputRef]
   * @property {boolean} [iconTrailing]
   */
  import { Popover, PopoverTrigger, PopoverContent } from "$lib/components/ui/popover/index.js";
  import { Calendar } from "$lib/components/ui/calendar/index.js";
  import { CalendarDate, parseDate, toCalendarDate } from "@internationalized/date";
  import { untrack } from "svelte";
  import Check from "@lucide/svelte/icons/check";
  import CalendarIcon from "@lucide/svelte/icons/calendar";
  import { cn } from "$lib/utils.js";

  let {
    value = $bindable(""),
    /** A committed value: a calendar pick, a time change, or "now". */
    onchange,
    /**
     * A keystroke in the text field. Split from `onchange` because a caller may
     * want to COMMIT on a pick but only stage on typing - the grid's inline cell
     * editor does exactly that, and while both went through `onchange` it
     * committed after the first character and tore its own editor down, so the
     * field accepted exactly one keypress. Defaults to `onchange` so callers
     * that genuinely want both are unaffected.
     */
    oninput = undefined,
    /**
     * Keys the picker does not claim itself (Enter, Tab, Ctrl+Backspace) go
     * back to the caller, so an inline grid editor keeps its own commit and
     * cancel keys while the field still owns ArrowDown and Escape.
     */
    onkeydown: onKeyDown = undefined,
    /** The text field itself, for a caller that focuses or selects it. */
    inputRef = $bindable(null),
    /**
     * Cell-editor trim: the calendar button sits after the value rather than
     * before it, so the text stays on the x the canvas drew it at.
     */
    iconTrailing = false,
    class: className = "",
    inputClass = "",
    inputStyle = "",
    showTime = true,
    disabled = false,
    colName,
    onfocus,
  } = $props();
  const onType = $derived(oninput ?? onchange);

  let open = $state(false);

  // Which wire format the incoming value is in. An edit has to write the column
  // back in the format it already stores: a `created_at` column holding Unix
  // millis (as text or an integer) must not come back as "2026-07-30T12:34", and
  // a seconds column must not silently gain three digits.
  const valueFormat = $derived.by(() => {
    const s = String(value ?? "").trim();
    if (/^-?\d{11,}$/.test(s)) return "epoch-ms";
    if (/^-?\d{9,10}$/.test(s)) return "epoch-s";
    return "iso";
  });

  /**
   * The fields of a timestamp, read straight out of its text.
   *
   * Deliberately textual rather than via `new Date`. A database timestamp is a
   * wall clock plus, sometimes, an offset - and the user is editing those
   * digits, not an instant. Going through a local `Date` rendered
   * `2026-05-22 11:28:40.501 UTC` in the machine's own zone and dropped the
   * offset, so every edit moved the value. It also could not parse that string
   * at all: the old code appended `T00:00:00` to anything without a `T`, making
   * `…501 UTCT00:00:00`, an Invalid Date - which is why the calendar said
   * "Pick a date…" and the hour and minute boxes both read 00 for a row that
   * plainly held a time.
   *
   * `second`, `fraction` and `zone` are carried untouched so a date pick cannot
   * spend precision the picker never showed.
   */
  const ISO_PARTS_RE =
    /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2})(?::(\d{2}))?(\.\d+)?\s*(Z|UTC|GMT|[+-]\d{2}(?::?\d{2})?)?)?$/i;

  const parts = $derived.by(() => {
    const s = String(value ?? "").trim();
    if (!s) return null;

    // An epoch is a true instant, so it does go through a Date - in local time,
    // which is the zone its hour and minute boxes are showing.
    if (valueFormat === "epoch-ms" || valueFormat === "epoch-s") {
      const n = Number(s);
      if (!Number.isFinite(n)) return null;
      const d = new Date(valueFormat === "epoch-s" ? n * 1000 : n);
      if (isNaN(d.getTime())) return null;
      return {
        year: d.getFullYear(), month: d.getMonth() + 1, day: d.getDate(),
        hour: d.getHours(), minute: d.getMinutes(), second: d.getSeconds(),
        fraction: "", zone: "",
      };
    }

    const m = ISO_PARTS_RE.exec(s);
    if (!m) return null;
    return {
      year: Number(m[1]), month: Number(m[2]), day: Number(m[3]),
      hour: Number(m[4] ?? 0), minute: Number(m[5] ?? 0), second: Number(m[6] ?? 0),
      fraction: m[7] ?? "",
      zone: m[8] ?? "",
    };
  });

  const calValue = $derived.by(() => {
    if (!parts) return undefined;
    try {
      return new CalendarDate(parts.year, parts.month, parts.day);
    } catch {
      return undefined;
    }
  });

  const timeStr = $derived(
    parts
      ? `${String(parts.hour).padStart(2, "0")}:${String(parts.minute).padStart(2, "0")}:${String(parts.second ?? 0).padStart(2, "0")}`
      : "00:00:00",
  );

  const hourVal = $derived(parts ? parts.hour : 0);
  const minVal = $derived(parts ? parts.minute : 0);
  const secVal = $derived(parts ? (parts.second ?? 0) : 0);

  /**
   * The hour and minute boxes hold their OWN text while the popover is open.
   *
   * They used to render `value={hourVal}`, a number derived from the staged
   * value, so every keystroke round-tripped: typing `1` staged `01:09`, which
   * re-derived `hourVal` to 1 and rewrote the field - and typing the `2` of
   * `12` fought whatever the derive had just put there. Two digits could not be
   * entered. The local strings are the field's truth while you are in it; the
   * derived values re-seed them when the popover opens, or when the value
   * changes from outside while the boxes are not focused.
   * @type {{ h: string, m: string, s: string }}
   */
  let timeText = $state({ h: "", m: "", s: "" });
  let timeFocused = $state(false);

  $effect(() => {
    const h = String(hourVal).padStart(2, "0");
    const m = String(minVal).padStart(2, "0");
    const sec = String(secVal).padStart(2, "0");
    if (!open) { timeText = { h, m, s: sec }; return; }
    if (timeFocused) return;
    untrack(() => { timeText = { h, m, s: sec }; });
  });

  /** The value as the popover currently has it staged. */
  function stagedValue() {
    const base = calValue ?? new CalendarDate(new Date().getFullYear(), new Date().getMonth() + 1, new Date().getDate());
    const h = Math.max(0, Math.min(23, parseInt(timeText.h, 10) || 0));
    const m = Math.max(0, Math.min(59, parseInt(timeText.m, 10) || 0));
    const sec = Math.max(0, Math.min(59, parseInt(timeText.s, 10) || 0));
    const pad = (/** @type {number} */ n) => String(n).padStart(2, "0");
    return buildIsoString(base, `${pad(h)}:${pad(m)}:${pad(sec)}`);
  }

  /** Stage what the popover shows, without ending the edit. */
  function stage() {
    onType(stagedValue());
  }

  /** Commit it and close - Enter, or the confirm button. */
  function confirm() {
    onchange(stagedValue());
    open = false;
  }

  /**
   * Typing a time stages it and nothing else.
   *
   * Nothing in this popover commits on a keystroke any more. It did, through
   * `onchange`, and for the grid's inline cell editor `onchange` means COMMIT:
   * the first digit of an hour wrote the cell, tore the editor down, closed the
   * popover and took the focus with it. Enter and the confirm button are the
   * only ways out with a value.
   * @param {'h' | 'm'} part
   */
  function onTimeInput(part, /** @type {Event} */ e) {
    const raw = /** @type {HTMLInputElement} */ (e.currentTarget).value.replace(/\D/g, "").slice(0, 2);
    timeText = { ...timeText, [part]: raw };
    stage();
  }

  /** Field metadata, so the three boxes are a loop rather than three copies. */
  const TIME_FIELDS = /** @type {const} */ ([
    { part: "h", max: 23, label: "Hour", placeholder: "HH" },
    { part: "m", max: 59, label: "Minute", placeholder: "MM" },
    { part: "s", max: 59, label: "Second", placeholder: "SS" },
  ]);

  /** @param {KeyboardEvent} e */
  function onTimeKeydown(e) {
    if (e.key === "Enter") { e.preventDefault(); e.stopPropagation(); confirm(); return; }
    // Escape closes the popover only. What was typed is already staged in the
    // field behind it, and the caller's Escape would cancel the whole edit.
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); open = false; return; }
    // Up/down step the field, as they do in every time input.
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const part = /** @type {'h' | 'm' | 's'} */ (
        /** @type {HTMLInputElement} */ (e.currentTarget).dataset.part ?? "h"
      );
      const max = part === "h" ? 23 : 59;
      const cur = parseInt(timeText[part], 10) || 0;
      const next = (cur + (e.key === "ArrowUp" ? 1 : -1) + max + 1) % (max + 1);
      timeText = { ...timeText, [part]: String(next).padStart(2, "0") };
      stage();
    }
  }

  function setNow() {
    const now = new Date();
    const d = new CalendarDate(now.getFullYear(), now.getMonth() + 1, now.getDate());
    const h = String(now.getHours()).padStart(2, "0");
    const m = String(now.getMinutes()).padStart(2, "0");
    timeText = { h, m };
    // Staged, like every other control in here: "Now" is a value, not a decision
    // to stop editing.
    onType(buildIsoString(d, `${h}:${m}`));
  }

  const displayLabel = $derived.by(() => {
    if (!parts) return "Pick a date…";
    const date = `${String(parts.year).padStart(4, "0")}-${String(parts.month).padStart(2, "0")}-${String(parts.day).padStart(2, "0")}`;
    return showTime ? `${date}  ${timeStr}${parts.zone ? ` ${parts.zone}` : ""}` : date;
  });

  /**
   * Serialise a picked date back into the format the column already stores -
   * see `valueFormat`. Emitting ISO into an epoch column is a silent data
   * corruption: the write succeeds and the timestamp is destroyed.
   */
  function buildIsoString(dateVal, time) {
    if (!dateVal) return "";
    const yyyy = String(dateVal.year).padStart(4, "0");
    const mm = String(dateVal.month).padStart(2, "0");
    const dd = String(dateVal.day).padStart(2, "0");
    const [hh, mi, se] = (time || "00:00:00").split(":").map((s) => s.padStart(2, "0"));
    // The picker edits seconds now, so an absent third part means "zero", not
    // "keep whatever was stored".
    const secs = se ?? String(parts?.second ?? 0).padStart(2, "0");
    if (valueFormat === "epoch-ms" || valueFormat === "epoch-s") {
      // An epoch always carries a time component, so honour the time even when
      // the picker is in date-only mode. Fractional seconds are carried over
      // from the stored value rather than zeroed: the picker edits down to the
      // second, so it must not quietly drop precision it never showed.
      const fractionMs = parts?.fraction ? Math.round(Number(parts.fraction) * 1000) : 0;
      const ms = new Date(
        Number(yyyy), Number(mm) - 1, Number(dd), Number(hh), Number(mi),
        Number(secs), fractionMs,
      ).getTime();
      return String(valueFormat === "epoch-s" ? Math.floor(ms / 1000) : ms);
    }
    if (!showTime) return `${yyyy}-${mm}-${dd}`;
    // Fractional seconds and the offset ride along untouched, for the same
    // reason the epoch branch carries them: picking a date must change the date,
    // not the column's sub-second precision or its zone.
    const ss = secs;
    return `${yyyy}-${mm}-${dd}T${hh}:${mi}:${ss}${parts?.fraction ?? ""}${parts?.zone ?? ""}`;
  }

  function handleCalendarChange(newDate) {
    if (!newDate) return;
    // A date-only column has nothing else to set, so the pick IS the decision:
    // commit and close. With a time to set as well, the pick only stages - the
    // popover stays open for the hour and minute, and Enter or the tick ends it.
    if (!showTime) {
      onchange(buildIsoString(newDate, timeStr));
      open = false;
      return;
    }
    const h = timeText.h || String(hourVal).padStart(2, "0");
    const m = timeText.m || String(minVal).padStart(2, "0");
    onType(buildIsoString(newDate, `${h}:${m}`));
  }


</script>

<Popover bind:open>
  <!-- Type or pick, not one or the other. A calendar is the wrong tool for
       "same as the row above but a year earlier", and typing is the wrong tool
       for "some Tuesday in March" - the field does both, and the value is the
       column's own text either way, so an epoch column stays editable as digits
       rather than being hidden behind a formatted label. -->
  <div class={cn("relative flex w-full min-w-0 items-center gap-1.5", className)}>
    <!-- Anchor only: it spans the field so the calendar lines up with it, and
         takes no pointer events so the input keeps every click. -->
    <PopoverTrigger tabindex={-1} aria-hidden="true" class="pointer-events-none absolute inset-0 -z-10" />
    {#snippet calendarButton()}
      <button
        type="button"
        {disabled}
        tabindex={-1}
        aria-label="Open calendar"
        class="hit-area inline-flex size-4 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground disabled:opacity-50"
        onclick={() => (open = !open)}
      >
        <CalendarIcon class="size-3 shrink-0" />
      </button>
    {/snippet}
    {#if !iconTrailing}{@render calendarButton()}{/if}
    <input
      type="text"
      bind:this={inputRef}
      {disabled}
      data-new-row-input={colName}
      value={value ?? ""}
      placeholder={showTime ? "YYYY-MM-DD HH:mm" : "YYYY-MM-DD"}
      autocomplete="off"
      spellcheck="false"
      title={parts ? displayLabel : undefined}
      class={cn(
        "w-full min-w-0 bg-transparent font-mono text-ui-sm text-foreground outline-none",
        "placeholder:text-muted-foreground disabled:opacity-50",
        inputClass,
      )}
      style={inputStyle}
      oninput={(e) => onType(e.currentTarget.value)}
      onfocus={onfocus}
      onkeydown={(e) => {
        // The calendar is opt-in from the keyboard too, and Escape closes it
        // without the keystroke escaping to the row's cancel handler.
        if (e.key === "ArrowDown" && !open) { e.preventDefault(); open = true; return }
        if (e.key === "Escape" && open) { e.preventDefault(); e.stopPropagation(); open = false; return }
        // Everything else is the caller's: without this the grid's inline editor
        // never saw Enter or Tab, so a typed timestamp could not be committed.
        onKeyDown?.(e);
      }}
    />
    {#if iconTrailing}{@render calendarButton()}{/if}
  </div>
  <!-- Enter anywhere in here means "this one": on a calendar day the pick stages
       first and this applies it, in a time box the field's own handler has
       already claimed the key. Escape closes the popover and leaves what is
       staged in the field behind it. -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <PopoverContent
    class="w-auto"
    align="start"
    onkeydown={(e) => {
      if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); confirm(); return }
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); open = false }
    }}
  >
    <Calendar
      compact
      value={calValue}
      onValueChange={handleCalendarChange}
    />
    {#if showTime}
      <!-- One row: the time, then the two ways out. It was a labelled "TIME"
           band over two half-width boxes with a 20px colon between them and a
           bare tick floating in the corner - four type sizes, three alignment
           edges and no seconds field on a column that stores them. -->
      <div class="flex items-center gap-1.5 border-t border-border/40 bg-muted/15 px-2 py-2">
        <div class="flex items-center gap-1">
          {#each TIME_FIELDS as f, i (f.part)}
            {#if i > 0}<span class="select-none font-mono text-ui-2xs text-muted-foreground/60">:</span>{/if}
            <!-- `text`, not `number`: a number input rejects an empty string and
                 a leading zero, and its spinners had to be hidden anyway. The
                 digits are filtered on input and the arrows are handled. -->
            <input
              type="text"
              inputmode="numeric"
              maxlength="2"
              data-part={f.part}
              aria-label={f.label}
              title="{f.label} · ↑↓ to step"
              value={timeText[f.part]}
              oninput={(e) => onTimeInput(f.part, e)}
              onfocusin={() => (timeFocused = true)}
              onfocusout={() => (timeFocused = false)}
              onkeydown={onTimeKeydown}
              class="no-focus-ring h-7 w-9 rounded-md border border-border/50 bg-input/40 px-1 text-center font-mono text-ui-2xs tabular-nums text-foreground outline-none transition-colors hover:border-border focus:border-ring/60"
              placeholder={f.placeholder}
            />
          {/each}
        </div>

        <button
          type="button"
          onclick={setNow}
          title="Set to the current date and time"
          class="inline-flex h-7 shrink-0 items-center rounded-md px-2 font-mono text-ui-3xs text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
        >Now</button>

        <!-- The way out with a value, for a hand on the mouse; Enter does the
             same from any field in here. Labelled, because a lone tick in a
             corner does not say what it applies to. -->
        <Button
          onclick={confirm}
          title="Apply this date and time (Enter)"
          class="ml-auto shrink-0 font-mono" size="sm"
>
          <Check class="size-3 shrink-0" />
          Apply
        </Button>
      </div>
    {/if}
  </PopoverContent>
</Popover>
