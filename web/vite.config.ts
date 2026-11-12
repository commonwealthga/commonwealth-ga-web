import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

// Where the Rust API lives. Overridable so the dev server can run in a
// container and still reach a backend on the host (or in another container).
const apiTarget = process.env.VITE_API_TARGET ?? 'http://127.0.0.1:8080';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	server: {
		port: 5173,
		// Dev loop: Vite serves the app, the Rust server owns the API.
		// SSE works through this proxy (no buffering).
		proxy: {
			'/api': {
				target: apiTarget,
				changeOrigin: false
			},
			// Build screenshots are stored and served by the API process — the
			// same paths the edge routes there in production.
			'/images': {
				target: apiTarget,
				changeOrigin: false
			}
		}
	}
});
