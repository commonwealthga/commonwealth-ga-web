/**
 * og-cards.mjs — render the Open Graph cards, one per section, into `web/static/og/`.
 *
 * The site shipped one `og.jpg` for every page, so a link to /events and a link to
 * /builds unfurled identically. These are generated rather than hand-drawn so that
 * a copy change or a palette change is a re-run, not a redesign.
 *
 * **Set for the thumbnail, not for this file.** Discord renders the card about
 * 400px wide — a third of its size — and the first draft, set for a full-size
 * viewing, arrived as grey mush in an actual embed. Nothing here is smaller than
 * ~25px, the brand sits on two lines, and the small print is gone.
 *
 * Rendered in the same headless chromium as `shot.sh`, because the card has to use
 * the site's own fonts (Barlow Condensed, IBM Plex Mono) and its own colours.
 *
 * Env: OG_OUT (output dir, default /out), OG_ROOT (repo root inside the container).
 */
import { mkdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import puppeteer from 'puppeteer-core';

const out = process.env.OG_OUT ?? '/out';
const root = process.env.OG_ROOT ?? '/work';

/** 1200×630 — the size both Open Graph and Twitter's large card want. */
const WIDTH = 1200;
const HEIGHT = 630;

const font = (name) =>
	`data:font/woff2;base64,${readFileSync(join(root, 'web/static/fonts', name)).toString('base64')}`;

const FONTS = {
	display: font('barlow-condensed-700.woff2'),
	sans: font('barlow-400.woff2'),
	mono: font('plex-mono-500.woff2')
};

/** The header sigil (favicon.svg), inline: cards have to be recognisable without a caption. */
const SIGIL = `<svg class="sigil" viewBox="0 0 64 64" aria-hidden="true">
	<polygon points="32,8 51.5,19.25 51.5,44.75 32,56 12.5,44.75 12.5,19.25"
		fill="none" stroke="#ff7a18" stroke-width="4" stroke-linejoin="round" />
	<polygon points="32,23 39.79,27.5 39.79,36.5 32,41 24.21,36.5 24.21,27.5" fill="#e0391c" />
</svg>`;

/**
 * One card per section. `kicker` is the small line above the title — the same voice
 * as the `label` utility on the site — and `line` is the sentence under it, kept
 * short because it has to be readable at a third of its size.
 */
const CARDS = [
	{
		slug: 'home',
		kicker: 'Free · no shop · everything unlocked',
		title: 'Global Agenda,\nback online',
		line: 'Level 50, everything unlocked, people playing every night.'
	},
	{
		slug: 'play',
		kicker: 'Get started',
		title: 'Play Global Agenda',
		line: 'Install, sign in, pick a mission. About five minutes.'
	},
	{
		slug: 'server',
		kicker: 'Live status',
		title: 'Server status',
		line: 'Who is online right now, and when people usually play.'
	},
	{
		slug: 'events',
		kicker: 'Every week',
		title: 'Events',
		line: 'Friday raids, Saturday challenge matches, Sunday mercenary.'
	},
	{
		slug: 'guides',
		kicker: 'Written by players',
		title: 'Guides',
		line: 'Missions, classes and modes, explained by the people who play them.'
	},
	{
		slug: 'builds',
		kicker: 'Written by players',
		title: 'Builds',
		line: 'Skill trees for PvP and PvE, with a screenshot of each.'
	},
	{
		slug: 'news',
		kicker: 'From the community',
		title: 'News',
		line: 'Server updates, articles and videos from the people who play here.'
	},
	{
		slug: 'missions',
		kicker: 'The archive',
		title: 'Missions',
		line: 'Every completed mission on record: map, mode, winner, duration.'
	},
	{
		slug: 'agents',
		kicker: 'Roster',
		title: 'Agents',
		line: 'Profiles for every agent here, with ratings and career totals.'
	},
	{
		slug: 'leaderboard',
		kicker: 'PvP',
		title: 'Leaderboards',
		line: 'Where every agent stands, class by class.'
	},
	{
		slug: 'community',
		kicker: 'Who we are',
		title: 'Community',
		line: 'The Discord, the people, and how the server is run.'
	},
	{
		slug: 'roadmap',
		kicker: 'Where it is going',
		title: 'Roadmap',
		line: 'What exists, what is being worked on, and what is only an idea.'
	}
];

const html = (card) => `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<style>
	@font-face { font-family: Display; font-weight: 700; src: url(${FONTS.display}) format('woff2'); }
	@font-face { font-family: Sans; font-weight: 400; src: url(${FONTS.sans}) format('woff2'); }
	@font-face { font-family: Mono; font-weight: 500; src: url(${FONTS.mono}) format('woff2'); }

	* { margin: 0; padding: 0; box-sizing: border-box; }
	html, body { width: ${WIDTH}px; height: ${HEIGHT}px; }

	body {
		background: #07090d;
		color: #f2efe9;
		font-family: Sans, sans-serif;
		position: relative;
		overflow: hidden;
	}

	/* The warm glow the pages carry behind their headers. */
	.glow {
		position: absolute;
		inset: 0;
		background:
			radial-gradient(ellipse 70% 90% at 80% -12%, rgba(224, 57, 28, 0.34), transparent 64%),
			radial-gradient(ellipse 55% 70% at 8% 108%, rgba(255, 122, 24, 0.16), transparent 62%);
	}

	/* Corner ticks: the brand's one flourish, and the site's mark for a framed box. */
	.tick { position: absolute; width: 24px; height: 24px; border-color: rgba(224, 57, 28, 0.8); border-style: solid; }
	.tick.tl { top: 26px; left: 26px; border-width: 2px 0 0 2px; }
	.tick.br { bottom: 26px; right: 26px; border-width: 0 2px 2px 0; }

	.frame {
		position: absolute; inset: 26px; display: flex; flex-direction: column;
		justify-content: space-between; padding: 48px 58px 44px;
	}

	/* Wordmark and slogan on their own lines: side by side, both were too small to
	   read once the card was scaled down to an embed. */
	.brand { display: flex; align-items: center; gap: 24px; }
	.sigil { width: 64px; height: 64px; flex: none; }
	.words { display: flex; flex-direction: column; gap: 9px; }
	.name {
		font-family: Display, sans-serif; font-weight: 700; font-size: 54px; line-height: 1;
		letter-spacing: 0.1em; text-transform: uppercase;
	}
	.sub {
		font-family: Mono, monospace; font-size: 26px; line-height: 1; letter-spacing: 0.18em;
		text-transform: uppercase; color: #9aa0ab;
	}

	/* A floor, not an auto margin: the tallest card must not sit on the brand. */
	.body { padding-top: 40px; }
	.kicker {
		font-family: Mono, monospace; font-size: 27px; letter-spacing: 0.26em;
		text-transform: uppercase; color: #ff7a18;
	}
	h1 {
		font-family: Display, sans-serif; font-weight: 700; font-size: 122px; line-height: 0.9;
		text-transform: uppercase; letter-spacing: 0.01em; margin-top: 18px; white-space: pre-line;
	}
	h1.long { font-size: 88px; line-height: 0.94; }
	.line { margin-top: 24px; font-size: 37px; line-height: 1.28; color: #b6bbc5; max-width: 1020px; }

	.foot {
		margin-top: 38px; display: flex; align-items: center;
		font-family: Mono, monospace; font-size: 26px; letter-spacing: 0.16em;
		text-transform: uppercase; color: #7d828d;
	}
	.foot .host { color: #ff7a18; }
</style>
</head>
<body>
	<div class="glow"></div>
	<div class="frame">
		<div class="tick tl"></div>
		<div class="tick br"></div>
		<div class="brand">
			${SIGIL}
			<div class="words">
				<span class="name">The Commonwealth</span>
				<span class="sub">Global Agenda community server</span>
			</div>
		</div>
		<div class="body">
			<p class="kicker">${card.kicker}</p>
			<h1 class="${card.title.length > 16 ? 'long' : ''}">${card.title}</h1>
			<p class="line">${card.line}</p>
			<div class="foot">
				<span class="host">commonwealth.ydns.eu</span>
			</div>
		</div>
	</div>
</body>
</html>`;

mkdirSync(out, { recursive: true });

const browser = await puppeteer.launch({
	executablePath: '/usr/bin/chromium',
	args: [
		'--no-sandbox',
		'--disable-gpu',
		'--disable-dev-shm-usage',
		'--hide-scrollbars',
		'--force-color-profile=srgb'
	],
	protocolTimeout: 30_000
});

try {
	const page = await browser.newPage();
	await page.setViewport({ width: WIDTH, height: HEIGHT, deviceScaleFactor: 1 });

	for (const card of CARDS) {
		await page.setContent(html(card), { waitUntil: 'load' });
		// Fonts must be decoded before the frame is captured, or the card renders in
		// a fallback face — the one thing that would make these look wrong.
		await page.evaluate(() => document.fonts.ready);

		const file = join(out, `${card.slug}.jpg`);
		await page.screenshot({ path: file, type: 'jpeg', quality: 86 });
		const { size } = statSync(file);
		console.log(`  ${card.slug.padEnd(12)} ${String(size).padStart(8)} B  ${WIDTH}×${HEIGHT}`);
	}
} finally {
	await browser.close();
}
