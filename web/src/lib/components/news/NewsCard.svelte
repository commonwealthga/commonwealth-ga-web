<script lang="ts">
	import { decide, thumb } from '$lib/news/api';
	import type { NewsPost } from '$lib/news/types';
	import { newsDate } from '$lib/format';

	/**
	 * One entry in the feed, rendered in full.
	 *
	 * Server-rendered on first load (that is the point: a crawler has to read it),
	 * then live: the thumbs-up button and the moderator actions work without a
	 * reload, which is what every other page on the site already did.
	 */
	let { post, moderator = false }: { post: NewsPost; moderator?: boolean } = $props();

	const pending = $derived(post.status === 'pending');
	// The server's numbers until this reader changes them; derived, so a
	// re-render with new props is not fighting a captured value.
	let liked = $state<number | null>(null);
	let reactedOverride = $state<boolean | null>(null);
	const count = $derived(liked ?? post.thumbs_up);
	const reactedNow = $derived(reactedOverride ?? post.reacted);
	let notice = $state('');
	let busy = $state(false);

	const CONFIRM: Record<string, string> = {
		approve: 'publish this entry? It becomes visible to everyone.',
		unapprove: 'unpublish this entry? It returns to the queue and the author can fix it.',
		reject: 'delete this entry? It disappears from the feed everywhere and is kept only for review.'
	};

	const REASON: Record<string, string> = {
		unapprove: 'What should the author fix? (shown to them)',
		reject: 'Why is this being deleted? (kept with the entry)',
		note: 'What should the author change? (shown to them; the entry stays in the queue)'
	};

	async function like() {
		try {
			const result = await thumb(post.id, !reactedNow);
			liked = result.thumbs_up;
			reactedOverride = result.reacted;
			notice = '';
		} catch (error) {
			notice = error instanceof Error ? error.message : 'could not react';
		}
	}

	async function act(action: string) {
		// Anything that decides the entry's fate asks first. Writing a note decides
		// nothing, so its prompt is the deliberate step.
		if (CONFIRM[action] && !window.confirm(`About to ${CONFIRM[action]}\n\n"${post.title}"`)) {
			return;
		}

		let note = '';
		if (REASON[action]) {
			const answer = window.prompt(REASON[action], '');
			if (answer === null) return;
			note = answer.trim();
			if (!note) {
				notice = 'A reason is required for that.';
				return;
			}
		}

		busy = true;
		notice = '';
		try {
			await decide(post.id, action, note);
			// The entry changed state; the server knows what that means for the page.
			window.location.reload();
		} catch (error) {
			busy = false;
			notice = error instanceof Error ? error.message : 'could not complete that';
		}
	}
</script>

<article
	id="post-{post.id}"
	class="panel-ticks border p-5 {pending ? 'border-amber/40' : 'border-line/60'} bg-carbon/40"
>
	<p class="label text-faint">
		<span class={pending ? 'text-amber' : 'text-dim'}>{post.author}</span> posted {post.kind === 'news'
			? 'news'
			: post.kind === 'update'
				? 'a server update'
				: post.kind === 'article'
					? 'an article'
					: 'a video'}
		· <time datetime={new Date(post.created_at * 1000).toISOString().slice(0, 10)}
			>{newsDate(post.created_at)}</time
		>
	</p>

	{#if pending}
		{#if post.moderation_note}
			<p class="label mt-1 text-amber">A moderator asked for a change: {post.moderation_note}</p>
		{:else}
			<p class="label mt-1 text-amber">Waiting for review — only you and a moderator can see this.</p>
		{/if}
		{#if post.can_edit}
			<a
				class="label mt-2 inline-block text-ember underline decoration-ember/40 underline-offset-4 hover:text-amber"
				href="/news/submit?edit={post.id}">Edit this entry</a
			>
		{/if}
	{/if}

	{#if post.title}
		<h2 class="heading mt-2 text-2xl text-ink">{post.title}</h2>
	{/if}

	{#if post.summary && post.videos.length > 0}
		<p class="mt-2 text-sm leading-relaxed text-dim">{post.summary}</p>
	{/if}

	{#if post.body_html}
		<!-- Rendered by the server from a whitelist of tags, so this is inert. -->
		<div class="prose-news mt-4 flex flex-col gap-3 text-base leading-relaxed text-dim">
			{@html post.body_html}
		</div>
	{/if}

	{#if post.videos.length > 0}
		<div class="mt-4 grid gap-3 {post.videos.length > 1 ? 'sm:grid-cols-2' : 'grid-cols-1'}">
			{#each post.videos as video (video)}
				<div class="panel-ticks border border-line/60">
					<iframe
						class="aspect-video w-full"
						src="https://www.youtube-nocookie.com/embed/{video}"
						title="Video"
						loading="lazy"
						allow="accelerometer; encrypted-media; picture-in-picture"
						allowfullscreen
					></iframe>
				</div>
			{/each}
		</div>
	{/if}

	<div class="mt-5 flex items-center gap-4 border-t border-line/70 pt-4">
		<button
			type="button"
			onclick={like}
			aria-pressed={reactedNow}
			aria-label="Thumbs up"
			class="heading flex items-center gap-2 border border-line/70 px-3 py-1.5 text-sm tracking-[0.1em] text-dim transition-colors hover:border-ember/60 hover:text-ink"
		>
			<span aria-hidden="true">👍</span><span>{count}</span>
		</button>
		<span class="label text-faint">{notice}</span>
	</div>

	{#if moderator}
		<div class="mt-4 flex flex-wrap items-center gap-2 border-t border-line/60 pt-3">
			<span class="label text-faint">Moderator</span>
			{#if pending}
				<button
					type="button"
					disabled={busy}
					class="heading border border-ember/60 px-3 py-1.5 text-xs tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void disabled:opacity-50"
					onclick={() => act('approve')}>Approve</button
				>
				<button
					type="button"
					disabled={busy}
					class="heading border border-line/70 px-3 py-1.5 text-xs tracking-[0.1em] text-dim transition-colors hover:border-red/60 hover:text-ink disabled:opacity-50"
					onclick={() => act('reject')}>Reject</button
				>
				<button
					type="button"
					disabled={busy}
					class="heading border border-amber/50 px-3 py-1.5 text-xs tracking-[0.1em] text-amber transition-colors hover:bg-amber hover:text-void disabled:opacity-50"
					onclick={() => act('note')}>Request changes</button
				>
			{:else}
				<button
					type="button"
					disabled={busy}
					class="heading border border-ember/60 px-3 py-1.5 text-xs tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void disabled:opacity-50"
					onclick={() => act('unapprove')}>Unpublish</button
				>
				<button
					type="button"
					disabled={busy}
					class="heading border border-line/70 px-3 py-1.5 text-xs tracking-[0.1em] text-dim transition-colors hover:border-red/60 hover:text-ink disabled:opacity-50"
					onclick={() => act('reject')}>Delete</button
				>
			{/if}
		</div>
	{/if}
</article>
