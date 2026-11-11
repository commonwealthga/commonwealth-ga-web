# commonwealth-ga-web

Rust (`axum`) + SvelteKit rewrite of the Commonwealth GA website. See
README.md and docs/ARCHITECTURE.md first. The previous Python site lives on in
`../commonwealth-ga-website` (deployable until cutover) and the game server in
`../commonwealth-ga-server`.

## Commands

- Tests: `cargo test` (no DB or network needed)
- Lint/format: `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings`
- Run: `WEB_DB_LIVE=/path/to/server.db cargo run`
- Frontend dev (no host Node needed): `docker compose --profile dev up web-dev`
  (HMR on `:5173`, proxies `/api` to the host backend)
- Frontend build/check: `cd web && npm run build && npm run check` — needs
  Node ≥ 20 on the host (Tailwind v4's engine); the container and CI use 22.

CI (`.github/workflows/ci.yml`) runs all of the above.

## Hard invariants

- **`/api/status` is a public contract**: plain text like `3 players in-game`,
  polled by the Discord bot. Don't change its path, content type or wording
  without coordinating. Tests pin it (`crates/webapi/tests/status_contract.rs`).
- **The game DB is read-only.** Mounted `:ro` *and* opened with `mode=ro` +
  `query_only`. Website writes go to `site.db` only. Only `/api/status` reads
  the live DB; everything heavy reads `snapshot.db`, published by
  `crates/snapshotter` (redacted: session rows kept but their IPs blanked,
  password verifiers and operator notes nulled). The activity chart is built
  from those session rows, so they must stay; only the addresses go.
  Login and player settings deliberately stay on the live DB — a five-minute-old
  copy would reject an account registered since, and would show a toggle you
  just flipped as un-flipped.
- **`/files/*` is a public contract.** Restored-map archives the launcher
  downloads; the URLs outlive any reimplementation. Served from `WEB_FILES_DIR`,
  read from disk per request, so files can be added or replaced without a
  restart. Payloads are gitignored — the repo only tracks the directory.
- **The site is source-available, not open source.** PolyForm Shield 1.0.0
  (`LICENSE`, `NOTICE`): contributors may read, change and run it; nobody may
  use it to provide a competing product. Don't add code under another licence,
  don't relicense, keep the `Signed-off-by` rule in `CONTRIBUTING.md`, and don't
  describe the project as open source.
- **No secrets or host paths in the repo.** They live in the gitignored
  `docker-compose.override.yml`, rendered at deploy time from
  `deploy/docker-compose.override.template.yml`.
- **No passwords reach this service.** Sign-in posts an RC4 keystream derived in
  the browser; the server only compares `SHA256` against `ga_users.password_verifier`.
  Never log the keystream, never store it, never accept it over plain HTTP.
- **Every route degrades gracefully**: 404/503 with a small body, details in the
  log, never a panic or an internal error string in the response.
- **Compression never wraps the SSE stream** — it would buffer it.

## Conventions

- Route handlers stay thin; data access lives in small typed functions
  (`src/status.rs` is the model). New public data sources follow that shape.
- Config is environment-only (`src/config.rs`); no config files, no CLI args.
- The lib/bin split exists so tests build the router without a socket — keep
  `app()` free of I/O and side effects.
- The frontend is prerendered by default; interactive routes opt out per-route
  (`export const prerender = false; export const ssr = false`). Static fallback
  is `200.html`, never `index.html`.
- Add a test with every route or helper; contracts get fixture-DB HTTP tests,
  not unit tests of the query string.
