<script>
	import { ContextMenu as ContextMenuPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		inset,
		variant = "default",
		...restProps
	} = $props();
</script>

<ContextMenuPrimitive.Item
	bind:ref
	data-slot="context-menu-item"
	data-inset={inset}
	data-variant={variant}
	class={cn(
		"focus:bg-accent focus:text-foreground focus:*:[svg]:text-foreground data-[variant=destructive]:text-destructive data-[variant=destructive]:focus:bg-destructive/10 dark:data-[variant=destructive]:focus:bg-destructive/20 data-[variant=destructive]:focus:text-destructive data-[variant=destructive]:*:[svg]:text-destructive gap-1.5 rounded-menu-item px-1.5 py-1 text-ui-2xs data-inset:pl-7 [&_svg:not([class*='size-'])]:size-3.5 group/context-menu-item relative flex min-w-0 cursor-default items-center whitespace-nowrap outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-40 [&>[data-slot=menu-label]]:min-w-0 [&>[data-slot=menu-label]]:truncate [&_svg]:pointer-events-none [&_svg]:shrink-0",
		className
	)}
	{...restProps}
/>
