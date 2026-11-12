<script lang="ts">
	import { decide, thumb } from '$lib/guides/api';
	import type { Entry } from '$lib/guides/types';
	import { newsDate } from '$lib/format';

	/**
	 * One tile in the grid.
	 *
	 * A build leads with its screenshot — that is the thing you came to look at —
	 * and a guide leads with its title, because it is writing. Both are links, so
	 * neither carries the corner ticks: the whole tile is the target.
	 */
	let { entry, moderator = false }: { entry: Entry; moderator?: boolean } = $props();

	const pending = $derived(entry.status === 'pending');
	const rejected = $derived(entry.status === 'rejected');
	const href = $derived(entry.path);

	let liked = $state<number | null>(null);
	let reactedOverride = $state<boolean | null>(null);
	const count = $derived(liked ?? entry.thumbs_up);
	const reactedNow = $derived(reactedOverride ?? entry.reacted);
	let notice = $state('');
	let busy = $state(false);

	const CONFIRM: Record<string, string> = {
		approve: 'publish this entry? It becomes visible to everyone.',
		unapprove: 'unpublish this entry? It returns to the queue and the author can fix it.',
		reject: 'delete this entry? It disappears from the list everywhere and is kept only for review.'
	};

	const REASON: Record<string, string> = {
		unapprove: 'What should the author fix? (shown to them)',
		reject: 'Why is this being deleted? (kept with the entry)',
		note: 'What should the author change? (shown to them; the entry stays in the queue)'
	};

	async function like(event: MouseEvent) {
		// The tile is a link; the thumb is its own control.
		event.preventDefault();
		event.stopPropagation();
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
		if (CONFIRM[action] && !window.confirm(`About to ${CONFIRM[action]}\n\n"${entry.title}"`)) {
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
			await decide(entry.id, action, note);
			window.location.reload();
		} catch (error) {
			busy = false;
			notice = error instanceof Error ? error.message : 'could not complete that';
		}
	}
</script>

<article class="relative flex h-full flex-col border border-line/70 bg-carbon/40 transition-colors hover:border-ember/60">
	<a href={href} class="flex flex-1 flex-col focus:outline-none">
		{#if entry.image_url}
			<div class="border-b border-line/60 bg-void/40">
				<img
					src={entry.image_tile_url ?? entry.image_url}
					alt="Skill tree for the {entry.class_label} build"
					loading="lazy"
					class="aspect-[16/10] w-full object-cover"
				/>
			</div>
		{/if}

		<div class="flex flex-1 flex-col gap-2 p-4">
			<div class="flex flex-wrap items-center gap-2">
				{#if entry.class_label}
					<span class="label border border-ember/50 px-1.5 py-0.5 text-ember"
						>{entry.class_label}</span
					>
				{/if}
				{#each entry.mode_labels as mode (mode)}
					<span class="label border border-line/70 px-1.5 py-0.5 text-dim">{mode}</span>
				{/each}
				{#each entry.tag_labels as tag (tag)}
					<span class="label text-faint">#{tag}</span>
				{/each}
			</div>

			<h3 class="heading text-xl leading-tight text-ink">{entry.title}</h3>

			{#if entry.summary}
				<p class="text-sm leading-relaxed text-dim">{entry.summary}</p>
			{:else if entry.body_html}
				<!-- A build's description, flattened: the tile is not the article. -->
				<p class="line-clamp-3 text-sm leading-relaxed text-dim">
					{entry.body_html.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim()}
				</p>
			{/if}

			<p class="label mt-auto pt-2 text-faint">
				{entry.author} · {newsDate(entry.created_at)}
			</p>
		</div>
	</a>

	{#if pending || rejected}
		<p class="label border-t border-line/60 px-4 py-2 text-amber">
			{#if rejected}
				Deleted by a moderator{entry.moderation_note ? `: ${entry.moderation_note}` : ''}
			{:else if entry.moderation_note}
				Change requested: {entry.moderation_note}
			{:else}
				Waiting for review — only you and a moderator can see this.
			{/if}
		</p>
	{/if}

	<div class="flex items-center gap-3 border-t border-line/60 px-4 py-2">
		<button
			type="button"
			onclick={like}
			aria-pressed={reactedNow}
			aria-label={reactedNow ? 'Remove your thumbs up' : 'Thumbs up'}
			title={reactedNow ? 'You liked this — click to undo' : 'Like this entry'}
			class="heading flex items-center gap-1.5 border px-2 py-1 text-xs tracking-[0.1em] transition-colors {reactedNow
				? 'border-ember/60 bg-ember/10 text-ember hover:border-ember hover:bg-ember/15'
				: 'border-line/70 text-dim hover:border-ember/60 hover:text-ink'}"
		>
			<span aria-hidden="true">👍</span><span>{count}</span>
		</button>
		{#if entry.discord_url}
			<a
				href={entry.discord_url}
				target="_blank"
				rel="noopener noreferrer nofollow"
				class="label text-faint transition-colors hover:text-ember">Discuss on Discord →</a
			>
		{/if}
		{#if entry.can_edit}
			<a
				href="/{entry.category}s/submit?edit={entry.id}"
				class="label ml-auto text-ember underline decoration-ember/40 underline-offset-4 hover:text-amber"
				>Edit</a
			>
		{/if}
		<span class="label text-faint {entry.can_edit ? '' : 'ml-auto'}">{notice}</span>
	</div>

	{#if moderator}
		<div class="flex flex-wrap items-center gap-2 border-t border-line/60 px-4 py-2">
			<span class="label text-faint">Moderator</span>
			{#if pending}
				<button
					type="button"
					disabled={busy}
					class="heading border border-ember/60 px-2 py-1 text-[0.65rem] tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void disabled:opacity-50"
					onclick={() => act('approve')}>Approve</button
				>
				<button
					type="button"
					disabled={busy}
					class="heading border border-amber/50 px-2 py-1 text-[0.65rem] tracking-[0.1em] text-amber transition-colors hover:bg-amber hover:text-void disabled:opacity-50"
					onclick={() => act('note')}>Request changes</button
				>
			{:else if !rejected}
				<button
					type="button"
					disabled={busy}
					class="heading border border-ember/60 px-2 py-1 text-[0.65rem] tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void disabled:opacity-50"
					onclick={() => act('unapprove')}>Unpublish</button
				>
			{/if}
			{#if !rejected}
				<button
					type="button"
					disabled={busy}
					class="heading border border-line/70 px-2 py-1 text-[0.65rem] tracking-[0.1em] text-dim transition-colors hover:border-red/60 hover:text-ink disabled:opacity-50"
					onclick={() => act('reject')}>Delete</button
				>
			{/if}
		</div>
	{/if}
</article>
