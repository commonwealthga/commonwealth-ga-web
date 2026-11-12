<script lang="ts">
	import EntryList from '$lib/components/guides/EntryList.svelte';
	import Seo from '$lib/components/Seo.svelte';

	let { data } = $props();
</script>

<Seo
	title="Guides — The Commonwealth"
	description="Strategy guides, class guides and game mode tutorials for Global Agenda, written by the people who play on The Commonwealth."
/>

<EntryList category="guide" feed={data.feed} options={data.options} />
