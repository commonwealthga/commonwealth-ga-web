<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	import { reveal } from '$lib/reveal';
	import Panel from '$lib/components/Panel.svelte';
	import StatusReadout from '$lib/components/StatusReadout.svelte';
	import ActivityChart from '$lib/components/ActivityChart.svelte';
	import { untrack } from 'svelte';
	import { subscribeStatus, type StatusPayload } from '$lib/live';

	let { data } = $props();

	interface Health {
		ok: boolean;
		live_db: boolean;
		snapshot_db: boolean;
		site_db: boolean;
	}

	// Seeded from the server so the count is in the HTML; the stream then owns it.
	let status = $state<StatusPayload | null>(
		untrack(() =>
			data.count.players === null
				? null
				: {
						ok: true,
						players: data.count.players,
						text: data.count.text,
						control_server: null,
						ts: data.count.ts
					}
		)
	);
	$effect(() => subscribeStatus((next) => (status = next)));

	/// What the server already knew when it rendered this page.
	const initial = untrack(() => data.count);

	let health = $state<Health | null>(null);
	let healthFailed = $state(false);
	$effect(() => {
		let cancelled = false;
		fetch('/api/health', { headers: { accept: 'application/json' } })
			.then((r) => r.json() as Promise<Health>)
			.then((body) => {
				if (!cancelled) health = body;
			})
			.catch(() => {
				if (!cancelled) healthFailed = true;
			});
		return () => {
			cancelled = true;
		};
	});

	const link = (ok: boolean | undefined) =>
		ok === undefined ? { label: 'unknown', class: 'text-faint' } : ok
			? { label: 'operational', class: 'text-ember' }
			: { label: 'unavailable', class: 'text-red' };

	let rows = $derived([
		{
			// First, because it is the one that matters: without it nobody can
			// queue, log in, or be counted.
			name: 'Control server (login + matchmaking)',
			...(status?.control_server === true
				? { label: 'operational', class: 'text-ember' }
				: status?.control_server === false
					? { label: 'unreachable', class: 'text-red' }
					: { label: 'not configured', class: 'text-faint' })
		},
		{ name: 'Enforcement grid (live db)', ...link(health?.live_db) },
		{ name: 'Operation archive (snapshot)', ...link(health?.snapshot_db) },
		{ name: 'Civic datastore (site db)', ...link(health?.site_db) },
		{
			// In the same register as the rows above — a piece of public
			// infrastructure — without "telemetry", which reads as collecting
			// data about the reader. It broadcasts; it does not listen.
			name: 'Broadcast relay',
			...(status?.ok
				? { label: 'operational', class: 'text-ember' }
				: { label: healthFailed ? 'unavailable' : 'linking', class: 'text-faint' })
		}
	]);

	const steps = [
		{
			n: '01',
			title: 'Install the client',
			body: 'Grab the latest build and the retail game files, then launch through the loader.'
		},
		{
			n: '02',
			title: 'Register an agent',
			body: 'Accounts are created in-client on first launch. Pick a handle and a class.'
		},
		{
			n: '03',
			title: 'Enter the grid',
			body: 'Queue from the Dome. Every game mode is available at any time, so pick whatever you feel like playing.'
		}
	];
</script>

<svelte:head>
	<Seo
		title="Global Agenda server status | The Commonwealth"
		description="Live player count, control server and database status for The Commonwealth, a Global Agenda community server, plus when people usually play."
	/>
</svelte:head>

<header class="mb-8">
	<p class="label text-ember">Live</p>
	<h1 class="heading mt-3 text-[clamp(2rem,5vw,3.25rem)]">Server status</h1>
	<p class="mt-4 max-w-2xl text-base leading-relaxed text-dim">
		Live player count, read straight from the game database, plus the health of the services this
		website depends on. The server is hosted in London, which also gives players on the US east
		coast a workable ping. Nothing here is hand-maintained — if the server is down, this page says
		so.
	</p>
</header>

<div class="grid gap-4 lg:grid-cols-[1.1fr_0.9fr]">
	<Panel label="Players online" meta="live" bodyClass="flex min-h-[10rem] items-center">
		<StatusReadout size="xl" {initial} />
	</Panel>

	<div class="flex flex-col gap-4">
		<Panel label="Services" meta={health ? 'diagnosed' : 'querying'}>
			<ul class="divide-y divide-line/60">
				{#each rows as row (row.name)}
					<li class="flex items-center justify-between gap-3 py-2.5 first:pt-0 last:pb-0">
						<span class="text-sm text-dim">{row.name}</span>
						<span class="label flex items-center gap-2 {row.class}">
							<span class="size-1 rotate-45 bg-current"></span>
							{row.label}
						</span>
					</li>
				{/each}
			</ul>
		</Panel>

	</div>
</div>

<section id="when-people-play" class="mt-14 scroll-mt-36">
	<div class="mb-5 flex items-end justify-between gap-4 border-b border-line/60 pb-3">
		<h2 class="heading text-2xl tracking-[0.08em]">When people play</h2>
		<span class="text-sm text-faint">hourly peaks, reconstructed from sessions</span>
	</div>
	<Panel label="Activity" meta="peak concurrent">
		<ActivityChart />
	</Panel>
</section>

<section class="mt-14">
	<div class="mb-5 flex items-end justify-between gap-4 border-b border-line/70 pb-3">
		<h2 class="heading text-2xl tracking-[0.08em]">How to get in</h2>
		<a href="/play" class="text-sm text-dim transition-colors hover:text-ember">Full setup guide →</a>
	</div>

	<div class="grid gap-4 md:grid-cols-3">
		{#each steps as step, i (step.n)}
			<div use:reveal={{ delay: i * 90 }}>
				<div class="panel-ticks relative h-full border border-line/90 bg-panel/40 p-5">
					<span class="heading ember-text text-4xl">{step.n}</span>
					<h3 class="heading mt-3 text-lg tracking-[0.12em] text-ink">{step.title}</h3>
					<p class="mt-2 text-sm leading-relaxed text-dim">{step.body}</p>
				</div>
			</div>
		{/each}
	</div>
</section>
