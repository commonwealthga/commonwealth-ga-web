<script lang="ts">
	import type { GuidePointer } from '$lib/news/types';
	import { newsDate } from '$lib/format';

	/**
	 * A guide or build announced in the feed.
	 *
	 * The same tile a reader sees on `/builds` or `/guides`, full width and in the
	 * news format: one line saying who posted what, then the entry itself as the
	 * thing to click. It is a pointer, not a post — the entry's page holds the
	 * content, and this disappears if the entry is unpublished.
	 */
	let {
		guide,
		author,
		createdAt,
		id
	}: { guide: GuidePointer; author: string; createdAt: number; id?: number } = $props();

	const base = $derived(guide.category === 'build' ? '/builds' : '/guides');
</script>

<article
	id={id ? `post-${id}` : undefined}
	class="border border-line/70 bg-carbon/40 transition-colors hover:border-ember/60"
>
	<p class="label border-b border-line/60 px-4 py-2 text-faint">
		<span class="text-dim">{author}</span> has posted a {guide.category}
		· <time datetime={new Date(createdAt * 1000).toISOString().slice(0, 10)}>{newsDate(createdAt)}</time>
	</p>

	<a href={guide.path} class="flex flex-col gap-4 p-4 focus:outline-none sm:flex-row sm:items-center">
		{#if guide.image_url}
			<img
				src={guide.image_tile_url ?? guide.image_url}
				alt="Skill tree for the {guide.class_label} build"
				loading="lazy"
				class="w-full border border-line/60 object-cover sm:w-56"
			/>
		{/if}
		<div class="flex flex-col gap-2">
			<div class="flex flex-wrap items-center gap-2">
				<span class="label border border-ember/50 px-1.5 py-0.5 text-ember"
					>{guide.category === 'build' ? 'Build' : 'Guide'}</span
				>
				{#if guide.class_label}
					<span class="label border border-line/70 px-1.5 py-0.5 text-dim">{guide.class_label}</span>
				{/if}
			</div>
			<h3 class="heading text-2xl leading-tight text-ink">
				{guide.title.replace(/^.*? has posted a (?:guide|build): /, '')}
			</h3>
			<span class="label text-ember">Read it on {base} →</span>
		</div>
	</a>
</article>
