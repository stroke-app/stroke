<script>
	import { Popover as PopoverPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		sideOffset = 4,
		align = "start",
		...restProps
	} = $props();
</script>

<PopoverPrimitive.Portal>
	<PopoverPrimitive.Content
		bind:ref
		data-slot="popover-content"
		{sideOffset}
		{align}
		class={cn(
			"data-closed:animate-out data-closed:fade-out-0 ease-out duration-75 z-[60] rounded-lg border border-border/70 bg-popover p-0 elevate-2-rim outline-none",
			className
		)}
		{...restProps}
	/>
</PopoverPrimitive.Portal>
