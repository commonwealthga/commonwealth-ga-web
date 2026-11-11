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

## Certificates

The edge reads `/etc/letsencrypt/live/commonwealth.ydns.eu/{fullchain,privkey}.pem`
**at startup**; it does not reload them. Certbot renews via `certbot.timer`.

Renewal must go through the Caddy edge, because the edge owns port 80 and
certbot's original `standalone` authenticator needs that port to itself: once
Caddy is listening, a standalone renewal fails silently and the certificate
expires unwatched.

One-time switch, **after** the edge is serving port 80:

```bash
# The webroot Caddy serves /.well-known/acme-challenge/* from.
sudo install -d -m 755 /var/www/certbot

# Keep a copy, then point renewal at the webroot.
sudo cp /etc/letsencrypt/renewal/commonwealth.ydns.eu.conf{,.bak}
sudo sed -i 's|^authenticator = standalone$|authenticator = webroot\nwebroot_path = /var/www/certbot,|' \
  /etc/letsencrypt/renewal/commonwealth.ydns.eu.conf
sudo grep -nE 'authenticator|webroot_path' /etc/letsencrypt/renewal/commonwealth.ydns.eu.conf

# Restart the edge after a renewal, since Caddy reads the files once.
sudo tee /usr/local/bin/reload-edge >/dev/null <<'SH'
#!/bin/sh
id=$(docker ps -q \
  --filter label=com.docker.compose.project=commonwealth-ga-web \
  --filter label=com.docker.compose.service=edge | head -1)
[ -n "$id" ] && docker restart "$id"
SH
sudo chmod 755 /usr/local/bin/reload-edge
sudo sed -i '/^\[renewalparams\]/a renew_hook = /usr/local/bin/reload-edge' \
  /etc/letsencrypt/renewal/commonwealth.ydns.eu.conf

# Prove it without touching the live certificate.
sudo certbot renew --dry-run
```

`--dry-run` exercises the webroot path through Caddy. It fails if the edge is
down or its `http://:80` block is missing, which is the point.

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
