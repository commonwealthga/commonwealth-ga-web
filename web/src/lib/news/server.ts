/**
 * Server-side fetches for the news section.
 *
 * Only `+page.server.ts` files import this: `$env/dynamic/private` is server-only.
 * The same shape as `$lib/guides/server.ts`, so the two sections reach the API
 * the same way instead of one of them doing it from the browser.
 */
import { env } from '$env/dynamic/private';
import { error } from '@sveltejs/kit';
import type { NewsPost, RulesResponse } from './types';

export const apiBase = () => (env.API_ORIGIN ?? 'http://127.0.0.1:8080').replace(/\/$/, '');

type Fetcher = typeof globalThis.fetch;

async function call<T>(fetcher: Fetcher, request: Request, path: string): Promise<T | null> {
	const cookie = request.headers.get('cookie') ?? '';
	const response = await fetcher(`${apiBase()}${path}`, {
		headers: cookie ? { cookie } : {}
	});
	if (!response.ok) return null;
	return (await response.json()) as T;
}

/**
 * What the submission form needs to render: the kinds, the limits, and whether
 * this reader may post at all. Fetched here rather than in the browser so the
 * form — and its toolbar — is in the HTML.
 */
export async function loadRules(fetcher: Fetcher, request: Request): Promise<RulesResponse> {
	const rules = await call<RulesResponse>(fetcher, request, '/api/news/posting-rules');
	if (!rules) {
		throw error(503, 'Posting is unavailable just now.');
	}
	return rules;
}

/**
 * The entry a `?edit=` link points at, with whatever blocks editing it.
 *
 * A published entry cannot be quietly rewritten: a moderator has to unpublish it
 * first, which is a decision with a note attached. The form says so instead of
 * silently hiding the fields.
 */
export async function loadEditing(
	id: number,
	fetcher: Fetcher,
	request: Request
): Promise<{ post: NewsPost | null; blocked: string }> {
	const post = await call<NewsPost>(fetcher, request, `/api/news/${id}`);
	if (!post) {
		throw error(404, 'That entry does not exist.');
	}
	if (post.status !== 'pending') {
		return {
			post,
			blocked:
				'This entry has already been reviewed, so it cannot be changed here. Ask a moderator to unpublish it and it will come back to this form.'
		};
	}
	return { post, blocked: '' };
}
