<script>
	import { Command as CommandPrimitive, useId } from "bits-ui";
	import { getContext, setContext } from "svelte";
	import { cn } from "$lib/utils.js";
	import { LOCAL_FILTER, LOCAL_GROUP } from "$lib/command-local-filter.svelte.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		heading,
		value,
		...restProps
	} = $props();

	const fallbackId = useId();
	const groupValue = $derived(value ?? heading ?? `----${fallbackId}`);
	/** @type {ReturnType<typeof import("$lib/command-local-filter.svelte.js").createLocalFilter> | undefined} */
	const local = getContext(LOCAL_FILTER);
	setContext(LOCAL_GROUP, { get key() { return groupValue; } });
	// With the page filtering its own rows, a group none of whose rows match
	// hides, but stays mounted: its rows keep scoring as the search changes.
	const hidden = $derived(!!local?.search && local.best(groupValue) <= 0);
</script>

<CommandPrimitive.Group
	bind:ref
	data-slot="command-group"
	class={cn(
		"text-foreground py-1",
		"[&_[data-command-group-items]]:flex [&_[data-command-group-items]]:w-full [&_[data-command-group-items]]:min-w-0 [&_[data-command-group-items]]:flex-col [&_[data-command-group-items]]:gap-px",
		hidden && "hidden",
		className,
	)}
	value={groupValue}
	{...restProps}
>
	{#if heading}
		<!-- Plain case, as every menu's group labels are (DESIGN_SYSTEM §7): the
		     uppercase micro-label is for page and sidebar sections. px-2.5 puts
		     the label on the row icons' edge (the list's px-1.5 plus the item's
		     px-2.5), which is also where the search icon sits. -->
		<CommandPrimitive.GroupHeading
			class="px-2.5 pt-3 pb-1.5 text-ui-xs font-medium text-muted-foreground"
		>
			{heading}
		</CommandPrimitive.GroupHeading>
	{/if}
	<CommandPrimitive.GroupItems {children} />
</CommandPrimitive.Group>
