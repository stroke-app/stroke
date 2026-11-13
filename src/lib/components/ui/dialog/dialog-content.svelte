<script>
	import { Dialog as DialogPrimitive } from "bits-ui";
	import DialogPortal from "./dialog-portal.svelte";
	import * as Dialog from "./index.js";
	import { cn } from "$lib/utils.js";
	import X from '@lucide/svelte/icons/x';
	import { installFocusTracker, getLastExternalFocus } from "$lib/focus-tracker.js";

	let {
		ref = $bindable(null),
		class: className,
		portalProps,
		children,
		showCloseButton = true,
		/**
		 * 'pop' zooms and slides in. 'fade' only fades: for large dialogs, since
		 * Linux runs WebKitGTK on its software renderer (lib.rs) and a scaling
		 * panel has its whole surface and blurred shadow repainted every frame.
		 * @type {'pop' | 'fade'}
		 */
		motion = 'pop',
		/**
		 * bits-ui locks body scroll by default: it reads computed styles on
		 * <html> and <body>, then rewrites <body>'s overflow and padding, which
		 * restyles the whole app on open and again on close. Stroke's page never
		 * scrolls (html is overflow-hidden), so the lock guards nothing.
		 */
		preventScroll = false,
		/** @type {((e: Event) => void) | undefined} */
		onCloseAutoFocus,
		...restProps
	} = $props();

	// Track focus app-wide so we can restore it when this dialog closes. bits-ui
	// returns focus to a Dialog.Trigger automatically, but programmatically-opened
	// dialogs (no trigger) would otherwise drop focus to <body>.
	installFocusTracker();

	/** @param {Event} e */
	function handleCloseAutoFocus(e) {
		// Let an explicit consumer handler run / opt out first.
		onCloseAutoFocus?.(e);
		if (e.defaultPrevented) return;
		const prev = getLastExternalFocus();
		if (prev && document.contains(prev)) {
			e.preventDefault();
			try { prev.focus({ preventScroll: true }); } catch { /* element not focusable */ }
		}
	}
</script>

<DialogPortal {...portalProps}>
	<Dialog.Overlay />
	<DialogPrimitive.Content
		bind:ref
		{preventScroll}
		data-slot="dialog-content"
		onCloseAutoFocus={handleCloseAutoFocus}
		class={cn(
			"bg-background text-foreground data-open:animate-in data-closed:animate-out data-closed:fade-out-0 data-open:fade-in-0 ease-out flex flex-col gap-4 rounded-2xl border border-border/60 p-5 text-ui-sm elevate-3-rim w-full max-w-[calc(100%-2rem)] fixed top-1/2 left-1/2 z-50 -translate-x-1/2 -translate-y-1/2 outline-none",
			motion === 'pop'
				? "data-closed:zoom-out-[0.96] data-open:zoom-in-[0.96] data-open:slide-in-from-bottom-1 duration-200"
				: "duration-150",
			className
		)}
		{...restProps}
	>
		{@render children?.()}
		{#if showCloseButton}
			<DialogPrimitive.Close data-slot="dialog-close">
				{#snippet child({ props })}
					<button
						class="absolute right-3 top-3 inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/18"
						{...props}
					>
						<X class="size-3.5 shrink-0" />
						<span class="sr-only">Close</span>
					</button>
				{/snippet}
			</DialogPrimitive.Close>
		{/if}
	</DialogPrimitive.Content>
</DialogPortal>
