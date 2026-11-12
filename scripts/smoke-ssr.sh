#!/usr/bin/env bash
# Prove the site is server-rendered.
#
# Starts the API against a throwaway database, seeds one approved entry, starts
# the built Node server, and asserts that the entry's text is in the HTML `/news`
# returns. That is the property the whole architecture exists for: a crawler that
# never runs JavaScript still reads the news.
#
#   ./scripts/smoke-ssr.sh
#
# Expects `cargo build -p webapi` and `npm run build` in web/ to have run.
set -euo pipefail
cd "$(dirname "$0")/.."

API_PORT="${API_PORT:-8099}"
SITE_PORT="${SITE_PORT:-3099}"
API_BIN="${API_BIN:-target/debug/webapi}"
TITLE="Smoke test headline $RANDOM"
MARKER="Server-rendered body for the smoke test"

work="$(mktemp -d)"
api_pid=""
site_pid=""
cleanup() {
	[ -n "$site_pid" ] && kill "$site_pid" 2>/dev/null || true
	[ -n "$api_pid" ] && kill "$api_pid" 2>/dev/null || true
	rm -rf "$work"
}
trap cleanup EXIT

wait_for() { # url, what
	for _ in $(seq 1 60); do
		# Not `curl -f`: a degraded API answers 503 on some routes by design, and
		# the smoke test has no game database to give it.
		code="$(curl -sS -m 2 -o /dev/null -w '%{http_code}' "$1" 2>/dev/null || true)"
		if [ "${code:-000}" = "200" ]; then return 0; fi
		sleep 0.5
	done
	echo "FAIL: $2 never answered at $1" >&2
	return 1
}

# Fail loudly if something is already on our ports: the script would otherwise
# test whatever that process is serving and report on the wrong stack.
for port in "$API_PORT" "$SITE_PORT"; do
	if (command -v ss >/dev/null && ss -ltn 2>/dev/null | grep -q "[:.]$port\b"); then
		echo "FAIL port $port is already in use — stop it, or pick another (API_PORT=, SITE_PORT=)" >&2
		exit 1
	fi
done

echo "== api"
WEB_BIND="127.0.0.1:$API_PORT" \
	WEB_DB_SITE="$work/site.db" \
	"$API_BIN" >"$work/api.log" 2>&1 &
api_pid=$!
wait_for "http://127.0.0.1:$API_PORT/api/news?limit=1" "the API"

echo "== seed one approved entry"
python3 - "$work/site.db" "$TITLE" "$MARKER" <<'PY'
import sqlite3, sys, time

path, title, marker = sys.argv[1], sys.argv[2], sys.argv[3]
with sqlite3.connect(path) as db:
    db.execute(
        """INSERT INTO news_posts
           (kind, title, slug, body_markdown, body_html, status,
            author_id, author_name, created_at)
           VALUES ('news', ?, 'smoke-test-headline', ?, ?, 'approved', 1, 'smoketester', ?)""",
        (title, marker, f"<p>{marker}</p>", int(time.time())),
    )
PY

echo "== site"
PORT="$SITE_PORT" \
	ORIGIN="http://127.0.0.1:$SITE_PORT" \
	API_ORIGIN="http://127.0.0.1:$API_PORT" \
	node web/build/index.js >"$work/site.log" 2>&1 &
site_pid=$!
wait_for "http://127.0.0.1:$SITE_PORT/" "the site"

echo "== the HTML that leaves the server"
html="$work/news.html"
curl -fsS -m 15 "http://127.0.0.1:$SITE_PORT/news" >"$html"

fail=0
check() { # needle, why
	if grep -qF "$1" "$html"; then
		echo "ok   $2"
	else
		echo "FAIL $2" >&2
		fail=1
	fi
}
check 'id="post-' "the feed rendered a post element"
check "$TITLE" "the seeded entry's title is in the response body"
check "$MARKER" "the seeded entry's body is in the response body"
check '<link rel="canonical"' "the feed carries a canonical link"

if [ "$fail" -ne 0 ]; then
	echo "--- site log" >&2
	tail -20 "$work/site.log" >&2
	exit 1
fi

# Structured data is read by machines that cannot guess: a block that arrived as
# template source, or as HTML-escaped entities, is a Search Console error rather
# than a rich result. Parse it here rather than trusting that it looks right.
home="$work/home.html"
curl -fsS -m 15 "http://127.0.0.1:$SITE_PORT/" >"$home"
python3 - "$home" <<'JSON_CHECK' || exit 1
import json, re, sys

html = open(sys.argv[1], encoding="utf-8").read()
blocks = re.findall(r'<script type="application/ld\+json">(.*?)</script>', html, re.S)
if not blocks:
    raise SystemExit("FAIL no application/ld+json block on the home page")
for block in blocks:
    try:
        data = json.loads(block)
    except json.JSONDecodeError as error:
        raise SystemExit(f"FAIL the JSON-LD block does not parse: {error}")
    if data.get("@context") != "https://schema.org":
        raise SystemExit("FAIL the JSON-LD block is not schema.org")
print("ok   the JSON-LD block parses")
JSON_CHECK

curl -fsS -m 5 "http://127.0.0.1:$API_PORT/sitemap.xml" | grep -q "<urlset" \
	|| { echo "FAIL the sitemap did not render" >&2; exit 1; }
echo "ok   the sitemap renders"

# The guides section is server-rendered too: an empty database still has to
# produce its page, and the section's entry in the nav has to point somewhere.
guides="$work/guides.html"
curl -fsS -m 15 "http://127.0.0.1:$SITE_PORT/guides" >"$guides"
for needle in "Guides" "Nothing here yet" "/builds"; do
	grep -qF "$needle" "$guides" \
		|| { echo "FAIL /guides is missing: $needle" >&2; exit 1; }
done
echo "ok   /guides renders on the server"

# A filtered feed is a link: it has to render the filter and respect it.
filtered="$work/filtered.html"
curl -fsS -m 15 "http://127.0.0.1:$SITE_PORT/news?kind=update" >"$filtered"
grep -qF "Server updates" "$filtered" \
	|| { echo "FAIL the kind filter is missing from the feed" >&2; exit 1; }
if grep -qF "$TITLE" "$filtered"; then
	echo "FAIL a news entry appeared under the server-updates filter" >&2
	exit 1
fi
echo "ok   the feed filters by kind"

echo "all checks passed"
