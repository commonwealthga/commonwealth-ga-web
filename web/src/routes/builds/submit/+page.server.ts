import { error } from '@sveltejs/kit';
import { loadEditable, loadOptions } from '$lib/guides/server';
import type { Entry } from '$lib/guides/types';

/**
 * The build form, and the same form for editing.
 *
 * With `?edit=<id>` the entry is loaded for its author; anything else is a blank
 * form. The options come from the API, so the choices and the limits shown here
 * are the ones the endpoint will enforce.
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
	const options = await loadOptions(fetch, request);
	const editId = Number(url.searchParams.get('edit') ?? '');
	let editing: Entry | null = null;

	if (Number.isInteger(editId) && editId > 0) {
		editing = (await loadEditable('build', editId, fetch, request)).entry;
	}

	return { options, editing };
};
