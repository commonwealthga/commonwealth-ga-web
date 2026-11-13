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

# A live-DB fixture: thirty players, newest first. `/agents` pages at 24, so this
# spans two pages and the URL's page number can be checked against the rows.
python3 - "$work/server.db" <<'AGENT_FIXTURE'
import sqlite3, sys

with sqlite3.connect(sys.argv[1]) as db:
    db.executescript(
        """
        CREATE TABLE ga_instances (instance_id INTEGER PRIMARY KEY, state TEXT, queue_id INTEGER, started_at INTEGER);
        CREATE TABLE ga_users (id INTEGER PRIMARY KEY, username TEXT);
        CREATE TABLE ga_match_player_stats (instance_id INTEGER, user_id INTEGER, damage_dealt INTEGER);
        CREATE TABLE ga_instance_players (instance_id INTEGER, user_id INTEGER, left_at INTEGER);
        """
    )
    for index in range(30):
        # Descending start times, so Agent 01 sorts first and Agent 30 last.
        db.execute("INSERT INTO ga_instances VALUES (?, 'STOPPED', 1, ?)", (index + 1, 1_000_000 - index))
        db.execute("INSERT INTO ga_users VALUES (?, ?)", (index + 1, f"Agent {index + 1:02d}"))
        db.execute("INSERT INTO ga_match_player_stats VALUES (?, ?, 100)", (index + 1, index + 1))
AGENT_FIXTURE

echo "== api"
WEB_BIND="127.0.0.1:$API_PORT" \
	WEB_DB_SITE="$work/site.db" \
	WEB_DB_LIVE="$work/server.db" \
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

# Every page needs a card of its own and a description of the site: a link to
# /events used to unfurl with the same picture as a link to /builds, and a page
# said nothing about what the site is.
og_check() {
	local path="$1" want="$2"
	curl -fsS -m 15 "http://127.0.0.1:$SITE_PORT$path" \
		| grep -q "og:image\" content=\"https://commonwealth.ydns.eu/og/$want.jpg?v=[0-9]*\"" \
		|| { echo "FAIL $path does not advertise og/$want.jpg" >&2; exit 1; }
}
for spec in "/=home" "/play=play" "/server=server" "/events=events" "/builds=builds" \
	"/news=news" "/agents=agents" "/missions=missions"; do
	og_check "${spec%%=*}" "${spec##*=}"
done
echo "ok   every section advertises its own Open Graph card"

# `/api/agents` pages from zero, the URL from one. Getting that wrong shipped a
# directory whose first page showed the second page's rows while the paginator
# said "page 1", so the two are compared here rather than assumed.
agents_page() {
	curl -fsS -m 15 "http://127.0.0.1:$SITE_PORT/agents$1" \
		| python3 -c '
import re, sys
html = sys.stdin.read()
rows = re.findall(r"/agents/(\d+)", html)
label = re.search(r"page\s*(?:<!--[^>]*-->)?\s*(\d+)", re.sub(r"<[^>]+>", " ", html))
first = rows[0] if rows else "-"
number = label.group(1) if label else "?"
print(first + "|" + number)
'
}
first=$(agents_page "")
second=$(agents_page "?page=2")
[ "${first%%|*}" = "1" ] && [ "${first##*|}" = "1" ] \
	|| { echo "FAIL /agents page 1 rendered agent ${first%%|*}, labelled ${first##*|}" >&2; exit 1; }
[ "${second%%|*}" = "25" ] && [ "${second##*|}" = "2" ] \
	|| { echo "FAIL /agents?page=2 rendered agent ${second%%|*}, labelled ${second##*|}" >&2; exit 1; }
echo "ok   /agents pages from the page the URL asks for"

home_html="$work/home.html"
curl -fsS -m 15 "http://127.0.0.1:$SITE_PORT/" >"$home_html"
grep -q '"@type":"WebSite"' "$home_html" \
	|| { echo "FAIL the home page has no WebSite node" >&2; exit 1; }
grep -q '"@type":"Organization"' "$home_html" \
	|| { echo "FAIL the home page has no Organization node" >&2; exit 1; }

# The six reasons are the most quotable thing on the site; they are enumerated in
# the graph so a parser can treat them as one list rather than six paragraphs.
python3 - "$home_html" <<'LIST_CHECK' || exit 1
import json, re, sys

html = open(sys.argv[1], encoding="utf-8").read()
data = json.loads(re.search(r'<script type="application/ld\+json">(.*?)</script>', html, re.S).group(1))
lists = [n for n in data.get("@graph", []) if n.get("@type") == "ItemList"]
if not lists:
    raise SystemExit("FAIL the home page has no ItemList")
items = lists[0].get("itemListElement", [])
if len(items) != 6 or lists[0].get("numberOfItems") != 6:
    raise SystemExit(f"FAIL the reasons list has {len(items)} items, expected 6")
for position, item in enumerate(items, start=1):
    if item.get("position") != position or not item.get("name") or not item.get("description"):
        raise SystemExit(f"FAIL item {position} is incomplete: {item}")
    if item["name"] not in html:
        raise SystemExit(f"FAIL {item['name']!r} is in the graph but not on the page")
print("ok   the six reasons are enumerated and match the page")
LIST_CHECK
echo "ok   the site describes itself in structured data"

# The player count has to be in the HTML, not only in the panel that fills in
# after JavaScript: it is the fact a crawler or an assistant will quote.
grep -qE '(players? in-game right now|player count unavailable right now|nobody in-game)' "$home_html" \
	|| { echo "FAIL the home page does not state the player count server-side" >&2; exit 1; }
curl -fsS -m 10 "http://127.0.0.1:$API_PORT/api/players" | grep -q '"players"' \
	|| { echo "FAIL /api/players did not answer with a count" >&2; exit 1; }
echo "ok   the player count is server-rendered"

# The events page is server-rendered with titles, descriptions and — since the
# server now supplies its clock — real instants, so a crawler or a reader without
# JavaScript still learns when each night is.
events="$work/events.html"
curl -fsS -m 15 "http://127.0.0.1:$SITE_PORT/events" >"$events"
for needle in "Friday Raids" "Challenge matches" "Weekly mercenary" "your own timezone"; do
	grep -qF "$needle" "$events" \
		|| { echo "FAIL /events is missing: $needle" >&2; exit 1; }
done
echo "ok   /events renders on the server"

# The schedule has to be machine-readable: an instant per card, and an Event per
# night with the next start and the weekly recurrence.
python3 - "$events" <<'EVENT_CHECK' || exit 1
import json, re, sys

html = open(sys.argv[1], encoding="utf-8").read()
if not re.search(r'<time[^>]+datetime="\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[+-]\d{2}:\d{2}"', html):
    raise SystemExit("FAIL /events has no <time datetime> with an offset")

block = re.search(r'<script type="application/ld\+json">(.*?)</script>', html, re.S)
if not block:
    raise SystemExit("FAIL /events has no JSON-LD")
data = json.loads(block.group(1))
events = [node for node in data.get("@graph", [data]) if node.get("@type") == "Event"]
if len(events) < 3:
    raise SystemExit(f"FAIL /events describes {len(events)} Event nodes, expected 3")
for event in events:
    if not re.match(r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[+-]\d{2}:\d{2}$', event.get("startDate", "")):
        raise SystemExit(f"FAIL {event.get('name')} has no usable startDate: {event.get('startDate')}")
    schedule = event.get("eventSchedule") or {}
    if schedule.get("repeatFrequency") != "P1W" or not schedule.get("byDay"):
        raise SystemExit(f"FAIL {event.get('name')} has no weekly schedule")
print("ok   /events describes three weekly Events with real start dates")
EVENT_CHECK

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
