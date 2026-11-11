<script lang="ts">
	/** Small hexagon mark: the Commonwealth sigil, at any size. */
	let { size = 28 }: { size?: number } = $props();

	const points = Array.from({ length: 6 }, (_, i) => {
		const angle = (Math.PI / 180) * (60 * i - 90);
		return `${(22 + 18 * Math.cos(angle)).toFixed(2)},${(22 + 18 * Math.sin(angle)).toFixed(2)}`;
	}).join(' ');
</script>

<svg width={size} height={size} viewBox="0 0 44 44" aria-hidden="true" class="shrink-0">
	<polygon {points} fill="none" stroke="var(--color-ember)" stroke-width="2" stroke-opacity="0.85" />
	<polygon
		points={Array.from({ length: 6 }, (_, i) => {
			const angle = (Math.PI / 180) * (60 * i - 90);
			return `${(22 + 9 * Math.cos(angle)).toFixed(2)},${(22 + 9 * Math.sin(angle)).toFixed(2)}`;
		}).join(' ')}
		fill="var(--color-red)"
	/>
</svg>
