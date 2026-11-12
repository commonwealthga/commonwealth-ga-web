<script lang="ts">
	import SubmitForm from '$lib/components/guides/SubmitForm.svelte';
	import Seo from '$lib/components/Seo.svelte';

	let { data } = $props();
</script>

<Seo
	title="Write a guide — The Commonwealth"
	description="Submit a strategy, class or game mode guide for Global Agenda. A moderator reviews it before it appears."
/>

<SubmitForm category="guide" options={data.options} editing={data.editing} />
