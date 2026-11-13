<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	import { untrack } from 'svelte';
	import { SITE_ORIGIN } from '$lib/site';
	import {
		allStatuses,
		eventSchema,
		eventUrgency,
		eventZoneName,
		formatCountdown,
		formatInViewerTime,
		isoInEventZone,
		type EventStatus,
		type Urgency
	} from '$lib/events';
	import { subscribeStatus } from '$lib/live';
	import { formatClock, formatWeekday } from '$lib/time';

	let { data } = $props();

	/**
	 * The clock starts as the server's, so the page renders real dates and a
	 * nearest-first list on first paint. The moment it hydrates it switches to the
	 * visitor's clock — which is also when the times become *their* times.
	 */
	// Read once on purpose: the server's clock is a starting point, not a source of
	// truth to keep following — from the first tick the visitor's clock takes over.
	let now = $state<Date>(new Date(untrack(() => data.now)));
	let localised = $state(false);
	let players = $state(0);

	$effect(() => {
		const tick = () => {
			now = new Date();
			localised = true;
		};
		tick();
		const timer = setInterval(tick, 30_000);
		return () => clearInterval(timer);
	});

	$effect(() => subscribeStatus((status) => (players = status.ok ? (status.players ?? 0) : 0)));

	/** Nearest first: whatever is running, then whatever starts soonest. */
	let cards = $derived<EventStatus[]>(allStatuses(now, players));

	/**
	 * The schedule as a machine reads it. One `Event` per night, each carrying the
	 * next occurrence and the weekly recurrence — nothing here is visible, and
	 * nothing visible is invented by it.
	 */
	let structuredData = $derived({
		'@context': 'https://schema.org',
		'@graph': cards.map((status) => eventSchema(status, SITE_ORIGIN))
	});

	const urgencyOf = (status: EventStatus): Urgency => eventUrgency(status, now);

	const chip = (status: EventStatus): { text: string; class: string } => {
		const urgency = urgencyOf(status);
		if (urgency === 'live') return { text: 'live now', class: 'border-ember/60 bg-ember/10 text-ember' };
		if (urgency === 'past') return { text: 'finished for today', class: 'border-line-hi text-dim' };
		if (urgency === 'soon') return { text: 'starts soon', class: 'border-ember/60 bg-ember/15 text-ember' };
		return { text: 'upcoming', class: 'border-line-hi text-faint' };
	};
</script>

<svelte:head>
	<Seo
		title="Events | The Commonwealth, a Global Agenda community server"
		description="Weekly community events on the server: what is scheduled, when it starts, and how to join in."
		structuredData={structuredData}
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
	{#each cards as status (status.event.id)}
		{@const event = status.event}
		{@const urgency = urgencyOf(status)}
		{@const at = status.state === 'upcoming' ? status.nextAt : status.at}
		{@const look = chip(status)}
		<!--
			The date text is the visitor's clock once hydrated, and the schedule's own
			zone until then — so it is never wrong, only less personal. The `datetime`
			attribute is the instant either way, which is what a crawler reads.
		-->
		<article
			id={event.id}
			class="panel-ticks border p-6 transition-colors {urgency === 'soon' || urgency === 'live'
				? 'border-ember/45 bg-ember/[0.045]'
				: 'border-line/80 bg-panel/40'}"
		>
			<div class="flex flex-wrap items-baseline gap-x-4 gap-y-2">
				<h2 class="heading text-xl tracking-[0.08em] text-ink">{event.title}</h2>

				<time class="label text-faint" datetime={isoInEventZone(at)}>
					{formatWeekday(at)} · {formatClock(at)}
					{#if !localised}<span class="text-faint/70">{eventZoneName(at)}</span>{/if}
				</time>

				<span class="label ml-auto border px-2.5 py-1 {look.class}">{look.text}</span>
			</div>

			<p class="mt-3 text-sm leading-relaxed text-dim">{event.detail}</p>

			{#if status && now}
				<div
					class="mt-4 border-t pt-3 {urgency === 'soon' || urgency === 'live'
						? 'border-ember/30'
						: 'border-line/50'}"
				>
					{#if status.state === 'upcoming' && urgency === 'soon'}
						<!--
							The nearest card is the one worth interrupting someone for, so the
							countdown is set as a number rather than a footnote.
						-->
						<div class="flex flex-wrap items-baseline gap-x-4 gap-y-1">
							<span class="heading text-3xl tracking-[0.03em] text-ember tabular-nums">
								in {formatCountdown(status.nextAt, now)}
							</span>
							<span class="label text-dim">{formatInViewerTime(status.nextAt)} your time</span>
						</div>
					{:else if status.state === 'upcoming'}
						<span class="label text-dim">
							next one {formatInViewerTime(status.nextAt)}
							<span class="text-faint">· in {formatCountdown(status.nextAt, now)}</span>
						</span>
					{:else if status.state === 'live'}
						<div class="flex flex-wrap items-baseline gap-x-4 gap-y-1">
							<span class="heading text-3xl tracking-[0.03em] text-ember">live now</span>
							<span class="label text-dim">started {formatInViewerTime(status.at)} your time</span>
						</div>
					{:else}
						<span class="label text-dim">
							started {formatInViewerTime(status.at)}
							<span class="text-faint">· ended</span>
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
