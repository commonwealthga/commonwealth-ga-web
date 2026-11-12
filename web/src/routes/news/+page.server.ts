import { env } from '$env/dynamic/private';
import type { FeedResponse } from '$lib/news/types';

/**
 * The first page of the feed, rendered on the server.
 *
 * This is the whole reason the site moved to `adapter-node`: the HTML that leaves
 * here contains every entry in full, so a crawler reads the news, while the
 * browser that receives it navigates client-side from then on — no reload, and no
 * second rendering model for one page.
 *
 * The visitor's cookie is forwarded, because what the feed shows depends on who
 * is asking (a moderator's queue, an author's own pending entries).
 */
export const load = async ({
	fetch,
	request,
	url
}: {
	fetch: typeof globalThis.fetch;
	request: Request;
	url: URL;
}) => {
	const base = (env.API_ORIGIN ?? 'http://127.0.0.1:8080').replace(/\/$/, '');
	const cookie = request.headers.get('cookie') ?? '';

	// The kind filter is part of the URL, so a filtered feed is a link someone
	// can send and a crawler can follow.
	const query = new URLSearchParams({ limit: '10' });
	const kind = url.searchParams.get('kind');
	if (kind) query.set('kind', kind);

	const response = await fetch(`${base}/api/news?${query}`, {
		headers: cookie ? { cookie } : {}
	});

	if (!response.ok) {
		// A feed that cannot load is a page, not an error: say so and carry on.
		return {
			feed: {
				posts: [],
				pending: [],
				total: 0,
				offset: 0,
				moderator: false,
				filter: { kind: null },
				posting: {
					mode: 'unknown',
					signed_in: false,
					moderator: false,
					may_post: false,
					reason: 'The feed could not be loaded just now. Please try again.',
					allowance: null
				}
			} satisfies FeedResponse
		};
	}

	return { feed: (await response.json()) as FeedResponse };
};
