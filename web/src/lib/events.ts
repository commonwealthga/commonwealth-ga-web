/**
 * Weekly community events.
 *
 * The schedule is authored in Prague time (it is the server's home timezone),
 * but everything a visitor reads is rendered in *their* timezone by the
 * browser. The conversions below do not depend on any library: `Intl` knows the
 * zone offsets, including DST transitions.
 */

/** Authoring timezone for the schedule. Never shown to visitors. */
export const EVENT_TIMEZONE = 'Europe/Prague';

import { formatDateTime } from './time';

export interface GameEvent {
	id: string;
	title: string;
	/** JS weekday: 0 = Sunday … 6 = Saturday. */
	weekday: number;
	/** Local hour/minute in `EVENT_TIMEZONE`. */
	hour: number;
	minute: number;
	summary: string;
	detail: string;
}

export const EVENTS: GameEvent[] = [
	{
		id: 'friday-raids',
		title: 'Friday Raids',
		weekday: 5,
		hour: 18,
		minute: 0,
		summary: 'Group up for the raids.',
		detail:
			'Raids need a coordinated group, so they run on a fixed night. Bring whatever you like — everyone is geared here.'
	},
	{
		id: 'challenge-matches',
		title: 'Challenge matches',
		weekday: 6,
		hour: 19,
		minute: 0,
		summary: 'Organised PvP, arranged on the night.',
		detail:
			'Teams are balanced on the spot and matches are played back to back. The closest thing to a competitive night we run.'
	},
	{
		id: 'weekly-mercenary',
		title: 'Weekly mercenary',
		weekday: 0,
		hour: 19,
		minute: 0,
		summary: 'The mercenary queue, everyone welcome.',
		detail:
			'A relaxed end to the weekend: one queue, no roster, drop in and play. Good first event if you are new.'
	}
];

/** Wall-clock parts of an instant in a timezone. */
function zoneParts(date: Date, timeZone: string) {
	const parts = new Intl.DateTimeFormat('en-US', {
		timeZone,
		hour12: false,
		year: 'numeric',
		month: '2-digit',
		day: '2-digit',
		hour: '2-digit',
		minute: '2-digit',
		second: '2-digit'
	}).formatToParts(date);

	const value = (type: string) => Number(parts.find((part) => part.type === type)?.value ?? 0);
	return {
		year: value('year'),
		month: value('month'),
		day: value('day'),
		// `hour12: false` can yield 24 for midnight in some engines.
		hour: value('hour') % 24,
		minute: value('minute'),
		second: value('second')
	};
}

/** Offset of `timeZone` from UTC at `date`, in milliseconds. */
function zoneOffsetMs(date: Date, timeZone: string): number {
	const parts = zoneParts(date, timeZone);
	const asUtc = Date.UTC(
		parts.year,
		parts.month - 1,
		parts.day,
		parts.hour,
		parts.minute,
		parts.second
	);
	return asUtc - date.getTime();
}

/** The instant at which the given wall-clock time occurs in `timeZone`. */
function zonedInstant(
	year: number,
	month: number,
	day: number,
	hour: number,
	minute: number,
	timeZone: string
): Date {
	const naive = Date.UTC(year, month - 1, day, hour, minute, 0);
	// Two passes settle the offset either side of a DST change.
	let instant = naive;
	for (let pass = 0; pass < 2; pass += 1) {
		instant = naive - zoneOffsetMs(new Date(instant), timeZone);
	}
	return new Date(instant);
}

/** The occurrence of `event` on the Prague date `offsetDays` from `now`. */
function occurrenceOn(event: GameEvent, now: Date, offsetDays: number): Date | null {
	const today = zoneParts(now, EVENT_TIMEZONE);
	const shifted = new Date(Date.UTC(today.year, today.month - 1, today.day + offsetDays));
	if (shifted.getUTCDay() !== event.weekday) return null;

	return zonedInstant(
		shifted.getUTCFullYear(),
		shifted.getUTCMonth() + 1,
		shifted.getUTCDate(),
		event.hour,
		event.minute,
		EVENT_TIMEZONE
	);
}

/** Most recent occurrence at or before `now`. */
export function previousOccurrence(event: GameEvent, now: Date): Date | null {
	for (let i = 0; i >= -7; i -= 1) {
		const candidate = occurrenceOn(event, now, i);
		if (candidate && candidate.getTime() <= now.getTime()) return candidate;
	}
	return null;
}

/** Next occurrence strictly after `now`. */
export function nextOccurrence(event: GameEvent, now: Date): Date {
	for (let i = 0; i <= 7; i += 1) {
		const candidate = occurrenceOn(event, now, i);
		if (candidate && candidate.getTime() > now.getTime()) return candidate;
	}
	// Unreachable for a valid weekly schedule; keeps the type honest.
	return new Date(now.getTime() + 7 * 24 * 3600 * 1000);
}

export type EventState = 'live' | 'just-ended' | 'upcoming';

export interface EventStatus {
	event: GameEvent;
	/** The occurrence the status refers to. */
	at: Date;
	state: EventState;
	/** Only meaningful while `upcoming`. */
	nextAt: Date;
}

/**
 * Status of one event.
 *
 * An event counts as running for the rest of its own (Prague) day. If that day
 * has arrived but nobody is online any more, it reads as "just ended" rather
 * than as if it were still going.
 */
export function eventStatus(
	event: GameEvent,
	now: Date,
	playersOnline: number
): EventStatus {
	const last = previousOccurrence(event, now);
	const next = nextOccurrence(event, now);

	if (last) {
		const lastDay = zoneParts(last, EVENT_TIMEZONE);
		const today = zoneParts(now, EVENT_TIMEZONE);
		const sameDay =
			lastDay.year === today.year && lastDay.month === today.month && lastDay.day === today.day;

		if (sameDay) {
			return {
				event,
				at: last,
				state: playersOnline > 0 ? 'live' : 'just-ended',
				nextAt: next
			};
		}
	}

	return { event, at: next, state: 'upcoming', nextAt: next };
}

/** All events with their status, soonest-relevant first. */
export function allStatuses(now: Date, playersOnline: number): EventStatus[] {
	// Nearest first: what is running, then what starts soonest, then what has
	// finished. A finished event is the least useful thing to lead with.
	const order: Record<EventState, number> = { live: 0, upcoming: 1, 'just-ended': 2 };
	return EVENTS.map((event) => eventStatus(event, now, playersOnline)).sort((a, b) => {
		if (order[a.state] !== order[b.state]) return order[a.state] - order[b.state];
		if (a.state === 'upcoming') return a.nextAt.getTime() - b.nextAt.getTime();
		return b.at.getTime() - a.at.getTime();
	});
}

/** The event worth showing on the homepage: live, else just ended, else next. */
export function headlineEvent(now: Date, playersOnline: number): EventStatus {
	return allStatuses(now, playersOnline)[0];
}

/**
 * "Fri 9 Nov, 18:00" in the visitor's own timezone, always 24-hour.
 *
 * The schedule is authored in EVENT_TIMEZONE, but that is an implementation
 * detail: visitors only ever see their own clock.
 */
export function formatInViewerTime(date: Date): string {
	return formatDateTime(date);
}

/**
 * How close an event has to be before the page stops treating it as "some time
 * this week" and starts telling people to get in game.
 *
 * Twelve hours is roughly one waking day: close enough that it is worth
 * interrupting someone for, far enough that a player reading in the morning still
 * has the evening ahead of them.
 */
export const SOON_WINDOW_MS = 12 * 60 * 60 * 1000;

/** What the page should do with a status: shout, count, or stay quiet. */
export type Urgency = 'live' | 'soon' | 'later' | 'past';

export function eventUrgency(status: EventStatus, now: Date): Urgency {
	if (status.state === 'live') return 'live';
	if (status.state === 'just-ended') return 'past';
	return status.nextAt.getTime() - now.getTime() <= SOON_WINDOW_MS ? 'soon' : 'later';
}

/**
 * The instant as ISO 8601 *with the schedule's own offset*, e.g.
 * `2026-11-13T18:00:00+01:00`.
 *
 * This is the one form of an event's time that does not depend on who is reading
 * it: a crawler, a calendar client and a `<time datetime>` all get the same
 * unambiguous instant, while the visible text stays in the viewer's clock.
 */
export function isoInEventZone(date: Date): string {
	const parts = zoneParts(date, EVENT_TIMEZONE);
	const offsetMinutes = zoneOffsetMs(date, EVENT_TIMEZONE) / 60000;
	const sign = offsetMinutes < 0 ? '-' : '+';
	const absolute = Math.abs(offsetMinutes);
	const pad = (value: number) => String(value).padStart(2, '0');

	return (
		`${parts.year}-${pad(parts.month)}-${pad(parts.day)}` +
		`T${pad(parts.hour)}:${pad(parts.minute)}:00` +
		`${sign}${pad(Math.floor(absolute / 60))}:${pad(absolute % 60)}`
	);
}

/** "CET" / "CEST" — the schedule's zone, for text that has not been localised yet. */
export function eventZoneName(date: Date): string {
	const parts = new Intl.DateTimeFormat('en-GB', {
		timeZone: EVENT_TIMEZONE,
		timeZoneName: 'short'
	}).formatToParts(date);
	return parts.find((part) => part.type === 'timeZoneName')?.value ?? 'CET';
}

/** schema.org's day URIs, indexed like `Date#getDay`. */
const SCHEMA_DAYS = [
	'https://schema.org/Sunday',
	'https://schema.org/Monday',
	'https://schema.org/Tuesday',
	'https://schema.org/Wednesday',
	'https://schema.org/Thursday',
	'https://schema.org/Friday',
	'https://schema.org/Saturday'
];

/**
 * An `Event` node for one occurrence, for the page's JSON-LD.
 *
 * Weekly rather than a single date, because that is what these are: `eventSchedule`
 * says so, and `startDate` gives the next one so the node is never stale by more
 * than a week without anyone editing it.
 */
export function eventSchema(status: EventStatus, siteOrigin: string) {
	const start = status.state === 'upcoming' ? status.nextAt : status.at;
	const parts = zoneParts(start, EVENT_TIMEZONE);

	return {
		'@type': 'Event',
		name: status.event.title,
		description: status.event.detail,
		startDate: isoInEventZone(start),
		eventSchedule: {
			'@type': 'Schedule',
			byDay: SCHEMA_DAYS[status.event.weekday],
			startTime: `${String(parts.hour).padStart(2, '0')}:${String(parts.minute).padStart(2, '0')}:00`,
			repeatFrequency: 'P1W'
		},
		eventAttendanceMode: 'https://schema.org/OnlineEventAttendanceMode',
		eventStatus: 'https://schema.org/EventScheduled',
		location: {
			'@type': 'VirtualLocation',
			url: `${siteOrigin}/play`
		},
		organizer: {
			'@type': 'Organization',
			name: 'The Commonwealth',
			url: `${siteOrigin}/`
		},
		isAccessibleForFree: true,
		// Rich results want an image; the events card is accurate for all three.
		image: `${siteOrigin}/og/events.jpg`,
		url: `${siteOrigin}/events#${status.event.id}`
	};
}

/** "2d 4h" / "45m" / "now". */
export function formatCountdown(target: Date, now: Date): string {
	const ms = target.getTime() - now.getTime();
	if (ms <= 0) return 'now';

	const minutes = Math.floor(ms / 60000);
	const days = Math.floor(minutes / (24 * 60));
	const hours = Math.floor((minutes % (24 * 60)) / 60);
	if (days > 0) return `${days}d ${hours}h`;
	if (hours > 0) return `${hours}h ${minutes % 60}m`;
	return `${minutes}m`;
}
