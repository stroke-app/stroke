<script>
	import { ContextMenu as ContextMenuPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";
	import CheckIcon from '@lucide/svelte/icons/check';

	let {
		ref = $bindable(null),
		class: className,
		inset,
		children: childrenProp,
		...restProps
	} = $props();
</script>

<ContextMenuPrimitive.RadioItem
	bind:ref
	data-slot="context-menu-radio-item"
	data-inset={inset}
	class={cn(
		"focus:bg-accent focus:text-foreground min-h-7 gap-2 rounded-menu-item py-1 pr-7 pl-2 text-ui-xs data-inset:pl-7 [&_svg:not([class*='size-'])]:size-3.5 relative flex min-w-0 cursor-default items-center outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-40 [&>[data-slot=menu-label]]:min-w-0 [&>[data-slot=menu-label]]:truncate [&_svg]:pointer-events-none [&_svg]:shrink-0",
		className
	)}
	{...restProps}
>
	{#snippet children({ checked })}
		<span class="pointer-events-none absolute right-1.5">
			{#if checked}
				<CheckIcon  />
			{/if}
		</span>
		{@render childrenProp?.({ checked })}
	{/snippet}
</ContextMenuPrimitive.RadioItem>