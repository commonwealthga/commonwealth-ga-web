// Server-side rendering, every page. The news feed needs it (its content is
// written after any build could run, and a crawler has to read it), and having
// one model for the whole site means no full-page reload when crossing into it.
// Pages that fetch their data in the browser still render their shell here and
// hydrate as before.
export const prerender = false;
export const trailingSlash = 'never';
