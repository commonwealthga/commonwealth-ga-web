import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/**
 * One rendering model for the whole site: server-side rendering through
 * `adapter-node`.
 *
 * The news feed used to be rendered by the Rust server while everything else was
 * a prerendered static build. That gave crawlable news but two rendering models,
 * two copies of the site chrome, and a full page reload whenever you crossed
 * between them. Rendering everything on one Node process removes the seam: every
 * page is HTML for a crawler on first request, and a client-side navigation
 * afterwards.
 *
 * Pages that fetch their data in the browser (agents, missions, leaderboard)
 * still render their shell on the server and hydrate as before; the news feed is
 * rendered with its content, which is the page where that matters.
 */
/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: {
		adapter: adapter({ out: 'build' }),
		paths: {
			relative: false
		}
	}
};

export default config;
