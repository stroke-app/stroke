<script>
	import { Command as CommandPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	} = $props();
</script>

<CommandPrimitive.Item
	bind:ref
	data-slot="command-item"
	class={cn(
		"group/command-item relative flex w-full min-w-0 cursor-default items-center gap-3 rounded-md px-2.5 py-1.5 min-h-9 text-ui-sm outline-hidden select-none",
		"text-foreground data-selected:bg-accent",
		// Icons sit back in the muted ink and come forward on the selected row;
		// an icon that brings its own colour (an engine mark) keeps it.
		"[&>svg:not([class*='text-'])]:text-muted-foreground data-selected:[&>svg:not([class*='text-'])]:text-foreground",
		"data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-40",
		"[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
		"[&>[data-slot=command-label]]:min-w-0 [&>[data-slot=command-label]]:flex-1 [&>[data-slot=command-label]]:truncate",
		"[&>[data-slot=command-trailing]]:ml-auto [&>[data-slot=command-trailing]]:shrink-0",
		className,
	)}
	{...restProps}
>
	{@render children?.()}
</CommandPrimitive.Item>
