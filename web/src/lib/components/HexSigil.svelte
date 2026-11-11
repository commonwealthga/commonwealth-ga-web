<script lang="ts">
	import { untrack } from 'svelte';
	import { fetchActivity, windowLabel, type Activity } from '$lib/activity';
	import { countUp } from '$lib/countup';

	// Not the live count — that is a different question, and it is answerable in
	// the header chip and the "Right now" panel. This is "has anything been
	// happening here", which is what a visitor deciding whether to install
	// actually wants to know.
	let activity = $state<Activity | null>(null);
	let failed = $state(false);

	$effect(() => {
		fetchActivity()
			.then((body) => (activity = body))
			.catch(() => (failed = true));
	});

	const agents = $derived(activity?.agents ?? 0);
	const missions = $derived(activity?.missions ?? 0);

	// Shown numerals tick up from zero so the figures read as freshly measured.
	let agentsShown = $state(0);
	let missionsShown = $state(0);

	$effect(() => {
		const body = activity;
		if (!body) return;
		// `untrack` keeps the frame writes below from re-running this effect.
		const fromAgents = untrack(() => agentsShown);
		const fromMissions = untrack(() => missionsShown);
		const stopAgents = countUp(fromAgents, body.agents, (value) => (agentsShown = value));
		const stopMissions = countUp(fromMissions, body.missions, (value) => (missionsShown = value));
		return () => {
			stopAgents();
			stopMissions();
		};
	});
	const span = $derived(activity ? windowLabel(activity.window_hours) : '');

	const hex = (r: number) =>
		Array.from({ length: 6 }, (_, i) => {
			const a = (Math.PI / 180) * (60 * i - 90);
			return `${(160 + r * Math.cos(a)).toFixed(1)},${(160 + r * Math.sin(a)).toFixed(1)}`;
		}).join(' ');

	const vertices = (r: number) =>
		Array.from({ length: 6 }, (_, i) => {
			const a = (Math.PI / 180) * (60 * i - 90);
			return { x: 160 + r * Math.cos(a), y: 160 + r * Math.sin(a), i };
		});
</script>

<!-- The hero sigil doubles as the live readout: a hex cell, like one tile of
     the game's hex-grid campaign map, reporting the population it holds. -->
<div class="relative mx-auto aspect-square w-full max-w-[360px] select-none">
	<svg viewBox="0 0 320 320" class="absolute inset-0 size-full" aria-hidden="true">
		<defs>
			<radialGradient id="sigil-fill" cx="50%" cy="50%" r="62%">
				<stop offset="0%" stop-color="var(--color-red)" stop-opacity="0.3" />
				<stop offset="100%" stop-color="var(--color-red)" stop-opacity="0" />
			</radialGradient>
		</defs>

		<!-- Outer survey ring -->
		<polygon
			points={hex(150)}
			fill="none"
			stroke="var(--color-ember)"
			stroke-opacity="0.28"
			stroke-width="1.25"
			stroke-dasharray="5 9"
			class="origin-center animate-sweep [animation-duration:70s]"
		/>
		<!-- Second ring, opposite direction -->
		<polygon
			points={hex(136)}
			fill="none"
			stroke="var(--color-red)"
			stroke-opacity="0.38"
			stroke-width="1"
			stroke-dasharray="2 16"
			class="origin-center animate-sweep [animation-direction:reverse] [animation-duration:45s]"
		/>

		<!-- Cell body -->
		<polygon
			points={hex(118)}
			fill="url(#sigil-fill)"
			stroke="var(--color-ember)"
			stroke-opacity="0.55"
			stroke-width="1.5"
		/>
		<polygon points={hex(84)} fill="none" stroke="var(--color-red)" stroke-opacity="0.3" stroke-width="1" />

		<!-- Vertex markers -->
		{#each vertices(118) as v (v.i)}
			<circle
				cx={v.x}
				cy={v.y}
				r="2.5"
				fill="var(--color-ember)"
				class="animate-hex-pulse"
				style="animation-delay:{v.i * 420}ms"
			/>
		{/each}
	</svg>

	<div class="absolute inset-0 grid place-items-center px-10">
		<div class="flex flex-col items-center text-center">
			{#if failed}
				<div class="flex flex-col items-center">
					<span class="heading text-4xl leading-none text-faint">—</span>
					<span class="label mt-2 text-dim">activity unavailable</span>
				</div>
			{:else if activity === null}
				<span class="heading text-[4rem] leading-none text-faint/40">···</span>
			{:else}
				<!-- Window on top, facts in the middle, "played" underneath.
				     Everything that is not a numeral uses the brighter of the two
				     secondary tones: it is a dark cell, and `faint` disappears in
				     it. Equal grid columns keep the divider on the cell's axis. -->
				<div class="flex flex-col items-center gap-y-4">
					<span class="label text-dim">{span}</span>

					<div class="relative grid grid-cols-2 items-start gap-x-5">
						<span
							class="absolute inset-y-1 left-1/2 w-px -translate-x-1/2"
							style="background:var(--color-line-hi)"
						></span>

						<div class="flex flex-col items-center">
							<span class="heading text-[3rem] leading-none tabular-nums ember-text">{agentsShown}</span>
							<span class="label mt-2.5 text-dim">{agents === 1 ? 'agent' : 'agents'}</span>
						</div>

						<div class="flex flex-col items-center">
							<span class="heading text-[3rem] leading-none tabular-nums text-ink">{missionsShown}</span>
							<span class="label mt-2.5 text-dim">{missions === 1 ? 'mission' : 'missions'}</span>
						</div>
					</div>

					<span class="label text-dim">played</span>
				</div>
			{/if}
		</div>
	</div>
</div>
