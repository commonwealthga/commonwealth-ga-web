<script lang="ts">
	import { subscribeStatus, type StatusPayload } from '$lib/live';

	let status = $state<StatusPayload | null>(null);

	$effect(() => subscribeStatus((next) => (status = next)));

	let online = $derived(status?.ok === true);
	let players = $derived(status?.players ?? 0);
	// A count of zero on a dead control server is not "quiet", it is "closed".
	let controlDown = $derived(status?.control_server === false);
	let tone = $derived(
		!status ? 'pending' : controlDown ? 'down' : online ? (players > 0 ? 'active' : 'idle') : 'down'
	);
</script>

<a
	href="/server"
	class="clip-corner-sm flex shrink-0 items-center gap-2 whitespace-nowrap border border-line/90 bg-panel/80 px-2.5 py-1.5 transition-colors hover:border-ember/60 hover:bg-panel-hi"
	title={status?.text ?? 'Connecting to the live feed'}
>
	<span class="relative flex size-1.5">
		{#if tone === 'active'}
			<span class="absolute inset-0 animate-ping-slow rounded-full bg-ember"></span>
		{/if}
		<span
			class="relative size-1.5 rounded-full {tone === 'down'
				? 'bg-red'
				: tone === 'pending'
					? 'bg-faint'
					: 'bg-ember'}"
		></span>
	</span>
	<span class="label whitespace-nowrap {tone === 'down' ? 'text-red' : 'text-dim'}">
		{#if !status}
			linking
		{:else if controlDown}
			server down
		{:else if online}
			{players}
			<!-- Phones have no room for "agents": the count plus "in-game" still
			     says what the number is. -->
			<!-- The space is an expression: Svelte trims literal whitespace
			     before a closing tag, which rendered "0 agentsin-game". -->
			<span class="hidden sm:inline">{players === 1 ? 'agent' : 'agents'}{' '}</span>in-game
		{:else}
			offline
		{/if}
	</span>
</a>
