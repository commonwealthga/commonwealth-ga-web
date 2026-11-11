<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	import Button from '$lib/components/Button.svelte';

	const projects = [
		{
			href: 'https://github.com/Zotikus1001/commonwealth-ga-launcher/releases/latest',
			title: 'Commonwealth Launcher',
			who: 'Sizzh',
			body: 'The recommended way to launch the game. Finds your existing installation, applies client patches and optional restored maps, and keeps itself updated.',
			tags: ['Launcher', 'Windows', 'Linux']
		},
		{
			href: 'https://gatcg.net/',
			title: 'GA Cards Registry',
			who: 'Jeronix',
			body: 'A companion registry: card drops, achievements, PvP reports and other extras the original game never had. Worth a look even if you only play occasionally.',
			tags: ['Website', 'Stats', 'Cards']
		},
		{
			href: 'https://github.com/kithanakodah/cw-globalagenda-ksp',
			title: 'Singleplayer build',
			who: 'Kithana',
			body: 'An offline take on the server, with gameplay tuned for solo play. Good for practising or just seeing the maps without a queue.',
			tags: ['Offline', 'Solo']
		}
	];

	const contributing = [
		{
			title: 'Server software',
			body: 'The reimplementation itself — control server, game DLL, protocol work and database. AGPL-3.0.',
			href: 'https://github.com/commonwealthga/commonwealth-ga-server',
			cta: 'commonwealth-ga-server'
		},
		{
			title: 'This website',
			body: 'Rust API, server-sent live updates and a SvelteKit frontend. Source-available rather than open source — contributions welcome, reuse for another community is not.',
			href: 'https://github.com/commonwealthga/commonwealth-ga-website',
			cta: 'commonwealth-ga-website'
		}
	];
</script>

<svelte:head>
	<Seo
		title="Community | The Commonwealth"
		description="Where the community lives: Discord, source code, how the server is run, and how to help."
	/>
</svelte:head>

<header class="mb-10">
	<p class="label text-ember">People</p>
	<h1 class="heading mt-3 text-[clamp(2rem,5vw,3.25rem)]">Community</h1>
	<p class="mt-4 max-w-2xl text-base leading-relaxed text-dim">
		The server is only part of it. Several people build their own tools around it — those projects
		deserve the spotlight, and most of them are looking for feedback.
	</p>
</header>

<div class="flex flex-col gap-4">
	{#each projects as project (project.href)}
		<a
			href={project.href}
			target="_blank"
			rel="noopener noreferrer"
			class="group grid gap-2 border border-line/80 bg-panel/40 p-6 transition-colors hover:border-ember/50 hover:bg-panel-hi/40 sm:grid-cols-[1fr_auto] sm:items-start"
		>
			<div class="flex flex-col gap-2">
				<div class="flex flex-wrap items-baseline gap-3">
					<h2 class="heading text-xl tracking-[0.08em] text-ink group-hover:text-amber">
						{project.title}
					</h2>
					<span class="label text-faint">by {project.who}</span>
				</div>
				<p class="max-w-2xl text-sm leading-relaxed text-dim">{project.body}</p>
				<div class="mt-1 flex flex-wrap gap-2">
					{#each project.tags as tag (tag)}
						<span class="label border border-line/80 px-2 py-1 text-faint">{tag}</span>
					{/each}
				</div>
			</div>
			<span class="text-lg text-faint transition-transform group-hover:translate-x-1 group-hover:text-ember">
				→
			</span>
		</a>
	{/each}
</div>

<section class="mt-14">
	<div class="mb-5 border-b border-line/60 pb-3">
		<h2 class="heading text-2xl tracking-[0.08em]">Get involved</h2>
	</div>

	<div class="grid gap-4 md:grid-cols-2">
		{#each contributing as repo (repo.title)}
			<a
				href={repo.href}
				target="_blank"
				rel="noopener noreferrer"
				class="group flex flex-col gap-2 border border-line/80 bg-panel/40 p-5 transition-colors hover:border-ember/50"
			>
				<h3 class="heading text-base tracking-[0.1em] text-ink group-hover:text-amber">{repo.title}</h3>
				<p class="text-sm leading-relaxed text-dim">{repo.body}</p>
				<span class="label mt-1 text-faint group-hover:text-ember">{repo.cta} →</span>
			</a>
		{/each}
	</div>
</section>

<section class="panel-ticks mt-6 border border-line/80 bg-panel/40 p-6 sm:p-8">
	<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
		<div>
			<h2 class="heading text-xl tracking-[0.08em]">Everything happens on Discord</h2>
			<p class="mt-2 max-w-xl text-sm leading-relaxed text-dim">
				Organising matches, reporting bugs, suggesting features, sharing screenshots. It is the
				fastest way to reach the people who maintain the server.
			</p>
		</div>
		<Button href="https://discord.gg/CqACf4qArk" variant="ember" class="shrink-0">Join Discord</Button>
	</div>
</section>
