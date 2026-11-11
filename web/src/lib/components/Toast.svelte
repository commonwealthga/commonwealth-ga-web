<script lang="ts">
	/**
	 * Transient feedback that does not move the page.
	 *
	 * These messages used to be inline banners inserted above the content, so
	 * every save pushed the layout down for a frame and then pulled it back —
	 * the "one-frame jump". A toast is out of flow by construction: it can
	 * appear, change and leave without touching anything the reader is looking
	 * at, which is also why it is fine for it to be short-lived.
	 */
	let {
		message,
		tone = 'ok'
	}: { message: string | null; tone?: 'ok' | 'error' } = $props();

	const sticky = 4500;
	let visible = $state(false);
	let text = $state<string | null>(null);

	$effect(() => {
		const next = message;
		if (!next) {
			visible = false;
			return;
		}
		text = next;
		visible = true;
		const timer = setTimeout(() => (visible = false), sticky);
		return () => clearTimeout(timer);
	});
</script>

<div
	class="pointer-events-none fixed inset-x-0 bottom-6 z-50 flex justify-center px-4 transition-opacity duration-200 {visible
		? 'opacity-100'
		: 'opacity-0'}"
	aria-live="polite"
	aria-atomic="true"
>
	{#if text}
		<div
			class="panel-ticks border bg-carbon/95 px-4 py-2.5 shadow-lg {tone === 'error'
				? 'border-red/50'
				: 'border-ember/50'} {visible ? '' : 'invisible'}"
		>
			<span class="label {tone === 'error' ? 'text-red' : 'text-amber'}">{text}</span>
		</div>
	{/if}
</div>
