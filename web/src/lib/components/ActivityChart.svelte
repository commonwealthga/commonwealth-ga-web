<script lang="ts">
	/**
	 * Hourly peak concurrent players.
	 *
	 * No chart library: two parallel arrays, a path and a few labels.
	 *
	 * Interaction: drag across the plot and release to zoom into that period.
	 * The preset buttons are just shortcuts into the same state, and "Reset"
	 * returns to the preset's full window. Zooming keeps the loaded series in
	 * memory, so it never re-fetches.
	 */
	import { clockFromUnix, formatDateTime, formatDay, formatDayYear } from '$lib/time';

	const RANGES = [
		{ label: '24h', days: 1 },
		{ label: '7d', days: 7 },
		{ label: '30d', days: 30 },
		{ label: '90d', days: 90 },
		{ label: 'All', days: 0 }
	];

	const WIDTH = 1000;
	const HEIGHT = 220;
	const PAD_TOP = 12;
	const PAD_BOTTOM = 26;

	let days = $state(30);
	let allTimes = $state<number[]>([]);
	let allPlayers = $state<number[]>([]);
	let phase = $state<'loading' | 'ready' | 'error'>('loading');
	let problem = $state('');

	// Zoom window in unix seconds. `null` means "the whole preset range".
	let zoom = $state<{ from: number; to: number } | null>(null);

	// Drag rubber band, as indices into the visible series.
	let dragStart = $state<number | null>(null);
	let dragEnd = $state<number | null>(null);
	let hoverIndex = $state<number | null>(null);

	function resetZoom() {
		zoom = null;
		dragStart = null;
		dragEnd = null;
	}

	$effect(() => {
		const requested = days;
		let cancelled = false;
		phase = 'loading';

		fetch(`/api/online-history?days=${requested}`, { headers: { accept: 'application/json' } })
			.then(async (response) => {
				if (!response.ok) throw new Error(`unavailable (${response.status})`);
				return (await response.json()) as { times: number[]; players: number[] };
			})
			.then((body) => {
				if (cancelled) return;
				allTimes = body.times ?? [];
				allPlayers = body.players ?? [];
				resetZoom();
				hoverIndex = null;
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

	/** The slice currently drawn: the preset range, narrowed by any zoom. */
	let view = $derived.by(() => {
		if (!zoom || allTimes.length === 0) {
			return { times: allTimes, players: allPlayers };
		}

		const { from, to } = zoom;
		const start = allTimes.findIndex((t) => t >= from);
		let end = -1;
		for (let i = allTimes.length - 1; i >= 0; i -= 1) {
			if (allTimes[i] <= to) {
				end = i;
				break;
			}
		}
		// Degenerate selection (or one that misses the data): keep the full range.
		if (start < 0 || end - start < 1) {
			return { times: allTimes, players: allPlayers };
		}
		return { times: allTimes.slice(start, end + 1), players: allPlayers.slice(start, end + 1) };
	});

	let times = $derived(view.times);
	let players = $derived(view.players);

	let peak = $derived(Math.max(1, ...players));
	let ceiling = $derived(
		[1, 2, 5, 10, 20, 25, 50, 75, 100, 150, 200].find((step) => step >= peak) ?? peak
	);
	let average = $derived(
		players.length === 0 ? 0 : players.reduce((sum, value) => sum + value, 0) / players.length
	);

	let xAt = $derived((index: number) =>
		times.length <= 1 ? 0 : (index / (times.length - 1)) * WIDTH
	);
	let yAt = $derived((value: number) => {
		const usable = HEIGHT - PAD_TOP - PAD_BOTTOM;
		return PAD_TOP + usable - (value / ceiling) * usable;
	});

	let linePath = $derived(
		players
			.map(
				(value, index) =>
					`${index === 0 ? 'M' : 'L'}${xAt(index).toFixed(1)},${yAt(value).toFixed(1)}`
			)
			.join(' ')
	);
	let areaPath = $derived(
		players.length === 0 ? '' : `${linePath} L${WIDTH},${yAt(0)} L0,${yAt(0)} Z`
	);

	// Axis labels follow the visible span: hours, dates, or dates with years.
	let spanHours = $derived(times.length);
	let labelFor = $derived((unix: number) =>
		spanHours <= 36
			? clockFromUnix(unix)
			: spanHours <= 24 * 120
				? formatDay(new Date(unix * 1000))
				: formatDayYear(new Date(unix * 1000))
	);
	let labelIndices = $derived(
		times.length < 2
			? []
			: [0, 1, 2, 3, 4].map((step) =>
					Math.min(times.length - 1, Math.round((step / 4) * (times.length - 1)))
				)
	);

	function indexFromClientX(clientX: number, element: SVGSVGElement) {
		const rect = element.getBoundingClientRect();
		const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
		return Math.round(ratio * Math.max(0, times.length - 1));
	}

	function onPointerDown(event: PointerEvent & { currentTarget: SVGSVGElement }) {
		if (times.length < 2) return;
		event.currentTarget.setPointerCapture(event.pointerId);
		const index = indexFromClientX(event.clientX, event.currentTarget);
		dragStart = index;
		dragEnd = index;
		hoverIndex = null;
	}

	function onPointerMove(event: PointerEvent & { currentTarget: SVGSVGElement }) {
		if (times.length === 0) return;
		const index = indexFromClientX(event.clientX, event.currentTarget);
		if (dragStart !== null) {
			dragEnd = index;
		} else {
			hoverIndex = index;
		}
	}

	function onPointerUp(event: PointerEvent & { currentTarget: SVGSVGElement }) {
		if (event.currentTarget.hasPointerCapture(event.pointerId)) {
			event.currentTarget.releasePointerCapture(event.pointerId);
		}
		if (dragStart === null || dragEnd === null) return;

		const from = Math.min(dragStart, dragEnd);
		const to = Math.max(dragStart, dragEnd);
		dragStart = null;
		dragEnd = null;

		// A click (no drag) is a zoom-out request when something is zoomed.
		if (to - from < 1) {
			if (zoom) resetZoom();
			return;
		}

		const previous = zoom;
		zoom = { from: times[from], to: times[to] };

		// Guard against a selection that maps to nothing (e.g. rounding at the edge).
		if (view.times.length < 2) zoom = previous;
	}

	const dragging = $derived(dragStart !== null && dragEnd !== null);
	const bandFrom = $derived(dragging ? Math.min(dragStart!, dragEnd!) : 0);
	const bandTo = $derived(dragging ? Math.max(dragStart!, dragEnd!) : 0);
</script>

<div class="flex flex-col gap-4">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<div class="flex items-baseline gap-3">
			<span class="heading text-2xl tabular-nums text-ink">
				{#if phase === 'ready'}{peak}{:else}—{/if}
			</span>
			<span class="label text-faint">
				peak concurrent{#if zoom}<span class="text-amber"> · in view</span>{/if}
			</span>
		</div>

		<div class="flex gap-1">
			{#each RANGES as range (range.label)}
				<button
					type="button"
					onclick={() => {
						days = range.days;
						resetZoom();
					}}
					class="label border px-2.5 py-1.5 transition-colors {days === range.days
						? 'border-ember/60 bg-ember/10 text-ember'
						: 'border-line/80 text-faint hover:text-dim'}"
				>
					{range.label}
				</button>
			{/each}
		</div>
	</div>

	{#if phase === 'error'}
		<p class="label py-8 text-red">activity data unavailable — {problem}</p>
	{:else if phase === 'loading' && times.length === 0}
		<div class="flex h-[220px] items-end gap-1">
			{#each Array(48) as _, i (i)}
				<span class="flex-1 animate-pulse bg-line/50" style="height:{20 + ((i * 37) % 60)}%"></span>
			{/each}
		</div>
	{:else if times.length === 0}
		<p class="label py-8 text-faint">no activity recorded in this range yet</p>
	{:else}
		<div class="relative">
			<svg
				viewBox="0 0 {WIDTH} {HEIGHT}"
				class="w-full cursor-zoom-in touch-none"
				role="img"
				aria-label="Peak concurrent players per hour. Drag across the chart to zoom into a period."
				onpointerdown={onPointerDown}
				onpointermove={onPointerMove}
				onpointerup={onPointerUp}
				onpointerleave={() => (hoverIndex = null)}
			>
				<defs>
					<linearGradient id="activity-fill" x1="0" y1="0" x2="0" y2="1">
						<stop offset="0%" stop-color="var(--color-ember)" stop-opacity="0.35" />
						<stop offset="100%" stop-color="var(--color-ember)" stop-opacity="0.02" />
					</linearGradient>
				</defs>

				<!-- Horizontal guides -->
				{#each [0, 0.5, 1] as fraction (fraction)}
					<line
						x1="0"
						x2={WIDTH}
						y1={yAt(ceiling * fraction)}
						y2={yAt(ceiling * fraction)}
						stroke="var(--color-line)"
						stroke-width="1"
						vector-effect="non-scaling-stroke"
					/>
				{/each}

				<path d={areaPath} fill="url(#activity-fill)" />
				<path
					d={linePath}
					fill="none"
					stroke="var(--color-ember)"
					stroke-width="1.6"
					stroke-linejoin="round"
					vector-effect="non-scaling-stroke"
				/>

				{#if dragging && bandTo > bandFrom}
					<!-- Rubber band: what will become the new view on release. -->
					<rect
						x={xAt(bandFrom)}
						y={PAD_TOP}
						width={Math.max(1, xAt(bandTo) - xAt(bandFrom))}
						height={yAt(0) - PAD_TOP}
						fill="var(--color-amber)"
						fill-opacity="0.12"
						stroke="var(--color-amber)"
						stroke-opacity="0.6"
						stroke-width="1"
						vector-effect="non-scaling-stroke"
					/>
				{:else if hoverIndex !== null && times[hoverIndex] !== undefined}
					<line
						x1={xAt(hoverIndex)}
						x2={xAt(hoverIndex)}
						y1={PAD_TOP}
						y2={yAt(0)}
						stroke="var(--color-amber)"
						stroke-width="1"
						stroke-dasharray="3 3"
						vector-effect="non-scaling-stroke"
					/>
					<circle
						cx={xAt(hoverIndex)}
						cy={yAt(players[hoverIndex] ?? 0)}
						r="3"
						fill="var(--color-amber)"
					/>
				{/if}
			</svg>

			{#if hoverIndex !== null && times[hoverIndex] !== undefined && !dragging}
				<div
					class="pointer-events-none absolute top-0 z-10 border border-line-hi bg-void/95 px-2.5 py-1.5 whitespace-nowrap"
					style="left:{Math.min(85, Math.max(0, (hoverIndex / Math.max(1, times.length - 1)) * 100))}%"
				>
					<div class="label text-faint">{formatDateTime(new Date(times[hoverIndex] * 1000))}</div>
					<div class="text-sm text-ink">{players[hoverIndex]} online</div>
				</div>
			{/if}

			<div class="mt-1 flex justify-between">
				{#each labelIndices as index (index)}
					<span class="label text-faint">{labelFor(times[index])}</span>
				{/each}
			</div>
		</div>

		<!-- Zoom bar: what you are looking at, and the way back out. -->
		<div
			class="flex flex-wrap items-center gap-x-5 gap-y-1 border border-line/70 bg-panel/40 px-3 py-2"
		>
			{#if zoom}
				<span class="label text-amber">zoomed</span>
				<span class="label text-dim">
					{formatDateTime(new Date(times[0] * 1000))} → {formatDateTime(
						new Date(times[times.length - 1] * 1000)
					)}
				</span>
				<span class="label text-faint tabular-nums">{times.length}h</span>
				<span class="label text-dim tabular-nums">avg {average.toFixed(1)}</span>
				<button
					type="button"
					onclick={resetZoom}
					class="label ml-auto border border-line-hi px-2.5 py-1 text-dim transition-colors hover:border-ember/60 hover:text-ink"
					>Reset zoom</button
				>
			{:else}
				<span class="label text-faint">
					drag across the chart to zoom into a period · click to zoom back out
				</span>
			{/if}
		</div>
	{/if}

	<p class="text-xs leading-relaxed text-faint">
		Each point is the busiest moment of that hour, reconstructed from login sessions and match
		activity. Times are your local 24-hour clock.
	</p>
</div>
