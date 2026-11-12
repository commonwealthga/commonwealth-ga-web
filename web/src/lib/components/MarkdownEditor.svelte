<script lang="ts">
	/**
	 * The Markdown editor: toolbar, textarea, and a preview rendered by the
	 * server.
	 *
	 * One component for news and for guides, because two editors drifted apart
	 * once already — news grew a toolbar and a preview while the guides form
	 * offered a paragraph of help text instead. Whatever the toolbar can insert,
	 * the renderer supports: the list here and `news::markdown` are the same
	 * subset, so a button cannot promise formatting the page will drop.
	 *
	 * `previewUrl` is the endpoint that renders it. Both sections render with the
	 * same function that stores the body, so the preview cannot flatter the
	 * result.
	 */
	let {
		value = $bindable(''),
		maxlength,
		previewUrl,
		placeholder = 'Start writing…',
		rows = 12
	}: {
		value: string;
		maxlength?: number;
		previewUrl: string;
		placeholder?: string;
		rows?: number;
	} = $props();

	let area = $state<HTMLTextAreaElement | null>(null);
	let preview = $state('');
	let showing = $state<'write' | 'preview'>('write');
	let pending: ReturnType<typeof setTimeout> | undefined;

	const TOOLS = [
		{ text: 'B', title: 'Bold', wrap: '**' },
		{ text: 'I', title: 'Italic', wrap: '_' },
		{ text: 'H2', title: 'Heading', line: '## ' },
		{ text: 'H3', title: 'Smaller heading', line: '### ' },
		{ text: '•', title: 'Bullet list', line: '- ' },
		{ text: '1.', title: 'Numbered list', line: '1. ' },
		{ text: '❝', title: 'Quote', line: '> ' },
		{ text: '⚠', title: 'Warning box', callout: 'WARNING' },
		{ text: 'i', title: 'Info box', callout: 'INFO' },
		{ text: '‹›', title: 'Code', wrap: '`' },
		{ text: '{ }', title: 'Code block', line: '```\n' },
		{ text: '🔗', title: 'Link', link: true }
	] as const;

	// A callout is a blockquote with a marker on its first line, which is what
	// GitHub uses and what the renderer recognises.
	const CALLOUT_HINT = 'Write the box contents here.';

	function focusRange(start: number, end: number) {
		requestAnimationFrame(() => {
			area?.focus();
			if (area) {
				area.selectionStart = start;
				area.selectionEnd = end;
			}
		});
	}

	function run(tool: (typeof TOOLS)[number]) {
		const el = area;
		if (!el) return;
		const start = el.selectionStart ?? 0;
		const end = el.selectionEnd ?? 0;
		const chosen = value.slice(start, end);

		if ('line' in tool) {
			const lineStart = value.lastIndexOf('\n', start - 1) + 1;
			value = value.slice(0, lineStart) + tool.line + value.slice(lineStart);
			focusRange(start + tool.line.length, end + tool.line.length);
			return;
		}

		if ('callout' in tool) {
			const marker = `> [!${tool.callout}]\n> ${chosen || CALLOUT_HINT}\n`;
			const lineStart = value.lastIndexOf('\n', start - 1) + 1;
			const insertAt = start === lineStart ? start : end;
			const prefix = start === lineStart ? '' : '\n';
			value = value.slice(0, insertAt) + prefix + marker + value.slice(insertAt);
			const bodyStart = insertAt + prefix.length + `> [!${tool.callout}]\n> `.length;
			focusRange(bodyStart, bodyStart + (chosen || CALLOUT_HINT).length);
			return;
		}

		if ('link' in tool) {
			const text = chosen || 'link text';
			const snippet = `[${text}](https://)`;
			value = value.slice(0, start) + snippet + value.slice(end);
			const urlStart = start + text.length + 3;
			focusRange(urlStart, urlStart + 8);
			return;
		}

		const text = chosen || 'text';
		value = value.slice(0, start) + tool.wrap + text + tool.wrap + value.slice(end);
		focusRange(start + tool.wrap.length, start + tool.wrap.length + text.length);
	}

	// Debounced, and only while the preview is on screen: nobody needs the server
	// to render text they are not looking at.
	$effect(() => {
		const source = value;
		const visible = showing === 'preview';
		clearTimeout(pending);
		if (!visible || !source.trim()) {
			preview = '';
			return;
		}
		pending = setTimeout(() => {
			fetch(previewUrl, {
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
</script>

<div class="flex flex-col gap-2">
	<div class="flex flex-wrap items-center gap-1 border border-line/70 bg-panel/60 px-2 py-1.5">
		{#each TOOLS as tool (tool.title)}
			<button
				type="button"
				class="label px-2 py-1 text-dim transition-colors hover:bg-panel-hi hover:text-ink"
				title={tool.title}
				aria-label={tool.title}
				onclick={() => run(tool)}>{tool.text}</button
			>
		{/each}
		<div class="ml-auto flex items-center gap-1">
			{#each [{ value: 'write', label: 'Write' }, { value: 'preview', label: 'Preview' }] as mode (mode.value)}
				<button
					type="button"
					aria-pressed={showing === mode.value}
					class="label px-2 py-1 transition-colors {showing === mode.value
						? 'text-ember'
						: 'text-faint hover:text-ink'}"
					onclick={() => (showing = mode.value as 'write' | 'preview')}>{mode.label}</button
				>
			{/each}
		</div>
	</div>

	{#if showing === 'preview'}
		{#if preview}
			<!-- Rendered on the server from the whitelist, so this is inert. -->
			<div
				class="prose-news min-h-40 border border-line/90 bg-panel/40 px-3 py-2 text-sm leading-relaxed text-dim"
			>
				{@html preview}
			</div>
		{:else}
			<p
				class="label flex min-h-40 items-center border border-line/90 bg-panel/40 px-3 py-2 text-faint"
			>
				{value.trim() ? 'Rendering…' : 'Nothing to preview yet.'}
			</p>
		{/if}
	{:else}
		<textarea
			bind:this={area}
			bind:value
			{maxlength}
			{placeholder}
			{rows}
			class="min-h-40 w-full border border-line/90 bg-panel/40 px-3 py-2 text-sm leading-relaxed text-ink placeholder:text-faint focus:border-ember/60 focus:outline-none"
		></textarea>
	{/if}

	{#if maxlength}
		<span class="label self-end text-faint">{value.length} / {maxlength}</span>
	{/if}
</div>
