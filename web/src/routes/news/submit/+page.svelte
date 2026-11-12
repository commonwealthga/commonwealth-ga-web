<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
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

	interface Rules {
		mode: string;
		signed_in: boolean;
		moderator: boolean;
		may_post: boolean;
		reason: string | null;
		kinds: KindOption[];
		limits: {
			title_max: number;
			summary_max: number;
			body_max: number;
			per_day: number;
			max_videos: number;
		};
	}

	let rules = $state<Rules | null>(null);
	let kind = $state('news');
	let title = $state('');
	let summary = $state('');
	let body = $state('');
	let videoLinks = $state('');
	let preview = $state('');
	let message = $state('');
	let problem = $state('');
	let phase = $state<'loading' | 'ready' | 'sending' | 'sent' | 'error'>('loading');
	let bodyEl = $state<HTMLTextAreaElement | null>(null);

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

	$effect(() => {
		let cancelled = false;
		fetch('/api/news/posting-rules', { credentials: 'same-origin' })
			.then((response) => (response.ok ? response.json() : Promise.reject(response.status)))
			.then((data: Rules) => {
				if (cancelled) return;
				rules = data;
				phase = 'ready';
			})
			.catch(() => {
				if (cancelled) return;
				problem = 'Could not load the posting rules. Try reloading the page.';
				phase = 'error';
			});
		return () => {
			cancelled = true;
		};
	});

	// Preview, rendered by the server with the same function that stores the
	// body, so it cannot flatter the result.
	let previewTimer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		const source = body;
		clearTimeout(previewTimer);
		if (!hasEditor || !source.trim()) {
			preview = '';
			return;
		}
		previewTimer = setTimeout(() => {
			fetch('/api/news/preview', {
				method: 'POST',
				credentials: 'same-origin',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ body: source })
			})
				.then((response) => (response.ok ? response.json() : Promise.reject(response.status)))
				.then((data: { html: string }) => {
					preview = data.html;
				})
				.catch(() => {
					preview = '';
				});
		}, 350);
	});

	function apply(wrap: string, prefix = false) {
		const el = bodyEl;
		if (!el) return;
		const start = el.selectionStart ?? 0;
		const end = el.selectionEnd ?? 0;
		const chosen = body.slice(start, end);

		if (prefix) {
			const lineStart = body.lastIndexOf('\n', start - 1) + 1;
			body = body.slice(0, lineStart) + wrap + body.slice(lineStart);
			requestAnimationFrame(() => {
				el.focus();
				el.selectionStart = start + wrap.length;
				el.selectionEnd = end + wrap.length;
			});
			return;
		}

		const text = chosen || 'text';
		body = body.slice(0, start) + wrap + text + wrap + body.slice(end);
		requestAnimationFrame(() => {
			el.focus();
			el.selectionStart = start + wrap.length;
			el.selectionEnd = start + wrap.length + text.length;
		});
	}

	function makeLink() {
		const el = bodyEl;
		if (!el) return;
		const start = el.selectionStart ?? 0;
		const end = el.selectionEnd ?? 0;
		const text = body.slice(start, end) || 'link text';
		const snippet = `[${text}](https://)`;
		body = body.slice(0, start) + snippet + body.slice(end);
		requestAnimationFrame(() => {
			el.focus();
			const urlStart = start + text.length + 3;
			el.selectionStart = urlStart;
			el.selectionEnd = urlStart + 8;
		});
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (phase === 'sending') return;
		phase = 'sending';
		problem = '';
		message = '';

		try {
			const response = await fetch('/api/news', {
				method: 'POST',
				credentials: 'same-origin',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					kind,
					// A video carries no title or body: the form does not ask.
					title: hasEditor ? title : '',
					summary: hasEditor ? summary : '',
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
			message =
				payload?.message ?? 'Thanks — a moderator will review it before it appears on the site.';
			phase = 'sent';
			title = '';
			summary = '';
			body = '';
			videoLinks = '';
			preview = '';
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
		<h1 class="heading text-4xl text-ink sm:text-5xl">Post news</h1>
		<p class="text-sm leading-relaxed text-dim">
			Anyone with a game account can post. A moderator reviews every entry before it appears in the
			feed.
		</p>
	</header>

	{#if phase === 'loading'}
		<p class="label text-faint">Loading…</p>
	{:else if rules && !rules.may_post}
		<Panel label="Posting is closed to you">
			<p class="text-sm leading-relaxed text-dim">{rules.reason}</p>
			{#if !rules.signed_in}
				<a
					class="heading mt-4 inline-block border border-ember/60 px-4 py-2 text-sm tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void"
					href="/login">Sign in</a
				>
			{/if}
		</Panel>
	{:else if rules}
		{#if phase === 'sent'}
			<Panel label="In the queue">
				<p class="text-sm leading-relaxed text-dim">{message}</p>
				<div class="mt-4 flex gap-3">
					<a class="label text-ember hover:text-amber" href="/news">Back to the feed</a>
					<button
						type="button"
						class="label text-dim hover:text-ink"
						onclick={() => {
							phase = 'ready';
							message = '';
						}}>Post another</button
					>
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
						<p class="border border-red/50 bg-red-deep/20 px-3 py-2 text-sm text-ink">
							Not a YouTube address: {unusableLinks.join(', ')}
						</p>
					{/if}

					{#if links.length > 0}
						<div class="grid gap-3 sm:grid-cols-2">
							{#each links as link (link)}
								{#if youtubeId(link)}
									<div class="border border-line/60">
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

					<label class="flex flex-col gap-1.5">
						<span class="label flex justify-between text-faint">
							<span>Summary (the line search results show)</span>
							<span>{summary.length}/{rules.limits.summary_max}</span>
						</span>
						<input
							class="border border-line/70 bg-panel/60 px-3 py-2 text-sm text-ink"
							maxlength={rules.limits.summary_max}
							bind:value={summary}
							placeholder="One sentence about the post"
						/>
					</label>

					<div class="flex flex-col gap-1.5">
						<span class="label flex justify-between text-faint">
							<span>Body</span>
							<span>{body.length}/{rules.limits.body_max}</span>
						</span>
						<div class="flex flex-wrap gap-1 border border-line/70 bg-panel/60 px-2 py-1.5">
							{#each [
								{ text: 'B', title: 'Bold', run: () => apply('**') },
								{ text: 'I', title: 'Italic', run: () => apply('_') },
								{ text: 'H2', title: 'Heading', run: () => apply('## ', true) },
								{ text: 'H3', title: 'Smaller heading', run: () => apply('### ', true) },
								{ text: '•', title: 'Bullet list', run: () => apply('- ', true) },
								{ text: '1.', title: 'Numbered list', run: () => apply('1. ', true) },
								{ text: '❝', title: 'Quote', run: () => apply('> ', true) },
								{ text: '‹›', title: 'Code', run: () => apply('`') },
								{ text: '{ }', title: 'Code block', run: () => apply('```\n', true) },
								{ text: '🔗', title: 'Link', run: makeLink }
							] as tool (tool.title)}
								<button
									type="button"
									class="label px-2 py-1 text-dim transition-colors hover:bg-panel-hi hover:text-ink"
									title={tool.title}
									onclick={tool.run}>{tool.text}</button
								>
							{/each}
						</div>
						<textarea
							bind:this={bodyEl}
							class="min-h-64 border border-line/70 bg-panel/60 px-3 py-2 font-mono text-sm leading-relaxed text-ink"
							maxlength={rules.limits.body_max}
							bind:value={body}
							placeholder={'## Prep\n\nBring **two** medkits and _do not_ rush the lift.'}
						></textarea>
					</div>

					{#if preview}
						<div class="border border-line/60 bg-carbon/40 p-4">
							<span class="label text-faint">Preview</span>
							<div class="mt-2 flex flex-col gap-3 text-sm leading-relaxed text-dim">
								{@html preview}
							</div>
						</div>
					{/if}
				{/if}

				{#if problem}
					<p class="border border-red/50 bg-red-deep/20 px-3 py-2 text-sm text-ink">{problem}</p>
				{/if}

				<div class="flex items-center gap-4">
					<button
						type="submit"
						disabled={phase === 'sending' || (isVideo && (links.length === 0 || unusableLinks.length > 0))}
						class="heading border border-ember/60 px-5 py-2.5 text-sm tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void disabled:opacity-50"
					>
						{phase === 'sending' ? 'Sending…' : 'Submit for review'}
					</button>
					<span class="label text-faint">Up to {rules.limits.per_day} posts a day</span>
				</div>
			</form>
		{/if}
	{:else}
		<Panel label="Not available">
			<p class="text-sm leading-relaxed text-dim">{problem}</p>
		</Panel>
	{/if}
</div>
