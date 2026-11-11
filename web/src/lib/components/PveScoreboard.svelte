<script lang="ts">
	/**
	 * PvE scoreboard.
	 *
	 * A mission has no rating to argue about, so the table is the whole story:
	 * who was there, on which side, and what they did to the map. Columns follow
	 * the previous site's archive so old screenshots still line up.
	 */
	import type { Participant, TeamSummary } from '$lib/match';

	let {
		participants,
		teams
	}: { participants: Participant[]; teams: TeamSummary[] } = $props();

	const number = (value: number) =>
		Number.isInteger(value) ? value.toLocaleString('en-GB') : value.toFixed(1);
	const clock = (seconds: number) => {
		const total = Math.max(0, Math.round(seconds));
		return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
	};

	// Server order is attackers, defenders, then unknown sides. Group on that.
	const sides = $derived.by(() => {
		const known = teams.map((team) => ({
			key: team.task_force,
			label: team.label,
			won: team.won,
			rows: participants.filter((p) => p.task_force === team.task_force)
		}));
		const loose = participants.filter((p) => p.task_force === null || p.task_force === undefined);
		if (loose.length > 0) {
			known.push({ key: 0, label: 'Unassigned', won: false, rows: loose });
		}
		return known;
	});

	const COLUMNS: [keyof Participant, string][] = [
		['kills', 'K'],
		['deaths', 'D'],
		['assists', 'A'],
		['bot_kills', 'Bots'],
		['damage_dealt', 'Damage'],
		['damage_taken', 'Taken'],
		['healing', 'Healing'],
		['obj_points', 'Obj'],
		['beacons_destroyed', 'Beacons'],
		['time_played_seconds', 'Time']
	];
</script>

<div class="flex flex-col gap-5">
	{#each sides as side (side.key)}
		<div class="panel-ticks border border-line/90 bg-panel/50">
			<div class="flex items-center justify-between border-b border-line/80 px-4 py-2.5">
				<span class="label text-dim">{side.label}</span>
				<span class="flex items-center gap-3">
					<span class="label text-faint">{side.rows.length} agents</span>
					{#if side.won}
						<span class="label text-ember">carried the mission</span>
					{/if}
				</span>
			</div>

			<div class="overflow-x-auto">
				<table class="w-full min-w-[46rem] border-collapse text-sm">
					<thead>
						<tr class="label text-faint">
							<th class="px-4 py-2 text-left font-normal">Agent</th>
							<th class="px-2 py-2 text-left font-normal">Class</th>
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
	{/each}
</div>
