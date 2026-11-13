import { playerCount } from '$lib/server/status';

/**
 * The player count is the point of this page, so it is rendered server-side: a
 * crawler, an AI assistant quoting the site, or a reader with no JavaScript all
 * get "3 players in-game" instead of a loading state.
 *
 * The browser then carries on from the same live stream as before.
 */
export const load = async ({ fetch }: { fetch: typeof globalThis.fetch }) => ({
	count: await playerCount(fetch)
});
