/**
 * The events page needs one thing from the server: what time it is.
 *
 * The schedule is authored in a fixed timezone, so the *instants* can be
 * computed anywhere — only the rendering of them into a visitor's clock needs the
 * browser. Passing the server's clock in means the page can be server-rendered
 * with real dates (`<time datetime>` for crawlers, an authored-zone label for
 * anyone without JavaScript) instead of a placeholder, and the list is already in
 * nearest-first order on first paint.
 */
export const load = () => ({ now: Date.now() });
