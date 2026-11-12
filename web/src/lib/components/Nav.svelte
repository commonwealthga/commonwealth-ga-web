<script lang="ts">
	import { afterNavigate } from '$app/navigation';
	import { page } from '$app/state';
	import HexMark from './HexMark.svelte';
	import AccountLink from './AccountLink.svelte';
	import StatusChip from './StatusChip.svelte';

	// Plain names first: a visitor should not have to learn our vocabulary to
	// find the download page.
	const links = [
		{ href: '/play', label: 'Get started' },
		{ href: '/server', label: 'Server status' },
		{ href: '/roadmap', label: 'Roadmap' },
		{ href: '/community', label: 'Community' },
		{ href: '/events', label: 'Events' },
		{ href: '/agents', label: 'Agents' },
		{ href: '/missions', label: 'Missions' },
		// Two entries, because they are two things a visitor looks for; each page
		// also links to the other.
		{ href: '/guides', label: 'Guides' },
		{ href: '/builds', label: 'Builds' },
		{ href: '/news', label: 'News' }
	];

	const active = (href: string) => page.url.pathname.startsWith(href);

	/** The page you are on, named in the menu trigger so a phone reader is not
	 * guessing which of ten entries is theirs. Longest match wins, so `/guides`
	 * does not also claim `/guides-and-more`. */
	const current = $derived(
		[...links].sort((a, b) => b.href.length - a.href.length).find((link) => active(link.href))
	);

	// A menu that stays open across a navigation is a menu that lies about where
	// you are. No focus trap: this is a disclosure of links, not a modal.
	let open = $state(false);
	afterNavigate(() => {
		open = false;
	});

	// Escape closes it from anywhere, including from a link inside the panel —
	// which a listener on `<details>` would miss, since the panel is a sibling of
	// the summary rather than a child of it.
	function closeOnEscape(event: KeyboardEvent) {
		if (event.key === 'Escape') open = false;
	}
</script>

<svelte:window onkeydown={closeOnEscape} />

<header class="sticky top-0 z-30 border-b border-line/70 bg-void/85 backdrop-blur-md">
	<div class="h-[2px] w-full bg-gradient-to-r from-red-deep via-ember to-transparent"></div>

	<div class="mx-auto flex w-full max-w-6xl items-center gap-3 px-4 py-3 sm:px-6">
		<a href="/" class="group flex shrink-0 items-center gap-2.5" aria-label="The Commonwealth — home">
			<HexMark />
			<span class="hidden flex-col leading-none sm:flex">
				<span
					class="heading text-base tracking-[0.1em] text-ink transition-colors group-hover:text-amber sm:text-lg sm:tracking-[0.12em]"
				>
					The Commonwealth
				</span>
				<!-- Hidden on phones: the status chip now says "in-game", and the
				     wordmark plus a readable chip matter more than the tagline. -->
				<span class="label mt-1 hidden text-faint sm:block">Global Agenda community server</span>
			</span>
		</a>

		<div class="ml-auto flex items-center gap-2">
			<a
				href="https://discord.gg/CqACf4qArk"
				class="label hidden px-2 py-1.5 text-dim transition-colors hover:text-ink lg:block" target="_blank" rel="noopener noreferrer">Discord</a
			>
			<AccountLink />
			<StatusChip />
		</div>
	</div>

	<nav class="border-t border-line/50">
		<div class="mx-auto w-full max-w-6xl px-2 sm:px-4">
			<!-- Small screens: a disclosure. Ten links do not fit on a phone, and a
			     silently scrollable row hides the ones that do not — this says what
			     it is and shows everything when opened. Native `<details>`, so it
			     works before hydration and needs no focus trap. -->
			<details bind:open class="lg:hidden">
				<summary
					class="flex cursor-pointer list-none items-baseline gap-2.5 px-3 py-3 [&::-webkit-details-marker]:hidden"
				>
					<span class="heading text-sm tracking-[0.14em] text-ink">Menu</span>
					{#if current}
						<span class="label text-ember">{current.label}</span>
					{/if}
					<svg
						class="ml-auto size-3 self-center text-dim transition-transform {open ? 'rotate-180' : ''}"
						viewBox="0 0 12 8"
						fill="none"
						aria-hidden="true"
					>
						<path d="M1 1.5 6 6.5 11 1.5" stroke="currentColor" stroke-width="1.6" />
					</svg>
				</summary>

				<ul class="max-h-[70vh] overflow-y-auto border-t border-line/60 bg-void/95 pb-2">
					{#each links as link (link.href)}
						{@const on = active(link.href)}
						<li>
							<a
								href={link.href}
								aria-current={on ? 'page' : undefined}
								class="flex items-baseline gap-2 px-3 py-2.5 transition-colors {on
									? 'text-ink'
									: 'text-dim hover:text-ink'}"
							>
								<span class="heading text-sm tracking-[0.12em]">{link.label}</span>
								{#if on}
									<span class="size-1 rounded-full bg-ember" aria-hidden="true"></span>
								{/if}
							</a>
						</li>
					{/each}
					<li class="mt-1 border-t border-line/60 pt-1">
						<a
							href="https://discord.gg/CqACf4qArk"
							target="_blank"
							rel="noopener noreferrer"
							class="label flex items-baseline px-3 py-2.5 text-dim transition-colors hover:text-ink"
							>Discord ↗</a
						>
					</li>
				</ul>
			</details>

			<!-- Wide screens: the whole row, no scrolling, no hiding. It fits from
			     1024 px up; below that the disclosure above takes over. -->
			<ul class="hidden lg:flex">
				{#each links as link (link.href)}
					{@const on = active(link.href)}
					<li class="shrink-0">
						<!-- Every page is a SvelteKit route now, so nothing here opts out
						     of client-side navigation. -->
						<a
							href={link.href}
							aria-current={on ? 'page' : undefined}
							class="relative flex items-baseline px-3 py-3 transition-colors sm:px-4"
						>
							<span
								class="heading text-sm tracking-[0.14em] transition-colors {on
									? 'text-ink'
									: 'text-dim group-hover:text-ink hover:text-ink'}"
							>
								{link.label}
							</span>
							<span
								class="absolute inset-x-3 bottom-0 h-[2px] transition-opacity {on
									? 'bg-ember opacity-100'
									: 'bg-ember opacity-0'}"
							></span>
						</a>
					</li>
				{/each}
			</ul>
		</div>
	</nav>
</header>
