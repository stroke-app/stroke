<script>
	import { Command as CommandPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";
	import SearchIcon from '@lucide/svelte/icons/search';

	let {
		ref = $bindable(null),
		class: className,
		value = $bindable(""),
		...restProps
	} = $props();
</script>

<!-- Icon on the left, clean border separator at the bottom, Raycast style -->
<div data-slot="command-input-wrapper" class="flex items-center gap-3 border-b border-border/25 px-4 py-3">
	<SearchIcon class="size-4 shrink-0 text-muted-foreground" />
	<CommandPrimitive.Input
		bind:value
		data-slot="command-input"
		class={cn("", className)}
		{...restProps}
	>
		{#snippet child({ props })}
			<input
				{...props}
				bind:value
				bind:this={ref}
				class="no-focus-ring min-w-0 flex-1 bg-transparent text-ui-sm text-foreground outline-none placeholder:text-muted-foreground"
			/>
		{/snippet}
	</CommandPrimitive.Input>
</div>
