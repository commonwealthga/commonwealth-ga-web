<script lang="ts">
	/**
	 * A player's rating over time, per class.
	 *
	 * Same interaction as the activity chart on the home page — drag across the
	 * plot and release to zoom, presets for common windows, reset to the whole
	 * range — because it is the same gesture on the same site.
	 *
	 * The y-axis follows the data rather than starting at zero: an MMR line
	 * that moves 60 points is the interesting thing, and pinning it to zero
	 * would flatten every match into one line.
	 */
	import { formatDateTime, formatDay, formatDayYear } from '$lib/time';

	const RANGES = [
		{ label: '7d', days: 7 },
		{ label: '30d', days: 30 },
		{ label: '90d', days: 90 },
		{ label: 'All', days: 0 }
	];

	const WIDTH = 1000;
	const HEIGHT = 220;
	const PAD_TOP = 14;
	const PAD_BOTTOM = 26;
	const PAD_LEFT = 6;
	const PAD_RIGHT = 6;

	let { userId, class_name }: { userId: number | string; class_name: string | null } = $props();

	// Which rating engine produced these numbers; the history endpoint knows.
	let engine = $state<string | null>(null);

	let days = $state(30);
	let allTimes = $state<number[]>([]);
	let allMmr = $state<number[]>([]);
	let allInstances = $state<number[]>([]);
	let phase = $state<'loading' | 'ready' | 'error'>('loading');
	let problem = $state('');

	// Zoom window in unix seconds; null means the whole loaded range.
	let zoom = $state<{ from: number; to: number } | null>(null);
	let dragStart = $state<number | null>(null);
	let dragEnd = $state<number | null>(null);
	let hoverIndex = $state<number | null>(null);

	function resetZoom() {
		zoom = null;
		dragStart = null;
		dragEnd = null;
	}

	$effect(() => {
		const requestedDays = days;
		const className = class_name;
		const id = userId;
		let cancelled = false;
		phase = 'loading';

		const params = new URLSearchParams({ days: String(requestedDays) });
		if (className) params.set('class', className);

		fetch(`/api/agents/${id}/history?${params.toString()}`, {
			headers: { accept: 'application/json' }
		})
			.then(async (response) => {
				if (!response.ok) throw new Error(`unavailable (${response.status})`);
				return (await response.json()) as {
					times: number[];
					mmr: number[];
					instances: number[];
					engine: string;
				};
			})
			.then((body) => {
				if (cancelled) return;
				allTimes = body.times ?? [];
				allMmr = body.mmr ?? [];
				allInstances = body.instances ?? [];
				engine = body.engine ?? null;
				zoom = null;
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

	// The visible slice: everything, or the zoom window.
	const view = $derived.by(() => {
		const times: number[] = [];
		const mmr: number[] = [];
		const instances: number[] = [];
		for (let i = 0; i < allTimes.length; i += 1) {
			const at = allTimes[i];
			if (zoom && (at < zoom.from || at > zoom.to)) continue;
			times.push(at);
			mmr.push(allMmr[i]);
			instances.push(allInstances[i]);
		}
		return { times, mmr, instances };
	});

	const bounds = $derived.by(() => {
		const { times, mmr } = view;
		if (times.length === 0) return null;
		const minMmr = Math.min(...mmr);
		const maxMmr = Math.max(...mmr);
		// A flat line still needs a band to sit in.
		const pad = Math.max(10, (maxMmr - minMmr) * 0.12);
		return {
			from: times[0],
			to: times[times.length - 1],
			low: minMmr - pad,
			high: maxMmr + pad
		};
	});

	function xAt(time: number): number {
		if (!bounds) return PAD_LEFT;
		const span = Math.max(1, bounds.to - bounds.from);
		return PAD_LEFT + ((time - bounds.from) / span) * (WIDTH - PAD_LEFT - PAD_RIGHT);
	}

	function yAt(mmr: number): number {
		if (!bounds) return HEIGHT - PAD_BOTTOM;
		const span = Math.max(1, bounds.high - bounds.low);
		const ratio = (mmr - bounds.low) / span;
		return PAD_TOP + (1 - ratio) * (HEIGHT - PAD_TOP - PAD_BOTTOM);
	}

	const line = $derived.by(() => {
		const { times, mmr } = view;
		if (times.length === 0) return '';
		return times
			.map((at, i) => `${i === 0 ? 'M' : 'L'} ${xAt(at).toFixed(1)} ${yAt(mmr[i]).toFixed(1)}`)
			.join(' ');
	});

	const hover = $derived.by(() => {
		if (hoverIndex === null) return null;
		const at = view.times[hoverIndex];
		const value = view.mmr[hoverIndex];
		if (at === undefined || value === undefined) return null;
		return {
			at,
			value,
			instance: view.instances[hoverIndex],
			x: xAt(at),
			y: yAt(value)
		};
	});

	const change = $derived.by(() => {
		const { mmr } = view;
		if (mmr.length < 2) return null;
		const delta = mmr[mmr.length - 1] - mmr[0];
		return { delta, from: mmr[0], to: mmr[mmr.length - 1] };
	});

	function timeAtPixel(x: number): number {
		if (!bounds) return 0;
		const ratio = (x - PAD_LEFT) / (WIDTH - PAD_LEFT - PAD_RIGHT);
		return bounds.from + ratio * (bounds.to - bounds.from);
	}

	function onPointerDown(event: PointerEvent) {
		if (phase !== 'ready' || view.times.length < 2) return;
		const box = (event.currentTarget as SVGElement).getBoundingClientRect();
		dragStart = timeAtPixel(((event.clientX - box.left) / box.width) * WIDTH);
		dragEnd = dragStart;
	}

	function onPointerMove(event: PointerEvent) {
		if (phase !== 'ready' || view.times.length === 0) return;
		const box = (event.currentTarget as SVGElement).getBoundingClientRect();
		const px = ((event.clientX - box.left) / box.width) * WIDTH;

		if (dragStart !== null) {
			dragEnd = timeAtPixel(px);
			return;
		}

		// Nearest point, for the readout.
		let best = 0;
		let bestDistance = Number.POSITIVE_INFINITY;
		for (let i = 0; i < view.times.length; i += 1) {
			const distance = Math.abs(xAt(view.times[i]) - px);
			if (distance < bestDistance) {
				bestDistance = distance;
				best = i;
			}
		}
		hoverIndex = bestDistance < 40 ? best : null;
	}

	function onPointerUp() {
		if (dragStart === null || dragEnd === null) {
			dragStart = null;
			dragEnd = null;
			return;
		}
		const from = Math.min(dragStart, dragEnd);
		const to = Math.max(dragStart, dragEnd);
		dragStart = null;
		dragEnd = null;
		// A click, not a drag: ignore it rather than zooming to nothing.
		if (to - from < 3600) return;
		zoom = { from, to };
	}

	const band = $derived.by(() => {
		if (dragStart === null || dragEnd === null) return null;
		const from = xAt(Math.min(dragStart, dragEnd));
		const to = xAt(Math.max(dragStart, dragEnd));
		return { x: from, width: Math.max(1, to - from) };
	});

	const engineLabel = (code: string | null) =>
		code === 'perf' ? 'performance engine' : 'win/loss engine';
</script>

<div class="panel-ticks border border-line/90 bg-panel/40">
	<div class="flex flex-wrap items-center justify-between gap-3 border-b border-line/80 px-4 py-2.5">
		<span class="label text-dim">
			{class_name ?? 'All classes'} rating{#if engine}
				<span class="text-faint">· {engineLabel(engine)}</span>
			{/if}
		</span>
		<div class="flex items-center gap-1">
			{#each RANGES as range (range.label)}
				<button
					type="button"
					class="label border px-2 py-1 transition-colors {days === range.days
						? 'border-ember/60 text-ember'
						: 'border-transparent text-faint hover:text-dim'}"
					onclick={() => {
						days = range.days;
					}}
				>
					{range.label}
				</button>
			{/each}
			{#if zoom}
				<button
					type="button"
					class="label ml-2 border border-line-hi px-2 py-1 text-dim transition-colors hover:text-ink"
					onclick={resetZoom}>reset</button
				>
			{/if}
		</div>
	</div>

	{#if phase === 'loading'}
		<div class="h-[220px] animate-pulse bg-line/10"></div>
	{:else if phase === 'error'}
		<p class="label px-4 py-8 text-faint">rating history unavailable — {problem}</p>
	{:else if view.times.length === 0}
		<p class="label px-4 py-8 text-faint">
			{zoom ? 'no rated matches in that span' : 'no rated matches yet'}
		</p>
	{:else}
		<div class="relative">
			<svg
				viewBox="0 0 {WIDTH} {HEIGHT}"
				class="block w-full cursor-crosshair touch-none select-none"
				role="img"
				aria-label="Rating history for {class_name ?? 'all classes'}"
				onpointerdown={onPointerDown}
				onpointermove={onPointerMove}
				onpointerup={onPointerUp}
				onpointerleave={() => {
					hoverIndex = null;
					dragStart = null;
					dragEnd = null;
				}}
			>
				<!-- Horizontal guides at the visible extremes. -->
				{#if bounds}
					<line
						x1={PAD_LEFT}
						x2={WIDTH - PAD_RIGHT}
						y1={yAt(bounds.high)}
						y2={yAt(bounds.high)}
						stroke="var(--color-line)"
						stroke-dasharray="2 4"
					/>
					<line
						x1={PAD_LEFT}
						x2={WIDTH - PAD_RIGHT}
						y1={yAt(bounds.low)}
						y2={yAt(bounds.low)}
						stroke="var(--color-line)"
						stroke-dasharray="2 4"
					/>
				{/if}

				{#if band}
					<rect
						x={band.x}
						y={PAD_TOP}
						width={band.width}
						height={HEIGHT - PAD_TOP - PAD_BOTTOM}
						fill="var(--color-ember)"
						opacity="0.12"
					/>
				{/if}

				<path d={line} fill="none" stroke="var(--color-ember)" stroke-width="1.6" />

				{#if hover}
					<line
						x1={hover.x}
						x2={hover.x}
						y1={PAD_TOP}
						y2={HEIGHT - PAD_BOTTOM}
						stroke="var(--color-line-hi)"
					/>
					<circle cx={hover.x} cy={hover.y} r="3.4" fill="var(--color-amber)" />
				{/if}
			</svg>

			{#if hover}
				<div
					class="pointer-events-none absolute top-1 border border-line-hi bg-carbon/95 px-2 py-1"
					style="left:{Math.min(88, Math.max(2, (hover.x / WIDTH) * 100))}%"
				>
					<span class="label block text-amber">{hover.value.toFixed(1)}</span>
					<span class="label block text-faint">{formatDateTime(new Date(hover.at * 1000))}</span>
					{#if hover.instance}
						<a
							class="label mt-1 block text-dim underline decoration-line-hi hover:text-ember"
							href="/missions/{hover.instance}">match #{hover.instance}</a
						>
					{/if}
				</div>
			{/if}
		</div>

		<div class="flex flex-wrap items-center justify-between gap-2 border-t border-line/70 px-4 py-2">
			<span class="label text-faint">
				{formatDayYear(new Date(view.times[0] * 1000))} →
				{formatDay(new Date(view.times[view.times.length - 1] * 1000))}
			</span>
			<span class="label text-faint">
				{#if change}
					<span class={change.delta >= 0 ? 'text-amber' : 'text-dim'}>
						{change.delta >= 0 ? '+' : ''}{change.delta.toFixed(1)}
					</span>
					over {view.times.length} rated {view.times.length === 1 ? 'match' : 'matches'}
				{:else}
					{view.times.length} rated {view.times.length === 1 ? 'match' : 'matches'}
				{/if}
			</span>
		</div>
		<p class="label border-t border-line/70 px-4 py-1.5 text-faint">
			drag across the plot to zoom · release outside a match to reset
		</p>
	{/if}
</div>
