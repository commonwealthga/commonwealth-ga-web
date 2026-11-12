//! `/sitemap.xml`, generated rather than stored.
//!
//! The static pages are a constant here; the guides and builds come from the
//! database, because they are written after any build could run. That means the document has to be
//! correct by construction — Google answers an unreadable sitemap with
//! "Couldn't fetch" and nothing else, which is exactly the hole the hand-written
//! file fell into (a mistyped `xmlns`).
//!
//! The namespace is therefore a constant with a test, and every static path is
//! checked against the Svelte source tree so the sitemap cannot advertise a page
//! that does not exist.

/// The sitemaps.org namespace. Plural: `sitemaps`, not `sitemap`.
pub const NAMESPACE: &str = "http://www.sitemaps.org/schemas/sitemap/0.9";

/// Pages that exist as prerendered HTML, with their crawl hint. Agent and
/// mission detail pages are client-rendered and omitted on purpose; app pages
/// (`/me`, `/settings`, `/login`) and the moderation tools are not content.
/// Paths that exist as their own pages but are not static entries here: `/news`
/// because its posts are anchors on one page, `/guides` and `/builds` because
/// their entries are listed below individually.
pub const SERVER_RENDERED: [&str; 3] = ["/news", "/guides", "/builds"];

pub const STATIC_PAGES: [(&str, &str, &str); 12] = [
    ("/", "1.0", "daily"),
    ("/play", "0.9", "weekly"),
    ("/server", "0.9", "daily"),
    ("/missions", "0.8", "daily"),
    ("/guides", "0.8", "daily"),
    ("/builds", "0.8", "daily"),
    ("/news", "0.8", "daily"),
    ("/events", "0.7", "weekly"),
    ("/community", "0.7", "weekly"),
    ("/roadmap", "0.6", "weekly"),
    ("/agents", "0.6", "daily"),
    ("/leaderboard", "0.5", "daily"),
];

/// One entry page: its path and when it was published.
pub struct EntryRef<'a> {
    pub path: &'a str,
    pub updated: i64,
}

pub fn render(base: &str, entries: &[EntryRef<'_>]) -> String {
    let base = base.trim_end_matches('/');
    let mut out = String::with_capacity(1024);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<urlset xmlns=\"");
    out.push_str(NAMESPACE);
    out.push_str("\">\n");

    for (path, priority, changefreq) in STATIC_PAGES {
        out.push_str("  <url><loc>");
        out.push_str(&escape(&format!("{base}{path}")));
        out.push_str("</loc><changefreq>");
        out.push_str(changefreq);
        out.push_str("</changefreq><priority>");
        out.push_str(priority);
        out.push_str("</priority></url>\n");
    }

    // Entry pages: the guides and builds that are published right now. A page
    // that changes when a moderator approves something is exactly what a sitemap
    // is for.
    for entry in entries {
        out.push_str("  <url><loc>");
        out.push_str(&escape(&format!("{base}{}", entry.path)));
        out.push_str("</loc><lastmod>");
        out.push_str(&date(entry.updated));
        out.push_str("</lastmod><changefreq>monthly</changefreq><priority>0.6</priority></url>\n");
    }

    out.push_str("</urlset>\n");
    out
}

/// `YYYY-MM-DD` in UTC — the only date format a sitemap accepts.
fn date(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    // Civil-from-days, the same arithmetic the rest of the crate uses for clocks.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_namespace_is_the_one_crawlers_validate_against() {
        // The bug this guards: "sitemap.org" makes the document XML that is not
        // a sitemap, and Search Console says only "Couldn't fetch".
        assert_eq!(
            NAMESPACE, "http://www.sitemaps.org/schemas/sitemap/0.9",
            "the schema host is sitemaps.org"
        );
        let xml = render("https://example.test", &[]);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(
            xml.contains(&format!("<urlset xmlns=\"{NAMESPACE}\">")),
            "{xml}"
        );
        assert!(xml.trim_end().ends_with("</urlset>"));
    }

    #[test]
    fn every_static_page_exists_in_the_svelte_source_tree() {
        let routes = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/src/routes");
        for (path, _, _) in STATIC_PAGES {
            if SERVER_RENDERED.contains(&path) {
                continue;
            }
            let candidate = if path == "/" {
                routes.join("+page.svelte")
            } else {
                routes
                    .join(path.trim_start_matches('/'))
                    .join("+page.svelte")
            };
            assert!(
                candidate.exists(),
                "the sitemap lists {path} but {} does not exist",
                candidate.display()
            );
        }
    }

    #[test]
    fn posts_are_not_listed_because_they_have_no_page_of_their_own() {
        // Entries are shown in full on /news and a post URL redirects there.
        // Listing those redirects would spend crawl budget on nothing.
        let xml = render("https://example.test", &[]);
        assert!(!xml.contains("/news/"), "{xml}");
        assert!(
            xml.contains("<loc>https://example.test/news</loc>"),
            "{xml}"
        );
    }

    #[test]
    fn the_url_count_matches_the_page_list() {
        let xml = render("https://example.test", &[]);
        assert_eq!(xml.matches("<url>").count(), STATIC_PAGES.len());
        let mut seen = std::collections::HashSet::new();
        for chunk in xml.split("<loc>").skip(1) {
            let loc = chunk.split("</loc>").next().unwrap_or_default();
            assert!(seen.insert(loc.to_string()), "{loc} is listed twice");
        }
    }

    #[test]
    fn a_trailing_slash_on_the_base_url_does_not_double_up() {
        let xml = render("https://example.test/", &[]);
        assert!(!xml.contains("example.test//"), "{xml}");
        assert!(xml.contains("<loc>https://example.test/</loc>"), "{xml}");
    }
}
