<script lang="ts">
	import EntryDetail from '$lib/components/guides/EntryDetail.svelte';
	import Seo from '$lib/components/Seo.svelte';

	let { data } = $props();
	const entry = $derived(data.entry);
</script>

<Seo
	title="{entry.title} — Guides — The Commonwealth"
	description={entry.summary || `${entry.category_label} by ${entry.author} on The Commonwealth.`}
/>

<EntryDetail {entry} moderator={data.moderator} archived={data.archived} />
