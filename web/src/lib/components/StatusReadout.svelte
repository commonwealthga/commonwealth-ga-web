<script lang="ts">
	import { untrack } from 'svelte';
	import { countUp } from '$lib/countup';
	import { subscribeStatus, type StatusPayload } from '$lib/live';

	let { class: className = '', size = 'lg' }: { class?: string; size?: 'lg' | 'xl' } = $props();

	let status = $state<StatusPayload | null>(null);
	$effect(() => subscribeStatus((next) => (status = next)));

	let online = $derived(status?.ok === true);
	let target = $derived(online ? (status?.players ?? 0) : 0);

	// Count up to the new value rather than snapping between ticks.
	let shown = $state(0);
	$effect(() => {
		const to = target;
		const from = untrack(() => shown);
		return countUp(from, to, (value) => (shown = value), 420);
	});

	const numeral = $derived(size === 'xl' ? 'text-[5.5rem] leading-[0.8] sm:text-[7rem]' : 'text-5xl sm:text-6xl');
</script>

<div class="flex flex-col justify-center {className}">
	{#if status === null}
		<!-- Not linked yet: say so, rather than printing a placeholder numeral. -->
		<span class="flex items-center gap-1.5 {size === 'xl' ? 'h-[5.5rem] sm:h-[7rem]' : 'h-[3rem] sm:h-[3.75rem]'}">
			{#each [0, 1, 2] as bar (bar)}
				<span
					class="w-2 animate-hex-pulse bg-ember/70 {size === 'xl' ? 'h-14' : 'h-9'}"
					style="animation-delay:{bar * 220}ms; animation-duration:1.8s"
				></span>
			{/each}
		</span>
		<span class="label mt-2 text-faint">linking to grid</span>
	{:else if !online}
		<span class="heading text-red/90 {size === 'xl' ? 'text-5xl sm:text-6xl' : 'text-3xl'}">
			Uplink lost
		</span>
		<span class="label mt-2 text-dim">grid unreachable</span>
	{:else}
		<span class="heading tabular-nums {numeral} ember-text">
			{shown}
		</span>
		<span class="label mt-2 text-dim">
			{shown === 1 ? 'agent' : 'agents'} in-game
		</span>
	{/if}
</div>
