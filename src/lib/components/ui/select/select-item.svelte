<script>
	import { Select as SelectPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";
	import CheckIcon from '@lucide/svelte/icons/check';

	let {
		ref = $bindable(null),
		class: className,
		value,
		label,
		children: childrenProp,
		...restProps
	} = $props();
</script>

<!-- rounded-lg (8px) = the content's 12px radius minus its 4px padding.
	 min-h-7 is the design system's compact control height; py-1 alone left a ~22px
	 row, small enough to mis-click and cramped enough to read as an afterthought.
	 transition-colors (never `all`): the highlight used to snap from row to row,
	 so running the pointer down a list strobed. 120ms is the press/hover band.
	 data-selected carries its own weight - the current value was distinguished
	 only by a tick 32px away at the far edge, which is not where the eye is. -->
<SelectPrimitive.Item
	bind:ref
	{value}
	data-slot="select-item"
	class={cn(
		"focus:bg-accent focus:text-foreground min-h-7 gap-2 rounded-sm py-1 pr-7 pl-2 text-ui-xs text-foreground/85 [&_svg:not([class*='size-'])]:size-3.5 *:[span]:last:flex *:[span]:last:items-center *:[span]:last:gap-2 data-highlighted:bg-accent data-highlighted:text-foreground data-selected:font-medium data-selected:text-foreground active:bg-accent/70 relative flex w-full min-w-0 cursor-default items-center outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-40 [&_svg]:pointer-events-none [&_svg]:shrink-0",
		className
	)}
	{...restProps}
>
	{#snippet children({ selected, highlighted })}
		<span class="absolute end-2 flex size-3.5 items-center justify-center text-muted-foreground">
			{#if selected}
				<CheckIcon class="cn-select-item-indicator-icon" />
			{/if}
		</span>
		{#if childrenProp}
			{@render childrenProp({ selected, highlighted })}
		{:else}
			{label || value}
		{/if}
	{/snippet}
</SelectPrimitive.Item>