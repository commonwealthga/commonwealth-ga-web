<script lang="ts">
	import { morePosts } from '$lib/news/api';
	import type { NewsPost } from '$lib/news/types';
	import NewsCard from '$lib/components/news/NewsCard.svelte';
	import Seo from '$lib/components/Seo.svelte';

	/**
	 * The feed.
	 *
	 * Rendered on the server with its content (see `+page.server.ts`), and live
	 * afterwards: older entries arrive without leaving the page, a thumbs-up does
	 * not reload, and a moderator's decision refreshes what the server thinks.
	 */
	let { data } = $props();

	const feed = $derived(data.feed);
	const pending = $derived(feed.pending ?? []);
	const ownPending = $derived(feed.moderator ? [] : pending);

	// The server sent the first page; everything after it is appended here.
	let extra = $state<NewsPost[]>([]);
	let exhausted = $state(false);
	const posts = $derived([...feed.posts, ...extra]);
	const loaded = $derived(posts.length);
	const more = $derived(!exhausted && loaded < feed.total);
	let loading = $state(false);
	let problem = $state('');
	let sentinel = $state<HTMLElement | null>(null);

	async function loadOlder() {
		if (loading || !more) return;
		loading = true;
		problem = '';
		try {
			const page = await morePosts(loaded, 10);
			extra = [...extra, ...page.posts];
			// A page that added nothing means the total moved under us; stop asking.
			if (page.posts.length === 0) exhausted = true;
		} catch (error) {
			problem = error instanceof Error ? error.message : 'could not load older entries';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const node = sentinel;
		if (!node || !more || !('IntersectionObserver' in window)) return;
		const observer = new IntersectionObserver(
			(entries) => {
				if (entries.some((entry) => entry.isIntersecting)) void loadOlder();
			},
			{ rootMargin: '400px' }
		);
		observer.observe(node);
		return () => observer.disconnect();
	});
</script>

<Seo
	title="News — The Commonwealth"
	description="News, articles, server updates and videos posted by the Commonwealth Global Agenda community."
/>

<div class="flex flex-col gap-2">
	<h1 class="heading text-4xl text-ink sm:text-5xl">News</h1>
	<p class="max-w-2xl text-sm leading-relaxed text-dim">
		News, articles, server updates and videos posted by the people who play here. Every entry is
		reviewed by a moderator before it appears.
	</p>
</div>

{#if feed.posting.may_post && feed.posting.allowance?.remaining === 0}
	<div class="panel-ticks mt-8 border border-amber/40 bg-carbon/40 p-4 text-sm text-dim">
		You have used all {feed.posting.allowance.limit} posts for today.
		{#if feed.posting.allowance.resets_at_clock}
			You can post again after {feed.posting.allowance.resets_at_clock}.
		{/if}
	</div>
{:else if feed.posting.may_post}
	<div class="panel-ticks mt-8 flex flex-wrap items-center gap-3 border border-line/60 bg-carbon/40 p-4">
		<a
			class="heading border border-ember/60 px-4 py-2 text-sm tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void"
			href="/news/submit">Post news</a
		>
		<span class="label text-faint">Reviewed by a moderator before it goes live</span>
		{#if feed.posting.allowance}
			<span class="label text-faint">
				{feed.posting.allowance.remaining} of {feed.posting.allowance.limit} left today
			</span>
		{/if}
	</div>
{:else}
	<div class="panel-ticks mt-8 border border-line/60 bg-carbon/40 p-4 text-sm text-dim">
		{feed.posting.reason}
		{#if !feed.posting.signed_in}
			<a
				class="heading ml-2 underline decoration-ember/60 underline-offset-4 hover:text-ember"
				href="/login">Sign in</a
			>
		{/if}
	</div>
{/if}

{#if feed.moderator && pending.length > 0}
	<section class="mt-10">
		<div class="flex flex-wrap items-baseline gap-3">
			<h2 class="heading text-2xl text-ink">Waiting for review</h2>
			<span class="label text-amber">{pending.length} pending</span>
		</div>
		<p class="mt-1 text-sm text-dim">
			Only moderators see this section. Every decision asks first; rejecting and unpublishing
			also ask for a reason, which the author sees.
		</p>
		<ul class="mt-4 flex flex-col gap-3">
			{#each pending as post (post.id)}
				<li><NewsCard {post} moderator /></li>
			{/each}
		</ul>
	</section>
{/if}

{#if posts.length === 0 && ownPending.length === 0}
	<p class="panel-ticks mt-10 border border-line/60 bg-carbon/40 p-6 text-sm text-dim">
		{#if feed.moderator && pending.length > 0}
			Nothing published yet — the queue above is waiting for your review.
		{:else}
			Nothing published yet. The first post could be yours.
		{/if}
	</p>
{:else}
	<ul class="mt-10 flex flex-col gap-3">
		{#each ownPending as post (post.id)}
			<li><NewsCard {post} /></li>
		{/each}
		{#each posts as post (post.id)}
			<li><NewsCard {post} moderator={feed.moderator} /></li>
		{/each}
	</ul>

	{#if more}
		<div bind:this={sentinel} class="mt-6 flex flex-col items-center gap-3">
			<button
				type="button"
				disabled={loading}
				class="heading border border-line/70 px-5 py-2.5 text-sm tracking-[0.1em] text-dim transition-colors hover:border-ember/60 hover:text-ink disabled:opacity-50"
				onclick={loadOlder}
			>
				{loading ? 'Loading…' : 'Load older entries'}
			</button>
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
