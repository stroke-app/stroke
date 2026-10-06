<script>
	import { Command as CommandPrimitive, useId } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		heading,
		value,
		...restProps
	} = $props();
</script>

<CommandPrimitive.Group
	bind:ref
	data-slot="command-group"
	class={cn(
		"text-foreground py-1",
		"[&_[data-command-group-items]]:flex [&_[data-command-group-items]]:w-full [&_[data-command-group-items]]:min-w-0 [&_[data-command-group-items]]:flex-col [&_[data-command-group-items]]:gap-px",
		className,
	)}
	value={value ?? heading ?? `----${useId()}`}
	{...restProps}
>
	{#if heading}
		<!-- Plain case, as every menu's group labels are (DESIGN_SYSTEM §7): the
		     uppercase micro-label is for page and sidebar sections. px-2.5 puts
		     the label on the row icons' edge (the list's px-1.5 plus the item's
		     px-2.5), which is also where the search icon sits. -->
		<CommandPrimitive.GroupHeading
			class="px-2.5 pt-2 pb-1 text-ui-2xs font-medium text-muted-foreground"
		>
			{heading}
		</CommandPrimitive.GroupHeading>
	{/if}
	<CommandPrimitive.GroupItems {children} />
</CommandPrimitive.Group>
