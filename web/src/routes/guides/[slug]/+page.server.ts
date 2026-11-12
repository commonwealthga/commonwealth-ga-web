import { loadEntry } from '$lib/guides/server';

/// A guide page, rendered on the server so it can be read (and crawled) as text.
export const load = async ({
	fetch,
	request,
	params
}: {
	fetch: typeof globalThis.fetch;
	request: Request;
	params: { slug: string };
}) => loadEntry('guide', params.slug, fetch, request);
