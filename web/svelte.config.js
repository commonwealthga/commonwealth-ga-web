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
			// Pages are crawled at build time, when there is no API to answer for
			// them. The API is a runtime surface, not part of the static build.
			handleHttpError: ({ path, message }) => {
				if (path.startsWith('/api/')) return;
				throw new Error(message);
			}
		}
	}
};

export default config;
