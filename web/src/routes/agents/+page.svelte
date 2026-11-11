<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	/**
	 * The agent directory.
	 *
	 * Everyone with a recorded match, most recently active first — the way you
	 * find a player you just met, or look up someone from a past operation.
	 * Search is a plain name match; the ranking lives on its own page.
	 */
	import { afterNavigate } from '$app/navigation';
	import { fetchAgents, type AgentRow } from '$lib/agents';
	import { readPage, readText, trackScroll, restoreScroll, writeListState } from '$lib/listquery';
	import { relativeLabel } from '$lib/format';
	import { reveal } from '$lib/reveal';

	let query = $state(readText('q'));
	let page = $state(readPage() - 1);

	trackScroll();
	afterNavigate(() => {
		query = readText('q');
		page = readPage() - 1;
	});
	let agents = $state<AgentRow[]>([]);
	let total = $state(0);
	let hasMore = $state(false);
	let phase = $state<'loading' | 'ready' | 'error'>('loading');
	let problem = $state('');

	$effect(() => {
		const requestedQuery = query;
		const requestedPage = page;
		let cancelled = false;

		// Debounce keystrokes so typing a name is one query, not ten.
		const timer = setTimeout(() => {
			phase = 'loading';
			fetchAgents(requestedQuery, requestedPage)
				.then((body) => {
					if (cancelled) return;
					agents = body.agents;
					total = body.total;
					hasMore = body.has_more;
					phase = 'ready';
					void restoreScroll();
				})
				.catch((err: unknown) => {
					if (cancelled) return;
					problem = err instanceof Error ? err.message : String(err);
					phase = 'error';
				});
		}, requestedQuery === '' ? 0 : 220);

		return () => {
			cancelled = true;
			clearTimeout(timer);
		};
	});

	function search(next: string) {
		query = next;
		page = 0;
		writeListState({ query: next, page: 1 });
	}

	function goTo(next: number) {
		page = Math.max(0, next);
		writeListState({ page: page + 1, query });
	}
</script>

<svelte:head>
	<Seo
		title="Global Agenda agents and leaderboards | The Commonwealth"
		description="Profiles for every agent on The Commonwealth, with per-mode win rates, best matches and PvP ratings."
	/>
</svelte:head>

<header class="mb-8">
	<p class="label text-ember">Roster</p>
	<h1 class="heading mt-3 text-[clamp(2rem,5vw,3.25rem)]">Agents</h1>
	<p class="mt-4 max-w-2xl text-base leading-relaxed text-dim">
		Everyone who has played a recorded match here, most recently active first. Open an agent for
		their career, their rating history and where they sit in each class.
	</p>
	<p class="mt-3">
		<a href="/leaderboard" class="label text-ember transition-colors hover:text-amber"
			>Class leaderboards →</a
		>
	</p>
</header>

<div use:reveal class="mb-4 flex flex-wrap items-center justify-between gap-3">
	<label class="flex min-w-[16rem] flex-1 items-center gap-2 border border-line/90 bg-panel/40 px-3 py-2">
		<span class="label text-faint">Search</span>
		<input
			type="search"
			class="w-full bg-transparent text-sm text-ink placeholder:text-faint focus:outline-none"
			placeholder="agent name"
			value={query}
			oninput={(event) => search((event.currentTarget as HTMLInputElement).value)}
		/>
	</label>
	<span class="label text-faint">
		{#if phase === 'ready'}
			{total.toLocaleString('en-GB')} {total === 1 ? 'agent' : 'agents'}
		{/if}
	</span>
</div>

<div class="panel-ticks border border-line/90 bg-panel/50">
	<div
		class="label hidden border-b border-line/80 px-4 py-2.5 text-faint sm:grid sm:grid-cols-[minmax(0,1fr)_7rem_7rem_8rem] sm:gap-3"
	>
		<span>Agent</span>
		<span class="text-right">Matches</span>
		<span class="text-right">Damage</span>
		<span class="text-right">Last active</span>
	</div>

	{#if phase === 'loading'}
		<div class="space-y-px">
			{#each Array(8) as _, i (i)}
				<div class="px-4 py-3">
					<div class="h-3 animate-pulse bg-line/50" style="width:{30 + ((i * 13) % 45)}%"></div>
				</div>
			{/each}
		</div>
	{:else if phase === 'error'}
		<p class="label px-4 py-6 text-red">directory unavailable — {problem}</p>
	{:else if agents.length === 0}
		<p class="label px-4 py-6 text-faint">
			{query ? `no agent matches “${query}”` : 'no agents on record yet'}
		</p>
	{:else}
		<ul>
			{#each agents as agent (agent.user_id)}
				<li class="border-b border-line/50 last:border-b-0">
					<a
						href="/agents/{agent.user_id}"
						class="group relative grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1 px-4 py-3 transition-colors hover:bg-panel-hi/60 sm:grid-cols-[minmax(0,1fr)_7rem_7rem_8rem] sm:gap-3"
					>
						<span class="absolute inset-y-0 left-0 w-[2px] bg-ember opacity-0 transition-opacity group-hover:opacity-100"></span>
						<span class="min-w-0">
							<span class="block truncate text-sm text-ink group-hover:text-amber">{agent.username}</span>
							<span class="label mt-0.5 block text-faint sm:hidden">
								{agent.matches} matches · {agent.last_active
									? relativeLabel(agent.last_active)
									: 'no matches'}
							</span>
						</span>
						<span class="label hidden text-right tabular-nums text-dim sm:block">{agent.matches}</span>
						<span class="label hidden text-right tabular-nums text-dim sm:block">
							{agent.damage_dealt.toLocaleString('en-GB')}
						</span>
						<span class="label hidden text-right whitespace-nowrap text-faint sm:block">
							{agent.last_active ? relativeLabel(agent.last_active) : '—'}
						</span>
					</a>
				</li>
			{/each}
		</ul>
	{/if}

	{#if page > 0 || hasMore || phase !== 'ready'}
		<div class="flex items-center justify-between border-t border-line/60 px-4 py-2.5">
			<button
				type="button"
				class="label border border-line-hi px-2.5 py-1 transition-colors {page === 0
					? 'cursor-not-allowed text-faint/50'
					: 'text-dim hover:text-ink'}"
				disabled={page === 0}
				onclick={() => goTo(page - 1)}>← back</button
			>
			<span class="label text-faint">page {page + 1}</span>
			<button
				type="button"
				class="label border border-line-hi px-2.5 py-1 transition-colors {hasMore
					? 'text-dim hover:text-ink'
					: 'cursor-not-allowed text-faint/50'}"
				disabled={!hasMore}
				onclick={() => goTo(page + 1)}>more →</button
			>
		</div>
	{/if}
</div>
