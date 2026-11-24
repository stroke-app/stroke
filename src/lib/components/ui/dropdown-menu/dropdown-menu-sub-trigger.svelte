<script>
	import { DropdownMenu as DropdownMenuPrimitive } from "bits-ui";
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		inset,
		children,
		...restProps
	} = $props();
</script>

<DropdownMenuPrimitive.SubTrigger
	bind:ref
	data-slot="dropdown-menu-sub-trigger"
	data-inset={inset}
	class={cn(
		"focus:bg-accent focus:text-foreground data-open:bg-accent data-open:text-foreground not-data-[variant=destructive]:focus:**:text-foreground min-h-7 gap-2 rounded-menu-item px-2 py-1 text-ui-xs data-inset:pl-7 [&_svg:not([class*='size-'])]:size-3.5 flex min-w-0 cursor-default items-center outline-hidden select-none [&>[data-slot=menu-label]]:min-w-0 [&>[data-slot=menu-label]]:truncate [&_svg]:pointer-events-none [&_svg]:shrink-0",
		className
	)}
	{...restProps}
>
	{@render children?.()}
	<ChevronRightIcon class="ml-auto" />
</DropdownMenuPrimitive.SubTrigger>