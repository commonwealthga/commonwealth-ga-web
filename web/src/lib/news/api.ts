/**
 * Browser-side calls to the news API.
 *
 * All relative: the edge routes `/api/*` to the Rust process, so the browser does
 * not need to know where it lives. Server-side fetching (the first page, which
 * has to be in the HTML for a crawler) happens in `+page.server.ts`, which is the
 * only place that may read the private `API_ORIGIN`.
 */

async function send<T>(path: string, init?: RequestInit): Promise<T> {
	const response = await fetch(path, {
		credentials: 'same-origin',
		headers: { 'Content-Type': 'application/json' },
		...init
	});
	if (response.ok) return (await response.json()) as T;

	// The API answers `{ "error": "..." }`; surfacing that beats a status code.
	const body = await response.json().catch(() => ({}));
	throw new Error(body?.error ?? `request failed (${response.status})`);
}

export function thumb(postId: number, active: boolean) {
	return send<{ thumbs_up: number; reacted: boolean }>(`/api/news/${postId}/thumb`, {
		method: 'POST',
		body: JSON.stringify({ active })
	});
}

export function decide(postId: number, action: string, note = '') {
	return send<{ id: number; status: string }>(`/api/news/${postId}/${action}`, {
		method: 'POST',
		body: JSON.stringify({ note })
	});
}

export function morePosts(offset: number, limit: number, kind?: string | null) {
	const query = new URLSearchParams({ offset: String(offset), limit: String(limit) });
	if (kind) query.set('kind', kind);
	return send<{ posts: import('./types').NewsPost[]; total: number; offset: number }>(
		`/api/news?${query}`
	);
}
