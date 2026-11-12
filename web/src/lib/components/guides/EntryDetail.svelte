<script lang="ts">
	import { decide, thumb } from '$lib/guides/api';
	import type { Entry } from '$lib/guides/types';
	import { newsDate } from '$lib/format';

	/**
	 * One entry in full: the article or the screenshot, the metadata, the
	 * moderator's controls, and (for the author) whatever a moderator asked for.
	 */
	let {
		entry,
		moderator = false,
		archived = false
	}: { entry: Entry; moderator?: boolean; archived?: boolean } = $props();

	const isBuild = $derived(entry.category === 'build');
	const base = $derived(isBuild ? '/builds' : '/guides');

	let liked = $state<number | null>(null);
	let reactedOverride = $state<boolean | null>(null);
	const count = $derived(liked ?? entry.thumbs_up);
	const reactedNow = $derived(reactedOverride ?? entry.reacted);
	let notice = $state('');
	let busy = $state(false);

	const CONFIRM: Record<string, string> = {
		approve: 'publish this entry? It becomes visible to everyone.',
		unapprove: 'unpublish this entry? It returns to the queue and the author can fix it.',
		reject: 'delete this entry? It disappears from everywhere and is kept only for review.'
	};
	const REASON: Record<string, string> = {
		unapprove: 'What should the author fix? (shown to them)',
		reject: 'Why is this being deleted? (kept with the entry)',
		note: 'What should the author change? (shown to them; the entry stays in the queue)'
	};

	async function like() {
		try {
			const result = await thumb(entry.id, !reactedNow);
			liked = result.thumbs_up;
			reactedOverride = result.reacted;
			notice = '';
		} catch (error) {
			notice = error instanceof Error ? error.message : 'could not react';
		}
	}

	async function act(action: string) {
		if (CONFIRM[action] && !window.confirm(`About to ${CONFIRM[action]}\n\n"${entry.title}"`)) return;
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
			await decide(entry.id, action, note);
			window.location.reload();
		} catch (error) {
			busy = false;
			notice = error instanceof Error ? error.message : 'could not complete that';
		}
	}
</script>

<article class="flex flex-col gap-8">
	<header class="flex flex-col gap-3">
		<p class="label text-faint">
			<a href={base} class="text-ember transition-colors hover:text-amber">{isBuild ? 'Builds' : 'Guides'}</a>
			{#if entry.class_label}
				· <span class="text-ember">{entry.class_label}</span>
			{/if}
			{#each entry.mode_labels as mode (mode)}
				· <span>{mode}</span>
			{/each}
		</p>

		<h1 class="heading text-[clamp(1.9rem,4.5vw,3rem)] leading-tight text-ink">{entry.title}</h1>

		<p class="label text-faint">
			{entry.author} · <time datetime={new Date(entry.created_at * 1000).toISOString().slice(0, 10)}
				>{newsDate(entry.created_at)}</time
			>
			{#if entry.tags.length}
				· {entry.tag_labels.map((tag) => `#${tag}`).join(' ')}
			{/if}
		</p>

		{#if archived}
			<p class="panel-ticks border border-red/50 bg-carbon/40 p-4 text-sm text-dim">
				A moderator deleted this entry{entry.moderation_note
					? `: ${entry.moderation_note}`
					: '.'} Only you and moderators can see it.
			</p>
		{:else if entry.status === 'pending'}
			<p class="panel-ticks border border-amber/40 bg-carbon/40 p-4 text-sm text-dim">
				{entry.moderation_note
					? `A moderator asked for a change: ${entry.moderation_note}`
					: 'Waiting for review — only you and a moderator can see this.'}
				{#if entry.can_edit}
					<a
						class="heading ml-2 underline decoration-ember/60 underline-offset-4 hover:text-ember"
						href="{base}/submit?edit={entry.id}">Edit this entry</a
					>
				{/if}
			</p>
		{/if}
	</header>

	{#if entry.image_url}
		<figure class="panel-ticks border border-line/70 bg-void/40">
			<img
				src={entry.image_url}
				alt="Skill tree for the {entry.class_label} build"
				class="w-full object-contain"
			/>
			{#if entry.class_label}
				<figcaption class="label border-t border-line/60 px-4 py-2 text-faint">
					{entry.class_label}{#if entry.mode_labels.length}
						· {entry.mode_labels.join(' / ')}{/if}
				</figcaption>
			{/if}
		</figure>
	{/if}

	{#if entry.summary}
		<p class="text-lg leading-relaxed text-dim">{entry.summary}</p>
	{/if}

	{#if entry.body_html}
		<!-- Rendered by the server from the whitelist, so this is inert. -->
		<div class="prose-news flex max-w-3xl flex-col gap-4 text-base leading-relaxed text-dim">
			{@html entry.body_html}
		</div>
	{/if}

	{#if entry.discord_url}
		<div class="panel-ticks border border-line/60 bg-carbon/40 p-4">
			<p class="text-sm text-dim">
				This build is discussed on Discord.
				<a
					href={entry.discord_url}
					target="_blank"
					rel="noopener noreferrer nofollow"
					class="heading ml-1 text-ember underline decoration-ember/40 underline-offset-4 hover:text-amber"
					>Open the thread →</a
				>
			</p>
		</div>
	{/if}

	<div class="flex flex-wrap items-center gap-4 border-t border-line/70 pt-4">
		<button
			type="button"
			onclick={like}
			aria-pressed={reactedNow}
			aria-label={reactedNow ? 'Remove your thumbs up' : 'Thumbs up'}
			title={reactedNow ? 'You liked this — click to undo' : 'Like this entry'}
			class="heading flex items-center gap-2 border px-3 py-1.5 text-sm tracking-[0.1em] transition-colors {reactedNow
				? 'border-ember/60 bg-ember/10 text-ember hover:border-ember hover:bg-ember/15'
				: 'border-line/70 text-dim hover:border-ember/60 hover:text-ink'}"
		>
			<span aria-hidden="true">👍</span><span>{count}</span>
		</button>
		<span class="label text-faint">{notice}</span>
	</div>

	{#if moderator}
		<div class="flex flex-wrap items-center gap-2 border-t border-line/60 pt-3">
			<span class="label text-faint">Moderator</span>
			{#if entry.status === 'pending'}
				<button
					type="button"
					disabled={busy}
					class="heading border border-ember/60 px-3 py-1.5 text-xs tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void disabled:opacity-50"
					onclick={() => act('approve')}>Approve</button
				>
				<button
					type="button"
					disabled={busy}
					class="heading border border-amber/50 px-3 py-1.5 text-xs tracking-[0.1em] text-amber transition-colors hover:bg-amber hover:text-void disabled:opacity-50"
					onclick={() => act('note')}>Request changes</button
				>
			{:else if entry.status === 'approved'}
				<button
					type="button"
					disabled={busy}
					class="heading border border-ember/60 px-3 py-1.5 text-xs tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void disabled:opacity-50"
					onclick={() => act('unapprove')}>Unpublish</button
				>
			{/if}
			{#if entry.status !== 'rejected'}
				<button
					type="button"
					disabled={busy}
					class="heading border border-line/70 px-3 py-1.5 text-xs tracking-[0.1em] text-dim transition-colors hover:border-red/60 hover:text-ink disabled:opacity-50"
					onclick={() => act('reject')}>Delete</button
				>
			{/if}
		</div>
	{/if}

	<p class="label text-faint">
		<a href={base} class="transition-colors hover:text-ink">← All {isBuild ? 'builds' : 'guides'}</a>
	</p>
</article>
