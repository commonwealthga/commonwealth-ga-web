//! `/sitemap.xml`, generated rather than stored.
//!
//! The static pages are a constant here; the news posts come from the database,
//! because they are written after the build. That means the document has to be
//! correct by construction — Google answers an unreadable sitemap with
//! "Couldn't fetch" and nothing else, which is exactly the hole the hand-written
//! file fell into (a mistyped `xmlns`).
//!
//! The namespace is therefore a constant with a test, and every static path is
//! checked against the Svelte source tree so the sitemap cannot advertise a page
//! that does not exist.

use crate::news::pages::iso_date;
use crate::news::store::Post;

/// The sitemaps.org namespace. Plural: `sitemaps`, not `sitemap`.
pub const NAMESPACE: &str = "http://www.sitemaps.org/schemas/sitemap/0.9";

/// Pages that exist as prerendered HTML, with their crawl hint. Agent and
/// mission detail pages are client-rendered and omitted on purpose; app pages
/// (`/me`, `/settings`, `/login`) and the moderation tools are not content.
/// Paths this server renders itself rather than serving from the frontend
/// build, so the source-tree check below must skip them.
pub const SERVER_RENDERED: [&str; 1] = ["/news"];

pub const STATIC_PAGES: [(&str, &str, &str); 10] = [
    ("/", "1.0", "daily"),
    ("/play", "0.9", "weekly"),
    ("/server", "0.9", "daily"),
    ("/missions", "0.8", "daily"),
    ("/news", "0.8", "daily"),
    ("/events", "0.7", "weekly"),
    ("/community", "0.7", "weekly"),
    ("/roadmap", "0.6", "weekly"),
    ("/agents", "0.6", "daily"),
    ("/leaderboard", "0.5", "daily"),
];

pub fn render(base: &str, posts: &[Post]) -> String {
    let base = base.trim_end_matches('/');
    let mut out = String::with_capacity(1024 + posts.len() * 200);
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

    for post in posts {
        out.push_str("  <url><loc>");
        out.push_str(&escape(&format!("{base}{}", post.path())));
        out.push_str("</loc><lastmod>");
        // The date the post became visible, not the date it was submitted: to a
        // crawler this is when the page changed.
        out.push_str(&iso_date(post.reviewed_at.unwrap_or(post.created_at)));
        out.push_str("</lastmod></url>\n");
    }

    out.push_str("</urlset>\n");
    out
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
    use crate::news::{Kind, Status};
    use std::path::Path;

    fn post(id: i64, slug: &str, created_at: i64, reviewed_at: i64) -> Post {
        Post {
            id,
            kind: Kind::News,
            title: "A post".to_string(),
            slug: slug.to_string(),
            summary: String::new(),
            body_markdown: "body".to_string(),
            body_html: "<p>body</p>".to_string(),
            link: None,
            video_id: None,
            status: Status::Approved,
            author_id: 7,
            author_name: "Agent07".to_string(),
            created_at,
            reviewed_by_name: None,
            reviewed_at: Some(reviewed_at),
            moderation_note: None,
            thumbs_up: 0,
        }
    }

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
                "{} lists {path} but {} does not exist",
                "sitemap",
                candidate.display()
            );
        }
    }

    #[test]
    fn posts_appear_with_their_review_date() {
        let xml = render(
            "https://example.test",
            &[post(3, "first", 1_764_000_000, 1_764_086_400)],
        );
        assert!(
            xml.contains("<loc>https://example.test/news/3-first</loc>"),
            "{xml}"
        );
        // lastmod is when it went live, not when it was written.
        assert!(xml.contains("<lastmod>2025-11-"), "{xml}");
        assert!(!xml.contains("<loc>https://example.test/news/3-first/</loc>"));
    }

    #[test]
    fn the_url_count_matches_what_was_offered() {
        let posts = [post(1, "a", 0, 0), post(2, "b", 0, 0)];
        let xml = render("https://example.test", &posts);
        let urls = xml.matches("<url>").count();
        assert_eq!(urls, STATIC_PAGES.len() + 2);
        // No URL may be listed twice: a duplicate is a crawl-budget leak.
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
