import { loadEditing, loadRules } from '$lib/news/server';

/**
 * The submission form, rendered with the rules it enforces.
 *
 * The rules used to be fetched by the browser, which left the form an empty
 * shell until JavaScript ran — inconsistent with the guides form, which has
 * always taken them from here. Now both arrive with the page.
 *
 * With `?edit=<id>` the entry is loaded too; only a pending one is editable.
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
	const rules = await loadRules(fetch, request);

	const editId = Number(url.searchParams.get('edit') ?? '');
	let post = null;
	let blocked = '';
	if (Number.isInteger(editId) && editId > 0) {
		const editing = await loadEditing(editId, fetch, request);
		post = editing.post;
		blocked = editing.blocked;
	}

	return { rules, post, blocked, editId: post ? editId : 0 };
};
