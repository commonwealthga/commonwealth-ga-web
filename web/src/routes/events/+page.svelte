<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	import { EVENTS, allStatuses, formatCountdown, formatInViewerTime, type EventStatus } from '$lib/events';
	import { subscribeStatus } from '$lib/live';
	import { formatClock, formatWeekday } from '$lib/time';

	// Everything time-dependent is computed after hydration so the prerendered
	// HTML stays valid; until then the cards show the titles and descriptions.
	let now = $state<Date | null>(null);
	let players = $state(0);

	$effect(() => {
		const tick = () => (now = new Date());
		tick();
		const timer = setInterval(tick, 30_000);
		return () => clearInterval(timer);
	});

	$effect(() => subscribeStatus((status) => (players = status.ok ? (status.players ?? 0) : 0)));

	let statuses = $derived<EventStatus[]>(now ? allStatuses(now, players) : []);

	const chip = (state: EventStatus['state']) =>
		state === 'live'
			? { text: 'live now', class: 'border-ember/60 bg-ember/10 text-ember' }
			: state === 'just-ended'
				? { text: 'finished for today', class: 'border-line-hi text-dim' }
				: { text: 'upcoming', class: 'border-line-hi text-faint' };
</script>

<svelte:head>
	<Seo
		title="Events | The Commonwealth, a Global Agenda community server"
		description="Weekly community events on the server: what is scheduled, when it starts, and how to join in."
	/>
</svelte:head>

<header class="mb-10">
	<p class="label text-ember">Every week</p>
	<h1 class="heading mt-3 text-[clamp(2rem,5vw,3.25rem)]">Events</h1>
	<p class="mt-4 max-w-2xl text-base leading-relaxed text-dim">
		Three fixed nights — Friday raids, Saturday challenge matches, Sunday mercenary. Everything
		below is in <span class="text-ink">your own timezone</span>, 24-hour. Nothing needs signing up
		for: turn up and join the queue, or watch for the announcement in Discord.
	</p>
</header>

<div class="flex flex-col gap-4">
	{#each EVENTS as event (event.id)}
		{@const status = statuses.find((entry) => entry.event.id === event.id)}
		<article class="panel-ticks border border-line/80 bg-panel/40 p-6">
			<div class="flex flex-wrap items-baseline gap-x-4 gap-y-2">
				<h2 class="heading text-xl tracking-[0.08em] text-ink">{event.title}</h2>

				{#if status}
					{@const at = status.state === 'upcoming' ? status.nextAt : status.at}
					<span class="label text-faint">{formatWeekday(at)} · {formatClock(at)}</span>

					{@const look = chip(status.state)}
					<span class="label ml-auto border px-2.5 py-1 {look.class}">{look.text}</span>
				{:else}
					<span class="label text-faint">weekly</span>
				{/if}
			</div>

			<p class="mt-3 text-sm leading-relaxed text-dim">{event.detail}</p>

			{#if status && now}
				<div class="mt-4 border-t border-line/50 pt-3">
					{#if status.state === 'upcoming'}
						<span class="label text-dim">
							next one {formatInViewerTime(status.nextAt)}
							<span class="text-faint">· in {formatCountdown(status.nextAt, now)}</span>
						</span>
					{:else}
						<span class="label text-dim">
							started {formatInViewerTime(status.at)}
							<span class="text-faint">· {status.state === 'live' ? 'still going' : 'ended'}</span>
						</span>
					{/if}
				</div>
			{/if}
		</article>
	{/each}
</div>

<div class="panel-ticks mt-8 border border-line/80 bg-panel/30 p-6">
	<h2 class="heading text-lg tracking-[0.08em]">How to take part</h2>
	<p class="mt-3 max-w-3xl text-sm leading-relaxed text-dim">
		Get in-game a few minutes before the start time and join the matching queue. Events are
		announced as Discord events as well, so you can mark them and get a notification — the
		calendar there is the authoritative one if a night ever moves.
	</p>
	<div class="mt-4 flex flex-wrap gap-3">
		<a
			href="https://discord.gg/CqACf4qArk"
			class="label border border-line-hi px-3 py-2 text-dim transition-colors hover:border-ember/60 hover:text-ink" target="_blank" rel="noopener noreferrer">Discord events →</a
		>
		<a
			href="/play"
			class="label border border-line-hi px-3 py-2 text-dim transition-colors hover:border-ember/60 hover:text-ink"
			>How to play →</a
		>
	</div>
</div>
