<script lang="ts">
	import { fetchMatchPage, type MatchKindFilter, type MatchSummary } from '$lib/live';
	import { afterNavigate } from '$app/navigation';
	import { readKind, readPage, trackScroll, restoreScroll, writeListState } from '$lib/listquery';
	import {
		durationLabel,
		matchMapName,
		matchQueue,
		outcomeLabel,
		relativeLabel,
		viewerSuffix
	} from '$lib/format';

	let {
		limit = 12,
		filtered = false
	}: {
		limit?: number;
		/** Show the PvP/PvE toggle and pager (the archive, a player's history). */
		filtered?: boolean;
	} = $props();

	let matches = $state<MatchSummary[]>([]);
	let total = $state(0);
	let hasMore = $state(false);
	let page = $state(readPage() - 1);
	let kind = $state<MatchKindFilter>(readKind(['all', 'pvp', 'pve'], 'all'));

	// Back and Forward land here as navigations, and on a Back the component is
	// mounted before the address settles, so re-read after each one rather than
	// trusting the value the component started with.
	trackScroll();
	afterNavigate(() => {
		const nextPage = readPage() - 1;
		const nextKind = readKind(['all', 'pvp', 'pve'] as const, 'all' as MatchKindFilter);
		if (nextKind !== kind) kind = nextKind;
		if (nextPage !== page) page = nextPage;
	});
	let phase = $state<'loading' | 'ready' | 'error'>('loading');
	let problem = $state('');

	const KINDS: { value: MatchKindFilter; label: string }[] = [
		{ value: 'all', label: 'All' },
		{ value: 'pvp', label: 'PvP' },
		{ value: 'pve', label: 'PvE' }
	];

	$effect(() => {
		const requestedKind = kind;
		const requestedPage = page;
		let cancelled = false;
		phase = 'loading';

		fetchMatchPage(requestedKind, requestedPage, limit)
			.then((body) => {
				if (cancelled) return;
				matches = body.matches;
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
		return () => {
			cancelled = true;
		};
	});

	function chooseKind(next: MatchKindFilter) {
		if (next === kind) return;
		kind = next;
		page = 0;
		writeListState({ page: 1, kind });
	}

	function goTo(next: number) {
		page = Math.max(0, next);
		writeListState({ page: page + 1, kind });
	}

	const winnerClass = (tf: number | null) =>
		tf === 1 ? 'text-ember' : tf === 2 ? 'text-steel' : 'text-faint';
</script>

{#if filtered}
	<div class="mb-3 flex flex-wrap items-center justify-between gap-3">
		<div class="flex items-center gap-1">
			{#each KINDS as option (option.value)}
				<button
					type="button"
					class="label border px-2.5 py-1 transition-colors {kind === option.value
						? 'border-ember/60 text-ember'
						: 'border-transparent text-faint hover:text-dim'}"
					onclick={() => chooseKind(option.value)}
				>
					{option.label}
				</button>
			{/each}
		</div>
		<span class="label text-faint">
			{#if phase === 'ready'}
				{total.toLocaleString('en-GB')}
				{kind === 'all' ? 'matches' : kind === 'pvp' ? 'PvP matches' : 'PvE matches'}
			{/if}
		</span>
	</div>
{/if}

<div class="panel-ticks border border-line/90 bg-panel/50">
	<!-- Column headings: hidden on narrow screens, where rows stack instead. -->
	<div
		class="label hidden border-b border-line/80 px-4 py-2.5 text-faint sm:grid sm:grid-cols-[3rem_minmax(0,1fr)_13rem_5rem_6rem_6rem] sm:gap-3"
	>
		<span>Ref</span>
		<span>Map / queue</span>
		<span>Outcome</span>
		<span class="text-right">Agents</span>
		<span class="text-right">Duration</span>
		<span class="text-right">Logged</span>
	</div>

	{#if phase === 'loading'}
		<div class="space-y-px">
			{#each Array(6) as _, i (i)}
				<div class="flex items-center gap-3 px-4 py-3">
					<span class="h-3 w-3 animate-pulse bg-line/70"></span>
					<span
						class="h-3 animate-pulse bg-line/70"
						style="width:{35 + ((i * 17) % 40)}%"
					></span>
				</div>
			{/each}
		</div>
	{:else if phase === 'error'}
		<p class="label px-4 py-6 text-red">archive unavailable — {problem}</p>
	{:else if matches.length === 0}
		<p class="label px-4 py-6 text-faint">no completed operations on record</p>
	{:else}
		<ul>
			{#each matches as match (match.id)}
				{@const won = match.winning_task_force}
				<li class="border-b border-line/50 last:border-b-0">
					<a
						href="/missions/{match.instance_id}"
						class="group relative grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1 px-4 py-3 transition-colors hover:bg-panel-hi/60 sm:grid-cols-[3rem_minmax(0,1fr)_13rem_5rem_6rem_6rem] sm:gap-3"
					>
					<!-- Hover ember rail -->
					<span
						class="absolute inset-y-0 left-0 w-[2px] bg-ember opacity-0 transition-opacity group-hover:opacity-100"
					></span>

					<span class="label text-faint tabular-nums">#{match.id}</span>

					<span class="min-w-0">
						<span class="block truncate text-sm text-ink">{matchMapName(match)}</span>
						<span class="label mt-1 block truncate text-faint">{matchQueue(match)}</span>
					</span>

					<span class="label whitespace-nowrap {winnerClass(won)} sm:order-none">
						{outcomeLabel(match.outcome)}
						{#if match.viewer}
							{@const mine = viewerSuffix(match.winning_task_force, match.viewer.task_force)}
							{#if mine}
								<span class="ml-2 {mine.tone}">{mine.text}</span>
							{/if}
						{/if}
					</span>

					<span class="label hidden text-right tabular-nums text-dim sm:block">{match.players}</span>
					<span class="label hidden text-right tabular-nums text-dim sm:block">
						{durationLabel(match.started_at, match.sealed_at)}
					</span>
					<span class="label hidden text-right whitespace-nowrap text-faint sm:block">
						{relativeLabel(match.started_at)}
					</span>

					<!-- Stacked meta on mobile -->
					<span class="label col-span-3 mt-1 flex gap-3 text-faint sm:hidden">
						<span>{match.players} agents</span>
						<span>{durationLabel(match.started_at, match.sealed_at)}</span>
						<span>{relativeLabel(match.started_at)}</span>
					</span>
					</a>
				</li>
			{/each}
		</ul>
	{/if}

	{#if filtered && (page > 0 || hasMore || phase !== 'ready')}
		<div class="flex items-center justify-between border-t border-line/60 px-4 py-2.5">
			<button
				type="button"
				class="label border border-line-hi px-2.5 py-1 transition-colors {page === 0
					? 'cursor-not-allowed text-faint/50'
					: 'text-dim hover:text-ink'}"
				disabled={page === 0 || phase === 'loading'}
				onclick={() => goTo(page - 1)}>← newer</button
			>
			<span class="label text-faint">page {page + 1}</span>
			<button
				type="button"
				class="label border border-line-hi px-2.5 py-1 transition-colors {hasMore && phase !== 'loading'
					? 'text-dim hover:text-ink'
					: 'cursor-not-allowed text-faint/50'}"
				disabled={!hasMore || phase === 'loading'}
				onclick={() => goTo(page + 1)}>older →</button
			>
		</div>
	{/if}
</div>
