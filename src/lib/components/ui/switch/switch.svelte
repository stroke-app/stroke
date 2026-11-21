<script>
	import { Switch as SwitchPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		checked = $bindable(false),
		class: className,
		...restProps
	} = $props();
</script>

<SwitchPrimitive.Root
	bind:ref
	bind:checked
	data-slot="switch"
	class={cn("app-switch peer cursor-pointer border-0 p-0", className)}
	{...restProps}
>
	<SwitchPrimitive.Thumb data-slot="switch-thumb" class="app-switch-thumb" />
</SwitchPrimitive.Root>
