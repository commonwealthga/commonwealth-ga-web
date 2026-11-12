<script lang="ts">
	import { page } from '$app/state';
	import { OG_IMAGE, SITE_NAME, SITE_ORIGIN } from '$lib/site';

	/**
	 * Everything a crawler or a link unfurler needs, per page.
	 *
	 * Takes the two strings a page already had to write (title, description) and
	 * derives the rest: canonical, Open Graph, Twitter card. Keeping them
	 * together is what stops a page from having a title and a stale og:title.
	 */
	let {
		title,
		description,
		type = 'website',
		image = OG_IMAGE,
		noindex = false,
		structuredData
	}: {
		title: string;
		description: string;
		type?: string;
		image?: string;
		noindex?: boolean;
		/** schema.org JSON-LD for this page, if it has any. */
		structuredData?: Record<string, unknown>;
	} = $props();

	const canonical = $derived(`${SITE_ORIGIN}${page.url.pathname}`);
	const imageUrl = $derived(`${SITE_ORIGIN}${image}`);

	/**
	 * JSON-LD has to be built as a string and injected as markup.
	 *
	 * Svelte does not interpolate inside `<script>`, so writing
	 * `{JSON.stringify(...)}` in the template does not evaluate it — the source
	 * text itself is served, and Search Console reports the block as unparsable
	 * ("Missing '}' or object member name"). The `<` escape keeps a value from
	 * closing the script early; it is the same character in JSON.
	 */
	const jsonLd = $derived(
		structuredData
			? `<script type="application/ld+json">${JSON.stringify(structuredData).replace(/</g, '\\u003c')}<\/script>`
			: ''
	);
</script>

<svelte:head>
	<title>{title}</title>
	<meta name="description" content={description} />
	<link rel="canonical" href={canonical} />

	<meta property="og:site_name" content={SITE_NAME} />
	<meta property="og:type" content={type} />
	<meta property="og:title" content={title} />
	<meta property="og:description" content={description} />
	<meta property="og:url" content={canonical} />
	<meta property="og:image" content={imageUrl} />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta property="og:image:alt" content={title} />
	<meta property="og:locale" content="en_GB" />

	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:title" content={title} />
	<meta name="twitter:description" content={description} />
	<meta name="twitter:image" content={imageUrl} />

	{#if noindex}
		<meta name="robots" content="noindex, follow" />
	{/if}

	<!-- Built above, escaped there. -->
	{@html jsonLd}
</svelte:head>
