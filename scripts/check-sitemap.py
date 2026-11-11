#!/usr/bin/env python3
"""Check the sitemap before it reaches a crawler.

Google Search Console only says "Couldn't fetch" when a sitemap is rejected, which
cost half an hour to trace back to a mistyped namespace (`sitemap.org` for
`sitemaps.org`): the file was well-formed XML, served with the right content type
and a 200, and no crawler could read it. Nothing in the build noticed, because
nothing was looking.

Checks, all offline:
  * the root element is in the sitemaps.org namespace,
  * every <loc> is https on the expected host, absolute, and unique,
  * every <loc> maps to a page that actually exists in the source tree,
  * robots.txt advertises the sitemap.

Exit code 1 with a readable message on the first problem.
"""

from __future__ import annotations

import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parent.parent
SITEMAP = ROOT / "web" / "static" / "sitemap.xml"
ROBOTS = ROOT / "web" / "static" / "robots.txt"
ROUTES = ROOT / "web" / "src" / "routes"
HOST = "commonwealth.ydns.eu"
NAMESPACE = "http://www.sitemaps.org/schemas/sitemap/0.9"


def fail(message: str) -> None:
    print(f"sitemap: {message}", file=sys.stderr)
    raise SystemExit(1)


def route_file(path: str) -> Path:
    """The prerendered page that answers `path`, per SvelteKit's file layout."""
    if path in ("", "/"):
        return ROUTES / "+page.svelte"
    return ROUTES / path.strip("/") / "+page.svelte"


def main() -> None:
    if not SITEMAP.exists():
        fail(f"no sitemap at {SITEMAP}")

    try:
        tree = ET.parse(SITEMAP)
    except ET.ParseError as exc:
        fail(f"{SITEMAP.name} is not well-formed XML: {exc}")

    root = tree.getroot()
    if root.tag != f"{{{NAMESPACE}}}urlset":
        fail(
            f"root element is {root.tag!r}, expected {{{NAMESPACE}}}urlset — "
            "a wrong or missing xmlns makes this XML that is not a sitemap, "
            "which crawlers report as an unreadable sitemap"
        )

    urls = root.findall(f"{{{NAMESPACE}}}url")
    if not urls:
        fail("no <url> entries")

    seen: set[str] = set()
    for entry in urls:
        loc = entry.find(f"{{{NAMESPACE}}}loc")
        if loc is None or not (loc.text or "").strip():
            fail("<url> without a <loc>")
        value = loc.text.strip()
        if value in seen:
            fail(f"duplicate <loc>: {value}")
        seen.add(value)

        parts = urlsplit(value)
        if parts.scheme != "https":
            fail(f"{value} is not https")
        if parts.netloc != HOST:
            fail(f"{value} points at {parts.netloc}, expected {HOST}")
        if parts.query or parts.fragment:
            fail(f"{value} has a query or fragment; crawlers ignore both")

        page = route_file(parts.path)
        if not page.exists():
            fail(f"{value} has no page at {page.relative_to(ROOT)}")

        for tag in ("changefreq", "priority"):
            if entry.find(f"{{{NAMESPACE}}}{tag}") is None:
                fail(f"{value} is missing <{tag}>")

    advertised = f"Sitemap: https://{HOST}/sitemap.xml"
    if advertised not in ROBOTS.read_text():
        fail(f"robots.txt does not advertise {advertised!r}")

    print(f"sitemap ok: {len(urls)} urls, namespace {NAMESPACE}")
    for value in sorted(seen):
        print(f"  {value}")


if __name__ == "__main__":
    main()
