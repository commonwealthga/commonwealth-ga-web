import { loadFeed } from '$lib/guides/server';

/// The builds list. Same page component as the guides, different category: a
/// build's metadata is class + queue rather than tags.
export const load = async ({ fetch, request, url }: { fetch: typeof globalThis.fetch; request: Request; url: URL }) =>
	loadFeed('build', fetch, request, url);
