/**
 * List state (page, filter, search) kept in the URL, and the scroll offset that
 * goes with it.
 *
 * A paginated list that holds its page only in component memory loses it the
 * moment you open a row and press Back: the route remounts on page 1, wherever
 * you had scrolled to. Writing the page into the query string fixes that,
 * because the history entry the reader returns to already says `?page=4`.
 *
 * Reads go through `window.location`, not `page.url` from `$app/state`:
 * `replaceState` rewrites the address bar without updating the router's copy of
 * the URL, so a value derived from `page.url` never notices a page change. Reads
 * are also guarded by `browser`, because during prerendering SvelteKit throws on
 * `url.searchParams` — a prerendered page has exactly one address.
 *
 * `replaceState` rather than `pushState`: paging through sixteen pages of
 * archive should not leave sixteen Back presses to escape from.
 */
import { browser } from '$app/environment';
import { replaceState } from '$app/navigation';
import { page } from '$app/state';
import { tick } from 'svelte';

function search(): URLSearchParams {
	return new URL(window.location.href).searchParams;
}

/** 1-based page number from the URL, clamped; `1` when absent or nonsense. */
export function readPage(): number {
	if (!browser) return 1;
	const parsed = Number.parseInt(search().get('page') ?? '1', 10);
	return Number.isFinite(parsed) && parsed > 0 ? parsed : 1;
}

/** A parameter validated against the values a list actually accepts. */
export function readChoice<T extends string>(allowed: readonly T[], fallback: T, name = 'kind'): T {
	if (!browser) return fallback;
	const raw = search().get(name);
	return allowed.includes(raw as T) ? (raw as T) : fallback;
}

/** The PvP/PvE filter, the most common choice parameter. */
export function readKind<T extends string>(allowed: readonly T[], fallback: T): T {
	return readChoice(allowed, fallback);
}

/** A free-text parameter (a search box), empty when absent. */
export function readText(name: string): string {
	return browser ? (search().get(name) ?? '') : '';
}

/**
 * Reflect list state in the URL without navigating.
 *
 * `null`, `undefined`, `''` and page 1 remove their parameter, so the canonical
 * list URL stays clean. The comparison is against the live address, so calling
 * this with unchanged state costs nothing.
 */
export function writeListState(next: {
	page?: number | null;
	kind?: string | null;
	query?: string | null;
	params?: Record<string, string | null>;
}): void {
	if (!browser) return;
	const url = new URL(window.location.href);

	const apply = (name: string, value: string | null | undefined) => {
		if (value === undefined) return;
		if (value === null || value === '') url.searchParams.delete(name);
		else url.searchParams.set(name, value);
	};

	const { page: nextPage } = next;
	apply(
		'page',
		nextPage === undefined ? undefined : nextPage === null || nextPage <= 1 ? null : String(nextPage)
	);
	apply('kind', next.kind);
	apply('q', next.query);
	for (const [name, value] of Object.entries(next.params ?? {})) apply(name, value);

	if (url.href === window.location.href) return;
	replaceState(url, page.state);
}

// --- Scroll memory -----------------------------------------------------------

/**
 * Where the reader was in a list, per address.
 *
 * `window.scrollY` is recorded as the list is left and put back once the page
 * has re-rendered and its data has landed. SvelteKit's own scroll restoration
 * runs before the fetch resolves, against a page that is still short, so it
 * lands on the top.
 */
const offsets = new Map<string, number>();
/**
 * Addresses whose offset is being put back right now.
 *
 * Restoring scrolls the page, which emits scroll events, which the tracker
 * would record — overwriting the real offset with whatever intermediate value
 * the browser landed on (SvelteKit's own restoration runs first, against a page
 * that is still short). So the tracker stays quiet while its address is being
 * restored.
 */
const restoring = new Set<string>();

function address(): string {
	return window.location.pathname + window.location.search;
}

/**
 * Follow the reader's position in this list.
 *
 * Recorded on scroll — but only for scrolls the reader caused. A position
 * restored by SvelteKit or by the browser also emits scroll events, and those
 * arrive *before* this helper has a chance to correct them, so recording every
 * event stored the wrong offset and then faithfully restored it.
 *
 * Zero is ignored too: leaving at the top needs no restoring.
 *
 * Safe to call from every list: the listeners are installed once per page.
 */
let tracking = false;

export function trackScroll(): void {
	if (!browser || tracking) return;
	tracking = true;

	let lastInput = 0;
	const noteInput = () => {
		lastInput = performance.now();
	};
	for (const type of ['wheel', 'touchmove', 'mousedown', 'keydown'] as const) {
		window.addEventListener(type, noteInput, { passive: true, capture: true });
	}

	let scheduled = false;
	window.addEventListener(
		'scroll',
		() => {
			if (scheduled) return;
			scheduled = true;
			requestAnimationFrame(() => {
				scheduled = false;
				// 500ms covers momentum and a scrollbar drag in progress.
				if (performance.now() - lastInput > 500) return;
				const offset = Math.round(window.scrollY);
				if (offset > 0 && !restoring.has(address())) offsets.set(address(), offset);
			});
		},
		{ passive: true }
	);
}

/**
 * Put the offset back.
 *
 * The list is empty at the moment the route mounts and fills in when the fetch
 * lands, so a single `scrollTo` is clamped by a page that is still short — and
 * SvelteKit's own restoration has usually already run, also against the short
 * page. So this retries on each frame until the offset takes or the deadline
 * passes, which is when the fetch should have landed.
 */
export async function restoreScroll(): Promise<void> {
	if (!browser) return;
	const saved = offsets.get(address());
	if (saved === undefined) return;
	offsets.delete(address());

	restoring.add(address());
	await tick();
	const deadline = performance.now() + 1000;
	const step = () => {
		window.scrollTo(0, saved);
		const settled = Math.abs(window.scrollY - saved) < 2;
		if (!settled && performance.now() < deadline) requestAnimationFrame(step);
		else restoring.delete(address());
	};
	requestAnimationFrame(step);
}
