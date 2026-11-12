<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import Seo from '$lib/components/Seo.svelte';

	/**
	 * The moderator queue.
	 *
	 * Approving is one click; rejecting demands a note, because the note is the
	 * only thing that explains the decision later. Both are enforced server-side
	 * as well — this page just refuses to send a rejection it knows is empty.
	 */

	interface QueuePost {
		id: number;
		kind_label: string;
		title: string;
		summary: string;
		body_markdown: string | null;
		body_html: string;
		author: string;
		created_at: number;
		path: string;
	}

	let posts = $state<QueuePost[]>([]);
	let phase = $state<'loading' | 'ready' | 'denied' | 'error'>('loading');
	let problem = $state('');
	let notices = $state<Record<number, string>>({});
	let notes = $state<Record<number, string>>({});
	let busy = $state<Record<number, boolean>>({});

	function when(ts: number): string {
		return new Date(ts * 1000).toLocaleString(undefined, {
			day: 'numeric',
			month: 'short',
			hour: '2-digit',
			minute: '2-digit'
		});
	}

	async function load() {
		try {
			const response = await fetch('/api/news/queue', { credentials: 'same-origin' });
			if (response.status === 401 || response.status === 403) {
				phase = 'denied';
				return;
			}
			if (!response.ok) throw new Error(String(response.status));
			const data = (await response.json()) as { posts: QueuePost[] };
			posts = data.posts;
			phase = 'ready';
		} catch {
			problem = 'Could not load the queue.';
			phase = 'error';
		}
	}

	$effect(() => {
		load();
	});

	async function decide(id: number, action: 'approve' | 'reject') {
		if (busy[id]) return;
		const note = (notes[id] ?? '').trim();
		if (action === 'reject' && note.length === 0) {
			notices[id] = 'A note is required to reject: say what was wrong with it.';
			return;
		}
		busy[id] = true;
		notices[id] = '';
		try {
			const response = await fetch(`/api/news/${id}/${action}`, {
				method: 'POST',
				credentials: 'same-origin',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ note })
			});
			const payload = await response.json().catch(() => ({}));
			if (!response.ok) {
				notices[id] = payload?.error ?? `Failed (${response.status}).`;
				return;
			}
			posts = posts.filter((post) => post.id !== id);
		} catch {
			notices[id] = 'Could not reach the server.';
		} finally {
			busy[id] = false;
		}
	}
</script>

<Seo title="News queue — The Commonwealth" description="Review submitted news posts." />

<div class="mx-auto flex w-full max-w-3xl flex-col gap-6">
	<header class="flex flex-col gap-2">
		<a class="label text-faint transition-colors hover:text-ember" href="/news">← News</a>
		<h1 class="heading text-4xl text-ink sm:text-5xl">Review queue</h1>
		<p class="text-sm leading-relaxed text-dim">
			Pending submissions. Approving publishes immediately and is logged against your account.
			Rejecting hides the post everywhere and keeps it in the database for review, with your note
			as the reason.
		</p>
	</header>

	{#if phase === 'loading'}
		<p class="label text-faint">Loading…</p>
	{:else if phase === 'denied'}
		<Panel label="Moderators only">
			<p class="text-sm leading-relaxed text-dim">
				This page needs the <span class="text-ink">news_moderator</span> role. Ask an admin to grant
				it in the dashboard.
			</p>
		</Panel>
	{:else if phase === 'error'}
		<Panel label="Unavailable">
			<p class="text-sm leading-relaxed text-dim">{problem}</p>
		</Panel>
	{:else if posts.length === 0}
		<Panel label="Nothing waiting">
			<p class="text-sm leading-relaxed text-dim">
				The queue is empty. New submissions appear here within a moment of being sent.
			</p>
		</Panel>
	{:else}
		{#each posts as post (post.id)}
			<Panel label="{post.kind_label} · submitted {when(post.created_at)}" meta={post.author}>
				<div class="flex flex-col gap-3">
					<h2 class="heading text-xl text-ink">{post.title}</h2>
					{#if post.summary}
						<p class="text-sm text-dim">{post.summary}</p>
					{/if}
					<!-- Rendered by the server from a whitelist of tags, so it is inert. -->
					<div class="flex flex-col gap-3 border-y border-line/50 py-3 text-sm leading-relaxed text-dim">
						{@html post.body_html}
					</div>

					<label class="flex flex-col gap-1.5">
						<span class="label text-faint">Note (required to reject)</span>
						<textarea
							class="min-h-20 border border-line/70 bg-void/60 px-3 py-2 text-sm text-ink"
							placeholder="What is wrong with it?"
							bind:value={notes[post.id]}
						></textarea>
					</label>

					{#if notices[post.id]}
						<p class="text-sm text-ember">{notices[post.id]}</p>
					{/if}

					<div class="flex flex-wrap items-center gap-3">
						<button
							type="button"
							disabled={busy[post.id]}
							class="heading border border-ember/60 px-4 py-2 text-sm tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void disabled:opacity-50"
							onclick={() => decide(post.id, 'approve')}>Approve</button
						>
						<button
							type="button"
							disabled={busy[post.id] || !(notes[post.id] ?? '').trim()}
							class="heading border border-line/70 px-4 py-2 text-sm tracking-[0.1em] text-dim transition-colors hover:border-red/60 hover:text-ink disabled:opacity-40"
							onclick={() => decide(post.id, 'reject')}>Reject</button
						>
						<a class="label text-faint hover:text-ink" href={post.path} target="_blank" rel="noopener"
							>Preview page</a
						>
					</div>
				</div>
			</Panel>
		{/each}
	{/if}
</div>
