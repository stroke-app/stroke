<script>
	import { cn } from "$lib/utils.js";
	import Kbd from "$lib/components/Kbd.svelte";
	import { keycaps } from "$lib/shortcuts.js";
	let {
		ref = $bindable(null),
		class: className,
		keys = '',
		children,
		...restProps
	} = $props();

	// Split on "+" separators ("Ctrl+Shift+B") or spread Unicode codepoints ("⌘⇧B").
	// Unicode spread handles symbols correctly: [...'⌘⇧B'] = ['⌘','⇧','B'].
	// Rows write their chords as macOS glyphs (`⌘⇧S`, `⌃\``). Printed as written,
	// Linux and Windows showed ⌘ and ⌃ for keys they call Ctrl. Read the glyphs
	// as names and let `keycaps` print each platform's own spelling.
	const GLYPH_NAME = { '⌘': 'Mod', '⇧': 'Shift', '⌥': 'Alt', '⌃': 'Ctrl', '↵': 'Enter' };
	const tokens = $derived(
		!keys ? [] :
		keycaps(
			keys.includes('+')
				? keys
				: [...keys].map((k) => GLYPH_NAME[k] ?? k.toUpperCase()).join('+')
		)
	);
</script>

{#if tokens.length > 0}
	<span
		bind:this={ref}
		data-slot="command-shortcut"
		class={cn("ml-auto flex shrink-0 items-center pl-3", className)}
		{...restProps}
	>
		<Kbd keys={tokens} />
	</span>
{:else}
	<span
		bind:this={ref}
		data-slot="command-shortcut"
		class={cn("ml-auto shrink-0 pl-3 font-mono text-ui-3xs tabular-nums text-muted-foreground group-data-[selected]/command-item:text-muted-foreground", className)}
		{...restProps}
	>
		{@render children?.()}
	</span>
{/if}
