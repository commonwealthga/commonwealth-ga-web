#!/usr/bin/env bash
# Run the API against the local game database, with the player-settings endpoint
# wired up so /settings can actually write.
#
# The site itself is a SvelteKit server now: run `npm run dev` in web/ alongside
# this, and Vite proxies /api here. This script only starts the Rust process.
#
# The settings token is the control server's own admin_token. It is read out of
# the control server's config at start time and never written anywhere: this
# repo is public, and the config it comes from is gitignored for the same
# reason.
#
#   ./scripts/dev.sh                 # http://127.0.0.1:8080
#   ./scripts/dev.sh --stop          # stop a server this script started
#   ./scripts/dev.sh --restart       # stop it, then start again
#   ./scripts/dev.sh --snapshot      # build a snapshot first, read stats from it
#                                    # and enable /db-export/<token>
#   PORT=9000 ./scripts/dev.sh       # somewhere else
#   BIND=0.0.0.0 ./scripts/dev.sh    # reachable from your network
#   SERVER_REPO=~/src/ga-server ./scripts/dev.sh
#
# Everything can also be set by hand instead:
#   WEB_CONTROL_API_URL=http://127.0.0.1:19020 WEB_CONTROL_API_TOKEN=... \
#     WEB_DB_LIVE=... ./target/release/webapi
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
SERVER_REPO="${SERVER_REPO:-$ROOT/../commonwealth-ga-server}"
CONTROL_CONFIG="${CONTROL_CONFIG:-$SERVER_REPO/control-server.json}"
PORT="${PORT:-8080}"
# So `--stop` can find the server without anyone reaching for pgrep. The pid
# survives `exec`: the shell is replaced, not forked.
PIDFILE="${PIDFILE:-/tmp/commonwealth-webapi.$PORT.pid}"
# Loopback by default; set BIND=0.0.0.0 to reach the site from another machine
# (a phone on the same network, say).
BIND="${BIND:-127.0.0.1}"

# Stop a running server: SIGTERM, then give it the same grace the server itself
# allows before insisting.
stop_server() {
  if [[ ! -f "$PIDFILE" ]]; then
    echo "no server recorded for port $PORT ($PIDFILE)" >&2
    return 1
  fi
  local pid
  pid=$(cat "$PIDFILE" 2>/dev/null || true)
  if [[ -z "$pid" ]] || ! kill -0 "$pid" 2>/dev/null; then
    echo "nothing running on port $PORT (stale pidfile removed)" >&2
    rm -f "$PIDFILE"
    return 1
  fi
  echo "stopping pid $pid…" >&2
  kill "$pid" 2>/dev/null || true
  for _ in $(seq 1 60); do
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "stopped" >&2
      rm -f "$PIDFILE"
      return 0
    fi
    sleep 0.1
  done
  echo "still alive after 6s — killing" >&2
  kill -9 "$pid" 2>/dev/null || true
  rm -f "$PIDFILE"
  return 0
}

case "${1:-}" in
  --stop|-s) stop_server; exit $? ;;
  --restart|-r) stop_server || true; shift ;;
  --help|-h)
    sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
esac

# Rebuild when the binary is missing OR older than the sources. A stale binary
# serves the old behaviour and makes a fresh change look broken: that is exactly
# how a missing database column sat undetected behind a build from minutes ago.
if [[ ! -f "$ROOT/target/release/webapi" ]] ||
   [[ -n "$(find "$ROOT/crates" "$ROOT/Cargo.toml" -newer "$ROOT/target/release/webapi" -print -quit 2>/dev/null)" ]]; then
  echo "building webapi…" >&2
  (cd "$ROOT" && cargo build --release)
fi

# Snapshot mode: publish a redacted copy now, point the site at it, and hand out
# a local export token so /db-export is testable. Mirrors what the snapshotter
# service does on a timer in production.
snapshot_mode=0
for arg in "$@"; do
  [[ "$arg" == "--snapshot" ]] && snapshot_mode=1
done

# Fail early and clearly if the port is taken — the raw bind error arrives after
# the snapshot work, which makes it look like the snapshotter failed.
if (command -v ss >/dev/null && ss -ltn 2>/dev/null | grep -q "[:.]$PORT\b"); then
  echo "port $PORT is already in use." >&2
  echo "Stop it:  ./scripts/dev.sh --stop" >&2
  echo "Or move:  PORT=8081 ./scripts/dev.sh$([[ $snapshot_mode -eq 1 ]] && echo ' --snapshot')" >&2
  exit 1
fi

if [[ $snapshot_mode -eq 1 ]]; then
  if [[ ! -x "$ROOT/target/release/snapshotter" ]]; then
    echo "building snapshotter…" >&2
    (cd "$ROOT" && cargo build --release -p snapshotter)
  fi
  export WEB_DB_SNAPSHOT="${WEB_DB_SNAPSHOT:-$ROOT/site-data/snapshot.db}"
  echo "snapshotting $SERVER_REPO/server.db -> $WEB_DB_SNAPSHOT"
  WEB_DB_LIVE="${WEB_DB_LIVE:-$SERVER_REPO/server.db}" \
  WEB_DB_SNAPSHOT="$WEB_DB_SNAPSHOT" \
    "$ROOT/target/release/snapshotter" --once
  # A local token, so the download route exists while testing. Never used in
  # production: there the value comes from a deploy secret.
  export WEB_DB_EXPORT_TOKEN="${WEB_DB_EXPORT_TOKEN:-dev-export-token}"
fi

token=""
if [[ -f "$CONTROL_CONFIG" ]]; then
  token=$(python3 - "$CONTROL_CONFIG" <<'PY'
import json, sys
try:
    print(json.load(open(sys.argv[1])).get("admin_token", ""))
except Exception:
    print("")
PY
)
fi

if [[ -z "$token" ]]; then
  cat >&2 <<'MSG'
No control-server token found, so /settings will render read-only.

To enable it: point CONTROL_CONFIG at the control server's config, or export
WEB_CONTROL_API_TOKEN with the value of its admin_token. The control server
also needs settings_api_port set (19020 here).
MSG
fi

# Same database the local control server writes to, so a change made in the UI
# is read straight back: there is no snapshot in the middle.
export WEB_DB_LIVE="${WEB_DB_LIVE:-$SERVER_REPO/server.db}"
export WEB_DB_SITE="${WEB_DB_SITE:-$ROOT/site-data/site.db}"
export WEB_STATIC_DIR="${WEB_STATIC_DIR:-$ROOT/web/build}"
export WEB_BIND="$BIND:$PORT"
export WEB_CONTROL_API_URL="${WEB_CONTROL_API_URL:-http://127.0.0.1:19020}"
[[ -n "$token" ]] && export WEB_CONTROL_API_TOKEN="${WEB_CONTROL_API_TOKEN:-$token}"

echo "live db : $WEB_DB_LIVE"
echo "site db : $WEB_DB_SITE"
echo "settings: $WEB_CONTROL_API_URL $([[ -n "${WEB_CONTROL_API_TOKEN:-}" ]] && echo '(token set)' || echo '(no token — read-only)')"
echo "api     : http://$BIND:$PORT"
echo "site    : cd web && npm run dev   (proxies /api to the line above)"
if [[ -n "${WEB_DB_SNAPSHOT:-}" ]]; then
  echo "stats   : from $WEB_DB_SNAPSHOT"
fi
if [[ -n "${WEB_DB_EXPORT_TOKEN:-}" ]]; then
  echo "export  : http://$BIND:$PORT/db-export/$WEB_DB_EXPORT_TOKEN"
fi
echo "stop    : ./scripts/dev.sh --stop   (or Ctrl-C twice)"
# Written after every check has passed: a pidfile for a server that never bound
# would send `--stop` after the wrong process.
echo $$ > "$PIDFILE"
exec "$ROOT/target/release/webapi"
