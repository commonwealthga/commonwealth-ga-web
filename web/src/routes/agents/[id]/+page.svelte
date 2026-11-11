<script lang="ts">
	import { afterNavigate } from '$app/navigation';
	import { readKind, readPage, trackScroll, restoreScroll, writeListState } from '$lib/listquery';
	import Seo from '$lib/components/Seo.svelte';
	/**
	 * One agent, in public.
	 *
	 * The same career a player sees on their own page, plus what only makes
	 * sense publicly: where they stand in each class, and their rating over
	 * time per class, with the same zoom gesture as the activity chart.
	 */
	import { page } from '$app/stores';
	import MmrChart from '$lib/components/MmrChart.svelte';
	import Panel from '$lib/components/Panel.svelte';
	import {
		fetchAgent,
		fetchAgentMatches,
		type AgentMatch,
		type AgentProfile,
		type PlayMode
	} from '$lib/agents';
	import { durationLabel, matchMapName, matchQueue, outcomeLabel, relativeLabel } from '$lib/format';
	import { formatDayYear } from '$lib/time';

	const userId = $derived($page.params.id ?? '');

	let profile = $state<AgentProfile | null>(null);
	let phase = $state<'loading' | 'ready' | 'error'>('loading');
	let problem = $state('');

	let matches = $state<AgentMatch[]>([]);
	let matchTotal = $state(0);
	let matchHasMore = $state(false);
	let matchPage = $state(readPage() - 1);
	let kind = $state<'all' | 'pvp' | 'pve'>(readKind(['all', 'pvp', 'pve'], 'all'));

	trackScroll();
	afterNavigate(() => {
		kind = readKind(['all', 'pvp', 'pve'] as const, 'all' as 'all' | 'pvp' | 'pve');
		matchPage = readPage() - 1;
	});

	function goToMatchPage(next: number) {
		matchPage = Math.max(0, next);
		writeListState({ page: matchPage + 1, kind });
	}
	let matchesPhase = $state<'loading' | 'ready' | 'error'>('loading');

	// Which class's rating the chart shows; null is all classes together.
	let chartClass = $state<string | null>(null);

	// PvP and PvE are kept apart on purpose: a player who mostly runs missions
	// has deaths and bot kills that say nothing about how they do against
	// people, and blended totals let one hide the other.
	let playMode = $state<PlayMode>('all');
	const MODES: { value: PlayMode; label: string }[] = [
		{ value: 'all', label: 'All' },
		{ value: 'pvp', label: 'PvP' },
		{ value: 'pve', label: 'PvE' }
	];

	const totals = $derived(profile ? profile.totals[playMode] : null);
	const bests = $derived(profile ? profile.bests[playMode] : []);

	$effect(() => {
		const id = userId;
		let cancelled = false;
		phase = 'loading';

		fetchAgent(id)
			.then((body) => {
				if (cancelled) return;
				profile = body;
				// Default to the class they have played most, so the chart opens
				// on the line that says the most about them.
				const busiest = [...body.ratings].sort((a, b) => b.games - a.games)[0];
				chartClass = busiest?.class_name ?? null;
				phase = 'ready';
			})
			.catch((err: unknown) => {
				if (cancelled) return;
				problem = err instanceof Error ? err.message : String(err);
				phase = 'error';
			});

		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		const id = userId;
		const requestedKind = kind;
		const requestedPage = matchPage;
		let cancelled = false;
		matchesPhase = 'loading';

		fetchAgentMatches(id, requestedKind, requestedPage)
			.then((body) => {
				if (cancelled) return;
				matches = body.matches;
				matchTotal = body.total;
				matchHasMore = body.has_more;
				matchesPhase = 'ready';
				void restoreScroll();
			})
			.catch(() => {
				if (cancelled) return;
				matchesPhase = 'error';
			});

		return () => {
			cancelled = true;
		};
	});

	const hours = (seconds: number) => Math.round(seconds / 360) / 10;
	const winRate = $derived.by(() => {
		if (!totals || totals.matches === 0) return '—';
		return `${Math.round((totals.wins / totals.matches) * 100)}%`;
	});

	const ratio = $derived.by(() => {
		if (!totals) return '—';
		return totals.deaths > 0 ? (totals.kills / totals.deaths).toFixed(2) : totals.kills.toFixed(2);
	});

	// Labels for the best-match cards; the machine name is the API contract.
	const STAT_LABELS: Record<string, string> = {
		kills: 'most kills',
		assists: 'most assists',
		damage_dealt: 'most damage',
		healing: 'most healing',
		obj_points: 'most objective points',
		damage_taken: 'most damage taken',
		bot_kills: 'most bot kills',
		beacons_destroyed: 'most beacons destroyed'
	};
	const statLabel = (stat: string) => STAT_LABELS[stat] ?? stat.replace(/_/g, ' ');

	const rankLabel = (rank: number, players: number) => {
		if (players <= 0) return null;
		if (rank === 1) return 'top of the class';
		return `#${rank} of ${players}`;
	};
</script>

<svelte:head>
	<Seo
		title={profile ? `${profile.username} | Global Agenda agent | The Commonwealth` : 'Agent | The Commonwealth'}
		description={profile
			? `${profile.username} on The Commonwealth: ${profile.totals.all.matches.toLocaleString('en-GB')} missions, per-class PvP ratings and full match history.`
			: 'Career record, per-class ratings and match history for a Global Agenda player on The Commonwealth.'}
		type="profile"
	/>
</svelte:head>

<p class="label">
	<a href="/agents" class="text-faint transition-colors hover:text-ember">← All agents</a>
</p>

{#if phase === 'loading'}
	<div class="mt-6 flex flex-col gap-3">
		<div class="h-10 w-64 animate-pulse bg-line/40"></div>
		<div class="mt-4 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
			{#each Array(4) as _, i (i)}
				<div class="h-24 animate-pulse border border-line/60 bg-panel/30"></div>
			{/each}
		</div>
	</div>
{:else if phase === 'error'}
	<Panel label="Agent" class="mt-6">
		<p class="text-sm leading-relaxed text-dim">{problem}</p>
		<p class="mt-3">
			<a href="/agents" class="label text-ember hover:text-amber">Back to the directory →</a>
		</p>
	</Panel>
{:else if profile}
	<header class="mt-4 mb-8">
		<p class="label text-ember">Agent</p>
		<h1 class="heading mt-2 text-[clamp(2rem,5vw,3.25rem)]">{profile.username}</h1>
		<p class="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-dim">
			{#if profile.last_active}
				<span>last active {relativeLabel(profile.last_active)}</span>
			{:else}
				<span class="text-faint">no matches recorded</span>
			{/if}
			{#if profile.first_seen}
				<span class="text-faint">first seen {formatDayYear(new Date(profile.first_seen * 1000))}</span>
			{/if}
			<span class="text-faint">
				{profile.totals.all.matches.toLocaleString('en-GB')} matches
				<span class="text-dim">
					· {profile.totals.pvp.matches.toLocaleString('en-GB')} PvP
				</span>
			</span>
		</p>
	</header>

	<section>
		<div class="mb-4 flex flex-wrap items-center justify-between gap-3 border-b border-line/60 pb-3">
			<h2 class="heading text-xl tracking-[0.08em]">Record</h2>
			<div class="flex items-center gap-1">
				{#each MODES as mode (mode.value)}
					<button
						type="button"
						class="label border px-2.5 py-1 transition-colors {playMode === mode.value
							? 'border-ember/60 text-ember'
							: 'border-transparent text-faint hover:text-dim'}"
						onclick={() => (playMode = mode.value)}
					>
						{mode.label}
					</button>
				{/each}
			</div>
		</div>

		{#if totals && totals.matches === 0}
			<p class="border border-line/80 bg-panel/30 px-4 py-6 text-sm text-dim">
				No {playMode === 'pvp' ? 'PvP ' : playMode === 'pve' ? 'PvE ' : ''}matches recorded.
			</p>
		{:else if totals}
			<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
				{#each [{ k: 'Matches', v: totals.matches.toLocaleString('en-GB') }, { k: 'Kills', v: totals.kills.toLocaleString('en-GB') }, { k: 'Deaths', v: totals.deaths.toLocaleString('en-GB') }, { k: 'Assists', v: totals.assists.toLocaleString('en-GB') }, { k: 'K/D', v: ratio }, { k: 'Damage dealt', v: totals.damage_dealt.toLocaleString('en-GB') }, { k: 'Damage taken', v: totals.damage_taken.toLocaleString('en-GB') }, { k: 'Healing', v: totals.healing.toLocaleString('en-GB') }, { k: 'Objective points', v: totals.obj_points.toLocaleString('en-GB') }, { k: 'Bot kills', v: totals.bot_kills.toLocaleString('en-GB') }, { k: 'Win rate', v: winRate }, { k: 'Time in missions', v: `${hours(totals.seconds_played)}h` }] as stat (stat.k)}
					<div class="panel-ticks border border-line/80 bg-panel/40 p-4">
						<dt class="label text-faint">{stat.k}</dt>
						<dd class="heading mt-2 text-2xl tabular-nums text-ink">{stat.v}</dd>
					</div>
				{/each}
			</div>

			{#if bests.length > 0}
				<div class="mt-6">
					<h3 class="label mb-2 text-dim">Best single match</h3>
					<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
						{#each bests as best (best.stat)}
							<a
								href="/missions/{best.instance_id}"
								class="group border border-line/80 bg-panel/40 p-3 transition-colors hover:border-ember/50 hover:bg-panel-hi/50"
							>
								<span class="label block text-faint">{statLabel(best.stat)}</span>
								<span class="heading mt-1 block text-xl tabular-nums text-amber">
									{best.value.toLocaleString('en-GB')}
								</span>
								<span class="label mt-1 block truncate text-faint group-hover:text-dim">
									{best.map_label ?? best.map_name}
								</span>
							</a>
						{/each}
					</div>
				</div>
			{/if}
		{/if}
	</section>

	{#if profile.ratings.length > 0}
		<section class="mt-10">
			<div class="mb-4 border-b border-line/60 pb-3">
				<h2 class="heading text-xl tracking-[0.08em]">Rating</h2>
			</div>
			<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
				{#each profile.ratings as rating (rating.class_name)}
					<button
						type="button"
						class="border bg-panel/40 p-4 text-left transition-colors {chartClass ===
						rating.class_name
							? 'border-ember/60'
							: 'border-line/80 hover:border-line-hi'}"
						onclick={() => (chartClass = rating.class_name)}
					>
						<div class="flex items-baseline justify-between">
							<span class="text-sm text-ink">{rating.class_name}</span>
							<span class="label text-faint">{rating.games} games</span>
						</div>
						<div class="heading mt-2 text-2xl tabular-nums ember-text">
							{rating.mmr.toFixed(1)}
						</div>
						<div class="label mt-1 text-faint">
							{rankLabel(rating.rank, rating.ranked_players) ?? 'unranked'}
						</div>
					</button>
				{/each}
			</div>

			<div class="mt-4">
				<MmrChart userId={profile.user_id} class_name={chartClass} />
			</div>
			{#if profile.ratings.length > 1}
				<button
					type="button"
					class="label mt-3 border border-line-hi px-2.5 py-1 text-dim transition-colors hover:text-ink"
					onclick={() => (chartClass = null)}>all classes together</button
				>
			{/if}
		</section>
	{/if}

	<section class="mt-10">
		<div class="mb-4 flex flex-wrap items-center justify-between gap-3 border-b border-line/60 pb-3">
			<h2 class="heading text-xl tracking-[0.08em]">Matches</h2>
			<div class="flex items-center gap-1">
				{#each [{ value: 'all', label: 'All' }, { value: 'pvp', label: 'PvP' }, { value: 'pve', label: 'PvE' }] as option (option.value)}
					<button
						type="button"
						class="label border px-2.5 py-1 transition-colors {kind === option.value
							? 'border-ember/60 text-ember'
							: 'border-transparent text-faint hover:text-dim'}"
						onclick={() => {
							kind = option.value as 'all' | 'pvp' | 'pve';
							matchPage = 0;
							writeListState({ page: 1, kind });
						}}
					>
						{option.label}
					</button>
				{/each}
			</div>
		</div>

		{#if matchesPhase === 'loading'}
			<div class="h-32 animate-pulse border border-line/60 bg-panel/30"></div>
		{:else if matchesPhase === 'error'}
			<p class="label border border-line/80 bg-panel/30 px-4 py-6 text-faint">
				match history unavailable
			</p>
		{:else if matches.length === 0}
			<p class="border border-line/80 bg-panel/30 px-4 py-6 text-sm text-dim">
				No {kind === 'all' ? '' : kind === 'pvp' ? 'PvP ' : 'PvE '}matches on record.
			</p>
		{:else}
			<div class="panel-ticks border border-line/80">
				<div
					class="label hidden border-b border-line/60 px-4 py-2.5 text-faint sm:grid sm:grid-cols-[minmax(0,1fr)_13rem_8rem_5rem_6rem] sm:gap-3"
				>
					<span>Map / queue</span>
					<span>Outcome</span>
					<span class="text-right">K / D / A</span>
					<span class="text-right">Damage</span>
					<span class="text-right">Played</span>
				</div>
				<ul>
					{#each matches as match (match.id)}
						{@const won =
							match.winning_task_force !== null && match.task_force === match.winning_task_force}
						<li class="border-b border-line/50 last:border-b-0">
							<a
								href="/missions/{match.instance_id}"
								class="group relative grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1 px-4 py-3 transition-colors hover:bg-panel-hi/40 sm:grid-cols-[minmax(0,1fr)_13rem_8rem_5rem_6rem] sm:gap-3"
							>
								<span
									class="absolute inset-y-0 left-0 w-[2px] bg-ember opacity-0 transition-opacity group-hover:opacity-100"
								></span>
								<span class="min-w-0">
									<span class="block truncate text-sm text-ink">{matchMapName(match)}</span>
									<span class="label mt-1 block truncate text-faint">{matchQueue(match)}</span>
								</span>
								<span class="label whitespace-nowrap">
									<span
										class={match.winning_task_force === 1
											? 'text-ember'
											: match.winning_task_force === 2
												? 'text-steel'
												: 'text-faint'}>{outcomeLabel(match.outcome)}</span
									>
									<span class="ml-2 {won ? 'text-amber' : 'text-dim'}"
										>· {won ? 'won' : 'lost'}</span
									>
								</span>
								<span class="label hidden text-right tabular-nums text-dim sm:block">
									{match.kills} / {match.deaths} / {match.assists}
								</span>
								<span class="label hidden text-right tabular-nums text-dim sm:block">
									{match.damage_dealt.toLocaleString('en-GB')}
								</span>
								<span class="label hidden text-right whitespace-nowrap text-faint sm:block">
									{relativeLabel(match.started_at)}
								</span>
								<span class="label col-span-2 mt-1 flex gap-3 text-faint sm:hidden">
									<span>{match.kills}/{match.deaths}/{match.assists}</span>
									<span>{durationLabel(match.started_at, match.sealed_at)}</span>
									<span>{relativeLabel(match.started_at)}</span>
								</span>
							</a>
						</li>
					{/each}
				</ul>
				{#if matchPage > 0 || matchHasMore}
					<div class="flex items-center justify-between border-t border-line/60 px-4 py-2.5">
						<button
							type="button"
							class="label border border-line-hi px-2.5 py-1 transition-colors {matchPage === 0
								? 'cursor-not-allowed text-faint/50'
								: 'text-dim hover:text-ink'}"
							disabled={matchPage === 0}
							onclick={() => goToMatchPage(matchPage - 1)}>← newer</button
						>
						<span class="label text-faint">
							page {matchPage + 1} · {matchTotal.toLocaleString('en-GB')} total
						</span>
						<button
							type="button"
							class="label border border-line-hi px-2.5 py-1 transition-colors {matchHasMore
								? 'text-dim hover:text-ink'
								: 'cursor-not-allowed text-faint/50'}"
							disabled={!matchHasMore}
							onclick={() => goToMatchPage(matchPage + 1)}>older →</button
						>
					</div>
				{/if}
			</div>
		{/if}
	</section>
{/if}
