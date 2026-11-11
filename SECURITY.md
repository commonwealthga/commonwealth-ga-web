# Security policy

## Reporting a vulnerability

Please **do not open a public issue** for anything security-related. Send a
direct message to a maintainer in [our Discord](https://discord.gg/CqACf4qArk)
with enough detail to reproduce the problem. You will get an acknowledgement,
and credit if you want it once a fix is out.

There is no bug bounty. This is a volunteer project running a free game server;
the reward is a working server and our thanks.

## Scope

In scope:

- This website: `crates/webapi` (HTTP API, sign-in, queries) and `web/`
  (SvelteKit frontend).
- The deployment configuration in `deploy/` and the workflows in
  `.github/workflows/`.
- Anything that lets one account act as another, exposes another player's
  private data, or lets a request write to the game database.

Out of scope:

- The game client itself, and the game server in
  [commonwealth-ga-server](https://github.com/commonwealthga/commonwealth-ga-server)
  (report those there).
- Denial of service by volume (it is one small VPS; we know).
- Findings that require an already-compromised host or database dump.

## What this service deliberately does

Some design choices look unusual but are intentional:

- **Passwords never reach this server.** The browser reproduces the game
  client's credential derivation and posts an RC4 keystream; the API compares
  `SHA256(keystream)` against `ga_users.password_verifier`. There is no password
  to steal here, and no way to recover one from what we store.
- **Account existence is disclosed on purpose.** Sign-in distinguishes "no such
  account" from "account exists but has never logged into the game", because
  that is the difference between "check your spelling" and "log into the game
  once first". The player-facing benefit outweighs the enumeration cost.
- **The game database is mounted read-only** and opened with `mode=ro` and
  `query_only`. The website physically cannot write to it.

## If a secret does leak

Deploy credentials are the only secrets in play, and they live in GitHub Actions
secrets: rotate `DEPLOY_SSH_KEY` and the `DEPLOY_KNOWN_HOSTS` pin, then
re-deploy. Nothing in this repository needs rotating.
