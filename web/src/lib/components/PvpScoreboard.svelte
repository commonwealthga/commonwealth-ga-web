<script lang="ts">
	/**
	 * PvP scoreboard.
	 *
	 * A rated match is read differently from a mission: what matters is who won,
	 * what each side was rated going in, and how the matchups lined up. So the
	 * table is split into winners and losers with pre-match MMR, and the rating
	 * context sits above it. Every per-player column the PvE archive shows is
	 * still here — nothing is dropped, the emphasis just moves.
	 */
	import type { Balance, Participant } from '$lib/match';

	let {
		participants,
		balance,
		winningTaskForce,
		mmrEngine
	}: {
		participants: Participant[];
		balance: Balance | null;
		winningTaskForce: number | null;
		mmrEngine: string | null;
	} = $props();

	const number = (value: number) =>
		Number.isInteger(value) ? value.toLocaleString('en-GB') : value.toFixed(1);
	const clock = (seconds: number) => {
		const total = Math.max(0, Math.round(seconds));
		return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
	};

	const COLUMNS: [keyof Participant, string][] = [
		['kills', 'K'],
		['deaths', 'D'],
		['assists', 'A'],
		['damage_dealt', 'Damage'],
		['damage_taken', 'Taken'],
		['healing', 'Healing'],
		['obj_points', 'Obj'],
		['bot_kills', 'Bots'],
		['beacons_destroyed', 'Beacons'],
		['time_played_seconds', 'Time']
	];

	const winner = $derived(
		participants.filter((p) => winningTaskForce !== null && p.task_force === winningTaskForce)
	);
	const loser = $derived(
		participants.filter((p) => winningTaskForce !== null && p.task_force !== winningTaskForce)
	);
	const unrated = $derived(winner.length === 0 && loser.length === 0);

	// The analyst's view: pair players by class, best-rated against best-rated,
	// so a lopsided class matchup is visible as a big delta.
	const CLASS_ORDER = ['Assault', 'Medic', 'Recon', 'Robotic'];
	const matchups = $derived.by(() => {
		if (winningTaskForce === null) return [];
		const byClass = (rows: Participant[]) => (cls: string) =>
			rows
				.filter((p) => (p.mmr_class ?? p.classes[0]) === cls)
				.sort((a, b) => (b.mmr ?? 0) - (a.mmr ?? 0));

		const winBy = byClass(winner);
		const loseBy = byClass(loser);
		const pairs: { cls: string; win: Participant | null; lose: Participant | null }[] = [];

		for (const cls of CLASS_ORDER) {
			const w = winBy(cls);
			const l = loseBy(cls);
			for (let i = 0; i < Math.max(w.length, l.length); i += 1) {
				pairs.push({ cls, win: w[i] ?? null, lose: l[i] ?? null });
			}
		}
		return pairs;
	});

	const engineLabel = (engine: string | null) =>
		engine === 'perf' ? 'performance engine' : engine === 'wl' ? 'win/loss engine' : 'rating engine';
</script>

<div class="flex flex-col gap-5">
	{#if balance}
		<div class="panel-ticks grid gap-3 border border-line/90 bg-panel/50 p-4 sm:grid-cols-3">
			<div class="flex flex-col gap-1">
				<span class="label text-faint">rating at start</span>
				<span class="text-sm text-dim">
					<span class="text-ember tabular-nums">{balance.wt_avg}</span> winners vs
					<span class="text-steel tabular-nums">{balance.lt_avg}</span> losers
				</span>
			</div>
			<div class="flex flex-col gap-1">
				<span class="label text-faint">difference</span>
				<span class="text-sm {balance.delta >= 0 ? 'text-ember' : 'text-steel'} tabular-nums">
					{balance.delta >= 0 ? '+' : ''}{balance.delta}
				</span>
			</div>
			<div class="flex flex-col gap-1">
				<span class="label text-faint">expected win chance</span>
				<span class="text-sm text-dim tabular-nums">
					{balance.delta >= 0 ? balance.wt_prob : (100 - balance.wt_prob).toFixed(1)}%
					<span class="text-faint">{balance.delta >= 0 ? 'winners' : 'losers'} favoured</span>
				</span>
			</div>
		</div>
	{/if}

	{#if !unrated}
		{#each [{ title: 'Winning side', rows: winner }, { title: 'Losing side', rows: loser }] as side (side.title)}
			{#if side.rows.length > 0}
				<div class="panel-ticks border border-line/90 bg-panel/50">
					<div class="flex items-center justify-between border-b border-line/80 px-4 py-2.5">
						<span class="label {side.title === 'Winning side' ? 'text-ember' : 'text-steel'}">
							{side.title}
						</span>
						<span class="label text-faint">{side.rows.length} agents</span>
					</div>
					<div class="overflow-x-auto">
						<table class="w-full min-w-[56rem] border-collapse text-sm">
							<thead>
								<tr class="label text-faint">
									<th class="px-4 py-2 text-left font-normal">Agent</th>
									<th class="px-2 py-2 text-left font-normal">Class</th>
									<th class="px-2 py-2 text-right font-normal">MMR</th>
									<th class="px-2 py-2 text-right font-normal">Δ</th>
									{#each COLUMNS as [key, label] (key)}
										<th class="px-2 py-2 text-right font-normal">{label}</th>
									{/each}
								</tr>
							</thead>
							<tbody>
								{#each side.rows as row (row.user_id)}
									<tr
										class="border-t border-line/50 {row.is_viewer
											? 'bg-ember/10 text-ink'
											: 'text-dim'}"
									>
										<td class="px-4 py-2 whitespace-nowrap">
											<a
												href="/agents/{row.user_id}"
												class="{row.is_viewer
													? 'text-amber'
													: 'text-ink'} transition-colors hover:text-ember hover:underline"
											>
												{row.player}
											</a>
											{#if row.is_viewer}
												<span class="label ml-2 text-amber">you</span>
											{/if}
										</td>
										<td class="px-2 py-2 whitespace-nowrap text-faint">
											{row.classes.join(', ') || '—'}
										</td>
										<td class="px-2 py-2 text-right tabular-nums">
											{row.mmr !== null ? row.mmr.toFixed(1) : '—'}
										</td>
										<td
											class="px-2 py-2 text-right tabular-nums {row.mmr_delta === null
												? 'text-faint'
												: row.mmr_delta > 0
													? 'text-ember'
													: row.mmr_delta < 0
														? 'text-steel'
														: 'text-faint'}"
										>
											{row.mmr_delta === null
												? '—'
												: `${row.mmr_delta > 0 ? '+' : ''}${row.mmr_delta.toFixed(1)}`}
										</td>
										{#each COLUMNS as [key, label] (key)}
											<td class="px-2 py-2 text-right tabular-nums">
												{key === 'time_played_seconds'
													? clock(row.time_played_seconds)
													: number(row[key] as number)}
											</td>
										{/each}
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				</div>
			{/if}
		{/each}
	{:else}
		<p class="label border border-line/80 bg-panel/40 px-4 py-6 text-faint">
			No scoreboard recorded for this match
		</p>
	{/if}

	{#if matchups.length > 0}
		<div class="panel-ticks border border-line/90 bg-panel/50">
			<div class="flex items-center justify-between border-b border-line/80 px-4 py-2.5">
				<span class="label text-dim">Class matchups</span>
				<span class="label text-faint">
					{engineLabel(mmrEngine)} · rating before, then the change
				</span>
			</div>
			<div class="overflow-x-auto">
				<table class="w-full min-w-[40rem] border-collapse text-sm">
					<thead>
						<tr class="label text-faint">
							<th class="px-4 py-2 text-left font-normal">Winners</th>
							<th class="px-2 py-2 text-right font-normal">MMR</th>
							<th class="px-2 py-2 text-center font-normal">Δ</th>
							<th class="px-2 py-2 text-right font-normal">MMR</th>
							<th class="px-4 py-2 text-right font-normal">Losers</th>
						</tr>
					</thead>
					<tbody>
						{#each matchups as pair (pair.cls + (pair.win?.user_id ?? 0) + '-' + (pair.lose?.user_id ?? 0))}
							{@const mine = pair.win?.is_viewer === true || pair.lose?.is_viewer === true}
							<tr class="border-t border-line/50 {mine ? 'bg-ember/10' : ''}">
								<td class="px-4 py-2 whitespace-nowrap text-dim">
									{#if pair.win}
										<a
											href="/agents/{pair.win.user_id}"
											class="{pair.win.is_viewer
												? 'text-amber'
												: 'text-ink'} transition-colors hover:text-ember hover:underline"
										>
											{pair.win.player}
										</a>
										{#if pair.win.is_viewer}
											<span class="label ml-1.5 text-amber">you</span>
										{/if}
										<span class="label ml-2 text-faint">{pair.cls}</span>
									{:else}
										<span class="text-faint">—</span>
									{/if}
								</td>
								<td class="px-2 py-2 text-right tabular-nums text-dim">
									{pair.win?.mmr?.toFixed(1) ?? '—'}
									{#if pair.win?.mmr_delta !== null && pair.win?.mmr_delta !== undefined}
										<span
											class="ml-1.5 text-xs {pair.win.mmr_delta > 0
												? 'text-ember'
												: pair.win.mmr_delta < 0
													? 'text-steel'
													: 'text-faint'}"
										>
											{pair.win.mmr_delta > 0 ? '+' : ''}{pair.win.mmr_delta.toFixed(1)}
										</span>
									{/if}
								</td>
								<td
									class="px-2 py-2 text-center tabular-nums {pair.win && pair.lose
										? (pair.win.mmr ?? 0) - (pair.lose.mmr ?? 0) >= 0
											? 'text-ember'
											: 'text-steel'
										: 'text-faint'}"
								>
									{#if pair.win && pair.lose}
										{((pair.win.mmr ?? 0) - (pair.lose.mmr ?? 0)).toFixed(1)}
									{:else}
										no match
									{/if}
								</td>
								<td class="px-2 py-2 text-right tabular-nums text-dim">
									{#if pair.lose?.mmr_delta !== null && pair.lose?.mmr_delta !== undefined}
										<span
											class="mr-1.5 text-xs {pair.lose.mmr_delta > 0
												? 'text-ember'
												: pair.lose.mmr_delta < 0
													? 'text-steel'
													: 'text-faint'}"
										>
											{pair.lose.mmr_delta > 0 ? '+' : ''}{pair.lose.mmr_delta.toFixed(1)}
										</span>
									{/if}
									{pair.lose?.mmr?.toFixed(1) ?? '—'}
								</td>
								<td class="px-4 py-2 text-right whitespace-nowrap text-dim">
									{#if pair.lose}
										<span class="text-faint">{pair.cls}</span>
										<span class="ml-2">
											<a
												href="/agents/{pair.lose.user_id}"
												class="{pair.lose.is_viewer
													? 'text-amber'
													: 'text-ink'} transition-colors hover:text-ember hover:underline"
											>
												{pair.lose.player}
											</a>
										</span>
										{#if pair.lose.is_viewer}
											<span class="label ml-1.5 text-amber">you</span>
										{/if}
									{:else}
										<span class="text-faint">—</span>
									{/if}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		</div>
	{/if}
</div>
