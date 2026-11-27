<script>
	import { cn } from "$lib/utils.js";
	import { Command as CommandPrimitive } from "bits-ui";

	let {
		api = $bindable(null),
		ref = $bindable(null),
		value = $bindable(""),
		class: className,
		...restProps
	} = $props();
</script>

<CommandPrimitive.Root
	bind:this={api}
	bind:value
	bind:ref
	data-slot="command"
	class={cn(
		// Transparent: it always sits in a dialog or popover that paints the
		// surface, and its own fill showed the page grey over the white menu.
		"bg-transparent text-foreground flex size-full flex-col overflow-hidden rounded-2xl! p-0",
		className,
	)}
	{...restProps}
/>
