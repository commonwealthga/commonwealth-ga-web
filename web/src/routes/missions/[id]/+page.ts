/**
 * One match, fetched at runtime.
 *
 * The detail page is a client-side route: which layout a match needs depends on
 * the queue (PvP or PvE), and that is only knowable from the database at
 * request time. Prerendering it would mean guessing.
 */
export const ssr = false;
