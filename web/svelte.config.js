import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/**
 * Hybrid rendering, deliberately:
 *
 * - Content pages are prerendered to real HTML (fast first paint, indexable).
 * - `fallback: '200.html'` emits an app shell that the Rust server serves for
 *   any path with no prerendered file, so client-side routes survive a hard
 *   refresh.
 *
 * This is why the server's static fallback is `200.html`, not `index.html`:
 * `index.html` belongs to the prerendered landing page.
 */
/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: {
		adapter: adapter({
			pages: 'build',
			assets: 'build',
			fallback: '200.html',
			precompress: false,
			strict: true
		}),
		paths: {
			relative: false
		},
		prerender: {
			// Reached only from the server-rendered news feed, so the build-time
			// crawl from `/` does not find them.
			entries: ['*', '/news/submit'],
			// Pages are crawled at build time, when there is no API to answer for
			// them. The API is a runtime surface, not part of the static build.
			handleHttpError: ({ path, message }) => {
				if (path.startsWith('/api/')) return;
				// The news feed and post pages are rendered by the Rust server,
				// so they can carry posts written after this build. The nav links
				// to /news, so the crawl reaches it and finds no prerendered file.
				// `/news/submit` and `/news/queue` are real pages and are built.
				if (path === '/news' || /^\/news\/\d+/.test(path)) return;
				throw new Error(message);
			}
		}
	}
};

export default config;
