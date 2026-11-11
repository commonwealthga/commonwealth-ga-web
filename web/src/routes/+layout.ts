// Content pages are prerendered to static HTML; anything interactive opts out
// per-route (see src/routes/app/+page.ts) and is served the app shell instead.
export const prerender = true;
export const trailingSlash = 'never';
