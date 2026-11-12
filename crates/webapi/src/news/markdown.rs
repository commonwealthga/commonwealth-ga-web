//! Markdown → HTML for news posts.
//!
//! Posts are written by anyone with an account, so the parser's output is never
//! trusted as HTML: this renderer walks pulldown-cmark's events and emits a
//! fixed set of tags it wrote itself. Nothing from the input reaches the output
//! except as escaped text or as a validated attribute value, which is what makes
//! `body_html` safe to store and serve without a sanitiser pass afterwards.
//!
//! The subset is deliberately small — it is what the editor's toolbar offers:
//! paragraphs, `##`/`###` headings (the page owns `<h1>`), bold, italic, links,
//! bullet and numbered lists, blockquotes, inline code and fenced code. Raw HTML
//! in a post is escaped and shown as text rather than dropped, so an author who
//! pastes `<b>` sees why it did not work. Images are not rendered at all: there
//! are no uploads, and hotlinked images are a tracking surface with no
//! moderation story.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// Opening and closing pairs this renderer is willing to emit.
enum Open {
    /// A tag we emit ourselves; the string is what closes it.
    Tag(&'static str),
    /// A start we deliberately render as nothing (unsupported construct).
    Nothing,
}

pub fn to_html(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len() + markdown.len() / 4);
    let mut stack: Vec<Open> = Vec::new();
    // While inside `![alt](src)` everything is discarded, including the
    // emphasis a caption might contain.
    let mut image_depth = 0usize;

    for event in Parser::new_ext(markdown, Options::empty()) {
        if image_depth > 0 {
            match event {
                Event::Start(Tag::Image { .. }) => image_depth += 1,
                Event::End(TagEnd::Image) => image_depth -= 1,
                _ => {}
            }
            continue;
        }

        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    out.push_str("<p>");
                    stack.push(Open::Tag("</p>"));
                }
                Tag::Heading { level, .. } => {
                    // `h1` belongs to the page title; nothing in a post body may
                    // compete with it for the outline.
                    let (open, close) = match level as u8 {
                        1 | 2 => ("<h2>", "</h2>"),
                        _ => ("<h3>", "</h3>"),
                    };
                    out.push_str(open);
                    stack.push(Open::Tag(close));
                }
                Tag::Strong => {
                    out.push_str("<strong>");
                    stack.push(Open::Tag("</strong>"));
                }
                Tag::Emphasis => {
                    out.push_str("<em>");
                    stack.push(Open::Tag("</em>"));
                }
                Tag::BlockQuote(_) => {
                    out.push_str("<blockquote>");
                    stack.push(Open::Tag("</blockquote>"));
                }
                Tag::CodeBlock(kind) => {
                    out.push_str("<pre><code");
                    if let CodeBlockKind::Fenced(info) = kind {
                        let language: String = info
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .chars()
                            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '+')
                            .take(20)
                            .collect();
                        if !language.is_empty() {
                            out.push_str(" class=\"language-");
                            escape_into(&mut out, &language);
                            out.push('"');
                        }
                    }
                    out.push('>');
                    stack.push(Open::Tag("</code></pre>"));
                }
                Tag::List(first) => {
                    let (open, close) = if first.is_some() {
                        ("<ol>", "</ol>")
                    } else {
                        ("<ul>", "</ul>")
                    };
                    out.push_str(open);
                    stack.push(Open::Tag(close));
                }
                Tag::Item => {
                    out.push_str("<li>");
                    stack.push(Open::Tag("</li>"));
                }
                Tag::Link { dest_url, .. } => match safe_href(&dest_url) {
                    Some(href) => {
                        out.push_str("<a href=\"");
                        escape_into(&mut out, &href);
                        // `ugc` tells search engines what this is; the rest is
                        // hygiene for a link a stranger chose.
                        out.push_str("\" rel=\"nofollow ugc noopener\" target=\"_blank\">");
                        stack.push(Open::Tag("</a>"));
                    }
                    None => stack.push(Open::Nothing),
                },
                Tag::Image { .. } => {
                    image_depth = 1;
                }
                // Tables, footnotes, task lists and anything the options above
                // do not enable: keep the text, drop the structure.
                _ => stack.push(Open::Nothing),
            },
            Event::End(tag) => {
                // The image branch is handled before this match.
                if let TagEnd::Image = tag {
                    continue;
                }
                match stack.pop() {
                    Some(Open::Tag(close)) => out.push_str(close),
                    Some(Open::Nothing) | None => {}
                }
            }
            Event::Text(text) => escape_into(&mut out, &text),
            Event::Code(code) => {
                out.push_str("<code>");
                escape_into(&mut out, &code);
                out.push_str("</code>");
            }
            // Raw HTML is shown, not interpreted — inert, and the author can see
            // that it did not work.
            Event::Html(html) | Event::InlineHtml(html) => escape_into(&mut out, &html),
            Event::SoftBreak => out.push('\n'),
            Event::HardBreak => out.push_str("<br>"),
            Event::Rule => out.push_str("<hr>"),
            // Footnotes/task lists are not enabled, so these cannot occur; if a
            // future option enables them, dropping them is the safe default.
            _ => {}
        }
    }

    // An unterminated construct leaves open tags; closing them keeps the page's
    // markup intact rather than letting the browser guess.
    while let Some(open) = stack.pop() {
        if let Open::Tag(close) = open {
            out.push_str(close);
        }
    }

    out
}

/// An href worth emitting: absolute http(s), mailto, or a site-relative path.
///
/// `javascript:` and friends are dropped along with the `<a>`, leaving the link
/// text behind — the post still reads, minus a link that would have been an
/// attack.
fn safe_href(dest: &str) -> Option<String> {
    let candidate = dest.trim();
    if candidate.is_empty() {
        return None;
    }
    let lowered = candidate.to_ascii_lowercase();
    let allowed = lowered.starts_with("https://")
        || lowered.starts_with("http://")
        || lowered.starts_with("mailto:")
        // Site-relative, but not protocol-relative (`//evil.example`).
        || (lowered.starts_with('/') && !lowered.starts_with("//"));
    allowed.then(|| candidate.to_string())
}

fn escape_into(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_the_toolbar_offers_survives() {
        assert_eq!(to_html("**bold**"), "<p><strong>bold</strong></p>");
        assert_eq!(to_html("_italic_"), "<p><em>italic</em></p>");
        assert_eq!(to_html("## Heading"), "<h2>Heading</h2>");
        assert_eq!(to_html("### Heading"), "<h3>Heading</h3>");
        assert_eq!(to_html("- a\n- b"), "<ul><li>a</li><li>b</li></ul>");
        assert_eq!(to_html("1. a"), "<ol><li>a</li></ol>");
        assert_eq!(
            to_html("> quoted"),
            "<blockquote><p>quoted</p></blockquote>"
        );
        assert_eq!(to_html("`code`"), "<p><code>code</code></p>");
        assert_eq!(
            to_html("```rust\nlet x = 1;\n```"),
            "<pre><code class=\"language-rust\">let x = 1;\n</code></pre>"
        );
    }

    #[test]
    fn heading_levels_cannot_outrank_the_page_title() {
        // `#` would be an h1 competing with the post title, which is the page's
        // only h1.
        assert_eq!(to_html("# Top"), "<h2>Top</h2>");
        assert_eq!(to_html("#### Deep"), "<h3>Deep</h3>");
    }

    #[test]
    fn raw_html_is_shown_as_text_rather_than_woven_in() {
        let html = to_html("<script>alert(1)</script>");
        assert!(
            !html.contains("<script>"),
            "raw HTML must never become markup: {html}"
        );
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn dangerous_link_schemes_are_dropped_but_the_text_stays() {
        let html = to_html("[click](javascript:alert(1))");
        assert!(!html.contains("javascript:"), "{html}");
        assert!(!html.contains("<a "), "no anchor at all: {html}");
        assert!(html.contains("click"), "the words remain: {html}");

        // Protocol-relative links are a redirect to someone else's origin.
        let html = to_html("[x](//evil.example/p)");
        assert!(!html.contains("evil.example"), "{html}");
    }

    #[test]
    fn allowed_links_are_marked_as_user_content() {
        let html = to_html("[site](https://example.org/page)");
        assert!(html.contains("href=\"https://example.org/page\""), "{html}");
        assert!(html.contains("rel=\"nofollow ugc noopener\""), "{html}");
        assert!(html.contains("target=\"_blank\""), "{html}");

        let relative = to_html("[news](/news/2-something)");
        assert!(
            relative.contains("href=\"/news/2-something\""),
            "{relative}"
        );
    }

    #[test]
    fn quotes_in_an_attribute_cannot_escape_it() {
        let html = to_html("[x](https://example.org/?a=%22onmouseover=%22alert)");
        // Whatever the destination, it is escaped: no bare quote inside the tag.
        let href = html
            .split("href=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_default();
        assert!(!href.contains('"'));
        assert!(!html.contains("onmouseover=\"x\""), "{html}");
    }

    #[test]
    fn images_are_not_rendered() {
        let html = to_html("![alt](https://example.org/tracker.png)");
        assert!(!html.contains("<img"), "{html}");
        assert!(!html.contains("tracker.png"), "{html}");
    }

    #[test]
    fn unclosed_markup_still_produces_balanced_output() {
        let html = to_html("**unclosed _and this");
        assert_eq!(
            html.matches("<strong>").count(),
            html.matches("</strong>").count()
        );
        assert_eq!(html.matches("<em>").count(), html.matches("</em>").count());
    }

    #[test]
    fn text_is_escaped_everywhere_it_appears() {
        assert_eq!(to_html("a & b < c"), "<p>a &amp; b &lt; c</p>");
        assert_eq!(
            to_html("`<b>`"),
            "<p><code>&lt;b&gt;</code></p>",
            "inline code is text, not markup"
        );
        let quoted = to_html("it's");
        assert!(quoted.contains("it&#39;s"), "{quoted}");
    }

    #[test]
    fn empty_input_renders_empty() {
        assert_eq!(to_html(""), "");
        assert_eq!(to_html("   \n\n "), "");
    }
}
