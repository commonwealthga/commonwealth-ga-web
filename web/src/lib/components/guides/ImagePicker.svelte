<script lang="ts">
	import { uploadImage } from '$lib/guides/api';
	import type { UploadedImage } from '$lib/guides/types';

	/**
	 * The screenshot field.
	 *
	 * Three ways in, all of them the same bytes: a file picker, a drag and drop,
	 * and Ctrl+V of an image copied from Discord or a screenshot tool. Each
	 * uploads immediately and shows what came back — which is the server's
	 * re-encoded copy, not the file that was chosen. What you see before pressing
	 * submit is exactly what will be stored, and a file that cannot be sanitised
	 * fails here rather than after the form is filled in.
	 */
	let {
		value = null,
		maxBytes = 4 * 1024 * 1024,
		maxWidth = 1600,
		onchange
	}: {
		value?: UploadedImage | null;
		maxBytes?: number;
		maxWidth?: number;
		onchange: (image: UploadedImage | null) => void;
	} = $props();

	let dragging = $state(false);
	let busy = $state(false);
	let problem = $state('');
	let input = $state<HTMLInputElement | null>(null);

	async function take(file: File | undefined) {
		if (!file) return;
		problem = '';
		if (file.size > maxBytes) {
			problem = `That image is ${(file.size / 1024 / 1024).toFixed(1)} MB — the limit is ${(maxBytes / 1024 / 1024).toFixed(0)} MB.`;
			return;
		}
		busy = true;
		try {
			const uploaded = await uploadImage(file);
			problem = '';
			onchange(uploaded);
		} catch (error) {
			problem = error instanceof Error ? error.message : 'that image could not be accepted';
		} finally {
			busy = false;
		}
	}

	function onDrop(event: DragEvent) {
		event.preventDefault();
		dragging = false;
		const file = event.dataTransfer?.files?.[0];
		void take(file);
	}

	function onPaste(event: ClipboardEvent) {
		// Only when the field is focused or the page is otherwise idle: a paste
		// into the description textarea is text, not a screenshot.
		const target = event.target as HTMLElement | null;
		if (target && ['INPUT', 'TEXTAREA'].includes(target.tagName) && target !== input) return;
		const file = Array.from(event.clipboardData?.files ?? [])[0];
		if (file) {
			event.preventDefault();
			void take(file);
		}
	}
</script>

<svelte:window onpaste={onPaste} />

<div class="flex flex-col gap-3">
	<button
		type="button"
		onclick={() => input?.click()}
		ondragover={(event) => {
			event.preventDefault();
			dragging = true;
		}}
		ondragleave={() => (dragging = false)}
		ondrop={onDrop}
		class="flex flex-col items-center justify-center gap-2 border border-dashed px-4 py-8 text-center transition-colors {dragging
			? 'border-ember bg-ember/5'
			: 'border-line/80 hover:border-ember/60'}"
	>
		{#if value}
			<img
				src={value.url}
				alt="The screenshot you selected, as it will be stored"
				class="max-h-64 w-auto object-contain"
			/>
			<span class="label text-faint">
				{value.width}×{value.height} · {(value.bytes / 1024).toFixed(0)} KB · click to replace
			</span>
		{:else}
			<span class="heading text-sm tracking-[0.1em] text-dim">
				{busy ? 'Reading the image…' : 'Drop a screenshot, paste it, or click to choose'}
			</span>
			<span class="label text-faint">
				PNG, JPEG or WebP · up to {(maxBytes / 1024 / 1024).toFixed(0)} MB · stored as WebP,
				{maxWidth} px wide at most
			</span>
		{/if}
	</button>

	<input
		bind:this={input}
		type="file"
		accept="image/png,image/jpeg,image/webp"
		class="sr-only"
		onchange={(event) => {
			const target = event.currentTarget as HTMLInputElement;
			void take(target.files?.[0]);
			target.value = '';
		}}
	/>

	{#if problem}
		<p class="label text-red">{problem}</p>
	{/if}
	{#if value}
		<button
			type="button"
			class="label self-start text-faint underline decoration-line underline-offset-4 hover:text-ink"
			onclick={() => onchange(null)}>Remove screenshot</button
		>
	{/if}
</div>
