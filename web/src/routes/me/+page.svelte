<script lang="ts">
	import { afterNavigate } from '$app/navigation';
	import { readKind, readPage, trackScroll, restoreScroll, writeListState } from '$lib/listquery';
	import Seo from '$lib/components/Seo.svelte';
	import { goto } from '$app/navigation';
	import {
		ApiError,
		currentUser,
		myMatches,
		mySummary,
		signOut,
		type MyMatch,
		type MySummary,
		type SignedInUser
	} from '$lib/auth';
	import {
		durationLabel,
		matchMapName,
		matchQueue,
		outcomeLabel,
		relativeLabel,
		viewerSuffix
	} from '$lib/format';

	let user = $state<SignedInUser | null>(null);
	let summary = $state<MySummary | null>(null);
	let matches = $state<MyMatch[]>([]);
	let matchKind = $state<'all' | 'pvp' | 'pve'>(readKind(['all', 'pvp', 'pve'], 'all'));
	let matchPage = $state(readPage() - 1);
	let matchTotal = $state(0);
	let matchHasMore = $state(false);
	let matchesPhase = $state<'loading' | 'ready' | 'error'>('loading');
	let phase = $state<'checking' | 'loading' | 'ready' | 'error'>('checking');
	let problem = $state('');

	// The list state lives in the URL; these keep it in step with the address
	// after Back/Forward, and remember where the reader was in the page.
	trackScroll();
	afterNavigate(() => {
		matchKind = readKind(['all', 'pvp', 'pve'] as const, 'all' as 'all' | 'pvp' | 'pve');
		matchPage = readPage() - 1;
	});

	$effect(() => {
		let cancelled = false;

		currentUser()
			.then(async (signedIn) => {
				if (cancelled) return;
				if (!signedIn) {
					await goto('/login', { replaceState: true });
					return;
				}
				user = signedIn;
				phase = 'loading';

				const summaryBody = await mySummary();
				if (cancelled) return;
				summary = summaryBody;
				phase = 'ready';
			})
			.catch((err: unknown) => {
				if (cancelled) return;
				problem = err instanceof ApiError ? err.message : 'Could not load your data.';
				phase = 'error';
			});

		return () => {
			cancelled = true;
		};
	});

	function goToMatchPage(next: number) {
		matchPage = Math.max(0, next);
		writeListState({ page: matchPage + 1, kind: matchKind });
	}

	// The match list refetches on its own when the filter or page changes.
	$effect(() => {
		if (phase !== 'ready' && phase !== 'loading') return;
		const requestedKind = matchKind;
		const requestedPage = matchPage;
		let cancelled = false;
		matchesPhase = 'loading';

		myMatches(requestedKind, requestedPage)
			.then((body) => {
				if (cancelled) return;
				matches = body.matches;
				matchTotal = body.total;
				matchHasMore = body.has_more;
				matchesPhase = 'ready';
			})
			.catch(() => {
				if (cancelled) return;
				matchesPhase = 'error';
			});

		return () => {
			cancelled = true;
		};
	});

	async function logout() {
		await signOut();
		await goto('/');
	}

</script>

<svelte:head>
	<Seo
		title={user ? `${user.username} | The Commonwealth` : 'Your agent | The Commonwealth'}
		description="Your own agent profile, missions and settings on The Commonwealth."
		noindex
	/>
</svelte:head>

<header class="mb-8 flex flex-wrap items-end justify-between gap-4">
	<div>
		<p class="label text-ember">Player area</p>
		<h1 class="heading mt-2 text-[clamp(1.8rem,4.5vw,2.75rem)]">
			{user?.username ?? 'Loading…'}
		</h1>
		{#if summary}
			<p class="mt-2 text-sm text-dim">
				{summary.totals.matches.toLocaleString('en-GB')} missions recorded
				{#if user}
					· <a class="text-ember transition-colors hover:text-amber" href="/agents/{user.id}"
						>see your record</a
					>
				{/if}
			</p>
		{/if}
	</div>

	<div class="flex flex-wrap items-center gap-2">
		<button
			type="button"
			onclick={logout}
			class="label border border-line-hi px-3 py-2 text-dim transition-colors hover:border-ember/60 hover:text-ink"
		>
			Sign out
		</button>
	</div>
</header>

{#if phase === 'error'}
	<p class="border border-red/40 bg-red/10 px-4 py-3 text-sm text-red">{problem}</p>
{:else if phase === 'checking' || phase === 'loading'}
	<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
		{#each Array(4) as _, i (i)}
			<div class="h-24 animate-pulse border border-line/60 bg-panel/30"></div>
		{/each}
	</div>
{:else if summary}
	<!-- A launcher, not a profile: the numbers are on the public page, and
	     repeating a thinner version of them here just made this look like the
	     profile with parts missing. -->
	<div class="grid gap-4 sm:grid-cols-2">
		{#each [
			{ href: `/agents/${user?.id ?? ''}`, title: 'Public profile', body: 'What everyone else sees: your record split by PvP and PvE, your rating per class and where you stand, and your best matches.' },
			{ href: '/settings', title: 'Settings', body: 'Suits and stutter, solo mode, and which DLC map packs your install has — the toggles that used to be chat commands.' },
			{ href: '/missions', title: 'Missions', body: 'Everything played here, with a PvP/PvE filter, so you can find a match you were in.' },
			{ href: '/server', title: 'Server status', body: 'Population right now and whether the services behind it are answering.' }
		] as tile (tile.title)}
			<a
				href={tile.href}
				class="panel-ticks group border border-line/80 bg-panel/40 p-5 transition-colors hover:border-ember/50 hover:bg-panel-hi/40"
			>
				<span class="heading block text-lg tracking-[0.06em] text-ink group-hover:text-amber">
					{tile.title}
				</span>
				<span class="mt-2 block text-sm leading-relaxed text-dim">{tile.body}</span>
				<span
					class="label mt-3 inline-flex items-center gap-1 text-faint transition-transform group-hover:translate-x-0.5 group-hover:text-ember"
				>
					open <span aria-hidden="true">→</span>
				</span>
			</a>
		{/each}
	</div>

	<!-- Recent matches -->
	<section class="mt-10">
		<div class="mb-4 flex flex-wrap items-center justify-between gap-3 border-b border-line/60 pb-3">
			<h2 class="heading text-xl tracking-[0.08em]">Your recent missions</h2>
			<div class="flex items-center gap-1">
				{#each [{ value: 'all', label: 'All' }, { value: 'pvp', label: 'PvP' }, { value: 'pve', label: 'PvE' }] as option (option.value)}
					<button
						type="button"
						class="label border px-2.5 py-1 transition-colors {matchKind === option.value
							? 'border-ember/60 text-ember'
							: 'border-transparent text-faint hover:text-dim'}"
						onclick={() => {
							matchKind = option.value as 'all' | 'pvp' | 'pve';
							matchPage = 0;
							writeListState({ page: 1, kind: matchKind });
						}}
					>
						{option.label}
					</button>
				{/each}
			</div>
		</div>

		{#if matchesPhase === 'loading'}
			<div class="h-32 animate-pulse border border-line/80 bg-panel/30"></div>
		{:else if matches.length === 0}
			<p class="border border-line/80 bg-panel/30 px-4 py-6 text-sm text-dim">
				{matchKind === 'all'
					? 'No matches recorded yet — play one and it will show up here.'
					: `No ${matchKind === 'pvp' ? 'PvP' : 'PvE'} matches recorded.`}
			</p>
		{:else}
			<div class="panel-ticks border border-line/80">
				<div class="label hidden border-b border-line/60 px-4 py-2.5 text-faint sm:grid sm:grid-cols-[minmax(0,1fr)_13rem_8rem_5rem_6rem] sm:gap-3">
					<span>Map / queue</span>
					<span>Outcome</span>
					<span class="text-right">K / D / A</span>
					<span class="text-right">Damage</span>
					<span class="text-right">Played</span>
				</div>
				<ul>
					{#each matches as match (match.id)}
						{@const mine = viewerSuffix(match.winning_task_force, match.task_force)}
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
								<span class={match.winning_task_force === 1 ? 'text-ember' : match.winning_task_force === 2 ? 'text-steel' : 'text-faint'}>
									{outcomeLabel(match.outcome)}
								</span>
								{#if mine}
									<span class="ml-2 {mine.tone}">{mine.text}</span>
								{/if}
							</span>
							<span class="label hidden text-right tabular-nums text-dim sm:block">
								{match.kills} / {match.deaths} / {match.assists}
							</span>
							<span class="label hidden text-right tabular-nums text-dim sm:block">
								{match.damage_dealt.toLocaleString()}
							</span>
							<span class="label hidden text-right whitespace-nowrap text-faint sm:block">
								{relativeLabel(match.started_at)}
							</span>
							<span class="label col-span-2 mt-1 flex gap-3 text-faint sm:hidden">
								<span>{match.kills}/{match.deaths}/{match.assists}</span>
								<span>{match.damage_dealt.toLocaleString()} dmg</span>
								<span>{durationLabel(match.started_at, match.sealed_at)}</span>
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

	<p class="mt-8 text-xs leading-relaxed text-faint">
		Stats come straight from the server's match tables. Open any match for the full scoreboard;
		career history charts are still being built.
	</p>
{/if}
