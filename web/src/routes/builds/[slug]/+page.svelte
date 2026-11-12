<script lang="ts">
	import EntryDetail from '$lib/components/guides/EntryDetail.svelte';
	import Seo from '$lib/components/Seo.svelte';

	let { data } = $props();
	const entry = $derived(data.entry);
</script>

<Seo
	title="{entry.title} — Builds — The Commonwealth"
	description={entry.summary ||
		`${entry.class_label ?? ''} build by ${entry.author}${entry.mode_labels.length ? ` for ${entry.mode_labels.join(' and ')}` : ''}.`}
/>

<EntryDetail {entry} moderator={data.moderator} archived={data.archived} />
