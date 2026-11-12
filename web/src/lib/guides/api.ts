/**
 * Browser-side calls to the guides API.
 *
 * All relative: the edge routes `/api/*` to the Rust process. The first page of
 * each list is fetched server-side in `+page.server.ts` — that is what makes the
 * section crawlable — and this file covers everything after that.
 */
import type { Draft, Entry, FeedResponse, UploadedImage } from './types';

async function send<T>(path: string, init?: RequestInit): Promise<T> {
	const response = await fetch(path, { credentials: 'same-origin', ...init });
	if (response.ok) return (await response.json()) as T;

	// The API answers `{ "error": "..." }`; a status code alone is not an answer.
	const body = await response.json().catch(() => ({}));
	throw new Error(body?.error ?? `request failed (${response.status})`);
}

/** One list page. `offset` is how many entries are already on screen. */
export function moreEntries(
	category: string,
	filter: { tag?: string | null; class?: string | null; mode?: string | null; q?: string; sort?: string },
	offset: number,
	limit: number
): Promise<FeedResponse> {
	const query = new URLSearchParams({ category, offset: String(offset), limit: String(limit) });
	if (filter.tag) query.set('tag', filter.tag);
	if (filter.class) query.set('class', filter.class);
	if (filter.mode) query.set('mode', filter.mode);
	if (filter.q) query.set('q', filter.q);
	if (filter.sort) query.set('sort', filter.sort);
	return send<FeedResponse>(`/api/guides?${query}`);
}

export function thumb(entryId: number, active: boolean) {
	return send<{ thumbs_up: number; reacted: boolean }>(`/api/guides/${entryId}/thumb`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ active })
	});
}

export function decide(entryId: number, action: string, note = '') {
	return send<{ id: number; status: string }>(`/api/guides/${entryId}/${action}`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ note })
	});
}

/**
 * Upload a screenshot. The body *is* the image: no multipart, no filename, no
 * declared type — the server sniffs the bytes and hands back a derivative it
 * re-encoded itself. What comes back is what the form will submit.
 */
export async function uploadImage(file: File): Promise<UploadedImage> {
	const response = await fetch('/api/guides/images', {
		method: 'POST',
		credentials: 'same-origin',
		headers: { 'Content-Type': file.type || 'application/octet-stream' },
		body: file
	});
	if (response.ok) return (await response.json()) as UploadedImage;
	const body = await response.json().catch(() => ({}));
	throw new Error(body?.error ?? `upload failed (${response.status})`);
}

export function submit(draft: Draft) {
	return send<{ id: number; status: string; path: string }>('/api/guides', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(draft)
	});
}

export function update(entryId: number, draft: Draft) {
	return send<{ id: number; status: string }>(`/api/guides/${entryId}`, {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(draft)
	});
}

/** Where a category's entries live, for redirects and links. */
export function categoryPath(category: string): string {
	return category === 'build' ? '/builds' : '/guides';
}

export type { Entry };
