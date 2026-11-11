<script lang="ts">
	import '../app.css';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import HexBackdrop from '$lib/components/HexBackdrop.svelte';
	import HexMark from '$lib/components/HexMark.svelte';
	import Nav from '$lib/components/Nav.svelte';

	let { children } = $props();

	// The archive moved from /matches to /missions. Nothing was ever deployed
	// under the old name, but a bookmark or a link from Discord would otherwise
	// land on the 404 page; this turns it into the page the visitor meant.
	$effect(() => {
		// SvelteKit types the pathname as the literals it knows about; the whole
		// point here is the one it no longer knows.
		const path: string = page.url.pathname;
		if (path === '/matches' || path.startsWith('/matches/')) {
			goto(path.replace('/matches', '/missions'), { replaceState: true });
		}
	});
</script>

<HexBackdrop />
<Nav />

<main class="mx-auto w-full max-w-6xl px-4 py-12 sm:px-6 sm:py-16">
	{#key page.url.pathname}
		<div style="animation: rise 380ms cubic-bezier(0.22, 0.68, 0, 1) both">
			{@render children()}
		</div>
	{/key}
</main>

<footer class="mt-12 border-t border-line/60 bg-carbon/50">
	<div class="mx-auto flex w-full max-w-6xl flex-col gap-6 px-4 py-10 sm:px-6">
		<div class="grid gap-8 sm:grid-cols-[1.4fr_1fr_1fr]">
			<div class="flex flex-col gap-3">
				<div class="flex items-center gap-2.5">
					<HexMark size={20} />
					<span class="heading text-base tracking-[0.12em] text-ink">The Commonwealth</span>
				</div>
				<p class="max-w-md text-sm leading-relaxed text-dim">
					A community-run server for Global Agenda, the 2010 team shooter that Hi-Rez Studios
					shut down in 2018. Free to play, open source, online around the clock.
				</p>
			</div>

			<nav class="flex flex-col gap-2">
				<span class="label text-faint">Play</span>
				<a class="text-sm text-dim transition-colors hover:text-ember" href="/play">Get started</a>
				<a class="text-sm text-dim transition-colors hover:text-ember" href="/server">Server status</a>
				<a class="text-sm text-dim transition-colors hover:text-ember" href="/missions">Missions</a>
				<a class="text-sm text-dim transition-colors hover:text-ember" href="/events">Events</a>
				<a class="text-sm text-dim transition-colors hover:text-ember" href="/roadmap">Roadmap</a>
			</nav>

			<nav class="flex flex-col gap-2">
				<span class="label text-faint">Elsewhere</span>
				<a
					class="text-sm text-dim transition-colors hover:text-ember"
					href="https://discord.gg/CqACf4qArk" target="_blank" rel="noopener noreferrer">Discord</a
				>
				<a
					class="text-sm text-dim transition-colors hover:text-ember"
					href="https://github.com/commonwealthga/commonwealth-ga-server" target="_blank" rel="noopener noreferrer">Source code</a
				>
				<a class="text-sm text-dim transition-colors hover:text-ember" href="/login">Sign in</a>
				<a class="text-sm text-dim transition-colors hover:text-ember" href="/api/status">Status API</a>
			</nav>
		</div>

		<p class="border-t border-line/50 pt-5 text-xs leading-relaxed text-faint">
			Not affiliated with Hi-Rez Studios. Global Agenda and all related assets are the property of
			their respective owners. This server is a non-commercial fan project.
		</p>
	</div>
</footer>
