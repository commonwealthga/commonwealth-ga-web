#!/bin/sh
# Point certificate renewal at the Caddy edge, and make a renewal restart what
# reads the files.
#
# Run this once, with sudo, on the host:
#
#   sudo ~/website/current/scripts/enable-cert-renewal.sh --verify
#
# It is idempotent: running it again changes nothing. WHY it exists:
#
#   * Caddy owns port 80 to serve /var/www/certbot, but certbot's original
#     `standalone` authenticator expects port 80 to itself. Once the edge is
#     listening, a standalone renewal fails — and it fails silently, months
#     later, leaving the certificate to expire.
#   * Caddy and the dashboard read their certificate files once at startup, so a
#     renewal that is not followed by a restart keeps serving the old
#     certificate.
set -eu

CONF=/etc/letsencrypt/renewal/commonwealth.ydns.eu.conf
WEBROOT=/var/www/certbot
HOOK=/usr/local/bin/reload-certs

if [ "$(id -u)" -ne 0 ]; then
	echo "error: run this with sudo" >&2
	exit 1
fi
if [ ! -f "$CONF" ]; then
	echo "error: no renewal config at $CONF" >&2
	exit 1
fi
if ! command -v certbot >/dev/null 2>&1; then
	echo "error: certbot not found" >&2
	exit 1
fi

# 1. The directory Caddy serves ACME challenges from.
install -d -m 755 "$WEBROOT"

# 2. What a renewal has to restart afterwards.
cat >"$HOOK" <<'HOOK_SCRIPT'
#!/bin/sh
# Restart the services that read their certificate files at startup.
# Invoked by certbot as the renewal's deploy hook.
edge=$(docker ps -q \
	--filter label=com.docker.compose.project=commonwealth-ga-web \
	--filter label=com.docker.compose.service=edge | head -1)
[ -n "$edge" ] && docker restart "$edge"
docker restart ga-dashboard >/dev/null 2>&1 || true
HOOK_SCRIPT
chmod 755 "$HOOK"

# 3. Rewrite the renewal config in place, keeping the original.
cp -n "$CONF" "$CONF.bak" 2>/dev/null || true
python3 - "$CONF" "$WEBROOT" "$HOOK" <<'PY'
import pathlib
import sys

conf = pathlib.Path(sys.argv[1])
wanted = {
    "authenticator": "webroot",
    # certbot itself writes this one with a trailing comma.
    "webroot_path": f"{sys.argv[2]},",
    "renew_hook": sys.argv[3],
}

lines = conf.read_text().splitlines()
out, rewritten, i = [], False, 0
while i < len(lines):
    line = lines[i]
    if line.strip() == "[renewalparams]":
        rewritten = True
        out.append(line)
        i += 1
        # Collect the whole section, then re-emit it: wanted keys in the order
        # they already had, the rest untouched. Existing values are replaced
        # rather than duplicated, which is what makes re-running a no-op.
        block = []
        while i < len(lines) and not (
            lines[i].strip().startswith("[") and lines[i].strip().endswith("]")
        ):
            block.append(lines[i])
            i += 1
        found, other = {}, []
        for entry in block:
            text = entry.strip()
            if text and not text.startswith("#") and "=" in text:
                key = text.split("=", 1)[0].strip()
                if key in wanted:
                    found[key] = wanted[key]
                    continue
            other.append(entry)
        for key, value in wanted.items():
            found.setdefault(key, value)
        out.extend(f"{key} = {value}" for key, value in found.items())
        out.extend(other)
        continue
    out.append(line)
    i += 1

if not rewritten:
    sys.exit(f"error: no [renewalparams] section in {conf}")

conf.write_text("\n".join(out) + "\n")
print(f"updated {conf}")
PY

# 4. Show what it now says, and check the edge can answer a challenge.
echo "--- renewal settings:"
grep -nE '^(authenticator|webroot_path|renew_hook)' "$CONF"

edge=$(docker ps -q \
	--filter label=com.docker.compose.project=commonwealth-ga-web \
	--filter label=com.docker.compose.service=edge | head -1)
if [ -z "$edge" ]; then
	echo "warning: the edge container is not running; renewal will fail until it is" >&2
else
	echo "--- edge serving $WEBROOT on port 80"
fi

if [ "${1:-}" = "--verify" ]; then
	echo "--- certbot renew --dry-run"
	certbot renew --dry-run
fi

echo "done. Renewals now go through the webroot, and $HOOK restarts the edge and dashboard afterwards."
