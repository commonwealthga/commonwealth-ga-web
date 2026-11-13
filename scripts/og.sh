#!/usr/bin/env bash
#
# og.sh — regenerate the Open Graph cards in web/static/og/.
#
# Every page used to unfurl with the same og.jpg, so a /builds link looked exactly
# like a /news link. These are rendered from HTML + the site's own fonts and
# palette, which is only trustworthy if the last line of this script is "open one
# and look at it".
#
# Usage:
#   scripts/og.sh            # regenerate everything
#   ./scripts/og.sh          # same
#
# The browser runs in a container (the same image shot.sh uses), so the repo is
# mounted and the output is written into web/static/og/.

set -euo pipefail
cd "$(dirname "$0")/.."

IMAGE=${IMAGE:-zenika/alpine-chrome:with-node}
OUT=${OUT:-web/static/og}

mkdir -p "$OUT"
# Root-owned files inside the container would need sudo to replace, so the
# renderer runs as the invoking user.
docker run --rm -u "$(id -u):$(id -g)" \
	-e OG_OUT=/out -e OG_ROOT=/work \
	-v "$PWD:/work:ro" -v "$PWD/$OUT:/out" \
	-v "$PWD/scripts:/scripts:ro" \
	--entrypoint sh "$IMAGE" -c '
	set -u
	cp /scripts/og-cards.mjs /tmp/og-cards.mjs
	cd /tmp
	if ! npm i puppeteer-core@23 --silent --no-audit --no-fund >/dev/null 2>&1; then
		echo "og: puppeteer unavailable — cannot render the cards" >&2
		exit 1
	fi
	node /tmp/og-cards.mjs
'

echo "og: wrote $OUT/ ($(ls -1 "$OUT"/*.jpg | wc -l) cards)"
