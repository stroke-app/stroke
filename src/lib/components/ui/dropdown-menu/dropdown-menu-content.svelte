<script>
	import { cn } from "$lib/utils.js";
	import DropdownMenuPortal from "./dropdown-menu-portal.svelte";
	import { DropdownMenu as DropdownMenuPrimitive } from "bits-ui";
	let {
		ref = $bindable(null),
		sideOffset = 4,
		align = "start",
		portalProps,
		class: className,
		...restProps
	} = $props();
</script>

<DropdownMenuPortal {...portalProps}>
	<DropdownMenuPrimitive.Content
		bind:ref
		data-slot="dropdown-menu-content"
		{sideOffset}
		{align}
		class={cn(
			" data-closed:animate-out data-closed:fade-out-0 [transform-origin:var(--bits-floating-transform-origin)] ease-out duration-100 border border-border/60 bg-popover text-popover-foreground min-w-[max(9rem,var(--bits-dropdown-menu-anchor-width,0px))] max-w-(--menu-max-w) rounded-[10px] p-1 elevate-2-rim z-50 overflow-x-hidden overflow-y-auto outline-none data-closed:overflow-hidden",
			className
		)}
		{...restProps}
	/>
</DropdownMenuPortal>