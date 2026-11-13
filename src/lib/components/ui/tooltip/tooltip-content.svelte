<script>
	import { Tooltip as TooltipPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		side = "bottom",
		sideOffset = 8,
		children,
		...restProps
	} = $props();
</script>

<TooltipPrimitive.Portal>
	<TooltipPrimitive.Content
		bind:ref
		data-slot="tooltip-content"
		{side}
		{sideOffset}
		class={cn(
			" data-closed:animate-out data-closed:fade-out-0 [transform-origin:var(--bits-floating-transform-origin)] ease-out duration-75 z-50 rounded-lg border border-border/60 bg-popover px-2.5 py-1.5 text-ui-xs elevate-2-rim outline-none",
			className
		)}
		{...restProps}
	>
		{@render children?.()}
		<TooltipPrimitive.Arrow class="text-popover drop-shadow-sm" />
	</TooltipPrimitive.Content>
</TooltipPrimitive.Portal>
