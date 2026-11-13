<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import HexSigil from '$lib/components/HexSigil.svelte';
	import Panel from '$lib/components/Panel.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import StatusReadout from '$lib/components/StatusReadout.svelte';
	import { untrack } from 'svelte';
	import { SITE_ORIGIN } from '$lib/site';
	import { subscribeStatus } from '$lib/live';
	import {
		eventUrgency,
		formatCountdown,
		formatInViewerTime,
		headlineEvent
	} from '$lib/events';

	/**
	 * schema.org description of the game itself, for the home page only: what it
	 * is, that it is free, and who runs it. `Seo` turns this into the script tag.
	 */
	const gameData = {
		'@type': 'VideoGame',
		name: 'Global Agenda',
		alternateName: 'The Commonwealth — Global Agenda community server',
		applicationCategory: 'Game',
		operatingSystem: 'Windows',
		playMode: ['Cooperative', 'MultiPlayer'],
		description:
			'Community-run Global Agenda server with an active playerbase: the 2010 team shooter Hi-Rez shut down in 2018, running again with everything unlocked.',
		url: `${SITE_ORIGIN}/`,
		image: `${SITE_ORIGIN}/og/home.jpg`,
		// The site's own nodes already describe the publisher; this points at them.
		publisher: { '@id': `${SITE_ORIGIN}/#organization` },
		offers: { '@type': 'Offer', price: '0', priceCurrency: 'GBP' }
	};

	// Where a newcomer actually needs to go, in plain language.
	const paths = [
		{
			href: '/play',
			title: 'Play',
			body: 'Install the launcher, sign in with an in-game account, pick a mission. About five minutes.'
		},
		{
			href: '/server',
			title: 'Server status',
			body: 'Who is online right now, and when people usually play.'
		},
		{
			href: '/missions',
			title: 'Missions',
			body: 'Every completed mission on record: map, mode, winner, duration and player count.'
		}
	];

	// What actually distinguishes this server from other Global Agenda projects,
	// ordered by importance: the community first.
	const differences = [
		{
			title: 'A community that actually plays',
			body: 'PvE runs every day, PvP every Saturday and Sunday, and the Discord stays busy in between. Other projects have servers; this one has people on them.'
		},
		{
			title: 'Endgame is the point',
			body: 'The focus is what happens at the cap: PvE raids, ultra-max and mercenary missions, and organised PvP. Leveling and grind are being added later, and will stay optional — the endgame remains the default.'
		},
		{
			title: 'Everything unlocked, nothing to buy',
			body: 'You start at level 50 with top-tier gear and every cosmetic already unlocked. No shop, no paid tiers, no catch-up grind, and the server software is open source.'
		},
		{
			title: 'Restored content and fixed old bugs',
			body: 'Content from older versions of the game has been brought back, and long-standing bugs from the retail build are fixed — including the infamous login bug.'
		},
		{
			title: 'Modes the original never had',
			body: 'The team keeps building game modes that never shipped in the retail game. Every mode is available at any time — nothing is on a rotation, so play what you feel like.'
		},
		{
			title: 'Three nights on the calendar',
			body: 'Friday raids, Saturday challenge matches, Sunday mercenary night. Fixed times, announced in Discord, shown on this site in your own timezone.'
		}
	];

	/**
	 * "Why play here", as an enumerable list.
	 *
	 * The six points are the most quotable thing on the site — concrete claims a
	 * reader (or an assistant answering "is this server worth playing?") can lift
	 * straight out. Saying so in the graph is what lets a parser treat them as six
	 * attributes of one thing rather than six loose paragraphs.
	 *
	 * Each `ListItem` carries the same words the page shows: nothing here is
	 * invented for machines, which is the whole point of structured data.
	 */
	const whyPlay = {
		'@type': 'ItemList',
		name: 'Why play on The Commonwealth',
		description: `Six reasons players stay on this Global Agenda server: ${differences.length} points, in the order the page shows them.`,
		numberOfItems: differences.length,
		itemListOrder: 'https://schema.org/ItemListOrderAscending',
		itemListElement: differences.map((item, index) => ({
			'@type': 'ListItem',
			position: index + 1,
			name: item.title,
			description: item.body
		}))
	};

	const structuredData = {
		'@context': 'https://schema.org',
		'@graph': [gameData, whyPlay]
	};

	const projects = [
		{
			href: 'https://github.com/Zotikus1001/commonwealth-ga-launcher/releases/latest',
			title: 'Commonwealth Launcher',
			who: 'by Sizzh',
			body: 'Finds your Steam install, applies client patches and launches you into the server.'
		},
		{
			href: 'https://gatcg.net/',
			title: 'GA Cards Registry',
			who: 'by Jeronix',
			body: 'A registry of card drops, achievements and PvP reports — community-built extras.'
		},
		{
			href: 'https://github.com/kithanakodah/cw-globalagenda-ksp',
			title: 'Singleplayer build',
			who: 'by Kithana',
			body: 'An offline version of the server, tuned for solo play.'
		}
	];

	const facts = [
		{ k: 'Platform', v: 'Windows · Linux' },
		{ k: 'Cost', v: 'Free' },
		{ k: 'Start at', v: 'Level 50' },
		{ k: 'Server', v: 'London' }
	];

	let { data } = $props();

	// Seeded with the server's reading, so the number is in the HTML; the live
	// stream replaces it a moment later without the page shifting. Read once on
	// purpose — after that the count belongs to the stream, not to the load.
	let players = $state(untrack(() => data.count.players ?? 0));

	// Event status is client-only: the prerendered HTML must not freeze a clock.
	let now = $state<Date | null>(null);
	$effect(() => {
		const tick = () => (now = new Date());
		tick();
		const timer = setInterval(tick, 30_000);
		return () => clearInterval(timer);
	});
	$effect(() => subscribeStatus((status) => (players = status.ok ? (status.players ?? 0) : 0)));

	let headline = $derived(now ? headlineEvent(now, players) : null);
	/** Whether the headline event is close enough to be the loud thing in its panel. */
	let headlineUrgency = $derived(headline && now ? eventUrgency(headline, now) : 'later');
</script>

<svelte:head>
	<!--
		Title leads with the phrase people search ("Global Agenda community
		server"); the brand goes last. The description names the playerbase,
		because that is the thing a reader deciding between dead servers and
		live ones is looking for.
	-->
	<Seo
		title="Global Agenda community server | The Commonwealth"
		description="A free Global Agenda private server with an active playerbase. Start at level 50 with everything unlocked, play endgame PvE and PvP, and join weekly events."
		{structuredData}
	/>
</svelte:head>

<section class="grid items-center gap-12 lg:grid-cols-[1.1fr_0.9fr]">
	<div class="flex flex-col gap-6">
		<p class="label text-ember">Community server · free to play</p>

		<h1 class="heading text-[clamp(2.4rem,7vw,4.6rem)]">
			Global Agenda,<br />
			<span class="ember-text">back online</span>
		</h1>

		<p class="max-w-xl text-base leading-relaxed text-dim">
			Global Agenda was a team shooter from 2010 that Hi-Rez Studios shut down in 2018. This is a
			volunteer-run private server for it, hosted in London with an active
			playerbase. You start at level 50 with everything
			unlocked, and the game picks up at the endgame.
		</p>

		<!--
			The count is in the copy, not only in the panel below: it is the single
			most convincing fact on this page, and a crawler (or an assistant quoting
			the site) only ever sees what the server renders.
		-->
		<p class="label text-ember">{data.sentence}</p>

		<div class="flex flex-wrap items-center gap-3">
			<Button href="/play" size="lg">Get started</Button>
			<Button href="https://discord.gg/CqACf4qArk" variant="outline" size="lg">Join Discord</Button>
		</div>

		<dl class="mt-2 flex flex-wrap gap-x-10 gap-y-3">
			{#each facts as fact (fact.k)}
				<div>
					<dt class="label text-faint">{fact.k}</dt>
					<dd class="mt-1 text-sm text-dim">{fact.v}</dd>
				</div>
			{/each}
		</dl>
	</div>

	<div class="flex flex-col items-center gap-4">
		<HexSigil />
		<p class="flex items-center gap-3 text-sm">
			<a
				href="/server#when-people-play"
				class="label text-dim transition-colors hover:text-ember">when people play</a
			>
			<span class="text-line-hi">·</span>
			<a href="/missions" class="label text-dim transition-colors hover:text-ember">recent missions</a>
		</p>
	</div>
</section>

<!-- What sets this server apart -->
<section class="mt-16">
	<div class="mb-5 flex items-end justify-between gap-4 border-b border-line/60 pb-3">
		<h2 class="heading text-2xl tracking-[0.08em]">Why play here</h2>
		<span class="text-sm text-faint">six reasons people stay</span>
	</div>

	<div class="grid gap-4 sm:grid-cols-2">
		{#each differences as item, index (item.title)}
			<div class="panel-ticks flex gap-4 border border-line/80 bg-panel/40 p-5">
				<span class="heading text-2xl leading-none text-ember/70">0{index + 1}</span>
				<div>
					<h3 class="text-sm font-medium text-ink">{item.title}</h3>
					<p class="mt-1.5 text-sm leading-relaxed text-dim">{item.body}</p>
				</div>
			</div>
		{/each}
	</div>
</section>

<section class="mt-12 grid gap-4 md:grid-cols-3">
	<Panel label="Right now">
		<StatusReadout />
	</Panel>

	<Panel label="Next event">
		<div class="flex h-full flex-col justify-between gap-3">
			{#if headline}
				<div>
					<p class="text-sm text-ink">{headline.event.title}</p>

					{#if headline.state === 'live'}
						<p class="heading mt-2 text-2xl tracking-[0.04em] text-ember">live now</p>
					{:else if headlineUrgency === 'soon'}
						<!--
							Close enough that the countdown is the point: set it as a number
							rather than a footnote, so it reads across the room.
						-->
						<p class="heading mt-2 text-3xl tracking-[0.03em] text-ember tabular-nums">
							in {formatCountdown(headline.nextAt, now ?? new Date())}
						</p>
						<p class="label mt-1 text-dim">{formatInViewerTime(headline.nextAt)} your time</p>
					{:else if headline.state === 'just-ended'}
						<p class="label mt-2 text-faint">finished for today</p>
					{:else}
						<p class="label mt-2 text-faint">
							in {formatCountdown(headline.nextAt, now ?? new Date())} · {formatInViewerTime(
								headline.nextAt
							)}
						</p>
					{/if}
				</div>
			{:else}
				<p class="text-sm text-dim">Friday raids · Saturday challenges · Sunday mercenary</p>
			{/if}
			<a href="/events" class="label text-ember transition-colors hover:text-amber">All events →</a>
		</div>
	</Panel>

	<Panel label="Start here">
		<div class="flex h-full flex-col justify-between gap-3">
			<p class="text-sm leading-relaxed text-dim">
				{paths[0].body}
			</p>
			<a href="/play" class="label text-ember transition-colors hover:text-amber">Setup guide →</a>
		</div>
	</Panel>
</section>

<section class="panel-ticks mt-6 border border-line/80 bg-panel/30 p-5">
	<h2 class="heading text-lg tracking-[0.08em]">What this is, honestly</h2>
	<div class="mt-3 grid gap-4 text-sm leading-relaxed text-dim sm:grid-cols-2">
		<p>
			The retail client is turned into its own dedicated server: a DLL hooks into the game
			process, a control server on the other side of it handles accounts, matchmaking and
			instances, and the whole thing is the result of reverse engineering the client and its
			protocol. Login, matchmaking, combat and the social systems work today — the client you
			already own is the client you play.
		</p>
		<p>
			It is not yet feature-complete: leveling and progression are still being built, and AvA
			territory war, the open world, vendors and crafting are ahead of us. The
			<a class="text-ember hover:text-amber" href="/roadmap">roadmap</a> lists exactly what is done
			and what is not.
		</p>
	</div>
</section>

<section class="mt-14">
	<div class="mb-5 flex items-end justify-between gap-4 border-b border-line/60 pb-3">
		<h2 class="heading text-2xl tracking-[0.08em]">Community projects</h2>
		<a href="/community" class="text-sm text-dim transition-colors hover:text-ember">All projects →</a>
	</div>

	<div class="grid gap-4 md:grid-cols-3">
		{#each projects as project (project.href)}
			<a
				href={project.href}
				target="_blank"
				rel="noopener noreferrer"
				class="group flex flex-col gap-1.5 border border-line/80 bg-panel/40 p-5 transition-colors hover:border-ember/50 hover:bg-panel-hi/40"
			>
				<h3 class="heading text-base tracking-[0.1em] text-ink group-hover:text-amber">
					{project.title}
				</h3>
				<span class="label text-faint">{project.who}</span>
				<p class="mt-1 text-sm leading-relaxed text-dim">{project.body}</p>
			</a>
		{/each}
	</div>
</section>

<section class="panel-ticks mt-14 border border-line/80 bg-panel/40 p-6 sm:p-8">
	<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
		<div>
			<h2 class="heading text-xl tracking-[0.08em]">Something not working?</h2>
			<p class="mt-2 max-w-xl text-sm leading-relaxed text-dim">
				Check the troubleshooting notes first — most problems are a missing launcher or a typo in
				the launch flags. If that doesn't cover it, ask in Discord; someone is usually around.
			</p>
		</div>
		<div class="flex shrink-0 flex-wrap gap-3">
			<Button href="/play#troubleshooting" variant="outline">Troubleshooting</Button>
			<Button href="https://discord.gg/CqACf4qArk" variant="ember">Ask on Discord</Button>
		</div>
	</div>
</section>
