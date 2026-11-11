<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	/**
	 * Class leaderboards.
	 *
	 * One board per class, because a player's rating is per class and mixing
	 * them would compare a Medic against an Assault who never healed.
	 */
	import { afterNavigate } from '$app/navigation';
	import { fetchLeaderboard, type Leaderboard } from '$lib/agents';
	import {
		readChoice,
		readPage,
		trackScroll,
		restoreScroll,
		writeListState
	} from '$lib/listquery';
	import { relativeLabel } from '$lib/format';
	import { reveal } from '$lib/reveal';

	const CLASSES = ['Assault', 'Medic', 'Recon', 'Robotic'] as const;
	let className = $state<string>(readChoice(CLASSES, 'Assault', 'class'));
	let page = $state(readPage() - 1);

	trackScroll();
	afterNavigate(() => {
		className = readChoice(CLASSES, 'Assault', 'class');
		page = readPage() - 1;
	});
	let board = $state<Leaderboard | null>(null);
	let phase = $state<'loading' | 'ready' | 'error'>('loading');
	let problem = $state('');

	$effect(() => {
		const requestedClass = className;
		const requestedPage = page;
		let cancelled = false;
		phase = 'loading';

		fetchLeaderboard(requestedClass, requestedPage)
			.then((body) => {
				if (cancelled) return;
				board = body;
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

	const classes = $derived(board?.classes ?? ['Assault', 'Medic', 'Recon', 'Robotic']);

	function goTo(next: number) {
		page = Math.max(0, next);
		writeListState({ page: page + 1, params: { class: className } });
	}
</script>

<svelte:head>
	<Seo
		title="Global Agenda PvP leaderboards | The Commonwealth"
		description="Ratings for every rated agent on The Commonwealth, split by class, with recent form."
	/>
</svelte:head>

<header class="mb-8">
	<p class="label text-ember">Standings</p>
	<h1 class="heading mt-3 text-[clamp(2rem,5vw,3.25rem)]">Leaderboards</h1>
	<p class="mt-4 max-w-2xl text-base leading-relaxed text-dim">
		Current rating per class. Only rated matches (PvP queues) move these numbers, and the board
		updates as soon as a match is recorded.
	</p>
	<p class="mt-3">
		<a href="/agents" class="label text-ember transition-colors hover:text-amber"
			>← Agent directory</a
		>
	</p>
</header>

<div use:reveal class="mb-4 flex flex-wrap items-center gap-1">
	{#each classes as name (name)}
		<button
			type="button"
			class="label border px-2.5 py-1 transition-colors {className === name
				? 'border-ember/60 text-ember'
				: 'border-transparent text-faint hover:text-dim'}"
			onclick={() => {
				className = name;
				page = 0;
				writeListState({ page: 1, params: { class: name } });
			}}
		>
			{name}
		</button>
	{/each}
	{#if phase === 'ready' && board}
		<span class="label ml-auto text-faint">{board.total} rated</span>
	{/if}
</div>

<div class="panel-ticks border border-line/90 bg-panel/50">
	<div
		class="label hidden border-b border-line/80 px-4 py-2.5 text-faint sm:grid sm:grid-cols-[3rem_minmax(0,1fr)_8rem_8rem] sm:gap-3"
	>
		<span>#</span>
		<span>Agent</span>
		<span class="text-right">Rating</span>
		<span class="text-right">Last rated</span>
	</div>

	{#if phase === 'loading'}
		<div class="space-y-px">
			{#each Array(10) as _, i (i)}
				<div class="px-4 py-3">
					<div class="h-3 animate-pulse bg-line/50" style="width:{25 + ((i * 11) % 40)}%"></div>
				</div>
			{/each}
		</div>
	{:else if phase === 'error'}
		<p class="label px-4 py-6 text-red">leaderboard unavailable — {problem}</p>
	{:else if !board || board.entries.length === 0}
		<p class="label px-4 py-6 text-faint">no rated {className} players yet</p>
	{:else}
		<ul>
			{#each board.entries as entry (entry.user_id)}
				<li class="border-b border-line/50 last:border-b-0">
					<a
						href="/agents/{entry.user_id}"
						class="group grid grid-cols-[2.5rem_minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1 px-4 py-3 transition-colors hover:bg-panel-hi/60 sm:grid-cols-[3rem_minmax(0,1fr)_8rem_8rem] sm:gap-3"
					>
						<span
							class="label tabular-nums {entry.rank === 1
								? 'text-amber'
								: entry.rank <= 3
									? 'text-ember'
									: 'text-faint'}"
						>
							{entry.rank}
						</span>
						<span class="min-w-0">
							<span class="block truncate text-sm text-ink group-hover:text-amber"
								>{entry.username}</span
							>
							<span class="label mt-0.5 block text-faint sm:hidden"
								>{entry.mmr.toFixed(1)}</span
							>
						</span>
						<span class="hidden text-right text-sm tabular-nums text-ink sm:block">
							{entry.mmr.toFixed(1)}
						</span>
						<span class="label hidden text-right whitespace-nowrap text-faint sm:block">
							{relativeLabel(entry.played_at)}
						</span>
					</a>
				</li>
			{/each}
		</ul>
	{/if}

	{#if (board && (page > 0 || board.has_more)) || phase !== 'ready'}
		<div class="flex items-center justify-between border-t border-line/60 px-4 py-2.5">
			<button
				type="button"
				class="label border border-line-hi px-2.5 py-1 transition-colors {page === 0
					? 'cursor-not-allowed text-faint/50'
					: 'text-dim hover:text-ink'}"
				disabled={page === 0 || phase === 'loading'}
				onclick={() => goTo(page - 1)}>← back</button
			>
			<span class="label text-faint">page {page + 1}</span>
			<button
				type="button"
				class="label border border-line-hi px-2.5 py-1 transition-colors {board?.has_more && phase !== 'loading'
					? 'text-dim hover:text-ink'
					: 'cursor-not-allowed text-faint/50'}"
				disabled={!board?.has_more || phase === 'loading'}
				onclick={() => goTo(page + 1)}>more →</button
			>
		</div>
	{/if}
</div>
