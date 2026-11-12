<script lang="ts">
	import { goto } from '$app/navigation';
	import { moreEntries } from '$lib/guides/api';
	import type { Entry, FeedResponse, OptionsResponse } from '$lib/guides/types';
	import EntryCard from './EntryCard.svelte';

	/**
	 * One category's list: the grid, its filters, and the queue.
	 *
	 * Both `/guides` and `/builds` are this component with a different category.
	 * Filters are part of the URL and the server renders the result, so a filtered
	 * list can be linked to, reloaded and crawled — the client only appends the
	 * next page as you scroll.
	 */
	let {
		category,
		feed,
		options
	}: {
		category: 'guide' | 'build';
		feed: FeedResponse;
		options: OptionsResponse;
	} = $props();

	const isGuide = $derived(category === 'guide');
	const base = $derived(isGuide ? '/guides' : '/builds');

	const filter = $derived(feed.filter);
	const pending = $derived(feed.pending ?? []);

	let extra = $state<Entry[]>([]);
	let exhausted = $state(false);
	const entries = $derived([...feed.entries, ...extra]);
	const loaded = $derived(entries.length);
	const more = $derived(!exhausted && loaded < feed.total);
	let loading = $state(false);
	let problem = $state('');
	let sentinel = $state<HTMLElement | null>(null);

	// The list is replaced by the server when a filter changes; anything appended
	// here belongs to the previous filter.
	$effect(() => {
		void feed;
		extra = [];
		exhausted = false;
	});

	function navigate(
		next: Partial<{ tag: string | null; class: string | null; mode: string | null; q: string; sort: string | null }>
	) {
		const query = new URLSearchParams();
		const merged = {
			tag: filter.tag,
			class: filter.class,
			mode: filter.mode,
			q: filter.q,
			sort: filter.sort === 'helpful' ? null : filter.sort,
			...next
		};
		for (const [key, value] of Object.entries(merged)) {
			if (value) query.set(key, String(value));
		}
		const suffix = query.toString();
		void goto(`${base}${suffix ? `?${suffix}` : ''}`, {
			keepFocus: true,
			noScroll: true,
			replaceState: true
		});
	}

	let searchTimer: ReturnType<typeof setTimeout> | undefined;
	function search(value: string) {
		clearTimeout(searchTimer);
		searchTimer = setTimeout(() => navigate({ q: value }), 220);
	}

	async function loadMore() {
		if (loading || !more) return;
		loading = true;
		problem = '';
		try {
			const page = await moreEntries(
				category,
				{
					tag: filter.tag ?? undefined,
					class: filter.class ?? undefined,
					mode: filter.mode ?? undefined,
					q: filter.q,
					sort: filter.sort
				},
				loaded,
				12
			);
			extra = [...extra, ...page.entries];
			if (page.entries.length === 0) exhausted = true;
		} catch (error) {
			problem = error instanceof Error ? error.message : 'could not load more entries';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const node = sentinel;
		if (!node || !more || !('IntersectionObserver' in window)) return;
		const observer = new IntersectionObserver(
			(observed) => {
				if (observed.some((entry) => entry.isIntersecting)) void loadMore();
			},
			{ rootMargin: '400px' }
		);
		observer.observe(node);
		return () => observer.disconnect();
	});
</script>

<header class="mb-8 flex flex-wrap items-end justify-between gap-4">
	<div>
		<p class="label text-ember">{isGuide ? 'Guides' : 'Builds'}</p>
		<h1 class="heading mt-3 text-[clamp(2rem,5vw,3.25rem)]">{isGuide ? 'Guides' : 'Builds'}</h1>
		<p class="mt-4 max-w-2xl text-base leading-relaxed text-dim">
			{#if isGuide}
				How things work and how to win: strategies, classes in depth, modes explained. Written by
				people who play here, reviewed by a moderator before they appear.
			{:else}
				Skill trees posted by the people using them — one class, the queues it is for, and a
				shot of the tree. Many of these live in Discord too; where the author linked the thread,
				you can join the discussion.
			{/if}
		</p>
		<p class="mt-3">
			<a href={isGuide ? '/builds' : '/guides'} class="label text-ember transition-colors hover:text-amber">
				{isGuide ? 'Looking for builds? →' : 'Looking for guides? →'}
			</a>
		</p>
	</div>
	{#if feed.posting.may_post}
		<a
			class="heading border border-ember/60 px-4 py-2 text-sm tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void"
			href="{base}/submit">{isGuide ? 'Write a guide' : 'Post a build'}</a
		>
	{/if}
</header>

{#if feed.posting.allowance && feed.posting.may_post && feed.posting.allowance.remaining <= 1}
	<p class="panel-ticks mb-6 border border-amber/40 bg-carbon/40 p-3 text-sm text-dim">
		You have {feed.posting.allowance.remaining} of {feed.posting.allowance.limit} submissions left
		today.
	</p>
{:else if !feed.posting.may_post}
	<div class="panel-ticks mb-6 border border-line/60 bg-carbon/40 p-4 text-sm text-dim">
		{feed.posting.reason ?? 'You cannot submit right now.'}
		{#if !feed.posting.signed_in}
			<a
				class="heading ml-2 underline decoration-ember/60 underline-offset-4 hover:text-ember"
				href="/login">Sign in</a
			>
		{/if}
	</div>
{/if}

{#if pending.length > 0}
	<section class="mb-8">
		<div class="flex flex-wrap items-baseline gap-3">
			<h2 class="heading text-2xl text-ink">
				{feed.moderator ? 'Waiting for review' : 'Your pending submissions'}
			</h2>
			<span class="label text-amber">{pending.length}</span>
		</div>
		<ul class="mt-4 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{#each pending as entry (entry.id)}
				<li><EntryCard {entry} moderator={feed.moderator} /></li>
			{/each}
		</ul>
	</section>
{/if}

<div class="mb-6 flex flex-wrap items-center gap-3">
	{#if isGuide}
		<div class="flex flex-wrap items-center gap-1.5">
			<button
				type="button"
				onclick={() => navigate({ tag: null })}
				class="label border px-2.5 py-1 transition-colors {filter.tag === null
					? 'border-ember/60 bg-ember/10 text-ember'
					: 'border-line/70 text-dim hover:border-ember/60 hover:text-ink'}"
				>All</button
			>
			{#each options.tags as tag (tag.value)}
				<button
					type="button"
					onclick={() => navigate({ tag: filter.tag === tag.value ? null : tag.value })}
					aria-pressed={filter.tag === tag.value}
					class="label border px-2.5 py-1 transition-colors {filter.tag === tag.value
						? 'border-ember/60 bg-ember/10 text-ember'
						: 'border-line/70 text-dim hover:border-ember/60 hover:text-ink'}"
					>{tag.label}</button
				>
			{/each}
		</div>
	{:else}
		<div class="flex flex-wrap items-center gap-1.5">
			{#each options.classes as option (option.value)}
				<button
					type="button"
					onclick={() => navigate({ class: filter.class === option.value ? null : option.value })}
					aria-pressed={filter.class === option.value}
					class="label border px-2.5 py-1 transition-colors {filter.class === option.value
						? 'border-ember/60 bg-ember/10 text-ember'
						: 'border-line/70 text-dim hover:border-ember/60 hover:text-ink'}"
					>{option.label}</button
				>
			{/each}
			<span class="mx-1 h-4 w-px bg-line/70"></span>
			{#each options.modes as option (option.value)}
				<button
					type="button"
					onclick={() => navigate({ mode: filter.mode === option.value ? null : option.value })}
					aria-pressed={filter.mode === option.value}
					class="label border px-2.5 py-1 transition-colors {filter.mode === option.value
						? 'border-ember/60 bg-ember/10 text-ember'
						: 'border-line/70 text-dim hover:border-ember/60 hover:text-ink'}"
					>{option.label}</button
				>
			{/each}
		</div>
	{/if}

	<label
		class="flex min-w-[12rem] flex-1 items-center gap-2 border border-line/90 bg-panel/40 px-3 py-1.5"
	>
		<span class="label text-faint">Search</span>
		<input
			type="search"
			class="w-full bg-transparent text-sm text-ink placeholder:text-faint focus:outline-none"
			placeholder={isGuide ? 'title or content' : 'title, class, description'}
			value={filter.q}
			oninput={(event) => search((event.currentTarget as HTMLInputElement).value)}
		/>
	</label>

	<div class="flex items-center gap-1.5">
		{#each [{ value: 'helpful', label: 'Most helpful' }, { value: 'newest', label: 'Newest' }] as option (option.value)}
			<button
				type="button"
				onclick={() => navigate({ sort: option.value === 'helpful' ? null : option.value })}
				aria-pressed={filter.sort === option.value}
				class="label border px-2.5 py-1 transition-colors {filter.sort === option.value
					? 'border-ember/60 bg-ember/10 text-ember'
					: 'border-line/70 text-dim hover:border-ember/60 hover:text-ink'}"
				>{option.label}</button
			>
		{/each}
	</div>
</div>

{#if entries.length === 0}
	<p class="panel-ticks border border-line/60 bg-carbon/40 p-6 text-sm text-dim">
		{#if filter.q || filter.tag || filter.class || filter.mode}
			Nothing matches that filter{#if filter.q} for “{filter.q}”{/if}. Try widening it.
		{:else}
			Nothing here yet. {isGuide ? 'The first guide could be yours.' : 'The first build could be yours.'}
		{/if}
	</p>
{:else}
	<ul class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
		{#each entries as entry (entry.id)}
			<li><EntryCard {entry} moderator={feed.moderator} /></li>
		{/each}
	</ul>

	{#if more}
		<div bind:this={sentinel} class="mt-6 flex flex-col items-center gap-3">
			<button
				type="button"
				disabled={loading}
				class="heading border border-line/70 px-5 py-2.5 text-sm tracking-[0.1em] text-dim transition-colors hover:border-ember/60 hover:text-ink disabled:opacity-50"
				onclick={loadMore}>{loading ? 'Loading…' : 'Load more'}</button
			>
			<span class="label text-faint">
				{#if problem}
					{problem}
				{:else}
					Showing {loaded} of {feed.total}
				{/if}
			</span>
		</div>
	{/if}
{/if}
