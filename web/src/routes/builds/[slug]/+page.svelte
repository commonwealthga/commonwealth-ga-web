<script lang="ts">
	import EntryDetail from '$lib/components/guides/EntryDetail.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import { breadcrumbSchema, SITE_ORIGIN } from '$lib/site';

	let { data } = $props();
	const entry = $derived(data.entry);

	/**
	 * A build is user-generated content rather than an article: it is a skill tree
	 * with a screenshot, so `CreativeWork` with the author and the image is the
	 * honest description of it.
	 */
	const work = $derived({
		'@type': 'CreativeWork',
		name: entry.title,
		description: entry.summary || `${entry.class_label ?? ''} build for Global Agenda.`,
		author: { '@type': 'Person', name: entry.author },
		dateCreated: new Date(entry.created_at * 1000).toISOString(),
		inLanguage: 'en-GB',
		url: `${SITE_ORIGIN}${entry.path}`,
		about: { '@type': 'VideoGame', name: 'Global Agenda' },
		...(entry.image_url ? { image: `${SITE_ORIGIN}${entry.image_url}` } : {}),
		publisher: { '@id': `${SITE_ORIGIN}/#organization` }
	});

	const structuredData = $derived({
		'@context': 'https://schema.org',
		'@graph': [
			work,
			breadcrumbSchema([
				{ name: 'Home', path: '/' },
				{ name: 'Builds', path: '/builds' },
				{ name: entry.title, path: entry.path }
			])
		]
	});
</script>

<Seo
	title="{entry.title} — Builds — The Commonwealth"
	description={entry.summary ||
		`${entry.class_label ?? ''} build by ${entry.author}${entry.mode_labels.length ? ` for ${entry.mode_labels.join(' and ')}` : ''}.`}
	image={entry.image_url ?? ''}
	structuredData={structuredData}
/>

<EntryDetail {entry} moderator={data.moderator} archived={data.archived} />
