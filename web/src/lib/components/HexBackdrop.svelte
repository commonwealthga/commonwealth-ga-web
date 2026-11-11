<script lang="ts">
	/**
	 * Page backdrop: a quiet hex field that leans towards the pointer.
	 *
	 * Deliberately restrained — one texture, one warm wash, one spotlight. The
	 * spotlight and the few pixels of parallax are the only things that move, and
	 * only on devices with a real pointer and without reduced-motion set.
	 */
	let root = $state<HTMLElement | null>(null);

	$effect(() => {
		const node = root;
		if (!node) return;
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		if (!window.matchMedia('(pointer: fine)').matches) return;

		let frame = 0;
		let px = 0.5;
		let py = 0.15;

		const apply = () => {
			frame = 0;
			node.style.setProperty('--mx', `${(px * 100).toFixed(2)}%`);
			node.style.setProperty('--my', `${(py * 100).toFixed(2)}%`);
			node.style.setProperty('--shift-x', `${((px - 0.5) * -14).toFixed(2)}px`);
			node.style.setProperty('--shift-y', `${((py - 0.5) * -10).toFixed(2)}px`);
		};

		const onMove = (event: PointerEvent) => {
			px = event.clientX / window.innerWidth;
			py = event.clientY / window.innerHeight;
			if (!frame) frame = requestAnimationFrame(apply);
		};

		window.addEventListener('pointermove', onMove, { passive: true });
		return () => {
			window.removeEventListener('pointermove', onMove);
			if (frame) cancelAnimationFrame(frame);
		};
	});
</script>

<div
	bind:this={root}
	class="pointer-events-none fixed inset-0 -z-10 overflow-hidden"
	aria-hidden="true"
	style="--mx: 50%; --my: 15%; --shift-x: 0px; --shift-y: 0px;"
>
	<!-- Hex field, a touch larger than the pointer travel so parallax never
	     exposes an edge. -->
	<div
		class="hexes absolute -inset-10 opacity-[0.045]"
		style="transform: translate3d(var(--shift-x), var(--shift-y), 0);"
	></div>

	<!-- Warm wash anchored to the top of the page. -->
	<div
		class="absolute inset-x-0 top-0 h-[55vh]"
		style="background: radial-gradient(ellipse 85% 100% at 50% 0%, rgba(224, 57, 28, 0.11), transparent 72%);"
	></div>

	<!-- Spotlight following the pointer: the grid only lights up where you look. -->
	<div
		class="absolute inset-0"
		style="background: radial-gradient(680px circle at var(--mx) var(--my), rgba(255, 122, 24, 0.075), transparent 62%);"
	></div>

	<!-- Vignette keeps the edges of the page quiet. -->
	<div
		class="absolute inset-0"
		style="background: radial-gradient(ellipse at 50% 35%, transparent 45%, #070506 100%);"
	></div>
</div>
