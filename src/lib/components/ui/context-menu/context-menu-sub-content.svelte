<script>
	import { ContextMenu as ContextMenuPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	} = $props();
</script>

<!-- Fade only, and fast. A submenu opens under a pointer that is already moving
     toward it, so a scale/slide stacked on the fade is exactly what reads as "the
     menu lagged" - the panel is still growing into place when the eye expects it
     to be there. -->
<ContextMenuPrimitive.SubContent
	bind:ref
	data-slot="context-menu-sub-content"
	class={cn("data-open:animate-in data-closed:animate-out data-closed:fade-out-0 data-open:fade-in-0 border border-border/60 bg-popover text-popover-foreground min-w-32 max-w-(--menu-max-w) rounded-menu p-1 elevate-2-rim duration-75 z-[60] overflow-x-hidden overflow-y-auto outline-none", className)}
	{...restProps}
/>