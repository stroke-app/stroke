<script module>
	import { tv } from "$lib/utils.js";
	export const inputGroupAddonVariants = tv({
		base: "text-muted-foreground h-auto gap-2 py-1.5 text-ui-sm font-medium group-data-[disabled=true]/input-group:opacity-50 [&>kbd]:rounded-[calc(var(--radius)-5px)] [&>svg:not([class*='size-'])]:size-4 flex cursor-text items-center justify-center select-none",
		variants: {
			align: {
				"inline-start": "pl-2 has-[>button]:ml-[-0.3rem] has-[>kbd]:ml-[-0.15rem] order-first",
				"inline-end": "pr-2 has-[>button]:mr-[-0.3rem] has-[>kbd]:mr-[-0.15rem] order-last",
				"block-start":
					"px-2.5 pt-2 [.border-b]:pb-2 order-first w-full justify-start",
				"block-end": "px-2.5 pb-2 [.border-t]:pt-2 order-last w-full justify-start",
			},
		},
		defaultVariants: {
			align: "inline-start",
		},
	});
</script>

<script>
	import { cn } from "$lib/utils.js";
	let {
		ref = $bindable(null),
		class: className,
		children,
		align = "inline-start",
		...restProps
	} = $props();
</script>

<div
	bind:this={ref}
	role="group"
	data-slot="input-group-addon"
	data-align={align}
	class={cn(inputGroupAddonVariants({ align }), className)}
	onclick={(e) => {
		if ((e.target).closest("button")) {
			return;
		}
		e.currentTarget.parentElement?.querySelector("input")?.focus();
	}}
	{...restProps}
>
	{@render children?.()}
</div>