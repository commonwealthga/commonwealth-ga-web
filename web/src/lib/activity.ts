/**
 * Recent play, for the homepage hero.
 *
 * Deliberately not the live player count: that is usually zero, and a zero in
 * the most prominent spot on the site reads as "this server is dead". The
 * server picks the narrowest window that has anything in it, so the numbers
 * always mean something.
 */

export interface Activity {
	window_hours: number;
	agents: number;
	missions: number;
}

export async function fetchActivity(): Promise<Activity> {
	const response = await fetch('/api/activity', {
		headers: { accept: 'application/json' }
	});
	if (!response.ok) throw new Error(`unavailable (${response.status})`);
	return (await response.json()) as Activity;
}

/**
 * "last 24 hours", "last 3 days", "all time" — 0 means all time.
 *
 * Deliberately without a leading "in the": the caption sits near the hexagon's
 * edge, and the shorter phrase leaves breathing room without losing meaning.
 */
export function windowLabel(hours: number): string {
	if (hours === 0) return 'all time';
	// Hours for the first two windows: "last 1 days" is not a thing.
	if (hours <= 48) return `last ${hours} hours`;
	if (hours % 24 === 0) {
		const days = hours / 24;
		return `last ${days} days`;
	}
	return `last ${hours} hours`;
}

/** Short form for where space is tight: "24h", "7d", "all time". */
export function windowChip(hours: number): string {
	if (hours === 0) return 'all time';
	if (hours % 24 === 0) return `${hours / 24}d`;
	return `${hours}h`;
}
