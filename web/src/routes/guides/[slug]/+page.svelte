<script lang="ts">
	import EntryDetail from '$lib/components/guides/EntryDetail.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import { breadcrumbSchema, SITE_ORIGIN } from '$lib/site';

	let { data } = $props();
	const entry = $derived(data.entry);

	/**
	 * The entry as an Article, with the trail back to the section: this is what
	 * tells a crawler that /guides/3-x is a page *about* something, not a listing.
	 */
	const article = $derived({
		'@type': 'Article',
		headline: entry.title,
		description: entry.summary,
		author: { '@type': 'Person', name: entry.author },
		datePublished: new Date(entry.created_at * 1000).toISOString(),
		inLanguage: 'en-GB',
		url: `${SITE_ORIGIN}${entry.path}`,
		mainEntityOfPage: `${SITE_ORIGIN}${entry.path}`,
		...(entry.image_url ? { image: `${SITE_ORIGIN}${entry.image_url}` } : {}),
		publisher: { '@id': `${SITE_ORIGIN}/#organization` }
	});

	const structuredData = $derived({
		'@context': 'https://schema.org',
		'@graph': [
			article,
			breadcrumbSchema([
				{ name: 'Home', path: '/' },
				{ name: 'Guides', path: '/guides' },
				{ name: entry.title, path: entry.path }
			])
		]
	});
</script>

<Seo
	title="{entry.title} — Guides — The Commonwealth"
	description={entry.summary || `${entry.category_label} by ${entry.author} on The Commonwealth.`}
	image={entry.image_url ?? ''}
	structuredData={structuredData}
/>

<EntryDetail {entry} moderator={data.moderator} archived={data.archived} />
