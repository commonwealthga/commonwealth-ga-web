<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		label = '',
		meta = '',
		class: className = '',
		bodyClass = '',
		children
	}: {
		label?: string;
		meta?: string;
		class?: string;
		bodyClass?: string;
		children: Snippet;
	} = $props();
</script>

<!-- Calm panel: hairline frame and two corner ticks, not four. -->
<section class="panel-ticks border border-line/80 bg-panel/50 {className}">

	{#if label}
		<header class="flex items-baseline gap-3 border-b border-line/60 px-5 py-3">
			<h2 class="label text-dim">{label}</h2>
			{#if meta}
				<span class="label ml-auto text-faint">{meta}</span>
			{/if}
		</header>
	{/if}

	<div class="p-5 {bodyClass}">
		{@render children()}
	</div>
</section>
