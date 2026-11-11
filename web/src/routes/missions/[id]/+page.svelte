<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	/**
	 * One match, in full.
	 *
	 * PvE and PvP get different layouts because they answer different questions:
	 * a mission is "who did what", a rated match is "who won and did it mean
	 * anything". The server decides which one (`kind`); this page renders it.
	 */
	import { page } from '$app/stores';
	import Panel from '$lib/components/Panel.svelte';
	import PveScoreboard from '$lib/components/PveScoreboard.svelte';
	import PvpScoreboard from '$lib/components/PvpScoreboard.svelte';
	import { fetchMatchDetail, type MatchDetail } from '$lib/match';
	import { formatDateTime, formatDayYear } from '$lib/time';

	const instanceId = $derived($page.params.id ?? '');

	let detail = $state<MatchDetail | null>(null);
	let phase = $state<'loading' | 'ready' | 'error'>('loading');
	let problem = $state('');

	$effect(() => {
		const id = instanceId;
		let cancelled = false;
		phase = 'loading';

		fetchMatchDetail(id)
			.then((data) => {
				if (cancelled) return;
				detail = data;
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

	const duration = (seconds: number | null) => {
		if (seconds === null) return '—';
		const total = Math.max(0, Math.round(seconds));
		const hours = Math.floor(total / 3600);
		const minutes = Math.floor((total % 3600) / 60);
		const secs = total % 60;
		return hours > 0
			? `${hours}h ${String(minutes).padStart(2, '0')}m`
			: `${minutes}m ${String(secs).padStart(2, '0')}s`;
	};

	const outcomeLabel = (match: MatchDetail) => {
		if (match.outcome === 'STALEMATE') return 'Stalemate';
		if (match.winning_task_force === 1) return 'Attackers won';
		if (match.winning_task_force === 2) return 'Defenders won';
		if (match.outcome === 'ABANDONED') return 'Abandoned';
		return 'No result recorded';
	};

	const viewerBadge = (match: MatchDetail) => {
		const viewer = match.viewer;
		if (!viewer) return null;
		if (viewer.draw) return { text: 'You drew', tone: 'text-faint' };
		return viewer.won
			? { text: 'You won', tone: 'text-amber' }
			: { text: 'You lost', tone: 'text-dim' };
	};
</script>

<svelte:head>
	<Seo
		title={detail ? `${detail.map_label ?? detail.map_name} | Global Agenda mission | The Commonwealth` : 'Mission | The Commonwealth'}
		description={detail
			? `${detail.kind === 'pvp' ? 'PvP' : 'PvE'} mission on The Commonwealth${detail.queue_label ? ` (${detail.queue_label})` : ''}: ${detail.participants.length} agents, full scoreboard and per-player stats.`
			: 'Full scoreboard for one Global Agenda mission on The Commonwealth, including per-player stats and PvP ratings.'}
	/>
</svelte:head>

<p class="label">
	<a href="/missions" class="text-faint transition-colors hover:text-ember">← All matches</a>
</p>

{#if phase === 'loading'}
	<div class="mt-6 flex flex-col gap-3">
		<div class="h-8 w-2/3 animate-pulse bg-line/40"></div>
		<div class="h-4 w-1/3 animate-pulse bg-line/30"></div>
		<div class="mt-4 h-40 animate-pulse bg-line/20"></div>
	</div>
{:else if phase === 'error'}
	<Panel label="Match" class="mt-6">
		<p class="text-sm leading-relaxed text-dim">{problem}</p>
		<p class="mt-3">
			<a href="/missions" class="label text-ember transition-colors hover:text-amber"
				>Back to the archive →</a
			>
		</p>
	</Panel>
{:else if detail}
	<header class="mt-4 mb-8">
		<div class="flex flex-wrap items-center gap-3">
			<span
				class="label border border-line-hi px-2 py-1 {detail.kind === 'pvp'
					? 'text-ember'
					: 'text-steel'}">{detail.kind === 'pvp' ? 'PvP' : 'PvE'}</span
			>
			<span class="label text-faint">match #{detail.instance_id}</span>
			{#if viewerBadge(detail)}
				<span class="label {viewerBadge(detail)?.tone}">{viewerBadge(detail)?.text}</span>
			{/if}
		</div>

		<h1 class="heading mt-3 text-[clamp(1.9rem,4.6vw,3rem)]">
			{detail.map_label ?? detail.map_name}
		</h1>

		<p class="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-dim">
			<span class="text-ink">
				{detail.queue_label ?? detail.queue_code ?? 'Unknown queue'}{#if detail.queue_description &&
				detail.queue_label?.startsWith('Double Agent')}<span class="text-faint">
						— {detail.queue_description}</span
					>{/if}
			</span>
			{#if detail.queue_location}
				<span class="text-faint">{detail.queue_location}</span>
			{/if}
			<span class="text-faint">{formatDateTime(new Date(detail.started_at * 1000))}</span>
			<span class="text-faint">{duration(detail.duration_seconds)}</span>
			<span class="text-faint">{detail.participants.length} agents</span>
		</p>

		<div class="mt-5 border-l-2 border-ember/60 pl-3">
			<p class="heading text-lg tracking-[0.06em] text-ink">{outcomeLabel(detail)}</p>
			{#if detail.kind === 'pvp' && detail.balance}
				<p class="label mt-1 text-faint">
					rated on the {detail.mmr_engine === 'perf' ? 'performance' : 'win/loss'} engine
				</p>
			{/if}
		</div>
	</header>

	{#if detail.kind === 'pvp'}
		<PvpScoreboard
			participants={detail.participants}
			balance={detail.balance}
			winningTaskForce={detail.winning_task_force}
			mmrEngine={detail.mmr_engine}
		/>
	{:else}
		<PveScoreboard participants={detail.participants} teams={detail.teams} />
	{/if}

	<div class="mt-8 grid gap-4 md:grid-cols-2">
		<Panel label="Reading the table">
			<ul class="space-y-2 text-sm leading-relaxed text-dim">
				<li>
					<span class="text-ink">Agents</span> — one row per player. Someone who swapped class or
					side mid-match has their stats added up and appears under the class and side they spent
					the most time on.
				</li>
				{#if detail.kind === 'pvp'}
					<li>
						<span class="text-ink">MMR</span> — the rating each player carried into the match, not
						after it. The Δ column compares winners against losers per class, while the small
						number beside each rating in the matchups is what this match gained or lost.
					</li>
				{/if}
				<li>
					<span class="text-ink">Obj</span> — objective points. <span class="text-ink">Bots</span>
					— NPC kills, which is how most PvE missions are actually won.
				</li>
				<li class="text-faint">Times are your local 24-hour clock.</li>
			</ul>
		</Panel>

		<Panel label="Match data">
			<dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-sm">
				<dt class="label text-faint">Started</dt>
				<dd class="text-dim">{formatDayYear(new Date(detail.started_at * 1000))}</dd>
				<dt class="label text-faint">Mode</dt>
				<dd class="text-dim">{detail.game_mode}</dd>
				<dt class="label text-faint">Outcome</dt>
				<dd class="text-dim">{detail.outcome ?? '—'}</dd>
				<dt class="label text-faint">Archive id</dt>
				<dd class="text-dim tabular-nums">{detail.instance_id}</dd>
			</dl>
		</Panel>
	</div>
{/if}
