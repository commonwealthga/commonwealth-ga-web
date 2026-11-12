<script lang="ts">
	import EntryList from '$lib/components/guides/EntryList.svelte';
	import Seo from '$lib/components/Seo.svelte';

	let { data } = $props();
</script>

<Seo
	title="Builds — The Commonwealth"
	description="Skill tree builds for Global Agenda: Assault, Medic, Recon and Robotic, for PvP, PvE or both, posted by the people using them."
/>

<EntryList category="build" feed={data.feed} options={data.options} />
