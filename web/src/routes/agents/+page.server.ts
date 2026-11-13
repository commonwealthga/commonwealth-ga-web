import { apiBase } from '$lib/news/server';
import type { DirectoryPage } from '$lib/agents';

/**
 * The agent directory, server-rendered.
 *
 * This route used to opt out of SSR entirely (`ssr = false`), which made it an
 * 800-byte shell to anything that does not run JavaScript — while the sitemap went
 * on advertising it. The directory is public and needs no session, so the first
 * page of it belongs in the HTML: search, paging and filtering still happen in
 * the browser afterwards, where they belong.
 *
 * The query string is read here too, so a shared `?q=zax` link renders the right
 * page rather than flashing the unfiltered list.
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
	const query = url.searchParams.get('q') ?? '';
	const page = Math.max(1, Number(url.searchParams.get('page') ?? '1') || 1);

	const params = new URLSearchParams({ limit: '24', page: String(page) });
	if (query.trim()) params.set('q', query.trim());

	// A directory that cannot be read is not an error page: the client retries and
	// shows its own message, so the head and the copy still render.
	try {
		const cookie = request.headers.get('cookie') ?? '';
		const response = await fetch(`${apiBase()}/api/agents?${params}`, {
			headers: cookie ? { cookie } : {}
		});
		if (!response.ok) return { directory: null };
		return { directory: (await response.json()) as DirectoryPage };
	} catch {
		return { directory: null };
	}
};
