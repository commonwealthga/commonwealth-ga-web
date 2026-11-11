<script lang="ts">
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
		{ href: '/missions', label: 'Missions' }
	];

	const active = (href: string) => page.url.pathname.startsWith(href);
</script>

<header class="sticky top-0 z-30 border-b border-line/70 bg-void/85 backdrop-blur-md">
	<div class="h-[2px] w-full bg-gradient-to-r from-red-deep via-ember to-transparent"></div>

	<div class="mx-auto flex w-full max-w-6xl items-center gap-3 px-4 py-3 sm:px-6">
		<a href="/" class="group flex items-center gap-2.5">
			<HexMark />
			<span class="flex flex-col leading-none">
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
			<ul class="flex overflow-x-auto [-ms-overflow-style:none] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
				{#each links as link (link.href)}
					{@const on = active(link.href)}
					<li class="shrink-0">
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
