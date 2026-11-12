#!/usr/bin/env bash
#
# shot.sh — screenshot pages of the site, fast, without touching your desktop.
#
# Three things this script knows that I kept rediscovering the hard way:
#
#   1. The browser runs in a container, so it cannot reach a server bound to
#      127.0.0.1. It starts its own server on 0.0.0.0 and talks to it over the
#      docker bridge gateway.
#   2. The screenshot directory must be writable by the uid the container runs
#      as, otherwise chromium fails silently and you get an empty directory.
#   3. Infinite CSS animations (ticker, radar sweep) can stall a headless
#      capture into a blank image. --force-prefers-reduced-motion makes the
#      stylesheet settle instantly, so the frame is deterministic.
#
# Every capture is one chromium process inside a single container, and the file
# size is checked afterwards: a "blank" result is reported, never silently
# accepted.
#
# Usage:
#   scripts/shot.sh                          # default pages
#   scripts/shot.sh home=/@1440,1000         # one page
#   URL=http://127.0.0.1:5173 scripts/shot.sh   # shoot an already-running server
#   WEB_DB_LIVE=/path/to/server.db scripts/shot.sh
#
# Page spec: name=/path@WIDTH,HEIGHT

set -euo pipefail
cd "$(dirname "$0")/.."

IMAGE=${IMAGE:-zenika/alpine-chrome:with-node}
OUT=${OUT:-.shots}
PORT=${PORT:-8091}
MIN_BYTES=${MIN_BYTES:-10000}
SETTLE=${SETTLE:-1200}

PAGES=(
	"home=/@1440,1400"
	"server=/server@1440,1250"
	"matches=/matches@1440,1050"
	"downloads=/downloads@1440,1100"
	"mobile=/@390,1000"
)
[ "$#" -gt 0 ] && PAGES=("$@")

SERVER_PID=""
API_PID=""
cleanup() {
	for pid in "$SERVER_PID" "$API_PID"; do
		if [ -n "$pid" ]; then
			kill "$pid" 2>/dev/null || true
			wait "$pid" 2>/dev/null || true
		fi
	done
}
trap cleanup EXIT

if [ -z "${URL:-}" ]; then
	BIN=./target/release/webapi
	[ -x "$BIN" ] || BIN=./target/debug/webapi
	if [ ! -x "$BIN" ]; then
		echo "shot: no server binary found — run 'cargo build --release' first" >&2
		exit 1
	fi

	# The site is a Node process; screenshot it, with the API behind it on its
	# own port. `WEB_BIND=0.0.0.0` on both so the browser container can reach
	# them through the docker bridge gateway.
	API_BIN="$BIN"
	API_PORT_INTERNAL="${API_PORT:-8080}"
	WEB_BIND="0.0.0.0:$API_PORT_INTERNAL" "$API_BIN" >/tmp/shot-api.log 2>&1 &
	API_PID=$!
	PORT="$PORT" ORIGIN="http://127.0.0.1:$PORT" \
		API_ORIGIN="http://127.0.0.1:$API_PORT_INTERNAL" \
		node web/build/index.js >/tmp/shot-server.log 2>&1 &
	SERVER_PID=$!

	# Wait for the site, not the API: the API's health endpoint is not reachable
	# through the Node server, which serves pages and nothing else.
	for _ in $(seq 1 120); do
		curl -sf -o /dev/null "http://127.0.0.1:$PORT/" && break
		sleep 0.25
	done
	if ! kill -0 "$SERVER_PID" 2>/dev/null; then
		echo "shot: server failed to start — see /tmp/shot-server.log" >&2
		exit 1
	fi

	GATEWAY=$(docker network inspect bridge --format '{{(index .IPAM.Config 0).Gateway}}' 2>/dev/null || true)
	URL="http://${GATEWAY:-172.17.0.1}:$PORT"
	echo "shot: serving the built site at $URL (api on :${API_PORT_INTERNAL:-8080})"
fi

rm -rf "$OUT"
mkdir -p "$OUT"
printf '%s\n' "${PAGES[@]}" >"$OUT/.manifest"

started=$(date +%s)

set +e
docker run --rm -u "$(id -u):$(id -g)" \
	-e SHOT_BASE="$URL" -e SHOT_MIN="$MIN_BYTES" -e SHOT_SETTLE="$SETTLE" \
	-v "$PWD/$OUT:/shots" -v "$PWD/scripts:/scripts:ro" \
	--entrypoint sh "$IMAGE" -c '
	set -u
	cp /scripts/shot.mjs /tmp/shot.mjs
	cd /tmp
	# puppeteer-core is tiny and only needs to drive the chromium already in the
	# image. If npm is unavailable, fall back to the plain CLI capture.
	if ! npm i puppeteer-core@23 --silent --no-audit --no-fund >/dev/null 2>&1; then
		echo "  ! puppeteer unavailable — falling back to cli capture (no settle wait)" >&2
		while IFS= read -r line; do
			[ -n "$line" ] || continue
			name=${line%%=*}; rest=${line#*=}; path=${rest%@*}; size=${rest#*@}
			timeout 20 chromium --no-sandbox --headless=new --disable-gpu \
				--disable-dev-shm-usage --hide-scrollbars --force-prefers-reduced-motion \
				--window-size="$size" --screenshot="/shots/$name.png" "$SHOT_BASE$path" >/dev/null 2>&1 || true
			printf "  ok    %-10s %8s B  %s\n" "$name" "$(stat -c%s "/shots/$name.png" 2>/dev/null || echo 0)" "$path"
		done < /shots/.manifest
		exit 0
	fi
	# shellcheck disable=SC2046  # word splitting is intended: one spec per arg
	node /tmp/shot.mjs $(cat /shots/.manifest)
' 2>&1
status=$?
set -e

echo "shot: ${OUT}/ in $(( $(date +%s) - started ))s"
if [ "$status" -ne 0 ]; then
	echo "shot: some captures came back blank — check $OUT and /tmp/shot-server.log" >&2
	exit 1
fi
