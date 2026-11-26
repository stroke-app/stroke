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

<CommandPrimitive.List
	bind:ref
	data-slot="command-list"
	class={cn(
		// `show-scrollbar` opts a long list (the command palette) into the app's
		// thin scrollbar; short menus keep it hidden.
		!String(className ?? "").includes("show-scrollbar") && "no-scrollbar",
		"scroll-py-1 px-1.5 pb-1.5 outline-none overflow-x-hidden overflow-y-auto",
		className
	)}
	{...restProps}
>
	<!-- bits-ui ranks a search by moving the best groups and rows up inside the
	     viewport, and does nothing without one: "ai" left Views (Advisor) above
	     the AI group, and Enter ran the wrong row. -->
	<CommandPrimitive.Viewport>
		{@render children?.()}
	</CommandPrimitive.Viewport>
</CommandPrimitive.List>
