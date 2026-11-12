<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { submit, update } from '$lib/guides/api';
	import type { Draft, Entry, OptionsResponse, UploadedImage } from '$lib/guides/types';
	import ImagePicker from './ImagePicker.svelte';

	/**
	 * The submission form, for both categories.
	 *
	 * A guide asks for writing and tags; a build asks for a screenshot, one class
	 * and at least one queue. The differences are read from the options the API
	 * serves, so the form cannot ask for something the API would refuse — and the
	 * refusal it does send is shown as written.
	 *
	 * Editing reuses this form with `?edit=<id>`: the same fields, submitted as a
	 * PATCH, and only while the entry is pending.
	 */
	let {
		category,
		options,
		editing = null
	}: { category: 'guide' | 'build'; options: OptionsResponse; editing?: Entry | null } = $props();

	const isBuild = $derived(category === 'build');
	const base = $derived(isBuild ? '/builds' : '/guides');

	// What the form starts with, read once: from here on the fields are the
	// author's, and a re-render from the server must not wipe what they typed.
	const seed = untrack(() => editing);

	let title = $state(seed?.title ?? '');
	let summary = $state(seed?.summary ?? '');
	let body = $state(seed?.body_markdown ?? '');
	let tags = $state<string[]>(seed?.tags ?? []);
	let buildClass = $state(seed?.build_class ?? '');
	let modes = $state<string[]>(seed?.modes ?? []);
	let discord = $state(seed?.discord_url ?? '');
	let image = $state<UploadedImage | null>(
		seed?.image_url
			? {
					image_id: seed.image_id ?? 0,
					url: seed.image_url,
					width: 0,
					height: 0,
					bytes: 0
				}
			: null
	);
	// An existing image keeps its id, so editing the title does not silently drop
	// the screenshot the build is entirely about.
	let imageId = $state<number | null>(seed?.image_id ?? null);
	let busy = $state(false);
	let problem = $state('');
	let sent = $state(false);

	const limits = $derived(options.limits);
	const bodyLimit = $derived(isBuild ? limits.description_max : limits.body_max);
	const remaining = $derived(options.posting.allowance?.remaining ?? limits.per_day);

	function toggle(list: string[], value: string): string[] {
		return list.includes(value) ? list.filter((item) => item !== value) : [...list, value];
	}

	function picked(next: UploadedImage | null) {
		image = next;
		imageId = next?.image_id ?? null;
	}

	async function send(event: SubmitEvent) {
		event.preventDefault();
		if (isBuild && !imageId && !editing?.image_url) {
			problem = 'A build needs its skill tree screenshot.';
			return;
		}
		busy = true;
		problem = '';

		const draft: Draft = {
			category,
			title,
			summary: isBuild ? '' : summary,
			body,
			tags: isBuild ? [] : tags,
			class: isBuild ? buildClass : '',
			modes: isBuild ? modes : [],
			discord,
			// Editing without replacing the screenshot keeps the one already stored.
			image_id: isBuild ? (imageId ?? null) : null
		};

		try {
			if (editing) {
				await update(editing.id, draft);
				sent = true;
				void goto(base, { invalidateAll: true });
			} else {
				await submit(draft);
				sent = true;
				void goto(base, { invalidateAll: true });
			}
		} catch (error) {
			problem = error instanceof Error ? error.message : 'that could not be submitted';
			busy = false;
		}
	}
</script>

<form class="flex max-w-3xl flex-col gap-6" onsubmit={send}>
	<header class="flex flex-col gap-2">
		<p class="label text-ember">{isBuild ? 'Builds' : 'Guides'}</p>
		<h1 class="heading text-[clamp(1.8rem,4vw,2.6rem)]">
			{editing ? 'Edit your entry' : isBuild ? 'Post a build' : 'Write a guide'}
		</h1>
		<p class="text-sm leading-relaxed text-dim">
			{#if isBuild}
				One class, the queues you use it in, and a screenshot of the tree. If the build already
				lives in Discord, link the thread — the discussion is half the value.
			{:else}
				Explain something you know well: a strategy, a class, a mode. A moderator reads it before
				it appears, so take the time you need.
			{/if}
		</p>
		{#if !editing}
			<p class="label text-faint">
				{remaining} of {limits.per_day} submissions left today · reviewed before it appears
			</p>
		{/if}
	</header>

	{#if editing?.moderation_note}
		<p class="panel-ticks border border-amber/40 bg-carbon/40 p-4 text-sm text-dim">
			A moderator asked for a change: {editing.moderation_note}
		</p>
	{/if}

	<label class="flex flex-col gap-1.5">
		<span class="label text-dim">Title</span>
		<input
			class="border border-line/90 bg-panel/40 px-3 py-2 text-base text-ink placeholder:text-faint focus:border-ember/60 focus:outline-none"
			bind:value={title}
			maxlength={limits.title_max}
			placeholder={isBuild ? 'e.g. Assault — burst PvP' : 'e.g. Holding the north lane'}
			required
		/>
	</label>

	{#if isBuild}
		<div class="flex flex-col gap-4">
			<div class="flex flex-col gap-2">
				<span class="label text-dim">Screenshot of the skill tree</span>
				<ImagePicker
					value={image}
					maxBytes={limits.max_upload_bytes}
					maxWidth={limits.max_upload_width}
					onchange={picked}
				/>
			</div>

			<div class="flex flex-col gap-2">
				<span class="label text-dim">Class</span>
				<div class="flex flex-wrap gap-1.5">
					{#each options.classes as option (option.value)}
						<button
							type="button"
							aria-pressed={buildClass === option.value}
							onclick={() => (buildClass = buildClass === option.value ? '' : option.value)}
							class="label border px-2.5 py-1 transition-colors {buildClass === option.value
								? 'border-ember/60 bg-ember/10 text-ember'
								: 'border-line/70 text-dim hover:border-ember/60 hover:text-ink'}"
							>{option.label}</button
						>
					{/each}
				</div>
			</div>

			<div class="flex flex-col gap-2">
				<span class="label text-dim">For which queues? <span class="text-faint">(both allowed)</span></span>
				<div class="flex flex-wrap gap-1.5">
					{#each options.modes as option (option.value)}
						<button
							type="button"
							aria-pressed={modes.includes(option.value)}
							onclick={() => (modes = toggle(modes, option.value))}
							class="label border px-2.5 py-1 transition-colors {modes.includes(option.value)
								? 'border-ember/60 bg-ember/10 text-ember'
								: 'border-line/70 text-dim hover:border-ember/60 hover:text-ink'}"
							>{option.label}</button
						>
					{/each}
				</div>
			</div>
		</div>
	{:else}
		<label class="flex flex-col gap-1.5">
			<span class="label text-dim">Summary <span class="text-faint">(optional, shown in the list)</span></span>
			<input
				class="border border-line/90 bg-panel/40 px-3 py-2 text-sm text-ink placeholder:text-faint focus:border-ember/60 focus:outline-none"
				bind:value={summary}
				maxlength={limits.summary_max}
				placeholder="One sentence on what this covers"
			/>
		</label>

		<div class="flex flex-col gap-2">
			<span class="label text-dim">Tags <span class="text-faint">(pick as many as apply)</span></span>
			<div class="flex flex-wrap gap-1.5">
				{#each options.tags as option (option.value)}
					<button
						type="button"
						aria-pressed={tags.includes(option.value)}
						onclick={() => (tags = toggle(tags, option.value))}
						class="label border px-2.5 py-1 transition-colors {tags.includes(option.value)
							? 'border-ember/60 bg-ember/10 text-ember'
							: 'border-line/70 text-dim hover:border-ember/60 hover:text-ink'}"
						>{option.label}</button
					>
				{/each}
			</div>
		</div>
	{/if}

	<label class="flex flex-col gap-1.5">
		<span class="label text-dim">
			{isBuild ? 'Description' : 'The guide'}
			<span class="text-faint">({isBuild ? 'optional — suggested gear, how to play it' : 'Markdown'})</span>
		</span>
		<textarea
			class="min-h-40 border border-line/90 bg-panel/40 px-3 py-2 text-sm leading-relaxed text-ink placeholder:text-faint focus:border-ember/60 focus:outline-none"
			bind:value={body}
			maxlength={bodyLimit}
			placeholder={isBuild ? 'What is this tree for? Gear, rotations, caveats.' : '## What it is\n\nStart writing…'}
		></textarea>
		<span class="label text-faint">{body.length} / {bodyLimit}</span>
	</label>

	{#if isBuild}
		<label class="flex flex-col gap-1.5">
			<span class="label text-dim">Discord thread <span class="text-faint">(optional)</span></span>
			<input
				class="border border-line/90 bg-panel/40 px-3 py-2 text-sm text-ink placeholder:text-faint focus:border-ember/60 focus:outline-none"
				bind:value={discord}
				placeholder="https://discord.com/channels/…"
			/>
			<span class="label text-faint">
				Copy the link from the thread itself, so readers can join the discussion.
			</span>
		</label>
	{/if}

	{#if options.formatting_help}
		<details class="border border-line/60 bg-carbon/30 p-4">
			<summary class="label cursor-pointer text-dim">Formatting</summary>
			<p class="mt-2 whitespace-pre-line text-sm leading-relaxed text-faint">
				{options.formatting_help}
			</p>
		</details>
	{/if}

	{#if problem}
		<p class="panel-ticks border border-red/50 bg-carbon/40 p-4 text-sm text-red">{problem}</p>
	{/if}
	{#if sent}
		<p class="label text-dim">Saved — a moderator will look at it shortly.</p>
	{/if}

	<div class="flex items-center gap-3">
		<button
			type="submit"
			disabled={busy}
			class="heading border border-ember/60 px-5 py-2.5 text-sm tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void disabled:opacity-50"
		>
			{busy ? 'Sending…' : editing ? 'Save changes' : isBuild ? 'Submit build' : 'Submit guide'}
		</button>
		<a href={base} class="label text-faint transition-colors hover:text-ink">Cancel</a>
	</div>
</form>
