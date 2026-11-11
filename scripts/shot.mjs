/**
 * shot.mjs — drives the container's chromium over CDP to capture pages.
 *
 * The plain `chromium --screenshot` CLI grabs the frame at the load event,
 * which is too early for anything that hydrates or fetches (the archive table
 * is still a skeleton, the live counter still says "linking"). This waits for
 * the page to settle first.
 *
 * Input: argv = ["name=/path@WIDTH,HEIGHT", ...]
 * Env:   SHOT_BASE (origin), SHOT_SETTLE (ms), SHOT_MIN (bytes)
 */
import { statSync } from 'node:fs';
import puppeteer from 'puppeteer-core';

const base = process.env.SHOT_BASE ?? 'http://127.0.0.1:8080';
const settle = Number(process.env.SHOT_SETTLE ?? 1200);
const min = Number(process.env.SHOT_MIN ?? 10000);
const out = '/shots';

const specs = process.argv.slice(2);
if (specs.length === 0) {
	console.error('shot.mjs: no pages requested');
	process.exit(2);
}

const browser = await puppeteer.launch({
	executablePath: '/usr/bin/chromium',
	args: [
		'--no-sandbox',
		'--disable-gpu',
		'--disable-dev-shm-usage',
		'--hide-scrollbars',
		// Animations settle instantly: no half-scrolled ticker, no blank frame.
		'--force-prefers-reduced-motion'
	],
	protocolTimeout: 30_000
});

let failed = 0;

for (const spec of specs) {
	const eq = spec.indexOf('=');
	const name = spec.slice(0, eq);
	const rest = spec.slice(eq + 1);
	const at = rest.lastIndexOf('@');
	const path = rest.slice(0, at);
	const [width, height] = rest.slice(at + 1).split(',').map(Number);

	const page = await browser.newPage();
	try {
		await page.setViewport({ width, height, deviceScaleFactor: 1 });
		await page.goto(base + path, { waitUntil: 'domcontentloaded', timeout: 20_000 });
		// Hydration + SSE handshake + the archive fetch all land well inside this.
		await new Promise((resolve) => setTimeout(resolve, settle));

		const file = `${out}/${name}.png`;
		await page.screenshot({ path: file });
		const bytes = statSync(file).size;
		if (bytes >= min) {
			console.log(`  ok    ${name.padEnd(10)} ${String(bytes).padStart(8)} B  ${path}`);
		} else {
			console.log(`  BLANK ${name.padEnd(10)} ${String(bytes).padStart(8)} B  ${path}`);
			failed += 1;
		}
	} catch (error) {
		console.log(`  FAIL  ${name.padEnd(10)} ${String(error).slice(0, 90)}`);
		failed += 1;
	} finally {
		await page.close().catch(() => {});
	}
}

await browser.close();
process.exit(failed === 0 ? 0 : 1);
