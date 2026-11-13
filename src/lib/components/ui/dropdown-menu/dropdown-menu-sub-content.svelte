<script>
	import { DropdownMenu as DropdownMenuPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	} = $props();
</script>

<DropdownMenuPrimitive.SubContent
	bind:ref
	data-slot="dropdown-menu-sub-content"
	class={cn(" data-closed:animate-out data-closed:fade-out-0 [transform-origin:var(--bits-floating-transform-origin)] ease-out duration-100 border border-border/60 bg-popover text-popover-foreground min-w-[96px] max-w-(--menu-max-w) rounded-[10px] p-1 elevate-2-rim w-auto z-50 overflow-x-hidden overflow-y-auto outline-none", className)}
	{...restProps}
/>