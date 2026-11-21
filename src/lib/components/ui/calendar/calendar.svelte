<script>
	import { Calendar as CalendarPrimitive } from "bits-ui";
	import ChevronLeft from "@lucide/svelte/icons/chevron-left";
	import ChevronRight from "@lucide/svelte/icons/chevron-right";
	import { cn } from "$lib/utils.js";

	let {
		type = "single",
		value = $bindable(),
		placeholder = $bindable(),
		class: className,
		weekdayFormat = "short",
		/**
		 * Dense variant: 28px days at the app's own data size, for a calendar in
		 * a popover over a grid whose rows are 28px tall. The default 36px/15px
		 * calendar is a page-sized control - in the cell editor it came out
		 * wider than the column it was editing.
		 */
		compact = false,
		...restProps
	} = $props();

	const nav = $derived(
		"inline-flex items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-40 " +
			(compact ? "size-6" : "size-7"),
	);
	const navIcon = $derived(compact ? "size-3.5" : "size-4");
	// `flex-1`, not a fixed `w-7`/`w-9`: the popover around this calendar is
	// sized by its widest child - in the datetime picker that is the time row,
	// which is wider than 7 x 28px of day - so seven fixed columns packed
	// against the leading edge and left a dead strip down the trailing side,
	// while the month header's `justify-between` spanned the full width. The
	// columns share the row now, so the grid lines up with the header above it
	// and the footer below.
	const headCell = $derived(
		compact
			? "flex-1 rounded-md font-mono text-ui-3xs font-normal text-muted-foreground"
			: "flex-1 rounded-md text-ui-2xs font-normal text-muted-foreground",
	);
	const dayCell = $derived(compact ? "size-7 font-mono text-ui-2xs tabular-nums" : "size-9 text-ui-sm");
</script>

<CalendarPrimitive.Root
	{type}
	bind:value
	bind:placeholder
	{weekdayFormat}
	class={cn(compact ? "p-2" : "p-3", className)}
	{...restProps}
>
	{#snippet children({ months, weekdays })}
		<CalendarPrimitive.Header class={cn(
			// No inset: the header and the day grid share the same leading and
			// trailing edge, so the chevrons sit over the first and last columns
			// rather than 4px inside them.
			"flex items-center justify-between",
			compact ? "mb-1" : "mb-2",
		)}>
			<CalendarPrimitive.PrevButton class={nav}>
				<ChevronLeft class={navIcon} />
			</CalendarPrimitive.PrevButton>
			<CalendarPrimitive.Heading class={cn("font-medium text-foreground", compact ? "text-ui-2xs" : "text-ui-sm")} />
			<CalendarPrimitive.NextButton class={nav}>
				<ChevronRight class={navIcon} />
			</CalendarPrimitive.NextButton>
		</CalendarPrimitive.Header>

		{#each months as month}
			<CalendarPrimitive.Grid class="w-full border-collapse">
				<CalendarPrimitive.GridHead>
					<CalendarPrimitive.GridRow class="flex">
						{#each weekdays as day}
							<CalendarPrimitive.HeadCell class={headCell}>
								{day.slice(0, 2)}
							</CalendarPrimitive.HeadCell>
						{/each}
					</CalendarPrimitive.GridRow>
				</CalendarPrimitive.GridHead>
				<CalendarPrimitive.GridBody>
					{#each month.weeks as weekDates}
						<CalendarPrimitive.GridRow class={cn("flex", compact ? "mt-0.5" : "mt-1")}>
							{#each weekDates as date}
								<CalendarPrimitive.Cell {date} month={month.value} class="relative flex flex-1 justify-center p-0 text-center">
									<CalendarPrimitive.Day
										class={cn(
											"inline-flex items-center justify-center rounded-md font-normal transition-colors",
											dayCell,
											"hover:bg-muted hover:text-foreground",
											"focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
											"data-selected:bg-primary data-selected:text-primary-foreground data-selected:hover:bg-primary/90",
											"data-today:bg-muted/60 data-today:font-medium",
											"data-outside-month:text-muted-foreground data-outside-month:pointer-events-none",
											"data-disabled:pointer-events-none data-disabled:opacity-30"
										)}
									/>
								</CalendarPrimitive.Cell>
							{/each}
						</CalendarPrimitive.GridRow>
					{/each}
				</CalendarPrimitive.GridBody>
			</CalendarPrimitive.Grid>
		{/each}
	{/snippet}
</CalendarPrimitive.Root>
