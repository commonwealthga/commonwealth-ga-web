/**
 * The live player count, as the *server* sees it.
 *
 * The count belongs in the HTML. Rendering it only in the browser meant that a
 * crawler — or an AI assistant, or anyone whose JavaScript did not run — saw
 * "linking to grid" where the single most compelling fact about the server goes:
 * that people are playing on it right now.
 *
 * The API caches its own reading for a few seconds and this adds a short TTL on
 * top, so a crawl of a dozen pages, or a thousand readers, is still one query.
 */
import { env } from '$env/dynamic/private';

export interface PlayerCount {
	/** `null` when the count is not knowable: no live DB, or a query failure. */
	players: number | null;
	/** The same sentence `/api/status` serves the Discord bot. */
	text: string;
	ts: number;
}

const apiBase = () => (env.API_ORIGIN ?? 'http://127.0.0.1:8080').replace(/\/$/, '');

/** Long enough to collapse a crawl, short enough that the page is not stale. */
const TTL_MS = 20_000;

let cached: { at: number; value: PlayerCount } | null = null;

export async function playerCount(fetcher: typeof globalThis.fetch): Promise<PlayerCount> {
	if (cached && Date.now() - cached.at < TTL_MS) return cached.value;

	const unknown: PlayerCount = { players: null, text: 'server status unavailable', ts: 0 };

	try {
		const response = await fetcher(`${apiBase()}/api/players`, {
			headers: { accept: 'application/json' }
		});
		if (!response.ok) return unknown;

		const body = (await response.json()) as {
			ok?: boolean;
			players?: number | null;
			text?: string;
		};
		const value: PlayerCount = {
			players: body.ok && typeof body.players === 'number' ? body.players : null,
			text: body.text ?? unknown.text,
			ts: Date.now()
		};
		cached = { at: Date.now(), value };
		return value;
	} catch {
		// A page that cannot reach the API still has to render: the client retries
		// over the live stream a moment later.
		return unknown;
	}
}

/** "3 agents in-game" — the sentence the page copy can use around the numeral. */
export function playersSentence(count: PlayerCount): string {
	if (count.players === null) return 'player count unavailable right now';
	if (count.players === 0) return 'nobody in-game at the moment';
	if (count.players === 1) return '1 player in-game right now';
	return `${count.players} players in-game right now`;
}
