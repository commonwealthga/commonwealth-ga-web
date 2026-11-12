/** Display formatting for values that arrive raw from the game database. */

/** `1P_CPLab01_P` -> `1P CPLAB01`. Unreal map names are developer-facing. */
/** `1P_CPLab01_P` -> `1P CPLAB01` — developer name, used only as a fallback. */
function mapLabel(name: string): string {
	return name.replace(/_P$/, '').replace(/_/g, ' ').toUpperCase();
}

/** `ATTACKERS_WIN` -> `Attackers win`. */
export function outcomeLabel(outcome: string | null): string {
	if (!outcome) return 'In progress';
	const text = outcome.replace(/_/g, ' ').toLowerCase();
	return text.charAt(0).toUpperCase() + text.slice(1);
}

/**
 * Map title for a match row: the client's translated name when we have one,
 * otherwise the prettified developer name.
 */
export function matchMapName(match: { map_label: string | null; map_name: string }): string {
	const label = match.map_label?.trim();
	return label && label.length > 0 ? label : mapLabel(match.map_name);
}

/** "Ultra Max Security · Commonwealth Prime" — queue name, plus location if set. */
export function matchQueue(match: {
	queue_label: string | null;
	queue_code: string | null;
	queue_location: string | null;
}): string {
	const name = match.queue_label?.trim() || match.queue_code?.trim() || '';
	const location = match.queue_location?.trim();
	if (!name) return location ?? '';
	return location ? `${name} · ${location}` : name;
}

/**
 * Your own result in a match, as a suffix for the outcome column.
 *
 * `null` when the match is not yours, so both lists can render the same way:
 * the side that won, then how it went for the reader.
 *
 * The tones are deliberately not the side colours. `steel` means "defenders"
 * everywhere else on the site, so using it for a loss made the two facts
 * collide; a loss is quiet grey instead, and a win is `amber` — still warm, but
 * a shade off the `ember` used for the winning side it sits beside.
 */
export function viewerSuffix(
	winningTaskForce: number | null,
	myTaskForce: number | null | undefined
): { text: string; tone: string } | null {
	if (myTaskForce === null || myTaskForce === undefined) return null;
	if (winningTaskForce === null) return { text: '· you drew', tone: 'text-faint' };
	return winningTaskForce === myTaskForce
		? { text: '· you won', tone: 'text-amber' }
		: { text: '· you lost', tone: 'text-dim' };
}

export function durationLabel(started: number, sealed: number | null): string {
	if (!sealed) return '—';
	const secs = Math.max(0, sealed - started);
	const m = Math.floor(secs / 60);
	const s = secs % 60;
	return `${m}m ${String(s).padStart(2, '0')}s`;
}

export function relativeLabel(unixSecs: number): string {
	const secs = Math.max(0, Math.round(Date.now() / 1000 - unixSecs));
	if (secs < 45) return 'just now';
	const mins = Math.round(secs / 60);
	if (mins < 60) return `${mins}m ago`;
	const hours = Math.round(mins / 60);
	if (hours < 24) return `${hours}h ago`;
	const days = Math.round(hours / 24);
	if (days < 30) return `${days}d ago`;
	return `${Math.round(days / 30)}mo ago`;
}


/** "24 November 2025" — a news entry's date, UTC and not relative: a feed is a
 * record of when something was posted, not of how long ago that feels. */
export function newsDate(unixSecs: number): string {
	return new Date(unixSecs * 1000).toLocaleDateString('en-GB', {
		day: 'numeric',
		month: 'long',
		year: 'numeric',
		timeZone: 'UTC'
	});
}
