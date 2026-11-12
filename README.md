# commonwealth-ga-web

The website for the Commonwealth Global Agenda server — live at
<https://commonwealth.ydns.eu/>.

A from-scratch rewrite of the previous Python site: a Rust API and app server, a
SvelteKit frontend, and a live event feed, built to grow into a real web
interface to the game (accounts, loadouts, matchmaking, match and player
statistics).

The previous site lives on in `commonwealth-ga-website` and stays deployable
until this one is cut over.

## Stack

| Layer | Choice | Why |
|---|---|---|
| API / app server | Rust, `axum` + `tokio` + `sqlx` (SQLite) | One small binary that serves the API, the live feed and the built frontend; heavy aggregate queries over a 200 MB DB without a per-request connection cost |
| Live data | Server-sent events | One process-wide ticker fans out to every client; no WebSocket plumbing for a one-way feed |
| Frontend | SvelteKit 2 / Svelte 5, Tailwind CSS v4, `adapter-static` | Prerendered content pages plus an app shell, small payloads on mobile, no Node in production |
| Edge | Caddy | TLS with the host's existing Let's Encrypt certificates, HTTP/2 + HTTP/3, SSE-safe proxying |
| Deploy | GitHub Actions → GHCR → VPS pull | A deploy is a pull and a container swap, not a Rust compile on the game server |

## Layout

```
crates/webapi/          Rust: API, live feed, static file serving
  src/config.rs         environment-only configuration
  src/db.rs             read-only / read-write SQLite pools
  src/files.rs          /files/* downloads, read from disk per request
  src/match_detail.rs   one match: scoreboard, ratings, PvP/PvE classification
  src/agents.rs         public profiles, directory, leaderboards, rating history
  src/mmr.rs            which rating table is live, and how to read it
  src/status.rs         the /api/status contract + TTL cache
  src/routes/           route table (thin handlers only)
  tests/                HTTP-level contract tests against a fixture DB
web/                    SvelteKit frontend (builds to web/build)
deploy/                 Caddy config, production compose override template
docs/ARCHITECTURE.md    how the pieces fit and why
```

## Development

Two processes: the Rust API on `:8080` and the Vite dev server on `:5173`
(which proxies `/api` to the backend, SSE included).

The frontend dev server runs in a container, so **nothing about the frontend
touches the host toolchain** — no Node needed on the machine at all:

```sh
# terminal 1 — API, on the host (Rust is the only host tool needed)
WEB_DB_LIVE=/path/to/server.db cargo run

# terminal 2 — frontend with HMR, in a Node 22 container
docker compose --profile dev up web-dev
```

Then open <http://localhost:5173>. Changes under `web/src` hot-reload.

`web-dev` proxies `/api` to `http://host.docker.internal:8080` (reaching the
backend on the host); set `VITE_API_TARGET=http://web:8080` to point at the
`web` service instead. Its `node_modules` live in a named volume, so a host-side
`npm install` can never collide with the container's.

Running npm directly on the host is optional and, if you do, needs **Node ≥ 20**
(Tailwind CSS v4's engine) — `cd web && npm install && npm run dev`. The
Docker build and CI always use Node 22.

To run the production shape locally instead:

```sh
cd web && npm run build          # writes web/build
WEB_DB_LIVE=/path/to/server.db WEB_STATIC_DIR=web/build cargo run
```

Or the container (compose **v2** is required — `docker compose`):

```sh
WEB_DB_HOST_DIR=/path/to/db-dir WEB_SITE_DATA_DIR=./site-data docker compose up --build web
curl http://127.0.0.1:8080/api/status
```

The `edge` profile (Caddy + TLS) is for production; to try it locally, put a
certificate pair in `deploy/certs/public.crt` / `public.key` and run
`docker compose --profile edge up`.

### Sign-in

Accounts are the in-game ones — there is no separate website account. The
browser reproduces the client's credential derivation
(`web/src/lib/crypto.mjs`), so the plaintext password never leaves the machine:
it posts a hex RC4 keystream, the API compares `SHA256(keystream)` against
`ga_users.password_verifier`, and exchanges it for an HttpOnly session cookie
kept in `site.db`. Failed attempts are throttled per address.

### Screenshots

```sh
scripts/shot.sh                       # home, network, archive, deploy + mobile
scripts/shot.sh "missions=/missions@1440,1050"
URL=http://127.0.0.1:5173 scripts/shot.sh    # against a running dev server
```

Captures land in `.shots/` (gitignored). The script starts its own server on
`0.0.0.0:8091` so the browser container can reach it, waits for hydration
before shooting, and fails loudly if a capture comes back blank.

### Tests and checks

```sh
cargo test                                  # contract tests, no DB or network needed
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings
cd web && npm run check && npm run build
```

CI runs exactly these (`.github/workflows/ci.yml`).

### Environment

| Variable | Default | Meaning |
|---|---|---|
| `WEB_BIND` | `0.0.0.0:8080` | Listen address (plain HTTP; TLS is terminated at the edge) |
| `WEB_DB_LIVE` | — | Live game DB, opened read-only. Only `/api/status` reads it |
| `WEB_DB_SNAPSHOT` | — | Published snapshot; every statistics query reads it |
| `WEB_SNAPSHOT_INTERVAL_SECS` | `300` | How often the snapshotter republishes |
| `WEB_DB_EXPORT_TOKEN` | — | Path token for `/db-export/<token>`; unset disables the route |
| `WEB_NEWS_POSTING_MODE` | `open` | Who may submit news: `open` or `allowlisted` (holders of `news_allowlisted` only). Anything else refuses to start |
| `WEB_SITE_URL` | `https://commonwealth.ydns.eu` | Absolute base for canonical links and the sitemap |
| `WEB_DB_SITE` | — | Website-owned read-write DB |
| `WEB_STATIC_DIR` | `web/build` | Built frontend |
| `WEB_FILES_DIR` | `files` | Downloads served under `/files/*` |
| `WEB_STATUS_TTL_SECS` | `60` | How long a player count may be reused |
| `WEB_LIVE_TICK_SECS` | `15` | How often the live feed recomputes |
| `WEB_STATUS_OVERRIDE` | `<dir of site db>/status-override.txt`, else next to the live DB | Text that replaces the status line while it exists |
| `RUST_LOG` | `info,tower_http=warn` | Log filter |

A missing database is never fatal: the process starts, `/api/health` reports
which DBs are up, and the affected routes answer 503.

To announce maintenance, write the text into `status-override.txt` in the
site-data directory (`~/website/site-data/` in production) — it takes effect on
the next request and deleting the file reverts it. The live DB's own directory is
read-only, hence the writable default.

## Endpoints

| Path | What it serves |
|---|---|
| `/api/status` | **Frozen contract.** Plain text, e.g. `3 players in-game` (or `1 player in-game`). The Discord bot polls it and prints the body verbatim |
| `/api/health` | JSON liveness: live/snapshot/site DB availability |
| `/api/live` | Server-sent events carrying `{ok, players, text, ts}` |
| `/api/matches/recent?kind=&page=&limit=N` | Newest completed missions, paged (`kind` = `all`, `pvp`, `pve`). Adds `viewer` per match when signed in |
| `/api/agents?q=&page=&limit=` | Public agent directory, most recently active first, name search |
| `/api/agents/{user_id}` | Public career profile: totals, per-class rating with leaderboard rank |
| `/api/agents/{user_id}/matches?kind=&page=` | That agent's matches (`kind` = `all`, `pvp`, `pve`) |
| `/api/agents/{user_id}/history?class=&days=` | Rating over time for one class (or all) |
| `/api/leaderboard?class=&page=` | Current standings for a class |
| `/api/me/settings` | Your toggles (`GET`) and changes to them (`POST`), relayed to the control server |
| `/api/me/settings/dlc` | Mark a DLC map pack installed/uninstalled (`POST`) |
| `/db-export/<token>` | Redacted database download for third parties (the cards registry). Disabled unless `WEB_DB_EXPORT_TOKEN` is set |
| `/news` | **Server-rendered HTML**, not JSON: the whole feed, every entry in full, with canonical link and Open Graph tags. Crawlable because it cannot be prerendered |
| `/news/{id}-{slug}` | `301` to `/news#post-{id}`. Kept so an existing link keeps working; there is no per-post page |
| `/news/cards?offset=&limit=` | The next batch of entries as an HTML fragment, with `X-News-More: true\|false` |
| `/sitemap.xml` | Generated: the static pages plus every published post. Served as `application/xml` |
| `/api/news?limit=&offset=` | Published posts, plus whether *you* may submit |
| `/api/news/posting-rules` | Per-kind form hints, limits, and *your* remaining allowance |
| `/api/news/mine` | Your own submissions and their moderation state |
| `/api/news/queue` | Pending submissions — `news_moderator` only |
| `/api/news` (`POST`) | Submit a post. Always lands pending; rate limited per account |
| `/api/news/{id}` (`PATCH`) | Edit your own entry, while it is pending only |
| `/api/news/{id}/approve`, `/api/news/{id}/reject` | Moderate. Rejecting requires a note — `news_moderator` only |
| `/api/news/{id}/unapprove` | Unpublish for a fix, with a note the author sees — `news_moderator` only |
| `/api/news/{id}/note` | Add that note to a pending entry without deciding it — `news_moderator` only |
| `/api/news/{id}/thumb` | Toggle your thumbs-up |
| `/api/news/preview`, `/api/news/posting-rules` | Editor support: the rendered body and the rules the form must respect |

Player settings (`/settings`) need the control server's endpoint: set
`WEB_CONTROL_API_URL` (e.g. `http://control.internal:19020`) and
`WEB_CONTROL_API_TOKEN` (the control server's `admin_token`). Both are optional —
without them the page renders read-only rather than failing. In production they
come from the `WEB_CONTROL_API_URL` repository variable and the
`WEB_CONTROL_API_TOKEN` secret, rendered into the override at deploy time.

### Repository secrets

Set with `gh secret set <name>`; the deploy job reads them and renders the
override. Everything else the deployment needs is a repository *variable*.

| Secret | Meaning |
|---|---|
| `DEPLOY_HOST` | VPS hostname or IP |
| `DEPLOY_PORT` | SSH port of the deploy account |
| `DEPLOY_USER` | Deploy account |
| `DEPLOY_SSH_KEY` | Private key for that account |
| `DEPLOY_KNOWN_HOSTS` | Pinned host key, from `ssh-keyscan -p <port> <host>` |
| `WEB_CONTROL_API_TOKEN` | `admin_token` from the game server's `control-server.json`; unset makes the settings page read-only |
| `WEB_DB_EXPORT_TOKEN` | Path token for `/db-export/<token>`; unset makes the route 404, which breaks the cards registry |

The last two are optional in the sense that the site still starts without them —
it just quietly loses a feature, so the deploy warns when either is empty.
| `/api/matches/{instance_id}` | One match in full: `kind` is `pve` or `pvp`, plus participants, team totals and — for PvP — pre-match MMR and team balance |
| `/api/online-history?days=N` | Hourly peak concurrent players, two parallel arrays |
| `/api/auth/login` · `/api/auth/logout` · `/api/auth/me` | Sign-in with in-game credentials (see below) |
| `/api/me/matches` · `/api/me/summary` | The signed-in player's own matches and career totals (401 without a session) |
| `/files/*` | **Public contract.** Download directory: restored maps and other archives the launcher fetches |
| everything else | The built SvelteKit app: prerendered pages, `200.html` shell for client-side routes |

Unknown `/api/*` paths answer JSON 404 — they never fall through to the SPA.

`/files/*` is served straight from disk on every request: drop an archive into
the directory (or replace one) and it is downloadable immediately, with no
restart and no rebuild. Responses carry `Cache-Control: no-cache`, `Accept-Ranges`
and `Last-Modified`, so a replaced file is revalidated rather than served stale
and a long download can resume. Dotfiles, `..` and other unsafe paths are 404s.

```bash
# production: /home/YOUR_USER/public/files (read-only mount at /srv/files)
# locally: the repository's own files/ directory
cp surfside-atoll.zip files/ && curl -I http://127.0.0.1:8080/files/surfside-atoll.zip
```

## News

A community feed with pre-moderation: anyone signed in may submit, and nothing
appears on the site until a moderator approves it. Posts live in `site.db`
(`news_posts`, `news_videos`, `news_reactions`, `news_audit`) — never in the game
database.

**There is one page.** `/news` shows every entry in full — headlined
"USERNAME posted an article · 24 November 2025" — with its body, its embeds and
its thumbs-up button. There is no detail page: a post URL (`/news/12-slug`)
redirects to the entry's anchor (`/news#post-12`), so old links still land
somewhere sensible, and the sitemap lists `/news` rather than hundreds of
URLs that only redirect.

**The feed arrives a page at a time.** The first ten entries are server-rendered;
scrolling to the bottom fetches the next ten from `/news/cards` — the same `<li>`
markup, so there is one renderer — and appends them. A `Load older entries`
button below the list does the same job with no JavaScript at all, and stays as
the fallback when the fetch is slow. `X-News-More` says whether to ask again.

**The allowance is visible before it is spent.** Every account may submit five
entries a day (rejected ones do not count). `/api/news/posting-rules` reports
`remaining`, `used`, `reset_at` and the clock time it lifts, and both the feed
and the submission form use it: the feed shows "3 of 5 left today", and at zero
it replaces the form link with when posting resumes rather than offering a form
that will be refused. The refusal message names the limit and the time, and does
not suggest asking a moderator — they cannot change it.

**Each kind asks for only what it needs.** `news`, `update` and `article` take a
title, a summary and a Markdown body. `video` takes YouTube links and nothing
else — no invented title, no empty body — up to six per post, laid out two to a
row on a wide screen. `guide` is not a kind: guides get their own page later and
will appear in this feed as "posted a guide".

**Roles** are `ga_user_roles` rows, the same table that gates `spectator`, and
this service only ever **reads** them. They are granted in the dashboard, which
goes through the control server's token-gated `set-role` action; the website
deliberately holds no admin token.

| Role | Effect |
|---|---|
| `news_moderator` | Sees the review queue on `/news`; may approve and reject there |
| `news_restricted` | May not submit, whatever the posting mode |
| `news_allowlisted` | May submit while `WEB_NEWS_POSTING_MODE=allowlisted` |

Closing posting to everyone but an allowlist is a repository variable change
(`WEB_NEWS_POSTING_MODE=allowlisted`) plus a deploy — see the RUNBOOK.

**Editing.** An author may edit their own entry only while it is still in the
queue (`PATCH /api/news/{id}`); the edited HTML is re-rendered and the edit is
recorded in `news_audit`. Published entries are not editable — that would make
the queue advisory — so a moderator who spots a typo **unpublishes** it with a
note, which puts it back in the queue with the reason shown to the author, and
the author then fixes it and it is reviewed again.

**Moderation happens on the feed.** `/news` renders the pending queue at the top
for anyone with `news_moderator`, and every entry carries the actions for its
state: **Approve** / **Request changes** / **Reject** while pending,
**Unpublish** / **Delete** once published. Every button that decides an entry's
fate asks for confirmation first, and the ones that take it out of the feed —
plus Request changes — ask for a reason, which the author sees on the entry.
Requesting changes leaves the entry exactly where it is; approving clears any
note, so a published entry never still says "please fix". There is no
separate admin page to remember. An author sees their own unpublished posts in
the same feed, labelled `Waiting for review` — they are the only other person who
can see them.

**Rejection is the soft delete.** The row keeps its body, author, Markdown and
the moderator's note; it disappears from the feed, the queue, the author's own
list and the sitemap. Moderators can still open it by id for review. Rejection is
final: a second reject or a re-approve is refused rather than overwriting the
record.

**Video posts carry no text**, so `news_videos` holds their ids in author order;
the legacy `video_id` column keeps the first one for older readers.

**Post bodies are a Markdown subset** rendered by `news::markdown` to HTML it
writes itself — raw HTML is escaped and shown rather than interpreted, images are
dropped, links are limited to `http(s)`/`mailto`/site-relative and marked
`nofollow ugc`. The rendered HTML is stored at submission, so what a moderator
approves is exactly what is served, and a preview cannot disagree with the
published page.

**Why the pages are rendered in Rust** rather than SvelteKit: the frontend is
`adapter-static`, which cannot serve a post written after the build, and an app
shell is invisible to a crawler. The cost is that the site chrome is duplicated
in `news::pages`, which a test guards by reading `Nav.svelte` and failing if the
links diverge.

**And the cost you can feel:** `/news` is not a SvelteKit route, so entering or
leaving it is a real document load rather than a client-side navigation — a white
flash where every other page glides. Two mitigations are in place, neither of
which pretends the seam is gone:

- `@view-transition { navigation: auto; }` in `app.css`, which both documents
  link, makes supporting browsers cross-fade the load instead of flashing.
- The nav's news link carries `data-sveltekit-reload`, so the router does not
  waste a request discovering that the app cannot render it.

Removing the seam entirely means server-rendering the whole site (`adapter-node`
instead of `adapter-static`), which is a different deployment shape: a Node
process instead of static files, for every page, not just this one.

## Invariants

- **`/api/status` is a public contract.** Don't change its path, content type or
  wording without coordinating with the Discord bot.
- **The game DB is read-only.** `server.db` belongs to the game server, is
  mounted `:ro`, and is opened with `mode=ro` + `query_only`. Website writes go
  to `site.db` only.
- **`/files/*` paths are a public contract.** The launcher downloads content
  from these exact URLs. Files may be added or replaced at runtime; the URLs
  must keep working. Payloads are mounted, never committed (`.gitignore`).
- **No secrets in the repo.** Host paths and TLS paths live in the gitignored
  `docker-compose.override.yml`, rendered at deploy time from
  `deploy/docker-compose.override.template.yml`.
- **Every route degrades gracefully** — 404/503 with a small body, never a
  panic and never an internal error string in the response.

## Running locally

```bash
./scripts/dev.sh            # http://127.0.0.1:8080
./scripts/dev.sh --stop     # stop it again
./scripts/dev.sh --restart  # stop, then start (picks up Rust changes)
```

Stopping is one command because the alternative is not obvious: the server holds
open the server-sent-event connections from any browser tab, and a graceful
shutdown would wait on a stream that never ends. So `Ctrl-C` drains (the SSE
streams are hung up for you), a **second** `Ctrl-C` exits immediately, and a
5-second backstop exits regardless. `--stop` sends `SIGTERM`, waits the same
grace, and only then uses `SIGKILL`.

The script builds if needed, points at the game server repo's `server.db`, and
reads the control server's `admin_token` out of its `control-server.json` so
`/settings` can actually write. Override with `PORT`, `SERVER_REPO`,
`CONTROL_CONFIG`, or the individual `WEB_*` variables; the header it prints says
whether the settings page is writable.

To exercise the snapshot path (heavy stats read `snapshot.db`, and
`/db-export/...` starts working) run it with `--snapshot`: it publishes a
redacted copy into `site-data/`, points the site at it, and prints the export
URL with a local token. Without the flag everything reads the live database, so
a plain `./scripts/dev.sh` behaves like a checkout with no snapshotter.

Player settings need both halves running:

1. the **control server** (`./out/control-server` in the game server repo) with
   `settings_api_port` set in its config — 19020 in this tree;
2. this site, with `WEB_CONTROL_API_URL` pointing at that port and
   `WEB_CONTROL_API_TOKEN` set to the same `admin_token`.

Without them the page still renders — values, help text, and a plain "read-only
for now" notice — so it degrades instead of pretending.

## Deployment

The VPS needs **compose v2** (`docker compose`); the legacy `docker-compose`
v1 binary cannot read this compose file.

Actions → Deploy → Run workflow. `deploy` is a git ref (default `master`) or
`previous` / `previous-N` to roll back.

The workflow builds the image, pushes it to `ghcr.io/commonwealthga/commonwealth-ga-web`
tagged with the commit SHA, then on the VPS:

1. extracts the release into `~/website/releases/<utc-stamp>-<sha>/` with a
   rendered `docker-compose.override.yml` and a `.env` pinning the image tag,
2. flips the `~/website/current` symlink,
3. `docker compose --profile edge pull && up -d`,
4. health-checks `/api/status` from inside the VPS.

Because each release directory pins its own image tag, rolling back is a symlink
flip plus `up -d`.

### TLS / certificates

The edge reads the host's existing Let's Encrypt certificates from
`/etc/letsencrypt/live/<domain>/`. Caddy loads them at start, so renewal is a
restart — and because the edge owns port 80, renewals must use the **webroot**
authenticator rather than `standalone`:

```sh
certbot renew --webroot -w /var/www/certbot \
  --deploy-hook "cd ~/website/current && docker compose -p commonwealth-ga-web --profile edge restart edge"
```

`/var/www/certbot` is mounted read-only into the edge container, which serves
`/.well-known/acme-challenge/*` from it. Nothing about cookies depends on the
deployment: the session cookie is host-scoped by the browser, and its `Secure`
flag is set from the `X-Forwarded-Proto` header that Caddy sends.

Deploys are gated on the `production` environment, which should require a
reviewer: **Settings → Environments → `production` → Required reviewers**. With
the workflows public, that click is what stands between a mistake and the live
site.

### Security

This repository is public, and is written to be safe to publish:

- **No credentials in the code or its history.** Deployment secrets live in
  GitHub Actions secrets; host paths and certificate paths live in the
  gitignored `docker-compose.override.yml`, rendered at deploy time from the
  template.
- **Nothing privileged depends on secrecy.** Sign-in compares a hash the game
  client can already compute — the salt ships inside the client — so reading
  this code grants no advantage. Session tokens are 32 bytes from the OS CSPRNG,
  cookies are `HttpOnly; SameSite=Lax` and `Secure` behind TLS, failed attempts
  are throttled per address, and every `/api/me/*` route requires a session.
- **The game database is read-only** (`mode=ro` + `query_only`, mounted `:ro`).
  Website writes go to `site.db` only.
- **A missed deploy gate is the main operational risk**, hence the required
  reviewer on the `production` environment.

Found something? Please report it privately — a DM to a maintainer in
[Discord](https://discord.gg/CqACf4qArk) rather than a public issue.

### Secrets

| Secret | Meaning |
|---|---|
| `DEPLOY_HOST` / `DEPLOY_PORT` / `DEPLOY_USER` | SSH endpoint of the VPS |
| `DEPLOY_SSH_KEY` | Private key of the dedicated deploy keypair |
| `DEPLOY_KNOWN_HOSTS` | `ssh-keyscan` pin of the VPS host key |

Repository variables:

| Variable | Meaning |
|---|---|
| `WEB_DATA_HOME` | **Required.** Home directory that holds the mounted data — `<home>/games`, `<home>/website/site-data`, `<home>/public/files`. This is deliberately *not* derived from `DEPLOY_USER`: the deploy account is unprivileged and the data belongs to the admin account. |
| `WEB_CONTROL_API_URL` | Control server base URL for the player-settings relay. Unset leaves the page read-only. |
| `WEB_UID` / `WEB_GID` | uid/gid the containers run as, matching the owner of the mounted files (default `1000`). |

The deploy workflow also takes a `start` input. With `start: false` it uploads a
release and its compose files without flipping `current` or starting anything —
useful for staging a release before a cutover. The default, `true`, is the
normal deploy.

## Operations

The deploy/cutover/rollback sequences, the certificate renewal switch and the
fail2ban settings are in [docs/RUNBOOK.md](docs/RUNBOOK.md).

## Licence

Source-available under the [PolyForm Shield License 1.0.0](LICENSE): read it,
run it, change it, fork it, send pull requests — but don't use it to provide a
product that competes with The Commonwealth. This is not an OSI open-source
licence, and the community page describes it as source-available for that
reason.

The intent is narrow and worth stating plainly: the site was built for this
server, and it should not turn up as the website of another Global Agenda
community. Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md),
which also covers the `Signed-off-by` requirement — and they land under the same
licence.

The name, wordmark, palette and site art are not covered by the licence at all;
see [NOTICE](NOTICE). Third-party components keep their own terms: the fonts are
OFL-1.1, the dependencies are listed in `Cargo.lock` and
`web/package-lock.json`.

The game server is a separate work under AGPL-3.0, chosen so it can never be
taken closed; the website is not, because a permissive or copyleft licence would
let another community run this exact site.
