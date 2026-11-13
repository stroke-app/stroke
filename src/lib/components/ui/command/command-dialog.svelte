<script>
	import Command from "./command.svelte";
	import * as Dialog from "$lib/components/ui/dialog/index.js";
	import { cn } from "$lib/utils.js";

	let {
		open = $bindable(false),
		ref = $bindable(null),
		value = $bindable(""),
		title = "Command Palette",
		description = "Search for a command to run...",
		showCloseButton = false,
		portalProps,
		children,
		filter = undefined,
		shouldFilter = undefined,
		class: className,
		...restProps
	} = $props();
</script>

<Dialog.Root bind:open {...restProps}>
	<Dialog.Header class="sr-only">
		<Dialog.Title>{title}</Dialog.Title>
		<Dialog.Description>{description}</Dialog.Description>
	</Dialog.Header>
	<Dialog.Content
		class={cn(
			"rounded-xl! top-[16vh] translate-y-0 overflow-hidden border border-border/60 bg-popover p-0 elevate-3-rim duration-100",
			className
		)}
		{showCloseButton}
		{portalProps}
	>
		<Command filter={filter} shouldFilter={shouldFilter} {...restProps} bind:value bind:ref {children} />
	</Dialog.Content>
</Dialog.Root>
