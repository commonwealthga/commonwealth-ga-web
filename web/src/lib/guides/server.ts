/**
 * Server-side fetches for the guides section.
 *
 * Every list and every detail page is rendered on the server (that is the whole
 * architecture), so this module is where the API is reached from Node. Only
 * `+page.server.ts` files may import it: `$env/dynamic/private` is server-only.
 */
import { env } from '$env/dynamic/private';
import { error } from '@sveltejs/kit';
import type { DetailResponse, FeedResponse, OptionsResponse } from './types';

const base = () => (env.API_ORIGIN ?? 'http://127.0.0.1:8080').replace(/\/$/, '');

type Fetcher = typeof globalThis.fetch;

async function call<T>(fetcher: Fetcher, request: Request, path: string): Promise<T | null> {
	const cookie = request.headers.get('cookie') ?? '';
	const response = await fetcher(`${base()}${path}`, {
		headers: cookie ? { cookie } : {}
	});
	if (!response.ok) return null;
	return (await response.json()) as T;
}

/** The vocabulary and the posting rules: what the filters and the form need. */
export async function loadOptions(fetcher: Fetcher, request: Request): Promise<OptionsResponse> {
	const options = await call<OptionsResponse>(fetcher, request, '/api/guides/options');
	if (!options) {
		throw error(503, 'The guides section is unavailable just now.');
	}
	return options;
}

/**
 * One list page, rendered with its content so a crawler reads it.
 *
 * The query string is passed straight through: filters and sort are the URL, so a
 * filtered list is a link someone can send.
 */
export async function loadFeed(
	category: 'guide' | 'build',
	fetcher: Fetcher,
	request: Request,
	url: URL
): Promise<{ feed: FeedResponse; options: OptionsResponse }> {
	const query = new URLSearchParams(url.searchParams);
	query.set('category', category);

	const [feed, options] = await Promise.all([
		call<FeedResponse>(fetcher, request, `/api/guides?${query}`),
		loadOptions(fetcher, request)
	]);

	if (!feed) {
		throw error(503, 'The guides section is unavailable just now.');
	}
	return { feed, options };
}

/**
 * An entry its author is editing. Only a pending entry is editable, and only by
 * its author — the API decides that, and this refuses the page rather than
 * showing a form whose submit would fail.
 */
export async function loadEditable(
	category: 'guide' | 'build',
	id: number,
	fetcher: Fetcher,
	request: Request
): Promise<DetailResponse> {
	const detail = await call<DetailResponse>(fetcher, request, `/api/guides/${id}`);
	if (!detail) throw error(404, 'That entry does not exist.');
	if (detail.entry.category !== category) throw error(404, 'That entry is not here.');
	if (!detail.entry.can_edit) throw error(403, 'That entry is no longer editable.');
	return detail;
}

/**
 * One entry. The path is `/<category>/<id>-<slug>`; the id is what identifies it,
 * the slug is there to be read.
 */
export async function loadEntry(
	category: 'guide' | 'build',
	slug: string,
	fetcher: Fetcher,
	request: Request
): Promise<DetailResponse> {
	const id = Number(slug.split('-')[0]);
	if (!Number.isInteger(id) || id <= 0) {
		throw error(404, 'That entry does not exist.');
	}

	const detail = await call<DetailResponse>(fetcher, request, `/api/guides/${id}`);
	if (!detail) throw error(404, 'That entry does not exist.');

	// The id is unique across both categories, so a build addressed under
	// `/guides/` is a wrong link rather than a second copy of the same page.
	if (detail.entry.category !== category) {
		throw error(404, 'That entry is not here.');
	}
	return detail;
}
