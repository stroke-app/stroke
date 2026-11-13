<script>
	import { cn } from "$lib/utils.js";
	import { DropdownMenu as DropdownMenuPrimitive } from "bits-ui";

	let {
		ref = $bindable(null),
		class: className,
		inset,
		variant = "default",
		...restProps
	} = $props();
</script>

<DropdownMenuPrimitive.Item
	bind:ref
	data-slot="dropdown-menu-item"
	data-inset={inset}
	data-variant={variant}
	class={cn(
		"focus:bg-accent focus:text-foreground focus:*:[svg]:text-foreground data-[variant=destructive]:text-destructive data-[variant=destructive]:focus:bg-destructive/10 data-[variant=destructive]:focus:text-destructive data-[variant=destructive]:*:[svg]:text-destructive min-h-7 gap-2 rounded-sm px-2 py-1 text-ui-xs data-inset:pl-7 [&_svg:not([class*='size-'])]:size-3.5 *:[svg]:text-muted-foreground data-highlighted:*:[svg]:text-foreground group/dropdown-menu-item relative flex min-w-0 cursor-default items-center outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-40 [&>[data-slot=menu-label]]:min-w-0 [&>[data-slot=menu-label]]:truncate [&_svg]:pointer-events-none [&_svg]:shrink-0",
		className
	)}
	{...restProps}
/>