<script lang="ts">
	import { page } from '$app/state';
	import Button from '$lib/components/Button.svelte';

	let code = $derived(page.status);
	let message = $derived(page.error?.message ?? 'Transmission could not be completed.');
</script>

<svelte:head>
	<title>Signal lost — The Commonwealth</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<section class="flex min-h-[55vh] flex-col justify-center">
	<p class="label text-ember">Not found</p>

	<div class="mt-4 flex items-end gap-4">
		<span class="heading ember-text text-[5rem] leading-none">{code}</span>
		<span class="mb-2 ml-1 text-sm text-dim">{message}</span>
	</div>

	<p class="mt-6 max-w-xl text-sm leading-relaxed text-dim">
		That page does not exist. It may have been renamed — try the navigation above, or start from
		the overview.
	</p>

	<div class="mt-8 flex flex-wrap gap-3">
		<Button href="/">Back to overview</Button>
		<Button href="/missions" variant="outline">See matches</Button>
	</div>
</section>
