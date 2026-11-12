<script lang="ts">
	import { untrack } from 'svelte';
	import type { PageData } from './$types';
	import Panel from '$lib/components/Panel.svelte';
	import MarkdownEditor from '$lib/components/MarkdownEditor.svelte';
	import Seo from '$lib/components/Seo.svelte';

	/**
	 * The submission form.
	 *
	 * Each kind asks for only what it needs: an article gets a title and an
	 * editor, a video gets a box for links and nothing else — no invented title,
	 * no empty body to fill. Everything here is enforced again on the server.
	 */

	interface KindOption {
		value: string;
		label: string;
		phrase: string;
		requires_link: boolean;
		requires_body: boolean;
		has_editor: boolean;
	}

	interface Allowance {
		limit: number;
		used: number;
		remaining: number;
		resets_at: number | null;
		resets_at_clock: string | null;
	}

	interface Rules {
		mode: string;
		signed_in: boolean;
		moderator: boolean;
		may_post: boolean;
		reason: string | null;
		allowance: Allowance | null;
		kinds: KindOption[];
		limits: {
			title_max: number;
			body_max: number;
			per_day: number;
			max_videos: number;
		};
	}

	let { data }: { data: PageData } = $props();

	// The rules arrive with the page, so the form is in the HTML rather than
	// appearing once JavaScript has asked for them.
	const rules = $derived(data.rules);

	// What the form starts with, read once: from here the fields are the author's.
	const seed = untrack(() => data.post);

	let kind = $state(seed?.kind ?? 'news');
	let title = $state(seed?.title ?? '');
	let body = $state(seed?.body_markdown ?? '');
	let videoLinks = $state(
		(seed?.videos ?? []).map((id: string) => 'https://youtu.be/' + id).join('\n')
	);
	let message = $state('');
	let problem = $state('');
	let phase = $state<'ready' | 'sending' | 'sent'>('ready');

	const editId = $derived(data.editId);
	const editBlocked = $derived(data.blocked);

	const selected = $derived(rules?.kinds.find((option) => option.value === kind) ?? null);
	const isVideo = $derived(selected?.requires_link ?? false);
	const hasEditor = $derived(selected?.has_editor ?? true);

	/** The links as the server will parse them: one per line, blanks dropped. */
	const links = $derived(
		videoLinks
			.split(/[\n,]/)
			.map((line) => line.trim())
			.filter((line) => line.length > 0)
	);

	const youtubeId = (url: string): string | null => {
		const patterns = [
			/(?:youtu\.be\/)([\w-]{6,20})/,
			/[?&]v=([\w-]{6,20})/,
			/(?:embed|shorts|live)\/([\w-]{6,20})/
		];
		for (const pattern of patterns) {
			const match = url.match(pattern);
			if (match) return match[1];
		}
		return null;
	};

	const unusableLinks = $derived(links.filter((link) => youtubeId(link) === null));

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (phase === 'sending') return;
		phase = 'sending';
		problem = '';
		message = '';

		try {
			const response = await fetch(editId ? '/api/news/' + editId : '/api/news', {
				method: editId ? 'PATCH' : 'POST',
				credentials: 'same-origin',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					kind,
					// A video carries no title or body: the form does not ask.
					title: hasEditor ? title : '',
					body: hasEditor ? body : '',
					links: isVideo ? links : []
				})
			});
			const payload = await response.json().catch(() => ({}));
			if (!response.ok) {
				problem = payload?.error ?? `The server refused the post (${response.status}).`;
				phase = 'ready';
				return;
			}
			message = editId
				? 'Saved. The entry is back in the queue with your changes.'
				: (payload?.message ??
					'Thanks — a moderator will review it before it appears on the site.');
			phase = 'sent';
			title = '';
			body = '';
			videoLinks = '';
		} catch {
			problem = 'Could not reach the server. Your text is still here — try again.';
			phase = 'ready';
		}
	}
</script>

<Seo
	title="Submit a post — The Commonwealth"
	description="Post a news entry, server update, article or video for the Commonwealth Global Agenda community."
/>

<div class="mx-auto flex w-full max-w-3xl flex-col gap-6">
	<header class="flex flex-col gap-2">
		<a class="label text-faint transition-colors hover:text-ember" href="/news">← News</a>
		<h1 class="heading text-4xl text-ink sm:text-5xl">
			{editId ? 'Edit entry' : 'Post news'}
		</h1>
		<p class="text-sm leading-relaxed text-dim">
			{#if editId}
				Changes go back into the review queue — an edited entry is not published until a moderator
				approves it again.
			{:else}
				Anyone with a game account can post. A moderator reviews every entry before it appears in
				the feed.
			{/if}
		</p>
	</header>

	{#if !rules.may_post}
		<Panel label="Posting is closed to you">
			<p class="text-sm leading-relaxed text-dim">{rules.reason}</p>
			{#if !rules.signed_in}
				<a
					class="heading mt-4 inline-block border border-ember/60 px-4 py-2 text-sm tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void"
					href="/login">Sign in</a
				>
			{/if}
		</Panel>
	{:else if editBlocked}
		<Panel label="Cannot be edited here">
			<p class="text-sm leading-relaxed text-dim">{editBlocked}</p>
			<a class="label mt-4 inline-block text-ember hover:text-amber" href="/news">Back to the feed</a>
		</Panel>
	{:else if rules && rules.may_post && rules.allowance && rules.allowance.remaining === 0 && !editId}
		<Panel label="No posts left today">
			<p class="text-sm leading-relaxed text-dim">
				You have used all {rules.allowance.limit} of today's posts.
				{#if rules.allowance.resets_at_clock}
					You can post again after {rules.allowance.resets_at_clock}.
				{:else}
					Come back tomorrow.
				{/if}
			</p>
			<a class="label mt-4 inline-block text-ember hover:text-amber" href="/news">Back to the feed</a>
		</Panel>
	{:else if rules}
		{#if phase === 'sent'}
			<Panel label="In the queue">
				<p class="text-sm leading-relaxed text-dim">{message}</p>
				<div class="mt-4 flex gap-3">
					<a class="label text-ember hover:text-amber" href="/news">Back to the feed</a>
					{#if !editId}
						<button
							type="button"
							class="label text-dim hover:text-ink"
							onclick={() => {
								phase = 'ready';
								message = '';
							}}>Post another</button
						>
					{/if}
				</div>
			</Panel>
		{:else}
			<form class="flex flex-col gap-5" onsubmit={submit}>
				<fieldset class="flex flex-col gap-2">
					<legend class="label text-faint">What are you posting?</legend>
					<div class="flex flex-wrap gap-2">
						{#each rules.kinds as option (option.value)}
							<label
								class="label cursor-pointer border px-3 py-2 transition-colors {kind === option.value
									? 'border-ember/60 text-ember'
									: 'border-line/70 text-dim hover:border-ember/40 hover:text-ink'}"
							>
								<input
									class="sr-only"
									type="radio"
									name="kind"
									value={option.value}
									bind:group={kind}
								/>
								{option.label}
							</label>
						{/each}
					</div>
				</fieldset>

				{#if isVideo}
					<label class="flex flex-col gap-1.5">
						<span class="label flex justify-between text-faint">
							<span>YouTube links — one per line</span>
							<span>{links.length}/{rules.limits.max_videos}</span>
						</span>
						<textarea
							class="min-h-40 border border-line/70 bg-panel/60 px-3 py-2 font-mono text-sm leading-relaxed text-ink"
							bind:value={videoLinks}
							placeholder={'https://youtu.be/dQw4w9WgXcQ\nhttps://www.youtube.com/watch?v=dQw4w9WgXcQ'}
						></textarea>
						<span class="text-xs leading-relaxed text-faint">
							Nothing else is needed: the feed shows your name, the date and the videos.
						</span>
					</label>

					{#if unusableLinks.length > 0}
						<p class="panel-ticks border border-red/50 bg-red-deep/20 px-3 py-2 text-sm text-ink">
							Not a YouTube address: {unusableLinks.join(', ')}
						</p>
					{/if}

					{#if links.length > 0}
						<div class="grid gap-3 sm:grid-cols-2">
							{#each links as link (link)}
								{#if youtubeId(link)}
									<div class="panel-ticks border border-line/60">
										<iframe
											class="aspect-video w-full"
											src="https://www.youtube-nocookie.com/embed/{youtubeId(link)}"
											title="Video preview"
											loading="lazy"
											allowfullscreen
										></iframe>
									</div>
								{/if}
							{/each}
						</div>
					{/if}
				{:else}
					<label class="flex flex-col gap-1.5">
						<span class="label flex justify-between text-faint">
							<span>Title</span>
							<span>{title.length}/{rules.limits.title_max}</span>
						</span>
						<input
							class="border border-line/70 bg-panel/60 px-3 py-2 text-base text-ink"
							maxlength={rules.limits.title_max}
							bind:value={title}
							placeholder="Surfside Atoll is live"
						/>
					</label>

					<div class="flex flex-col gap-1.5">
						<span class="label text-faint">Body</span>
						<MarkdownEditor
							bind:value={body}
							maxlength={rules.limits.body_max}
							previewUrl="/api/news/preview"
							placeholder={'## Prep\n\nBring **two** medkits and _do not_ rush the lift.'}
						/>
					</div>
				{/if}

				{#if problem}
					<p class="panel-ticks border border-red/50 bg-red-deep/20 px-3 py-2 text-sm text-ink">{problem}</p>
				{/if}

				<div class="flex items-center gap-4">
					<button
						type="submit"
						disabled={phase === 'sending' || (isVideo && (links.length === 0 || unusableLinks.length > 0))}
						class="heading border border-ember/60 px-5 py-2.5 text-sm tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void disabled:opacity-50"
					>
						{phase === 'sending'
							? 'Sending…'
							: editId
								? 'Save changes'
								: 'Submit for review'}
					</button>
					{#if editId}
						<span class="label text-faint">Editing an entry in the queue</span>
					{:else if rules.allowance}
						<span class="label text-faint">
							{rules.allowance.remaining} of {rules.allowance.limit} left today
						</span>
					{/if}
				</div>
			</form>
		{/if}
	{:else}
		<Panel label="Not available">
			<p class="text-sm leading-relaxed text-dim">{problem}</p>
		</Panel>
	{/if}
</div>
