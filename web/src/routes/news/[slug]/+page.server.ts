import { redirect } from '@sveltejs/kit';

/**
 * An entry's canonical path, `/news/<id>-<slug>`, resolves to its place in the
 * feed.
 *
 * There is no per-post page: the feed holds every post in full, so a link to one
 * is a link to an anchor. `/news` used to be rendered by the Rust server, which
 * answered this redirect; when the pages moved here the redirect was lost, and
 * the API kept handing out paths that 404'd.
 */
export const load = ({ params }: { params: { slug: string } }) => {
	const id = Number(params.slug.split('-')[0]);
	if (!Number.isInteger(id) || id <= 0) {
		redirect(302, '/news');
	}
	redirect(301, `/news#post-${id}`);
};
