<script lang="ts">
	import { page } from '$app/state';
	import { OG_IMAGE, SITE_NAME, SITE_ORIGIN, ogImageFor, siteSchema } from '$lib/site';

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
		image = '',
		noindex = false,
		structuredData
	}: {
		title: string;
		description: string;
		type?: string;
		/** A card of its own — a build's screenshot, say. Defaults to the section's. */
		image?: string;
		noindex?: boolean;
		/** schema.org JSON-LD for this page, if it has any. */
		structuredData?: Record<string, unknown>;
	} = $props();

	const canonical = $derived(`${SITE_ORIGIN}${page.url.pathname}`);
	// The section's card unless the page brought its own.
	const imageUrl = $derived(`${SITE_ORIGIN}${image || ogImageFor(page.url.pathname)}`);

	/**
	 * JSON-LD has to be built as a string and injected as markup.
	 *
	 * Svelte does not interpolate inside `<script>`, so writing
	 * `{JSON.stringify(...)}` in the template does not evaluate it — the source
	 * text itself is served, and Search Console reports the block as unparsable
	 * ("Missing '}' or object member name"). The `<` escape keeps a value from
	 * closing the script early; it is the same character in JSON.
	 */
	/**
	 * One graph per page: what this site is, plus whatever the page itself has to
	 * say. A page that brought its own `@context` (or a `@graph`) is folded in
	 * rather than emitted twice, so there is one script and one model.
	 */
	const jsonLd = $derived.by(() => {
		const page: Record<string, unknown>[] = [];
		if (structuredData) {
			const own = (structuredData as Record<string, unknown>)['@graph'];
			if (Array.isArray(own)) {
				page.push(...(own as Record<string, unknown>[]));
			} else {
				const { '@context': _context, ...rest } = structuredData as Record<string, unknown>;
				page.push(rest);
			}
		}

		const graph = { '@context': 'https://schema.org', '@graph': [...siteSchema(), ...page] };
		const json = JSON.stringify(graph).replace(/</g, '\\u003c');
		return `<script type="application/ld+json">${json}<\/script>`;
	});
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

	<!--
		Always stated, never left to the default. Absent means "index, follow", so
		this only repeats the default — but a page that is deliberately kept out of
		the index says so in the same place, and there is no third case to guess at.
	-->
	<meta name="robots" content={noindex ? 'noindex, follow' : 'index, follow'} />

	<!-- Built above, escaped there. -->
	{@html jsonLd}
</svelte:head>
