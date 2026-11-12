import { loadFeed } from '$lib/guides/server';

/// The guides list, rendered with its content: the filters, the search term and
/// the sort order are the query string, and the server answers them directly.
export const load = async ({ fetch, request, url }: { fetch: typeof globalThis.fetch; request: Request; url: URL }) =>
	loadFeed('guide', fetch, request, url);
