<script>
	import { Select as SelectPrimitive } from "bits-ui";
	import SelectPortal from "./select-portal.svelte";
	import SelectScrollUpButton from "./select-scroll-up-button.svelte";
	import SelectScrollDownButton from "./select-scroll-down-button.svelte";
	import { cn } from "$lib/utils.js";
	let {
		ref = $bindable(null),
		class: className,
		sideOffset = 4,
		portalProps,
		children,
		preventScroll = true,
		...restProps
	} = $props();
</script>

<!-- rounded-[10px] against the viewport's p-1 (4px) puts rounded-sm items at 6px - outer
	 radius minus padding - so item corners sit concentric inside the panel
	 instead of leaving a crescent of gap at each one.
	 The exit runs quicker than the enter: a menu should get out of the way faster
	 than it arrives. -->
<SelectPortal {...portalProps}>
	<SelectPrimitive.Content
		bind:ref
		{sideOffset}
		{preventScroll}
		data-slot="select-content"
		class={cn(
			"bg-popover text-popover-foreground data-closed:animate-out data-closed:fade-out-0 [transform-origin:var(--bits-floating-transform-origin)] ease-out data-closed:duration-100 border border-border/60 min-w-36 max-w-(--menu-max-w) rounded-[10px] elevate-2-rim relative isolate z-50 overflow-x-hidden overflow-y-auto",
			className
		)}
		{...restProps}
	>
		<SelectScrollUpButton />
		<SelectPrimitive.Viewport
			class={cn(
				"h-(--bits-select-anchor-height) w-full min-w-(--bits-select-anchor-width) p-1 scroll-my-1"
			)}
		>
			{@render children?.()}
		</SelectPrimitive.Viewport>
		<SelectScrollDownButton />
	</SelectPrimitive.Content>
</SelectPortal>