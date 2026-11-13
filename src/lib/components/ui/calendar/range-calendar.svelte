<script>
  import { RangeCalendar as RangeCalendarPrimitive } from "bits-ui";
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import { cn } from "$lib/utils.js";

  let {
    value = $bindable(),
    placeholder = $bindable(),
    numberOfMonths = 2,
    class: className,
    weekdayFormat = "short",
    ...restProps
  } = $props();
</script>

<RangeCalendarPrimitive.Root
  bind:value
  bind:placeholder
  {numberOfMonths}
  {weekdayFormat}
  class={cn("p-3", className)}
  {...restProps}
>
  {#snippet children({ months, weekdays })}
    <RangeCalendarPrimitive.Header class="mb-2 flex items-center justify-between px-1">
      <RangeCalendarPrimitive.PrevButton
        class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
      >
        <ChevronLeft class="size-4 shrink-0" />
      </RangeCalendarPrimitive.PrevButton>
      <RangeCalendarPrimitive.Heading class="text-ui-sm font-medium text-foreground" />
      <RangeCalendarPrimitive.NextButton
        class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
      >
        <ChevronRight class="size-4 shrink-0" />
      </RangeCalendarPrimitive.NextButton>
    </RangeCalendarPrimitive.Header>

    <div class="flex gap-4">
      {#each months as month}
        <RangeCalendarPrimitive.Grid class="w-full border-collapse">
          <RangeCalendarPrimitive.GridHead>
            <RangeCalendarPrimitive.GridRow class="flex">
              {#each weekdays as day}
                <RangeCalendarPrimitive.HeadCell
                  class="w-9 rounded-md text-ui-2xs font-normal text-muted-foreground"
                >
                  {day.slice(0, 2)}
                </RangeCalendarPrimitive.HeadCell>
              {/each}
            </RangeCalendarPrimitive.GridRow>
          </RangeCalendarPrimitive.GridHead>
          <RangeCalendarPrimitive.GridBody>
            {#each month.weeks as weekDates}
              <RangeCalendarPrimitive.GridRow class="mt-1 flex">
                {#each weekDates as date}
                  <RangeCalendarPrimitive.Cell
                    {date}
                    month={month.value}
                    class="relative p-0 text-center text-ui-sm"
                  >
                    <RangeCalendarPrimitive.Day
                      class={cn(
                        "inline-flex size-9 items-center justify-center rounded-md text-ui-sm font-normal transition-colors",
                        "hover:bg-muted hover:text-foreground",
                        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                        // All days in range: light tint, no rounding (continuous strip)
                        "data-selected:bg-primary/12 data-selected:text-foreground data-selected:rounded-none data-selected:hover:bg-primary/20",
                        // Hover preview
                        "data-highlighted:bg-primary/8 data-highlighted:text-foreground data-highlighted:rounded-none",
                        // Endpoints: solid bg, restore rounding - use ! to override data-selected
                        "data-selection-start:!bg-primary data-selection-start:!text-primary-foreground data-selection-start:rounded-l-full data-selection-start:rounded-r-none",
                        "data-selection-end:!bg-primary data-selection-end:!text-primary-foreground data-selection-end:rounded-r-full data-selection-end:rounded-l-none",
                        // Today marker (ring instead of bg so it doesn't fight the range tint)
                        "data-today:ring-1 data-today:ring-inset data-today:ring-border/60 data-today:font-medium",
                        "data-outside-month:text-muted-foreground data-outside-month:pointer-events-none data-outside-month:!bg-transparent",
                        "data-disabled:pointer-events-none data-disabled:opacity-30"
                      )}
                    />
                  </RangeCalendarPrimitive.Cell>
                {/each}
              </RangeCalendarPrimitive.GridRow>
            {/each}
          </RangeCalendarPrimitive.GridBody>
        </RangeCalendarPrimitive.Grid>
      {/each}
    </div>
  {/snippet}
</RangeCalendarPrimitive.Root>
