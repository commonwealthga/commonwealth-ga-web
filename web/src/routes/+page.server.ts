import { playerCount, playersSentence } from '$lib/server/status';

/**
 * The home page states the count in its own copy — "3 players in-game right now"
 * — so it exists in the HTML a search engine or an assistant reads, not only in
 * the panel that fills in once JavaScript connects.
 */
export const load = async ({ fetch }: { fetch: typeof globalThis.fetch }) => {
	const count = await playerCount(fetch);
	return { count, sentence: playersSentence(count) };
};
