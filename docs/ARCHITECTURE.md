# Architecture

## Shape

```
game server (same VPS)
   └── writes /home/YOUR_USER/games/server.db      (SQLite, WAL)
        │
        ├── :ro ──▶ web container ──▶ /api/status only (live player count)
        │
        └── snapshot job ──▶ /site-data/snapshot.db ──▶ every stats query
                                  │
                            /site-data/site.db    ──▶ website-owned writes

   caddy (edge, :443/:80) ──reverse_proxy──▶ web (:8080, loopback)
```

- **`web`** — one Rust binary (`crates/webapi`). Serves the JSON API, the SSE
  feed, and the built SvelteKit app as static files. Speaks plain HTTP; TLS is
  the edge's problem.
- **`edge`** — Caddy with the host's Let's Encrypt certificates. HTTP/2 + HTTP/3,
  HTTP→HTTPS redirect, and `flush_interval -1` on the proxy so SSE events are
  never buffered.
- **`snapshotter`** (`crates/snapshotter`) — rewrites `snapshot.db` every
  `WEB_SNAPSHOT_INTERVAL_SECS` (default 300): `VACUUM INTO` a temp file
  (consistent under WAL, ~1.5 s warm / ~20 s cold for 200 MB), then
  `secure_delete` and scrub the identifiers — session rows stay (the activity
  chart is built from their timestamps) but their IPs are blanked, password
  verifiers and operator notes are nulled, IP-intelligence rows are dropped —
  and rename into place atomically. A failed cycle keeps the previous snapshot.
- **`snapshot.db`** — the published copy. Every statistics query reads it
  (`AppState::stats_pool()`), so heavy SQL never touches the file the game
  server writes. `/db-export/<token>` serves this same file to third parties;
  it used to be built per download, which cost a backup plus a lock each time.
- **`site.db`** — the website's own database. Accounts, sessions, user content.
  The only writable database.

### Maintenance overrides

`status-override.txt` in the site-data directory replaces the status line
verbatim (`Server maintenance`), read fresh on every request. It defaults to the
*writable* directory because the live DB's directory is mounted read-only — the
one thing the previous site could not actually do in production.

### Why the player count deliberately bypasses the snapshot

`/api/status` is polled by the Discord bot and shown to members. A count that
lags a five-minute snapshot is worse than useless, and the query is trivial
(indices on `ga_instance_players(instance_id) WHERE left_at IS NULL` and
`ga_instances.instance_id`). So this — and only this — reads the live DB, behind
a 60-second TTL cache.

Note the first query after a cold start can take ~1–2 s while the OS pulls pages
of a ~200 MB file in; after that it is ~2 ms. The cache and the single-process
ticker hide that completely.

## Request path

Two processes behind one hostname: the edge sends the paths below to the Rust
API and everything else to SvelteKit's Node server.

```
api   GET /api/status   → StatusService { override file → TTL cache → live DB }
api   GET /api/live     → SSE: current value immediately, then broadcast ticks
api   GET /api/health   → SELECT 1 against the live pool
api   GET /files/*      → download directory, straight from disk
api   GET /sitemap.xml  → generated page list
api   GET /<anything>   → JSON 404 (it is not the website)
web   GET /<anything>   → server-rendered SvelteKit page
```

One background task owns the ticker: it recomputes the status every
`WEB_LIVE_TICK_SECS` and broadcasts it on a `tokio::sync::broadcast` channel.
Every open tab costs one subscriber, not one query.

### Match detail: one endpoint, two layouts

`/api/matches/{instance_id}` decides the shape and the frontend follows it:
the PvP queues (`merc`, `1v1`) get a rated scoreboard — winner/loser split,
pre-match MMR per player, team balance, per-class matchups — while every other
queue gets the mission layout, where the statistics and the map are the story.
Both carry the same participant rows, so nothing the PvE view shows is lost in
the PvP one.

The page itself (`web/src/routes/missions/[id]`) renders on the client: which
layout applies is only knowable from the database at request time, and fetching
that in the browser keeps the shell identical for every match.

### Player settings: the site relays, the control server decides

The switches that used to be chat commands only (`-togglebrokensuits`,
`-toggleallsuits`, `-togglesolomode`, `-enabledlc`/`-disabledlc`) land in
`server.db` (`ga_user_preferences`, `ga_user_dlc`), but the live effect of the
suit toggles is an IPC player-action the *control server* pushes to the game
DLL — and the DLL caches a player's preferences in memory, so writing the
database alone would not be seen until the player relogged.

So the website never writes the game database. `GET /api/me/settings` reads it
(cheap, read-only, and works even when nothing else is configured); `POST`
forwards the change to the control server's `POST /player/preferences`
(token in `X-Admin-Token`, loopback only), which persists it and pushes the
live action when the player is online.

With `WEB_CONTROL_API_URL` unset the page still renders — values, notes, and a
"read-only for now" notice — so a deployment can adopt this at its own pace.

### One rendering model

`adapter-node` in `web/svelte.config.js`: the site is a Node process, and every
page is server-rendered. There is no prerendered build and no app shell, and
therefore no second way for a page to come into existence.

This is what makes the news feed work. A post is written after any build could
have run, so a static site could only reach it by re-rendering in the browser,
which no crawler does. `/news` therefore has a `+page.server.ts` that fetches the
first page from the API with the visitor's cookie: the HTML that leaves the
server contains every entry in full. The browser that receives that HTML
navigates client-side from then on, so crossing into or out of the feed is not a
page load, and the site chrome exists once, in the layout.

Pages that read the API in the browser (`/agents`, `/missions/[id]`,
`/leaderboard`) still render their shell on the server and fill in on mount —
which is now hydration rather than a shell fetched as a file.

An earlier design split the two: static pages plus a Rust-rendered news section.
It was crawlable, and it cost two renderers, two copies of the chrome, and a full
reload whenever you crossed the seam. `scripts/smoke-ssr.sh` exists so the
property that design was protecting cannot quietly break again: it seeds an entry
and greps the server's HTML for it.

## Guides and builds

One section, two categories, one moderation queue — sharing authorship, review
and the tiled list, differing in what they are made of. A **guide** is writing
with 1..n tags from a closed set (`basics`, `strategy`, `class`, `mode`, `pve`,
`pvp`). A **build** is a skill tree screenshot with the one class it is spent on,
the queues it is for (`pvp`, `pve`, either or both), an optional description and
an optional Discord thread link — builds already live in Discord, and the link
keeps the discussion there. Which fields are required is decided in one
`guides::validate`, so the API, the form and the tests cannot hold different
opinions.

Approving an entry writes a **news post** that points at it: `kind = news`, the
entry's author, `guide_id` set, and its own copy of the title, class and image so
the feed never joins across to the guides tables. The feed draws those full width
as the entry's own tile. Rejecting or unpublishing deletes the announcement —
a post pointing at a page nobody can open is worse than no post.

Search is `LIKE` over title, summary and body with the wildcards escaped;
ordering is "most helpful" (thumbs, then newest) or newest. Likes are one row per
account per entry.

### Screenshots are never stored as uploaded

Players may not upload arbitrary files, and the pipeline is how that is kept
true rather than promised. Nothing a player sends is ever written to disk or
served:

```
sniff the format from the bytes (not the header, not the file name)
→ PNG, JPEG or WebP only — SVG is a document that can carry script, not an image
→ cap 4000×4000 and total allocation *before* decoding (a header claiming
  60000×60000 fails here, not in the OOM killer)
→ decode with a pure-Rust decoder, off the async workers
→ downscale to 1600 px (twice a tile, so it stays crisp on a retina screen)
→ re-encode to WebP → store content-addressed as site-data/images/<sha256>.webp
```

Re-encoding is the sanitiser: EXIF, XMP, ICC, text chunks and anything appended
after the image's end marker die with the original bytes. Two tests hold this
down — a PNG with a PHP payload glued after its end marker, and an SVG file —
and `/images/<name>` serves nothing that is not `<hash>.webp`.

The form accepts a file, a drag and drop, or a clipboard paste, and uploads
immediately so the preview *is* the derivative: what an author sees before
submitting is exactly what will be stored, and a file that cannot be sanitised
fails before the form is filled in. A submission may only reference an image its
own author uploaded.

## Data model worth knowing

`server.db` is far richer than the old site exposed:

| Table | Rows (Aug 2026) | Use |
|---|---|---|
| `ga_match_events` | 1.47M | Kills, assists, device events with `game_time` — timelines, per-match narratives |
| `ga_match_player_stats` | 33.7k | 20 metrics per player-match — the heart of player stats |
| `ga_match_device_stats` | 32.3k | Per-device effectiveness |
| `ga_instances` | 12.3k | Match history, outcomes, modes, maps |
| `ga_mmr_history` | 8.3k | Rating curves |
| `ga_character_*`, `ga_players_inventory` | 39k / 601k | Loadouts, gear, skills |
| `asm_data_set_*` | — | The game's own catalogue: devices, skills, items, loot tables |

**Known gap:** `ga_match_events` has no position columns (only actor/target ids,
`detail` and `flags`). Map heatmaps and kill-zone visuals need positional data,
which means a small game-server schema change — the single most visually
compelling feature is not free.

## Next: precomputed analytics

The plan, in order of leverage:

1. **`analytics.db`** — an `analytics` crate run by the snapshot job that folds
   the snapshot into per-player rollups, per-match summaries, MMR timelines,
   leaderboards and per-map/per-device effectiveness tables. Requests then read
   indexes instead of aggregating 1.5M rows. This is the single biggest
   performance decision in the project, and it is a data-shape decision, not a
   language one.
2. **Live game bridge** — the control server already exposes JSON over TCP:
   the IPC port (`MSG_ADMIN_ACTION` behind `admin_token`, `PLAYER_JOINED`,
   `INSTANCE_HELLO`) and ServiceHooks (`mission_end`, `instance_state`,
   `pvp_reward`, `character_login`, …). A Rust subscriber turns those into SSE
   events and, later, into interactive features (queue state, instance control).
   Nothing new is needed on the game side.
3. **Accounts — done.** Sign-in uses the in-game credentials. The client derives
   an RC4 keystream from `MD5(MD5(utf16le(lower(user) + password + SALT)))`; the
   control server stores `SHA256(keystream)` in `ga_users.password_verifier`
   (`Auth/LoginAuth.hpp`, and `.planning/2026-06-08-login-password-flow.md` for
   the full RE, including the salt `FA00CDF5-8D87-4a76-BEE1-D9E483220C13`).

   The browser reproduces that derivation (`web/src/lib/crypto.mjs`, verified
   against an independent implementation) and posts the keystream, so the
   plaintext password never reaches this server — nor does it ever need to. The
   API hashes the keystream and compares it in constant time, then issues an
   opaque token: HttpOnly, SameSite=Lax, `Secure` behind the HTTPS edge, 30 days,
   stored in `web_sessions` on `site.db`. Attempts are throttled per forwarded
   address. Accounts with `password_verifier IS NULL` have never logged in to
   the game and cannot sign in here.

   Discord identity is deliberately not part of this yet.

4. **User content** (custom cards, submissions) — uploads live on the
   `site-data` volume, metadata in `site.db`. Treat every upload as hostile:
   size-limit, re-encode images, never serve user bytes back verbatim.

## Deployment

```
~/website/
  releases/<utc-stamp>-<short-sha>/   compose files + .env (image tag) + override
  current -> releases/...             symlink; flipping it is the deploy/rollback
  site-data/                          website-owned writable data
```

The image is built in GitHub Actions and pushed to GHCR. The VPS only pulls, so
a deploy is a container swap — compiling Rust on the machine that also runs the
game server would turn a two-minute deploy into ten.

Each release directory pins its image tag in `.env`, which is what makes
rollback a symlink flip: the target release already knows which image it wants.

`docker compose -p commonwealth-ga-web --profile edge up -d` runs both
containers; the pinned project name means every release directory manages the
same containers.

### Certificate renewal

Caddy reads the host's certificate files at start. `certbot.timer` renews them
and `/etc/letsencrypt/renewal-hooks/deploy/*` restarts the edge container — the
same model the previous site used, minus the TLS code in the app.

## Invariants

- The game DB is read-only, always: `mode=ro` + `query_only`, mounted `:ro`.
- `/api/status` stays plain text with the same wording. It is a public contract.
- `/files/*` keeps its historical paths: the launcher downloads restored content
  from them, and the directory is read per request so files can be dropped in or
  replaced while the server runs.
- No secrets in the repo; the override template is structural only.
- Every route degrades: 404/503 with a small body, details in the log, never in
  the response.
- New dependencies in `crates/webapi` need a reason. The previous site's
  "stdlib only" rule became "no framework sprawl" rather than "no dependencies".
