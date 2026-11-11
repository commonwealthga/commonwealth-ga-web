<script lang="ts">
	import type { Snippet } from 'svelte';

	/**
	 * A button that can be clipped without losing its outline.
	 *
	 * `clip-path` cuts through borders at the bevels, which left open notches.
	 * The fix is to stop using `border` on the clipped element: an outer, filled
	 * clipped layer paints the rim and an inner, inset clipped layer paints the
	 * surface, so the outline follows the shape exactly.
	 */
	let {
		href,
		variant = 'solid',
		size = 'md',
		class: className = '',
		children
	}: {
		href: string;
		variant?: 'solid' | 'outline' | 'ember';
		size?: 'md' | 'lg';
		class?: string;
		children: Snippet;
	} = $props();

	const padding = $derived(size === 'lg' ? 'px-7 py-3.5' : 'px-5 py-2.5');
	const rim = $derived(
		variant === 'outline' ? 'bg-line-hi' : variant === 'ember' ? 'bg-amber' : 'bg-ember'
	);
	const face = $derived(
		variant === 'outline'
			? 'bg-panel group-hover:bg-panel-hi'
			: variant === 'ember'
				? 'bg-ember group-hover:bg-amber'
				: 'bg-red group-hover:bg-ember'
	);
	// Anything pointing off-site opens in a new tab; internal routes stay put.
	const external = $derived(href.startsWith('http'));
	const text = $derived(
		variant === 'outline' ? 'text-ink' : variant === 'ember' ? 'text-void' : 'text-white'
	);
</script>

<a
	{href}
	target={external ? '_blank' : undefined}
	rel={external ? 'noopener noreferrer' : undefined}
	class="clip-corner group inline-flex transition-transform duration-150 hover:-translate-y-px {className}"
>
	<span class="clip-corner absolute inset-0 {rim}" aria-hidden="true"></span>
	<span
		class="clip-corner relative m-px inline-flex items-center gap-2 {padding} {face} transition-colors"
	>
		<span class="heading text-sm tracking-[0.14em] {text}">
			{@render children()}
		</span>
		<span class="text-sm {text} opacity-70 transition-transform group-hover:translate-x-0.5">→</span>
	</span>
</a>
