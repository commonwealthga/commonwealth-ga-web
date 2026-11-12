<script lang="ts">
	import SubmitForm from '$lib/components/guides/SubmitForm.svelte';
	import Seo from '$lib/components/Seo.svelte';

	let { data } = $props();
</script>

<Seo
	title="Post a build — The Commonwealth"
	description="Share a Global Agenda skill tree: screenshot, class, and whether it is for PvP, PvE or both."
/>

<SubmitForm category="build" options={data.options} editing={data.editing} />
