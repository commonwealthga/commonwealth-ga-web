import { loadEntry } from '$lib/guides/server';

/// A build page. The screenshot is the content; this page is where it is legible.
export const load = async ({
	fetch,
	request,
	params
}: {
	fetch: typeof globalThis.fetch;
	request: Request;
	params: { slug: string };
}) => loadEntry('build', params.slug, fetch, request);
