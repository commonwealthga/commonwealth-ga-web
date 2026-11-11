<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	import MatchLog from '$lib/components/MatchLog.svelte';
	import Panel from '$lib/components/Panel.svelte';
</script>

<svelte:head>
	<Seo
		title="Global Agenda mission archive | The Commonwealth"
		description="Every PvE and PvP match played on the server: maps, modes, outcomes, durations and the agents present."
	/>
</svelte:head>

<header class="mb-8">
	<p class="label text-ember">History</p>
	<h1 class="heading mt-3 text-[clamp(2rem,5vw,3.25rem)]">Missions</h1>
	<p class="mt-4 max-w-2xl text-base leading-relaxed text-dim">
		Every mission played on this server, newest first. Entries come straight from the game
		database, so a match appears here as soon as it ends.
	</p>
</header>

<MatchLog limit={20} filtered />

<div class="mt-6 grid gap-4 md:grid-cols-2">
	<Panel label="Reading the table">
		<ul class="space-y-2 text-sm leading-relaxed text-dim">
			<li>
				<span class="text-ink">Outcome</span> — attackers or defenders carried the operation, and
				which task force did it.
			</li>
			<li>
				<span class="text-ink">Agents</span> — participants with recorded statistics. Instances
				that never filled show low counts.
			</li>
			<li>
				<span class="text-ink">Duration</span> — server-side mission time, not wall clock between
				queue and quit.
			</li>
		</ul>
	</Panel>

	<Panel label="Digging further">
		<p class="text-sm leading-relaxed text-dim">
			Click any match for the full scoreboard. PvP matches carry ratings and class matchups; PvE
			missions keep the raw mission statistics. Use the filter above to see only the queue type
			you care about, and <a class="text-ember hover:text-amber" href="/agents">the agent
			directory</a> to follow a player instead of a match.
		</p>
	</Panel>
</div>
