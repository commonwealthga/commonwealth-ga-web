# Runbook

Operational sequences for the live server. Written against the box as it is
today: `gaserver`, site under `/home/zax`, game server under `/home/zax/games`,
deploy over SSH as `zax` on port 554 with the key stored as `DEPLOY_SSH_KEY`.

## Layout

| path | holds |
|---|---|
| `/home/zax/website/releases/<stamp>-<sha>/` | one directory per release: compose files, rendered override, `.env` pinning the image tag |
| `/home/zax/website/current` | symlink to the live release; flipped atomically |
| `/home/zax/website/site-data/` | `site.db` and `snapshot.db`, written by the containers |
| `/home/zax/public/files/` | `/files/*` downloads for the launcher, ~560 MB, read per request |
| `/home/zax/games/` | the game server and its database, mounted read-only at `/data` |
| `/home/zax/website-old-aug/` | the retired August layout, kept only as an archive; safe to delete |

Every service that mounts host data needs a section in
`deploy/docker-compose.override.template.yml`. The base file's defaults
(`${WEB_DB_HOST_DIR:-.}`, `${WEB_SITE_DATA_DIR:-./site-data}`) resolve against
the release directory, so a service without an override quietly mounts the
release itself — that is how the snapshotter came up with `<release>/server.db`
as its "live" database and published nothing.

The pre-Rust site still runs as compose project `public` (container
`ga-public`) from `/home/zax/public`, and owns port 443 until the cutover.

## Deploy

Actions → Deploy → Run workflow. Inputs:

- `deploy` — a git ref, default `master`; or `previous` / `previous-N` to roll back.
- `start` — `true` (default) uploads and brings the release up. `false` uploads
  the release and its rendered compose file, creates `site-data/`, and stops
  **without** flipping `current` or starting anything.

A normal deploy keeps the last 8 releases plus whatever `current` points at.

### What it touches, and what it does not

The image is built and pushed to GHCR in CI; the VPS only pulls. `docker compose
up -d` recreates only the containers whose image or config changed, so an
ordinary code deploy restarts `web` and leaves Caddy serving — a blip of a second
or two, during which the edge answers 502. The health check polls
`https://127.0.0.1/api/status` for 60 s and fails the run if it never comes back.

The snapshotter runs on the same image and **snapshots on startup**, before its
first interval, so a fresh `site-data/` fills in immediately rather than after
five minutes. Statistics and `/db-export` read that file, never the live DB.

## Cutover from the Python site

Do this once. It is the only sequence here that causes real downtime, and it is
seconds long.

```bash
# 1. Prepare without starting anything (start = false), then check the release:
ls -la ~/website/releases/*/ ~/website/site-data
grep -nE 'games|site-data|srv/files|letsencrypt' ~/website/releases/*/docker-compose.override.yml
#    Expect /home/zax/games, /home/zax/website/site-data, /home/zax/public/files.

# 2. Stop the old site. This frees port 443 (and 80).
cd ~/public && docker compose -p public down

# 3. Deploy with start = true. It flips current and brings up web + snapshotter + edge.

# 4. Verify:
curl -sk https://127.0.0.1/api/status                  # "N players in-game"
curl -skI https://127.0.0.1/ | head -3                 # 200
curl -skI https://127.0.0.1/files/surfside-atoll.zip   # 200, large Content-Length
docker compose -p commonwealth-ga-web ps
```

### Rollback

```bash
# New site down, old site back:
docker compose -p commonwealth-ga-web down
cd ~/public && docker compose -p public up -d
```

Or, staying on the new stack, deploy with `deploy=previous` — a symlink flip plus
a restart, no build.

## The image registry

The package `ghcr.io/commonwealthga/commonwealth-ga-web` is **private** — it holds
the compiled binaries, and keeping it private avoids handing out a ready-to-run
copy of the site. GitHub creates organisation packages private by default
anyway, whatever the repository's visibility.

Pulling it therefore needs a credential, and the deploy supplies one without
storing anything:

- the deploy job declares `permissions: packages: read`, so its own
  `GITHUB_TOKEN` can read the package;
- it copies that token to the VPS through the SSH stdin script (never argv, so
  it does not show in `ps`), and logs in with `--config` pointed at a `mktemp -d`
  directory that a `trap` deletes on exit.

The result: `/home/zax/.docker/config.json` is never written, and the credential
expires with the workflow run — there is no registry secret to rotate and nothing
to revoke after a leak beyond the run itself.

If a pull ever fails with `unauthorized`, the token could not read the package:
check that the job still has `packages: read`, and that the package is linked to
this repository (Packages → *commonwealth-ga-web* → settings → *Manage access*).

`start = false` deploys now also `docker compose pull` without starting anything,
which proves the credential works while the currently live site keeps its ports.

## Certificates

The edge reads `/etc/letsencrypt/live/commonwealth.ydns.eu/{fullchain,privkey}.pem`
**at startup**; it does not reload them. Certbot renews via `certbot.timer`.

Renewal must go through the Caddy edge, because the edge owns port 80 and
certbot's original `standalone` authenticator needs that port to itself: once
Caddy is listening, a standalone renewal fails silently and the certificate
expires unwatched.

One-time switch, **after** the edge is serving port 80. It is a script, so it is
one command and repeatable rather than a pasted block:

```bash
sudo ~/website/current/scripts/enable-cert-renewal.sh --verify
```

It creates the webroot, writes the reload hook, rewrites the renewal config in
place (keeping `$CONF.bak`), prints the result, and with `--verify` runs
`certbot renew --dry-run`. Running it twice changes nothing.

What it does, for reference:

```bash
# The webroot Caddy serves /.well-known/acme-challenge/* from.
sudo install -d -m 755 /var/www/certbot

# Keep a copy, then point renewal at the webroot.
sudo cp /etc/letsencrypt/renewal/commonwealth.ydns.eu.conf{,.bak}
sudo sed -i 's|^authenticator = standalone$|authenticator = webroot\nwebroot_path = /var/www/certbot,|' \
  /etc/letsencrypt/renewal/commonwealth.ydns.eu.conf
sudo grep -nE 'authenticator|webroot_path' /etc/letsencrypt/renewal/commonwealth.ydns.eu.conf

# Restart whatever reads the certificates after a renewal. Both the Caddy edge
# and the dashboard load them at startup and never re-read the files.
sudo tee /usr/local/bin/reload-certs >/dev/null <<'SH'
#!/bin/sh
edge=$(docker ps -q \
  --filter label=com.docker.compose.project=commonwealth-ga-web \
  --filter label=com.docker.compose.service=edge | head -1)
[ -n "$edge" ] && docker restart "$edge"
docker restart ga-dashboard >/dev/null 2>&1 || true
SH
sudo chmod 755 /usr/local/bin/reload-certs
sudo sed -i '/^\[renewalparams\]/a renew_hook = /usr/local/bin/reload-certs' \
  /etc/letsencrypt/renewal/commonwealth.ydns.eu.conf

# Prove it without touching the live certificate.
sudo certbot renew --dry-run
```

### Every HTTPS service on this host needs a real certificate

The site sends `Strict-Transport-Security: max-age=31536000; includeSubDomains`.
HSTS is per **host**, not per port, so it covers every service on
`commonwealth.ydns.eu` — including ones on other ports. A self-signed
certificate on any of them becomes a hard browser error with **no** "accept the
risk" option, which is what the dashboard on `:8443` ran into: its entrypoint
generated one when no certificate was configured.

So the dashboard mounts `/etc/letsencrypt` read-only and takes
`DASHBOARD_TLS_CERT` / `DASHBOARD_TLS_KEY` from the deployment's own lineage,
and `reload-certs` restarts it after a renewal along with the edge. Anything else
added here must do the same, or be behind Caddy.

`--dry-run` exercises the webroot path through Caddy. It fails if the edge is
down or its `http://:80` block is missing, which is the point.

## Moderating news

Grant the role first, in the dashboard (users page → Roles) or directly:

```bash
# READ-ONLY check of who has it
docker exec commonwealth-ga-web-web-1 true   # (no shell in the image; use the host)
sqlite3 ~/games/server.db "SELECT u.username, r.role FROM ga_user_roles r
  JOIN ga_users u ON u.id = r.user_id ORDER BY r.role;"
```

Then the moderator works on `https://commonwealth.ydns.eu/news` itself: the queue
is the first section on the feed, approve is one click, reject demands a note.
Every decision is written to `news_audit`. Authors see their own pending posts in
the same feed, marked "Waiting for review".

**Reading a rejected post** — the content is kept on purpose:

```bash
sqlite3 ~/website/site-data/site.db \
  "SELECT id, title, author_name, reviewed_by_name, moderation_note, created_at
     FROM news_posts WHERE status = 'rejected' ORDER BY reviewed_at DESC LIMIT 20;"

# The full trail for one post, and its body
sqlite3 ~/website/site-data/site.db \
  "SELECT action, actor_name, note, datetime(created_at,'unixepoch') FROM news_audit
     WHERE post_id = 42 ORDER BY created_at;"
sqlite3 ~/website/site-data/site.db "SELECT body_markdown FROM news_posts WHERE id = 42;"
```

**Closing posting** (bots, a flood): set the repository variable
`WEB_NEWS_POSTING_MODE=allowlisted` and run the deploy. Only `news_allowlisted`
accounts may submit afterwards; existing posts are unaffected. Set it back to
`open` and deploy to reopen. To stop one account instead, grant it
`news_restricted` in the dashboard — no deploy needed.

## The dashboard

`~/dashboard` runs compose project `dashboard` (container `ga-dashboard`, port
8443) and is **not** a plain `docker compose up` project: its base file is
`docker-compose.host.yml`, because it needs `network_mode: host` to reach the
control server's IPC port on `127.0.0.1:8999` and the settings API on `:19020`.

Always name both files, or it silently comes up on a bridge network where
`127.0.0.1` is the container itself and every control-server call fails with
`Connection refused`:

```bash
cd ~/dashboard && docker compose -p dashboard -f docker-compose.host.yml \
  -f docker-compose.override.yml --project-directory ~/dashboard up -d
docker inspect ga-dashboard --format '{{.HostConfig.NetworkMode}}'   # must say: host
```

Its certificate is the Let's Encrypt lineage (`DASHBOARD_TLS_CERT`/`_KEY` in the
override, `/etc/letsencrypt` mounted read-only), because HSTS on the site covers
the whole host — see above.

## fail2ban

`/etc/fail2ban/jail.local` on the box is stricter than Debian's defaults:

```ini
[DEFAULT]
ignoreip = 127.0.0.1/8 ::1
bantime  = 1h
findtime = 10m
maxretry = 4
bantime.increment = true
bantime.maxtime   = 1w

[sshd]
enabled = true
port    = 554
mode    = aggressive
backend = systemd
```

`mode = aggressive` with `maxretry = 4` and incrementing bans (up to a week) is
what a burst of SSH connections runs into — a deploy makes about 14, and a
handful of back-to-back `ssh` commands is enough. The symptom is
`Connection refused`, which looks like the host is down while the site keeps
serving normally.

Relax it for `sshd` by adding the key **inside the existing `[sshd]` section**.
A second `[sshd]` block is a parse error: `systemctl reload fail2ban` exits 255,
the daemon keeps running its old config, and nothing tells you beyond that exit
code.

```bash
sudo cp /etc/fail2ban/jail.local{,.bak}
sudoedit /etc/fail2ban/jail.local        # add  maxretry = 20  under [sshd]
sudo fail2ban-client -t                  # parse test — before reloading
sudo systemctl reload fail2ban
sudo fail2ban-client status sshd
```

Lifting a ban:

```bash
sudo fail2ban-client status sshd                 # see banned IPs
sudo fail2ban-client set sshd unbanip <ip>
```

Bans increment (`bantime.increment`), so a repeat offender waits longer than an
hour and up to `bantime.maxtime` — worth unbanning your own address rather than
waiting it out.
