/**
 * Live data clients.
 *
 * One EventSource per page, shared by every subscriber, so navigating between
 * pages does not open a second connection. The server pushes the current value
 * immediately on connect and one update per tick after that; EventSource
 * reconnects on its own if the stream drops.
 */

export interface StatusPayload {
	ok: boolean;
	players: number | null;
	text: string;
	/** Control server reachable: null when this deployment has none. */
	control_server: boolean | null;
	ts: number;
}

export interface ViewerOutcome {
	user_id: number;
	task_force: number | null;
	won: boolean;
	draw: boolean;
}

export interface MatchSummary {
	id: number;
	/** The game's own match id — what detail URLs use. */
	instance_id: number;
	/** Raw map name, used when the client has no translation for it. */
	map_name: string;
	map_label: string | null;
	game_mode: string;
	started_at: number;
	sealed_at: number | null;
	outcome: string | null;
	winning_task_force: number | null;
	queue_code: string | null;
	queue_label: string | null;
	queue_location: string | null;
	players: number;
	/** Present only when the request carried a session. */
	viewer: ViewerOutcome | null;
}

type Listener = (status: StatusPayload) => void;

let source: EventSource | null = null;
let latest: StatusPayload | null = null;
const listeners = new Set<Listener>();

function ensureSource(): void {
	if (source || typeof window === 'undefined') return;

	source = new EventSource('/api/live');

	source.addEventListener('status', (event) => {
		try {
			latest = JSON.parse((event as MessageEvent<string>).data) as StatusPayload;
		} catch {
			return;
		}
		for (const listener of listeners) listener(latest);
	});
}

/** Subscribe to the live status. Returns an unsubscribe function. */
export function subscribeStatus(listener: Listener): () => void {
	ensureSource();
	listeners.add(listener);
	if (latest) listener(latest);

	return () => {
		listeners.delete(listener);
		if (listeners.size === 0) {
			source?.close();
			source = null;
		}
	};
}

/**
 * Recent completed missions.
 *
 * Fetched on the client, not during prerender: the page shell stays static and
 * cacheable, and a slow or broken archive degrades to an error line instead of
 * a failed page.
 */
export type MatchKindFilter = 'all' | 'pvp' | 'pve';

export interface MatchPage {
	kind: string;
	limit: number;
	page: number;
	total: number;
	has_more: boolean;
	matches: MatchSummary[];
}

export async function fetchMatchPage(
	kind: MatchKindFilter,
	page: number,
	limit = 20
): Promise<MatchPage> {
	const params = new URLSearchParams({
		kind,
		page: String(page),
		limit: String(limit)
	});
	const response = await fetch(`/api/matches/recent?${params.toString()}`, {
		headers: { accept: 'application/json' }
	});
	if (!response.ok) throw new Error(`archive unavailable (${response.status})`);
	const body = (await response.json()) as MatchPage;
	return { ...body, matches: body.matches ?? [] };
}

/** The first page, for the feeds that just want a handful of rows. */
export async function fetchRecentMatches(limit = 12): Promise<MatchSummary[]> {
	return (await fetchMatchPage('all', 0, limit)).matches;
}
