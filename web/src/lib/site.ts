/**
 * Canonical identity of the site, for search engines and link previews.
 *
 * One origin, in one place: canonical URLs, Open Graph images and the sitemap
 * must agree, and a relative `og:image` is silently ignored by every unfurler
 * (Discord included, which is how most people will first see a link to us).
 */
export const SITE_ORIGIN = 'https://commonwealth.ydns.eu';
export const SITE_NAME = 'The Commonwealth';
export const SITE_TAGLINE = 'a Global Agenda community server';
export const OG_IMAGE = '/og.jpg';
