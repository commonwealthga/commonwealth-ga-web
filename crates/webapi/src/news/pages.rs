//! Server-rendered news pages.
//!
//! These exist for search engines: a prerendered SvelteKit page cannot show a
//! post that was written this morning, and an app shell is invisible to a
//! crawler. So the feed and the post pages are rendered here, as complete HTML
//! with a canonical URL, a description and `Article` structured data.
//!
//! The cost is that the site chrome is duplicated: the header and footer below
//! mirror `web/src/lib/components/Nav.svelte` and the layout's footer. That
//! duplication is guarded by a test which reads the Svelte source and fails if
//! the link sets diverge, because a silently different nav is worse than a
//! duplicated one.
//!
//! Everything user-supplied goes through [`escape`] on the way in. The one
//! exception is `body_html`, which `news::markdown` produced from a whitelist of
//! tags and escaped text.

use super::access;
use super::store::Post;

/// Nav links, in the order `Nav.svelte` lists them. Kept in step by a test.
pub const NAV: [(&str, &str); 8] = [
    ("/play", "Get started"),
    ("/server", "Server status"),
    ("/roadmap", "Roadmap"),
    ("/community", "Community"),
    ("/events", "Events"),
    ("/agents", "Agents"),
    ("/missions", "Missions"),
    ("/news", "News"),
];

/// Footer groups, mirroring the Svelte footer.
const FOOTER_PLAY: [(&str, &str); 5] = [
    ("/play", "Get started"),
    ("/server", "Server status"),
    ("/missions", "Missions"),
    ("/events", "Events"),
    ("/roadmap", "Roadmap"),
];

const DISCORD: &str = "https://discord.gg/CqACf4qArk";
const GITHUB: &str = "https://github.com/commonwealthga";

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
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
    out
}

/// What a page needs that is not its content.
pub struct Meta<'a> {
    pub title: &'a str,
    pub description: &'a str,
    /// Absolute, for `<link rel="canonical">` and the Open Graph tags.
    pub canonical: &'a str,
    pub site_url: &'a str,
    /// Stylesheet paths from the Svelte build.
    pub stylesheets: &'a [String],
    /// Extra markup for `<head>` (structured data).
    pub head: String,
    /// Which nav entry is current.
    pub active: &'a str,
    pub signed_in_as: Option<&'a str>,
    /// What the status chip says, e.g. "3 agents in-game". `None` renders the
    /// same "linking" placeholder the Svelte chip shows before its feed arrives.
    pub status: Option<&'a str>,
}

fn document(meta: &Meta, content: &str) -> String {
    let title = escape(meta.title);
    let description = escape(meta.description);
    let canonical = escape(meta.canonical);

    let mut links = String::new();
    for href in meta.stylesheets {
        links.push_str("<link rel=\"stylesheet\" href=\"");
        links.push_str(&escape(href));
        links.push_str("\">");
    }

    // `AccountLink.svelte`: signed in is an ember chip linking to /me, signed out
    // is a plain bordered "Sign in". Same markup, so the two headers agree.
    let account = match meta.signed_in_as {
        Some(name) => format!(
            "<a href=\"/me\" title=\"Your player page\" class=\"label flex items-center gap-2 border border-ember/50 bg-ember/10 px-2.5 py-1.5 text-ember transition-colors hover:bg-ember/20\">\
<span class=\"size-1 rounded-full bg-ember\"></span>{}</a>",
            escape(name)
        ),
        None => "<a href=\"/login\" class=\"label border border-line-hi px-2.5 py-1.5 text-dim transition-colors hover:border-ember/60 hover:text-ink\">Sign in</a>"
            .to_string(),
    };

    format!(
        "<!doctype html>\
<html lang=\"en\">\
<head>\
<meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{title} · The Commonwealth</title>\
<meta name=\"description\" content=\"{description}\">\
<link rel=\"canonical\" href=\"{canonical}\">\
<meta property=\"og:type\" content=\"website\">\
<meta property=\"og:site_name\" content=\"The Commonwealth\">\
<meta property=\"og:title\" content=\"{title}\">\
<meta property=\"og:description\" content=\"{description}\">\
<meta property=\"og:url\" content=\"{canonical}\">\
<meta name=\"twitter:card\" content=\"summary\">\
<meta name=\"theme-color\" content=\"#0b0d10\">\
{links}\
{head}\
</head>\
<body class=\"min-h-screen bg-void text-ink\">\
{backdrop}\
{header}\
<main class=\"mx-auto w-full max-w-6xl px-4 py-12 sm:px-6 sm:py-16\">{content}</main>\
{footer}\
<script src=\"/news-admin.js\" defer></script>\
</body>\
</html>",
        header = header(meta.active, &account, meta.status),
        footer = footer(meta.site_url, meta.signed_in_as.is_some()),
        backdrop = backdrop(),
        head = meta.head,
    )
}

const HEX_MARK_INNER: &str =
    "22.00,13.00 29.79,17.50 29.79,26.50 22.00,31.00 14.21,26.50 14.21,17.50";
const HEX_MARK_OUTER: &str = "22.00,4.00 37.59,13.00 37.59,31.00 22.00,40.00 6.41,31.00 6.41,13.00";

/// The sigil next to the wordmark — the same geometry as `HexMark.svelte`
/// (outer ring at radius 18, filled inner hexagon at 9, in a 44×44 box).
fn hex_mark(size: u32) -> String {
    format!(
        "<svg width=\"{size}\" height=\"{size}\" viewBox=\"0 0 44 44\" aria-hidden=\"true\" class=\"shrink-0\">\
<polygon points=\"{HEX_MARK_OUTER}\" fill=\"none\" stroke=\"var(--color-ember)\" stroke-width=\"2\" stroke-opacity=\"0.85\" />\
<polygon points=\"{HEX_MARK_INNER}\" fill=\"var(--color-red)\" /></svg>"
    )
}

/// The live-feed chip: same shape as `StatusChip.svelte`, rendered with the
/// count the server already knows. It does not animate for the pointer, but it
/// says the same thing.
fn status_chip(status: Option<&str>) -> String {
    let label = escape(status.unwrap_or("linking"));
    let dot = if status.is_some() {
        "bg-ember"
    } else {
        "bg-faint"
    };
    let tone = if status.is_some() {
        "text-dim"
    } else {
        "text-faint"
    };
    format!(
        "<a href=\"/server\" title=\"{label}\" class=\"clip-corner-sm flex shrink-0 items-center gap-2 whitespace-nowrap border border-line/90 bg-panel/80 px-2.5 py-1.5 transition-colors hover:border-ember/60 hover:bg-panel-hi\">\
<span class=\"relative flex size-1.5\"><span class=\"relative size-1.5 rounded-full {dot}\"></span></span>\
<span class=\"label whitespace-nowrap {tone}\">{label}</span></a>"
    )
}

/// `HexBackdrop.svelte`: the hex field, the warm wash and the vignette. The
/// pointer-following spotlight keeps its default position, so nothing here needs
/// JavaScript.
fn backdrop() -> String {
    "<div class=\"pointer-events-none fixed inset-0 -z-10 overflow-hidden\" aria-hidden=\"true\" style=\"--mx: 50%; --my: 15%; --shift-x: 0px; --shift-y: 0px;\">\
<div class=\"hexes absolute -inset-10 opacity-[0.045]\"></div>\
<div class=\"absolute inset-x-0 top-0 h-[55vh]\" style=\"background: radial-gradient(ellipse 85% 100% at 50% 0%, rgba(224, 57, 28, 0.11), transparent 72%);\"></div>\
<div class=\"absolute inset-0\" style=\"background: radial-gradient(680px circle at var(--mx) var(--my), rgba(255, 122, 24, 0.075), transparent 62%);\"></div>\
<div class=\"absolute inset-0\" style=\"background: radial-gradient(ellipse at 50% 35%, transparent 45%, #070506 100%);\"></div>\
</div>"
        .to_string()
}

fn header(active: &str, account: &str, status: Option<&str>) -> String {
    let mut items = String::new();
    for (href, label) in NAV {
        let on = active == href;
        items.push_str(&format!(
            "<li class=\"shrink-0\"><a href=\"{href}\"{current} class=\"relative flex items-baseline px-3 py-3 sm:px-4\">\
<span class=\"heading text-sm tracking-[0.14em] transition-colors {colour}\">{label}</span>\
<span class=\"absolute inset-x-3 bottom-0 h-[2px] transition-opacity {underline}\"></span>\
</a></li>",
            current = if on { " aria-current=\"page\"" } else { "" },
            colour = if on { "text-ink" } else { "text-dim" },
            underline = if on {
                "bg-ember opacity-100"
            } else {
                "bg-ember opacity-0"
            },
            label = escape(label),
        ));
    }

    format!(
        "<header class=\"sticky top-0 z-30 border-b border-line/70 bg-void/85 backdrop-blur-md\">\
<div class=\"h-[2px] w-full bg-gradient-to-r from-red-deep via-ember to-transparent\"></div>\
<div class=\"mx-auto flex w-full max-w-6xl items-center gap-3 px-4 py-3 sm:px-6\">\
<a href=\"/\" class=\"group flex items-center gap-2.5\">\
{mark}\
<span class=\"flex flex-col leading-none\">\
<span class=\"heading text-base tracking-[0.1em] text-ink transition-colors group-hover:text-amber sm:text-lg sm:tracking-[0.12em]\">The Commonwealth</span>\
<span class=\"label mt-1 hidden text-faint sm:block\">Global Agenda community server</span>\
</span>\
</a>\
<div class=\"ml-auto flex items-center gap-2\">\
<a class=\"label hidden px-2 py-1.5 text-dim transition-colors hover:text-ink lg:block\" href=\"{DISCORD}\" target=\"_blank\" rel=\"noopener noreferrer\">Discord</a>\
{account}\
{chip}\
</div>\
</div>\
<nav class=\"border-t border-line/50\"><div class=\"mx-auto w-full max-w-6xl px-2 sm:px-4\">\
<ul class=\"flex overflow-x-auto [-ms-overflow-style:none] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden\">{items}</ul>\
</div></nav>\
</header>",
        mark = hex_mark(28),
        chip = status_chip(status),
    )
}

fn footer(site_url: &str, signed_in: bool) -> String {
    let mut play = String::new();
    for (href, label) in FOOTER_PLAY {
        play.push_str(&format!(
            "<a class=\"text-sm text-dim transition-colors hover:text-ember\" href=\"{href}\">{}</a>",
            escape(label)
        ));
    }
    // The Svelte footer offers sign-in; offering it to someone who is signed in
    // would be a dead end.
    let sign_in = if signed_in {
        "<a class=\"text-sm text-dim transition-colors hover:text-ember\" href=\"/me\">Your page</a>"
    } else {
        "<a class=\"text-sm text-dim transition-colors hover:text-ember\" href=\"/login\">Sign in</a>"
    };
    format!(
        "<footer class=\"mt-12 border-t border-line/60 bg-carbon/50\">\
<div class=\"mx-auto flex w-full max-w-6xl flex-col gap-6 px-4 py-10 sm:px-6\">\
<div class=\"grid gap-8 sm:grid-cols-[1.4fr_1fr_1fr]\">\
<div class=\"flex flex-col gap-3\">\
<div class=\"flex items-center gap-2.5\">{mark}\
<span class=\"heading text-base tracking-[0.12em] text-ink\">The Commonwealth</span></div>\
<p class=\"max-w-md text-sm leading-relaxed text-dim\">A community-run server for Global Agenda, the 2010 team shooter that Hi-Rez Studios shut down in 2018. Free to play, open source, online around the clock.</p>\
</div>\
<nav class=\"flex flex-col gap-2\"><span class=\"label text-faint\">Play</span>{play}</nav>\
<nav class=\"flex flex-col gap-2\"><span class=\"label text-faint\">Elsewhere</span>\
<a class=\"text-sm text-dim transition-colors hover:text-ember\" href=\"{DISCORD}\" target=\"_blank\" rel=\"noopener noreferrer\">Discord</a>\
<a class=\"text-sm text-dim transition-colors hover:text-ember\" href=\"/community\">Community</a>\
<a class=\"text-sm text-dim transition-colors hover:text-ember\" href=\"{GITHUB}\" target=\"_blank\" rel=\"noopener noreferrer\">Source code</a>\
{sign_in}\
<a class=\"text-sm text-dim transition-colors hover:text-ember\" href=\"/api/status\">Status API</a>\
</nav>\
</div>\
<p class=\"border-t border-line/50 pt-5 text-xs leading-relaxed text-faint\">Not affiliated with Hi-Rez Studios. Global Agenda and all related assets are the property of their respective owners. This server is a non-commercial fan project.</p>\
<!-- canonical site: {site} -->\
</div>\
</footer>",
        mark = hex_mark(20),
        site = escape(site_url),
    )
}

/// Who is looking: the two things in the chrome that differ per visitor.
#[derive(Default)]
pub struct Chrome<'a> {
    pub signed_in_as: Option<&'a str>,
    /// The status chip's text, e.g. "3 agents in-game".
    pub status: Option<&'a str>,
}

/// Everything the feed page needs to know about the viewer's rights.
pub struct PostingHint {
    pub mode: &'static str,
    pub signed_in: bool,
    pub may_post: bool,
    pub reason: Option<String>,
}

/// What the feed page shows. `pending` means two different things depending on
/// who is reading: the whole queue for a moderator, or only the reader's own
/// submissions for everyone else.
pub struct FeedData<'a> {
    pub approved: &'a [Post],
    pub pending: &'a [Post],
    pub moderator: bool,
    /// Posts this reader has already thumbed up.
    pub reacted: &'a std::collections::HashSet<i64>,
    pub hint: PostingHint,
}

pub fn feed(data: &FeedData, chrome: &Chrome, stylesheets: &[String], base: &str) -> String {
    let posts = data.approved;
    let hint = &data.hint;
    let canonical = format!("{base}/news");
    let description =
        "News, articles, server updates and videos posted by the Commonwealth Global Agenda community.";

    let mut body = String::from(
        "<div class=\"flex flex-col gap-2\">\
<h1 class=\"heading text-4xl text-ink sm:text-5xl\">News</h1>\
<p class=\"max-w-2xl text-sm leading-relaxed text-dim\">News, articles, server updates and videos posted by the people who play here. Every entry is reviewed by a moderator before it appears.</p>\
</div>",
    );

    body.push_str(&posting_box(hint));

    if data.moderator && !data.pending.is_empty() {
        body.push_str(&review_queue(data.pending));
    }

    // A reader's own unpublished work belongs in the feed, marked, rather than
    // only in a page they would have to know about.
    let own_pending: &[Post] = if data.moderator { &[] } else { data.pending };

    if posts.is_empty() && own_pending.is_empty() {
        // A moderator looking at their own queue is not looking at an empty
        // feed; saying "nothing published yet" under a queue of work reads as a
        // bug.
        let message = if data.moderator && !data.pending.is_empty() {
            "Nothing published yet — the queue above is waiting for your review."
        } else {
            "Nothing published yet. The first post could be yours."
        };
        body.push_str(&format!(
            "<p class=\"mt-10 border border-line/60 bg-carbon/40 p-6 text-sm text-dim\">{message}</p>"
        ));
    } else {
        body.push_str("<ul class=\"mt-10 flex flex-col gap-3\">");
        for post in own_pending {
            body.push_str(&card(post, false, true));
        }
        for post in posts {
            body.push_str(&card(post, data.reacted.contains(&post.id), false));
        }
        body.push_str("</ul>");
    }

    let meta = Meta {
        title: "News",
        description,
        canonical: &canonical,
        site_url: base,
        stylesheets,
        head: String::new(),
        active: "/news",
        signed_in_as: chrome.signed_in_as,
        status: chrome.status,
    };
    document(&meta, &body)
}

fn posting_box(hint: &PostingHint) -> String {
    if hint.may_post {
        return "<div class=\"mt-8 flex flex-wrap items-center gap-3 border border-line/60 bg-carbon/40 p-4\">\
<a class=\"heading border border-ember/60 px-4 py-2 text-sm tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void\" href=\"/news/submit\">Write a post</a>\
<span class=\"label text-faint\">Reviewed by a moderator before it goes live</span>\
</div>"
            .to_string();
    }

    let reason = hint
        .reason
        .as_deref()
        .unwrap_or("Sign in with your game account to submit a post.");
    let action = if hint.signed_in {
        String::new()
    } else {
        "<a class=\"heading ml-2 underline decoration-ember/60 underline-offset-4 hover:text-ember\" href=\"/login\">Sign in</a>".to_string()
    };
    format!(
        "<div class=\"mt-8 border border-line/60 bg-carbon/40 p-4 text-sm text-dim\">{}{action}</div>",
        escape(reason)
    )
}

/// The queue, on the page a moderator is already looking at.
fn review_queue(pending: &[Post]) -> String {
    let mut items = String::new();
    for post in pending {
        items.push_str(&format!(
            "<li data-pending=\"{id}\" class=\"border border-amber/40 bg-carbon/40\">\
<div class=\"flex flex-col gap-3 p-5\">\
<p class=\"label text-faint\"><span class=\"text-amber\">{author}</span> posted {phrase} · {date}</p>\
{title}\
<div class=\"prose-news flex flex-col gap-3 border-y border-line/50 py-3 text-sm leading-relaxed text-dim\">{html}</div>\
{videos}\
<label class=\"flex flex-col gap-1.5\">\
<span class=\"label text-faint\">Note (required to reject)</span>\
<textarea data-note rows=\"2\" placeholder=\"What is wrong with it?\" class=\"w-full border border-line/70 bg-void/60 px-3 py-2 text-sm text-ink\"></textarea>\
</label>\
<p data-message class=\"hidden text-sm text-ember\"></p>\
<div class=\"flex flex-wrap items-center gap-3\">\
<button type=\"button\" data-action=\"approve\" class=\"heading border border-ember/60 px-4 py-2 text-sm tracking-[0.1em] text-ember transition-colors hover:bg-ember hover:text-void\">Approve</button>\
<button type=\"button\" data-action=\"reject\" class=\"heading border border-line/70 px-4 py-2 text-sm tracking-[0.1em] text-dim transition-colors hover:border-red/60 hover:text-ink\">Reject</button>\
</div>\
</div></li>",
            id = post.id,
            phrase = post.kind.phrase(),
            date = escape(&human_date(post.created_at)),
            author = escape(&post.author_name),
            title = if post.title.is_empty() {
                String::new()
            } else {
                format!("<h2 class=\"heading text-xl text-ink\">{}</h2>", escape(&post.title))
            },
            html = post.body_html,
            videos = video_grid(&post.videos),
        ));
    }

    format!(
        "<section class=\"mt-10\">\
<div class=\"flex flex-wrap items-baseline gap-3\">\
<h2 class=\"heading text-2xl text-ink\">Waiting for review</h2>\
<span class=\"label text-amber\">{count} pending</span>\
</div>\
<p class=\"mt-1 text-sm text-dim\">Only moderators see this section. Approving publishes immediately; rejecting needs a note and hides the post everywhere.</p>\
<ul class=\"mt-4 flex flex-col gap-3\">{items}</ul>\
</section>",
        count = pending.len(),
        items = items,
    )
}

/// A full entry: who posted what and when, the body, the videos, a thumbs-up.
/// There is no detail page — the feed is the page.
fn card(post: &Post, reacted: bool, pending: bool) -> String {
    let mut header = format!(
        "<p class=\"label text-faint\"><span class=\"{} \">{}</span> posted {} · <time datetime=\"{}\">{}</time></p>",
        if pending { "text-amber" } else { "text-dim" },
        escape(&post.author_name),
        post.kind.phrase(),
        escape(&iso_date(post.created_at)),
        escape(&human_date(post.created_at)),
    );
    if pending {
        header.push_str(
            "<p class=\"label mt-1 text-amber\">Waiting for review — only you and a moderator can see this.</p>",
        );
    }

    let title = if post.title.is_empty() {
        String::new()
    } else {
        format!(
            "<h2 class=\"heading mt-2 text-2xl text-ink\">{}</h2>",
            escape(&post.title)
        )
    };

    let summary = if post.summary.is_empty() || post.videos.is_empty() {
        // A written post's summary is for search results, not for the reader who
        // is already here and can see the body.
        String::new()
    } else {
        format!(
            "<p class=\"mt-2 text-sm leading-relaxed text-dim\">{}</p>",
            escape(&post.summary)
        )
    };

    let body = if post.body_html.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"prose-news mt-4 flex flex-col gap-3 text-base leading-relaxed text-dim\">{}</div>",
            post.body_html
        )
    };

    format!(
        "<li id=\"post-{id}\" class=\"border {} bg-carbon/40 p-5\">{header}{title}{summary}{body}{videos}{reactions}</li>",
        if pending {
            "border-amber/40"
        } else {
            "border-line/60"
        },
        id = post.id,
        videos = video_grid(&post.videos),
        reactions = reactions(post, reacted),
    )
}

/// Video embeds, side by side when there is room. The privacy-preserving domain,
/// so a reader who never presses play is not tracked.
fn video_grid(videos: &[String]) -> String {
    if videos.is_empty() {
        return String::new();
    }
    let columns = if videos.len() > 1 {
        "sm:grid-cols-2"
    } else {
        "grid-cols-1"
    };
    let mut out = format!("<div class=\"mt-4 grid gap-3 {columns}\">");
    for id in videos {
        let id = escape(id);
        out.push_str(&format!(
            "<div class=\"border border-line/60\"><iframe class=\"aspect-video w-full\" \
src=\"https://www.youtube-nocookie.com/embed/{id}\" title=\"Video\" loading=\"lazy\" \
allow=\"accelerometer; encrypted-media; picture-in-picture\" allowfullscreen></iframe></div>"
        ));
    }
    out.push_str("</div>");
    out
}

fn reactions(post: &Post, reacted: bool) -> String {
    format!(
        "<div class=\"mt-5 flex items-center gap-4 border-t border-line/70 pt-4\">\
<button type=\"button\" data-thumb-post=\"{id}\" data-thumb-active=\"{active}\" aria-pressed=\"{active}\" \
aria-label=\"Thumbs up\" class=\"heading flex items-center gap-2 border border-line/70 px-3 py-1.5 text-sm tracking-[0.1em] text-dim transition-colors hover:border-ember/60 hover:text-ink\">\
<span aria-hidden=\"true\">👍</span><span data-thumb-count=\"{id}\">{count}</span>\
</button>\
<span class=\"label text-faint\" data-thumb-note=\"{id}\"></span>\
</div>",
        id = post.id,
        active = reacted,
        count = post.thumbs_up,
    )
}
/// `2026-11-11` — the `datetime` attribute and structured data.
pub fn iso_date(ts: i64) -> String {
    let (year, month, day) = civil_from_unix(ts);
    format!("{year:04}-{month:02}-{day:02}")
}

/// `11 November 2026` — what a reader sees. UTC, because a news archive is a
/// record of when something was published, not of the reader's location.
pub fn human_date(ts: i64) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let (year, month, day) = civil_from_unix(ts);
    let name = MONTHS[(month - 1) as usize];
    format!("{day} {name} {year}")
}

/// Days since the Unix epoch to a calendar date (Howard Hinnant's algorithm).
fn civil_from_unix(ts: i64) -> (i64, u32, u32) {
    let days = ts.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Markdown for the author's help text on the submission page.
pub fn formatting_help() -> &'static str {
    access::FORMATTING_HELP
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::news::{Kind, Status};
    use std::collections::HashSet;
    use std::path::Path;

    const BASE: &str = "https://example.test";

    fn styles() -> Vec<String> {
        vec!["/_app/immutable/assets/0.css".to_string()]
    }

    fn article() -> Post {
        Post {
            id: 12,
            kind: Kind::Article,
            title: "Surfside bunker, solo".to_string(),
            slug: "surfside-bunker-solo".to_string(),
            summary: String::new(),
            body_markdown: "## Prep\n\nBring **two** medkits.".to_string(),
            body_html: crate::news::markdown::to_html("## Prep\n\nBring **two** medkits."),
            link: None,
            videos: Vec::new(),
            status: Status::Approved,
            author_id: 7,
            author_name: "Agent07".to_string(),
            created_at: 1_764_000_000,
            reviewed_by_name: Some("ModOne".to_string()),
            reviewed_at: Some(1_764_000_600),
            moderation_note: None,
            thumbs_up: 3,
        }
    }

    fn video(ids: &[&str]) -> Post {
        Post {
            id: 20,
            kind: Kind::Video,
            title: String::new(),
            slug: "video".to_string(),
            summary: String::new(),
            body_markdown: String::new(),
            body_html: String::new(),
            link: Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string()),
            videos: ids.iter().map(|id| (*id).to_string()).collect(),
            status: Status::Approved,
            author_id: 8,
            author_name: "Agent08".to_string(),
            created_at: 1_764_100_000,
            reviewed_by_name: None,
            reviewed_at: Some(1_764_100_060),
            moderation_note: None,
            thumbs_up: 0,
        }
    }

    fn pending_article() -> Post {
        let mut post = article();
        post.status = Status::Pending;
        post.title = "Awaiting a decision".to_string();
        post
    }

    fn hint() -> PostingHint {
        PostingHint {
            mode: "open",
            signed_in: true,
            may_post: true,
            reason: None,
        }
    }

    fn feed_page(approved: &[Post], pending: &[Post], moderator: bool) -> String {
        let reacted = HashSet::new();
        let data = FeedData {
            approved,
            pending,
            moderator,
            reacted: &reacted,
            hint: hint(),
        };
        feed(&data, &Chrome::default(), &styles(), BASE)
    }

    #[test]
    fn the_feed_shows_whole_entries_not_teasers() {
        // No detail page: what a reader sees here is the post.
        let page = feed_page(&[article()], &[], false);
        assert!(page.contains("Surfside bunker, solo"), "the title");
        assert!(
            page.contains("<h2>Prep</h2>"),
            "the rendered body, in full: {page}"
        );
        assert!(page.contains("<strong>two</strong>"), "with its formatting");
        assert!(
            !page.contains("href=\"/news/12-surfside-bunker-solo\""),
            "and no link to a page that no longer exists"
        );
        assert!(page.contains("id=\"post-12\""), "an anchor for the entry");
    }

    #[test]
    fn each_entry_is_headlined_with_who_posted_what_and_when() {
        let page = feed_page(&[article()], &[], false);
        assert!(
            page.contains("Agent07</span> posted an article"),
            "the header line: {page}"
        );
        assert!(page.contains("24 November 2025"), "with the date");

        let page = feed_page(&[video(&["dQw4w9WgXcQ"])], &[], false);
        assert!(page.contains("Agent08</span> posted a video"), "{page}");

        let mut update = article();
        update.kind = Kind::Update;
        let page = feed_page(&[update], &[], false);
        assert!(page.contains("posted a server update"), "{page}");

        let mut news = article();
        news.kind = Kind::News;
        let page = feed_page(&[news], &[], false);
        assert!(page.contains("posted news"), "{page}");
    }

    #[test]
    fn a_video_entry_is_embeds_and_nothing_else() {
        let page = feed_page(&[video(&["dQw4w9WgXcQ"])], &[], false);
        assert!(
            page.contains("youtube-nocookie.com/embed/dQw4w9WgXcQ"),
            "the privacy-preserving embed: {page}"
        );
        assert_eq!(page.matches("<iframe").count(), 1);
        assert!(
            !page.contains("<h2 class=\"heading mt-2 text-2xl"),
            "no invented title: {page}"
        );
    }

    #[test]
    fn several_videos_sit_side_by_side() {
        let page = feed_page(&[video(&["dQw4w9WgXcQ", "aaaaaaaaaaa"])], &[], false);
        assert_eq!(page.matches("<iframe").count(), 2);
        assert!(
            page.contains("sm:grid-cols-2"),
            "two per row when there is room"
        );
    }

    #[test]
    fn every_entry_carries_its_own_thumbs_up() {
        let mut reacted = HashSet::new();
        reacted.insert(12);
        let data = FeedData {
            approved: &[article()],
            pending: &[],
            moderator: false,
            reacted: &reacted,
            hint: hint(),
        };
        let page = feed(&data, &Chrome::default(), &styles(), BASE);

        assert!(page.contains("data-thumb-post=\"12\""), "{page}");
        assert!(
            page.contains("data-thumb-count=\"12\">3"),
            "the count: {page}"
        );
        assert!(
            page.contains("data-thumb-active=\"true\""),
            "and this reader's own reaction is reflected"
        );
        // One button per entry, not one per page.
        assert_eq!(page.matches("data-thumb-post=").count(), 1);
    }

    #[test]
    fn the_authors_own_pending_entry_is_marked_and_has_no_controls() {
        let page = feed_page(&[], &[pending_article()], false);
        assert!(
            page.contains("Awaiting a decision"),
            "their own post is visible"
        );
        assert!(page.contains("Waiting for review"), "{page}");
        assert!(
            page.contains("only you and a moderator can see this"),
            "with the reason"
        );
        assert!(!page.contains("data-action=\"approve\""), "{page}");
    }

    #[test]
    fn a_moderator_sees_the_queue_with_controls_and_embeds() {
        let queue = [pending_article(), {
            let mut p = video(&["dQw4w9WgXcQ"]);
            p.status = Status::Pending;
            p
        }];
        let page = feed_page(&[], &queue, true);
        assert!(page.contains("Waiting for review"), "{page}");
        assert!(page.contains("2 pending"));
        assert!(page.contains("data-action=\"approve\""), "{page}");
        assert!(page.contains("data-action=\"reject\""));
        assert!(page.contains("data-note"), "somewhere to write the reason");
        assert!(
            page.contains("youtube-nocookie.com/embed/dQw4w9WgXcQ"),
            "a video submitted for review can be watched before deciding"
        );
        assert!(
            page.contains("the queue above is waiting for your review"),
            "the empty-feed line does not contradict the queue"
        );
    }

    #[test]
    fn nothing_is_published_says_so() {
        let page = feed_page(&[], &[], false);
        assert!(page.contains("Nothing published yet. The first post could be yours."));
    }

    #[test]
    fn user_text_is_escaped_wherever_it_lands() {
        let mut post = article();
        post.title = "<img src=x onerror=alert(1)>".to_string();
        post.author_name = "\"quoted\"".to_string();
        let page = feed_page(&[post], &[], false);
        assert!(!page.contains("<img src=x"), "{page}");
        assert!(page.contains("&lt;img src=x"));
        assert!(page.contains("&quot;quoted&quot;"));
    }

    #[test]
    fn no_javascript_is_generated_inside_the_rust_templates() {
        // This is the bug that shipped: JavaScript built with `format!` had its
        // braces mangled, the browser dropped the whole script on a syntax
        // error, and the approve button did nothing at all. Every page now
        // points at one checked file instead.
        let queue = [pending_article()];
        let page = feed_page(&[article()], &queue, true);
        assert!(
            page.contains("<script src=\"/news-admin.js\" defer></script>"),
            "the page loads the shared script"
        );
        let inline: Vec<&str> = page
            .split("<script>")
            .skip(1)
            .filter_map(|rest| rest.split("</script>").next())
            .collect();
        assert!(inline.is_empty(), "no inline behaviour scripts: {inline:?}");
    }

    #[test]
    fn dates_are_formatted_without_a_date_library() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(human_date(0), "1 January 1970");
        // 2026-11-11T00:00:00Z
        assert_eq!(iso_date(1_794_355_200), "2026-11-11");
        assert_eq!(human_date(1_794_355_200), "11 November 2026");
        // Leap day, to catch an off-by-one in the civil algorithm.
        assert_eq!(iso_date(1_709_164_800), "2024-02-29");
    }

    #[test]
    fn the_feed_has_crawlable_metadata() {
        let page = feed_page(&[article()], &[], false);
        assert!(page.starts_with("<!doctype html>"));
        assert!(
            page.contains("<title>News · The Commonwealth</title>"),
            "{page}"
        );
        assert!(
            page.contains("<link rel=\"canonical\" href=\"https://example.test/news\">"),
            "{page}"
        );
        assert!(page.contains("<meta name=\"description\""));
        assert_eq!(page.matches("<h1").count(), 1, "one h1 for the page");
    }

    #[test]
    fn the_header_shows_who_is_signed_in() {
        let signed_in = Chrome {
            signed_in_as: Some("Agent07"),
            status: Some("3 agents in-game"),
        };
        let reacted = HashSet::new();
        let data = FeedData {
            approved: &[article()],
            pending: &[],
            moderator: false,
            reacted: &reacted,
            hint: hint(),
        };
        let page = feed(&data, &signed_in, &styles(), BASE);

        let header = page
            .split("<header")
            .nth(1)
            .and_then(|rest| rest.split("</header>").next())
            .expect("the page has a header");
        assert!(header.contains("Agent07"), "the account chip: {header}");
        assert!(header.contains("href=\"/me\""));
        assert!(
            !header.contains("Sign in"),
            "not offered to a signed-in reader"
        );
        assert!(header.contains("3 agents in-game"), "and the live chip");

        let anonymous = page_with_chrome(&Chrome::default());
        let anonymous_header = anonymous
            .split("<header")
            .nth(1)
            .and_then(|rest| rest.split("</header>").next())
            .expect("header");
        assert!(anonymous_header.contains("Sign in"), "{anonymous_header}");
    }

    fn page_with_chrome(chrome: &Chrome) -> String {
        let reacted = HashSet::new();
        let data = FeedData {
            approved: &[article()],
            pending: &[],
            moderator: false,
            reacted: &reacted,
            hint: hint(),
        };
        feed(&data, chrome, &styles(), BASE)
    }

    #[test]
    fn the_chrome_is_the_sites_chrome_not_a_plainer_one() {
        let page = page_with_chrome(&Chrome::default());
        assert!(page.contains(HEX_MARK_OUTER), "the hex mark: {page}");
        assert!(
            page.contains("Global Agenda community server"),
            "the tagline"
        );
        assert!(page.contains("class=\"hexes"), "the hex backdrop");
        for expected in [
            "Source code",
            "Status API",
            "Not affiliated with Hi-Rez Studios",
        ] {
            assert!(page.contains(expected), "footer is missing {expected}");
        }
        assert!(
            page.contains("max-w-6xl px-4 py-12"),
            "the same page width as every other page"
        );
    }

    /// The chrome is duplicated between Rust and Svelte; this is what stops the
    /// two drifting. It reads the Svelte source, so it fails in the change that
    /// causes the divergence, not months later.
    #[test]
    fn the_navigation_matches_the_svelte_header() {
        let nav_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/src/lib/components/Nav.svelte");
        let source = std::fs::read_to_string(&nav_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", nav_path.display()));

        let mut svelte_links: Vec<(String, String)> = Vec::new();
        for line in source.lines() {
            let Some(start) = line.find("href: '") else {
                continue;
            };
            let rest = &line[start + 7..];
            let Some(end) = rest.find('\'') else { continue };
            let href = &rest[..end];
            let Some(label_start) = line.find("label: '") else {
                continue;
            };
            let label_rest = &line[label_start + 8..];
            let Some(label_end) = label_rest.find('\'') else {
                continue;
            };
            svelte_links.push((href.to_string(), label_rest[..label_end].to_string()));
        }

        assert!(
            !svelte_links.is_empty(),
            "no links parsed from Nav.svelte; the test needs updating with it"
        );

        let rust_links: Vec<(String, String)> = NAV
            .iter()
            .map(|(href, label)| (href.to_string(), label.to_string()))
            .collect();

        for (href, label) in &rust_links {
            let svelte_label = svelte_links
                .iter()
                .find(|(h, _)| h == href)
                .map(|(_, l)| l.as_str())
                .unwrap_or_else(|| panic!("{href} is in the news header but not in Nav.svelte"));
            assert_eq!(
                svelte_label, label,
                "{href} is labelled {label:?} here and {svelte_label:?} in Nav.svelte"
            );
        }
        for (href, _) in &svelte_links {
            assert!(
                rust_links.iter().any(|(h, _)| h == href),
                "{href} is in Nav.svelte but missing from the news header"
            );
        }
    }
}
