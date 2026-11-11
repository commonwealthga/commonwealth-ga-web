/**
 * Time formatting for the whole site.
 *
 * Everything is 24-hour and stable across locales: a player should never have
 * to translate "5:00 PM" or wonder whether a clock is theirs. `en-GB` is used
 * as the formatting locale because it is 24-hour and uses a day-before-month
 * order; the *instant* is still the viewer's own timezone, which is what
 * `Intl` does by default when no `timeZone` is given.
 */

/** "14:00" */
export function formatClock(instant: Date): string {
	return new Intl.DateTimeFormat('en-GB', {
		hour: '2-digit',
		minute: '2-digit',
		hour12: false
	}).format(instant);
}

/** "Sat" */
export function formatWeekday(instant: Date): string {
	return new Intl.DateTimeFormat('en-GB', { weekday: 'short' }).format(instant);
}

/** "9 Nov" */
export function formatDay(instant: Date): string {
	return new Intl.DateTimeFormat('en-GB', { day: 'numeric', month: 'short' }).format(instant);
}

/** "9 Nov 2025" */
export function formatDayYear(instant: Date): string {
	return new Intl.DateTimeFormat('en-GB', {
		day: 'numeric',
		month: 'short',
		year: 'numeric'
	}).format(instant);
}

/** "Sat 9 Nov, 14:00" */
export function formatDateTime(instant: Date): string {
	return `${formatWeekday(instant)} ${formatDay(instant)}, ${formatClock(instant)}`;
}

/** "14:00" from a unix-seconds timestamp. */
export function clockFromUnix(seconds: number): string {
	return formatClock(new Date(seconds * 1000));
}
