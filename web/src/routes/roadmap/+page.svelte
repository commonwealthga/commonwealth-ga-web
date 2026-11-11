<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	/**
	 * Mirrors the roadmap in the server repository's README
	 * (commonwealth-ga-server). Keep the two in sync when phases move.
	 */
	type State = 'done' | 'progress' | 'todo';
	type Item = { label: string; state: State };
	type Phase = { name: string; period: string; summary: string; items: Item[] };

	const phases: Phase[] = [
		{
			name: 'Phase 1 — start with a dream',
			period: 'Feb – Aug 2025',
			summary: 'Make the retail client run as a server at all.',
			items: [
				{ label: 'Set up an SDK and compile a server DLL', state: 'done' },
				{ label: 'Fix crashes when running the game with the "server" argument', state: 'done' },
				{ label: 'Reimplement the missing low-level server networking', state: 'done' },
				{ label: 'Proof-of-concept TCP login server and a working UDP game connection', state: 'done' }
			]
		},
		{
			name: 'Phase 2 — make it work',
			period: 'Aug 2025 – end of 2025',
			summary: 'From "it connects" to "a match can be played to the end".',
			items: [
				{ label: 'Release the source code', state: 'done' },
				{ label: 'Refactor the codebase', state: 'done' },
				{ label: 'Decode game modes and difficulty modifiers', state: 'done' },
				{ label: 'Allow multiple clients on one server', state: 'done' },
				{ label: 'Spawn a player-controlled character with gear and skills', state: 'done' },
				{ label: 'Play a match to the end with bots and enemy AI', state: 'done' }
			]
		},
		{
			name: 'Phase 3 — make it right',
			period: '2026',
			summary: 'Everything a real server needs around the match itself.',
			items: [
				{ label: 'Public server running 24/7', state: 'done' },
				{ label: 'Server orchestration: matchmaking, home map, instances', state: 'done' },
				{ label: 'Registrations and authentication', state: 'done' },
				{ label: 'Core TCP server and database (gear, skills, cosmetics)', state: 'done' },
				{ label: 'Chat', state: 'done' },
				{ label: 'Mercenary, ultra-max and double-agent modes', state: 'done' },
				{ label: 'Raids', state: 'done' },
				{ label: 'All devices working properly', state: 'progress' },
				{ label: 'All skills working properly', state: 'progress' },
				{ label: 'Open world and quests', state: 'progress' },
				{ label: 'Mission rewards and item drops', state: 'todo' },
				{ label: 'Vendors', state: 'todo' },
				{ label: 'Crafting', state: 'todo' },
				{ label: 'AvA gamemode', state: 'todo' },
				{ label: 'Optional leveling', state: 'todo' },
				{ label: 'Multiple regions', state: 'todo' }
			]
		}
	];

	const count = (items: Item[]) => ({
		done: items.filter((item) => item.state === 'done').length,
		total: items.length
	});

	const LEGEND: { state: State; label: string; hint: string }[] = [
		{ state: 'done', label: 'complete', hint: 'shipped and in use' },
		{ state: 'progress', label: 'in progress', hint: 'being worked on now' },
		{ state: 'todo', label: 'planned', hint: 'not started' }
	];
</script>

<svelte:head>
	<Seo
		title="Roadmap | The Commonwealth"
		description="What is working, what is next and what is planned for this Global Agenda community server."
	/>
</svelte:head>

<header class="mb-8">
	<p class="label text-ember">Status</p>
	<h1 class="heading mt-3 text-[clamp(2rem,5vw,3.25rem)]">Roadmap</h1>
	<p class="mt-4 max-w-2xl text-base leading-relaxed text-dim">
		The server has been built in three phases. Phases 1 and 2 are complete; phase 3 is the long
		tail of making every system behave the way the original did — plus the things the original
		never had. This mirrors the roadmap in the
		<a class="text-ember hover:text-amber" href="https://github.com/commonwealthga/commonwealth-ga-server" target="_blank" rel="noopener noreferrer">source repository</a
		>.
	</p>

	<!-- Legend, so the icons are never ambiguous. -->
	<div class="mt-6 flex flex-wrap gap-x-6 gap-y-2">
		{#each LEGEND as entry (entry.state)}
			<span class="flex items-center gap-2">
				<svg viewBox="0 0 16 16" class="size-3.5 shrink-0" aria-hidden="true">
					{#if entry.state === 'done'}
						<polygon points="8,1 14,4.5 14,11.5 8,15 2,11.5 2,4.5" fill="var(--color-ember)" />
						<path
							d="M5.2 8.1 L7.1 10 L10.9 6.2"
							fill="none"
							stroke="var(--color-void)"
							stroke-width="1.8"
							stroke-linecap="round"
							stroke-linejoin="round"
						/>
					{:else if entry.state === 'progress'}
						<polygon
							points="8,1 14,4.5 14,11.5 8,15 2,11.5 2,4.5"
							fill="var(--color-amber)"
							fill-opacity="0.18"
							stroke="var(--color-amber)"
							stroke-width="1.5"
						/>
						<circle cx="8" cy="8" r="2" fill="var(--color-amber)" />
					{:else}
						<polygon
							points="8,1 14,4.5 14,11.5 8,15 2,11.5 2,4.5"
							fill="none"
							stroke="var(--color-line-hi)"
							stroke-width="1.5"
						/>
					{/if}
				</svg>
				<span class="label {entry.state === 'done'
					? 'text-ember'
					: entry.state === 'progress'
						? 'text-amber'
						: 'text-faint'}">{entry.label}</span>
			</span>
		{/each}
	</div>
</header>

<div class="flex flex-col gap-6">
	{#each phases as phase (phase.name)}
		{@const stats = count(phase.items)}
		<section class="panel-ticks border border-line/80 bg-panel/40">
			<header class="flex flex-wrap items-baseline gap-x-4 gap-y-1 border-b border-line/60 px-5 py-4">
				<h2 class="heading text-lg tracking-[0.08em] text-ink">{phase.name}</h2>
				<span class="label text-faint">{phase.period}</span>
				<span class="ml-auto flex items-center gap-3">
					<span class="label text-dim tabular-nums">{stats.done}/{stats.total}</span>
					<span
						class="label border px-2 py-1 {stats.done === stats.total
							? 'border-ember/50 text-ember'
							: 'border-line-hi text-dim'}"
					>
						{stats.done === stats.total ? 'complete' : 'in progress'}
					</span>
				</span>
			</header>

			<div class="px-5 py-4">
				<p class="mb-4 text-sm text-dim">{phase.summary}</p>

				<ul class="grid gap-x-8 gap-y-2.5 sm:grid-cols-2">
					{#each phase.items as item (item.label)}
						<li class="flex items-start gap-2.5 text-sm">
							<svg viewBox="0 0 16 16" class="mt-0.5 size-3.5 shrink-0" aria-hidden="true">
								{#if item.state === 'done'}
									<polygon points="8,1 14,4.5 14,11.5 8,15 2,11.5 2,4.5" fill="var(--color-ember)" />
									<path
										d="M5.2 8.1 L7.1 10 L10.9 6.2"
										fill="none"
										stroke="var(--color-void)"
										stroke-width="1.8"
										stroke-linecap="round"
										stroke-linejoin="round"
									/>
								{:else if item.state === 'progress'}
									<polygon
										points="8,1 14,4.5 14,11.5 8,15 2,11.5 2,4.5"
										fill="var(--color-amber)"
										fill-opacity="0.18"
										stroke="var(--color-amber)"
										stroke-width="1.5"
									/>
									<circle cx="8" cy="8" r="2" fill="var(--color-amber)" />
								{:else}
									<polygon
										points="8,1 14,4.5 14,11.5 8,15 2,11.5 2,4.5"
										fill="none"
										stroke="var(--color-line-hi)"
										stroke-width="1.5"
									/>
								{/if}
							</svg>
							<span class={item.state === 'todo' ? 'text-faint' : 'text-ink'}>
								{item.label}
								{#if item.state === 'progress'}
									<span class="label ml-2 text-amber">in progress</span>
								{/if}
							</span>
						</li>
					{/each}
				</ul>
			</div>
		</section>
	{/each}
</div>

<section class="panel-ticks mt-6 flex flex-col gap-4 border border-line/80 bg-panel/40 p-5 sm:flex-row sm:items-center sm:justify-between">
	<p class="text-sm leading-relaxed text-dim">
		Want to help with any of this? The server is open source and contributions are welcome — the
		Discord has a development channel.
	</p>
	<a
		href="https://github.com/commonwealthga/commonwealth-ga-server"
		class="text-sm text-ember transition-colors hover:text-amber" target="_blank" rel="noopener noreferrer">Open the repository →</a
	>
</section>
