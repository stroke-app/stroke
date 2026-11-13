<script>
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	} = $props();
</script>

<!-- `[svg~&]` (a sibling selector), not shadcn's `group-has-[>svg]/alert`. That
     one compiles to `:is(.group\/alert:has(> svg) *)`, a `:has()` outside the
     subject, and WebKit answers it by re-checking the whole document on every
     SVG inserted or removed anywhere: ~5ms per icon with a 2,000-table sidebar
     open, so a tab switch or a ⌘K keystroke that mounts 50 icons stalled for
     hundreds of ms. Same placement: the icon comes first in an alert. -->
<div
	bind:this={ref}
	data-slot="alert-title"
	class={cn(
		"font-medium [svg~&]:col-start-2 [&_a]:hover:text-foreground [&_a]:underline [&_a]:underline-offset-3",
		className
	)}
	{...restProps}
>
	{@render children?.()}
</div>