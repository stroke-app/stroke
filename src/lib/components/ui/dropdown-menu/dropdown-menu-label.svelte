<script>
	import { cn } from "$lib/utils.js";
	let {
		ref = $bindable(null),
		class: className,
		inset,
		children,
		...restProps
	} = $props();
</script>

<div
	bind:this={ref}
	data-slot="dropdown-menu-label"
	data-inset={inset}
	class={cn("text-muted-foreground px-2 pt-1.5 pb-1 text-ui-2xs font-medium data-inset:pl-7", className)}
	{...restProps}
>
	{@render children?.()}
</div>