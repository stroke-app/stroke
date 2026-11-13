<script>
	import { cn } from "$lib/utils.js";
	import Kbd from "$lib/components/Kbd.svelte";
	let {
		ref = $bindable(null),
		class: className,
		/** A combo in `createHotkey` grammar (`Mod+Shift+X`) - printed as keycaps. */
		combo = "",
		/** A remappable command id (`tab.close`) - printed as its current keycaps. */
		command = "",
		children,
		...restProps
	} = $props();
</script>

<span
	bind:this={ref}
	data-slot="context-menu-shortcut"
	class={cn("ml-auto flex shrink-0 items-center pl-3 text-ui-3xs text-muted-foreground group-focus/context-menu-item:text-muted-foreground", className)}
	{...restProps}
>
	{#if command}
		<Kbd {command} />
	{:else if combo}
		<Kbd {combo} />
	{:else}
		{@render children?.()}
	{/if}
</span>
