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
    pub hint: PostingHint,
}

pub fn feed(data: &FeedData, chrome: &Chrome, stylesheets: &[String], base: &str) -> String {
    let posts = data.approved;
    let hint = &data.hint;
    let canonical = format!("{base}/news");
    let description =
        "News, guides, server updates and videos from the Commonwealth Global Agenda community.";

    let mut body = String::from(
        "<div class=\"flex flex-col gap-2\">\
<h1 class=\"heading text-4xl text-ink sm:text-5xl\">News</h1>\
<p class=\"max-w-2xl text-sm leading-relaxed text-dim\">Guides, server updates and articles written by the people who play here. Every post is reviewed by a moderator before it appears.</p>\
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
            body.push_str(&awaiting_card(post));
        }
        for post in posts {
            body.push_str(&card(post));
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
        let path = escape(&post.path());
        items.push_str(&format!(
            "<li data-pending=\"{id}\" class=\"border border-amber/40 bg-carbon/40\">\
<div class=\"flex flex-col gap-3 p-5\">\
<div class=\"flex flex-wrap items-center gap-3\">\
<span class=\"label border border-amber/50 px-1.5 py-0.5 text-amber\">{kind}</span>\
<span class=\"label text-faint\">{date}</span>\
<span class=\"label text-faint\">{author}</span>\
</div>\
<h2 class=\"heading text-xl text-ink\"><a class=\"hover:text-amber\" href=\"{path}\" target=\"_blank\" rel=\"noopener\">{title}</a></h2>\
<div class=\"prose-news flex flex-col gap-3 border-y border-line/50 py-3 text-sm leading-relaxed text-dim\">{html}</div>\
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
            kind = escape(post.kind.label()),
            date = escape(&iso_date(post.created_at)),
            author = escape(&post.author_name),
            title = escape(&post.title),
            html = post.body_html,
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

/// The reader's own unpublished post, in the feed with an honest label.
fn awaiting_card(post: &Post) -> String {
    let path = escape(&post.path());
    format!(
        "<li class=\"border border-amber/40 bg-carbon/40\">\
<a class=\"block p-5\" href=\"{path}\">\
<div class=\"flex flex-wrap items-center gap-3\">\
<span class=\"label border border-line/70 px-1.5 py-0.5 text-faint\">{kind}</span>\
<span class=\"label border border-amber/50 px-1.5 py-0.5 text-amber\">Waiting for review</span>\
<span class=\"label text-faint\">{date}</span>\
</div>\
<h2 class=\"heading mt-2 text-xl text-ink\">{title}</h2>\
<p class=\"mt-1 text-sm text-dim\">Only you (and a moderator) can see this until it is approved.</p>\
</a></li>",
        kind = escape(post.kind.label()),
        date = escape(&iso_date(post.created_at)),
        title = escape(&post.title),
    )
}

fn card(post: &Post) -> String {
    let path = escape(&post.path());
    let summary = if post.summary.is_empty() {
        excerpt(&post.body_markdown)
    } else {
        post.summary.clone()
    };
    let mut sub = String::new();
    if !summary.is_empty() {
        sub = format!(
            "<p class=\"mt-1 line-clamp-2 text-sm leading-relaxed text-dim\">{}</p>",
            escape(&summary)
        );
    }
    format!(
        "<li class=\"border border-line/60 bg-carbon/40 transition-colors hover:border-ember/40\">\
<a class=\"block p-5\" href=\"{path}\">\
<div class=\"flex flex-wrap items-center gap-3\">\
<span class=\"label border border-line/70 px-1.5 py-0.5 text-faint\">{kind}</span>\
<span class=\"label text-faint\">{date}</span>\
</div>\
<h2 class=\"heading mt-2 text-xl text-ink\">{title}</h2>{sub}\
<div class=\"label mt-3 flex items-center gap-3 text-faint\"><span>{author}</span><span>👍 {thumbs}</span></div>\
</a></li>",
        kind = escape(post.kind.label()),
        date = escape(&iso_date(post.created_at)),
        title = escape(&post.title),
        author = escape(&post.author_name),
        thumbs = post.thumbs_up,
    )
}

pub fn post_page(
    post: &Post,
    reacted: bool,
    pending: bool,
    chrome: &Chrome,
    stylesheets: &[String],
    base: &str,
) -> String {
    let path = post.path();
    let canonical = format!("{base}{path}");
    let description = if post.summary.is_empty() {
        format!(
            "{} — a {} post on The Commonwealth.",
            post.title,
            post.kind.label()
        )
    } else {
        post.summary.clone()
    };

    // Structured data: what a crawler reads to know this is an article and when
    // it was published.
    let head = format!(
        "<script type=\"application/ld+json\">{{\"@context\":\"https://schema.org\",\"@type\":\"Article\",\"headline\":\"{}\",\"description\":\"{}\",\"datePublished\":\"{}\",\"dateModified\":\"{}\",\"author\":{{\"@type\":\"Person\",\"name\":\"{}\"}},\"publisher\":{{\"@type\":\"Organization\",\"name\":\"The Commonwealth\"}},\"mainEntityOfPage\":\"{}\"}}</script>",
        json_escape(&post.title),
        json_escape(&description),
        iso_date(post.created_at),
        iso_date(post.reviewed_at.unwrap_or(post.created_at)),
        json_escape(&post.author_name),
        json_escape(&canonical),
    );

    let mut body = String::new();
    if pending {
        body.push_str(
            "<div class=\"mb-6 border border-amber/40 bg-carbon/40 p-4 text-sm text-dim\">This post is waiting for a moderator to review it. Only you can see it for now.</div>",
        );
    }

    body.push_str(&format!(
        "<nav class=\"label text-faint\"><a class=\"hover:text-ember\" href=\"/news\">News</a> / {kind}</nav>\
<article class=\"mt-4\">\
<header class=\"border-b border-line/60 pb-6\">\
<div class=\"flex flex-wrap items-center gap-3\">\
<span class=\"label border border-line/70 px-1.5 py-0.5 text-faint\">{kind}</span>\
<time class=\"label text-faint\" datetime=\"{stamp}\">{date}</time>\
</div>\
<h1 class=\"heading mt-3 text-3xl text-ink sm:text-4xl\">{title}</h1>\
{summary}\
<p class=\"label mt-4 text-faint\">By {author}</p>\
</header>\
<div class=\"prose-news mt-8 text-base leading-relaxed text-dim\">{html}</div>\
{media}\
{reactions}\
</article>",
        kind = escape(post.kind.label()),
        stamp = escape(&iso_date(post.created_at)),
        date = escape(&human_date(post.created_at)),
        title = escape(&post.title),
        summary = if post.summary.is_empty() {
            String::new()
        } else {
            format!(
                "<p class=\"mt-3 text-base leading-relaxed text-dim\">{}</p>",
                escape(&post.summary)
            )
        },
        author = escape(&post.author_name),
        html = post.body_html,
        media = media(post),
        reactions = reactions(post, reacted),
    ));

    let meta = Meta {
        title: &post.title,
        description: &description,
        canonical: &canonical,
        site_url: base,
        stylesheets,
        head,
        active: "/news",
        signed_in_as: chrome.signed_in_as,
        status: chrome.status,
    };
    document(&meta, &body)
}

/// A video embed or an outbound link, when the post has one.
fn media(post: &Post) -> String {
    if let Some(id) = &post.video_id {
        let id = escape(id);
        return format!(
            "<div class=\"mt-8 border border-line/60\"><iframe class=\"aspect-video w-full\" src=\"https://www.youtube-nocookie.com/embed/{id}\" title=\"Video\" loading=\"lazy\" allow=\"accelerometer; encrypted-media; picture-in-picture\" allowfullscreen></iframe></div>\
<p class=\"label mt-2 text-faint\"><a class=\"hover:text-ember\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">Watch on YouTube</a></p>",
            escape(post.link.as_deref().unwrap_or("https://www.youtube.com"))
        );
    }
    match post.link.as_deref() {
        Some(link) => format!(
            "<p class=\"mt-8 text-sm\"><a class=\"text-ember underline decoration-ember/40 underline-offset-4\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer nofollow\">Read the original</a></p>",
            escape(link)
        ),
        None => String::new(),
    }
}

/// Thumbs up. The count is rendered server-side; the button upgrades it to a
/// toggle with a small amount of JavaScript, so the number is correct even
/// without it.
fn reactions(post: &Post, reacted: bool) -> String {
    format!(
        "<div class=\"mt-10 flex items-center gap-4 border-t border-line/60 pt-6\">\
<button type=\"button\" id=\"thumb\" data-post=\"{id}\" data-active=\"{active}\" aria-pressed=\"{active}\" \
class=\"heading flex items-center gap-2 border border-line/70 px-4 py-2 text-sm tracking-[0.1em] text-dim transition-colors hover:border-ember/60 hover:text-ink\">\
<span aria-hidden=\"true\">👍</span><span id=\"thumb-count\">{count}</span>\
</button>\
<span class=\"label text-faint\" id=\"thumb-note\"></span>\
</div>",
        id = post.id,
        active = reacted,
        count = post.thumbs_up,
    )
}

/// Plain-text preview of a body, for cards that have no summary.
fn excerpt(markdown_source: &str) -> String {
    let mut out = String::new();
    let mut last_space = true;
    for ch in markdown_source.chars() {
        let ch = match ch {
            '#' | '*' | '_' | '`' | '>' | '[' | ']' | '(' | ')' | '!' | '\\' => continue,
            '\n' | '\r' | '\t' => ' ',
            other => other,
        };
        if ch == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        out.push(ch);
        if out.chars().count() >= 180 {
            out.push('…');
            break;
        }
    }
    out.trim().to_string()
}

fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' | '\t' => out.push(' '),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            other => out.push(other),
        }
    }
    out
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
    use std::path::Path;

    fn sample_post() -> Post {
        Post {
            id: 12,
            kind: Kind::Guide,
            title: "Solo the Surfside bunker <script>".to_string(),
            slug: "solo-the-surfside-bunker-script".to_string(),
            summary: String::new(),
            body_markdown: "## Prep\n\nBring **two** medkits.".to_string(),
            body_html: crate::news::markdown::to_html("## Prep\n\nBring **two** medkits."),
            link: None,
            video_id: None,
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

    /// A feed page with no pending posts and no moderator, for the tests that
    /// are about the posts themselves.
    fn feed_page(posts: &[Post], hint: PostingHint, chrome: &Chrome) -> String {
        let data = FeedData {
            approved: posts,
            pending: &[],
            moderator: false,
            hint,
        };
        feed(&data, chrome, &styles(), "https://example.test")
    }

    fn styles() -> Vec<String> {
        vec!["/_app/immutable/assets/0.css".to_string()]
    }

    #[test]
    fn the_post_page_is_a_complete_document_with_crawlable_metadata() {
        let page = post_page(
            &sample_post(),
            true,
            false,
            &Chrome::default(),
            &styles(),
            "https://example.test",
        );

        assert!(
            page.starts_with("<!doctype html>"),
            "a full document, not a fragment"
        );
        assert!(page.contains("<link rel=\"canonical\" href=\"https://example.test/news/12-solo-the-surfside-bunker-script\">"), "{page}");
        assert!(
            page.contains("<meta name=\"description\""),
            "a description for the snippet"
        );
        assert!(
            page.contains("\"@type\":\"Article\""),
            "structured data: {page}"
        );
        assert!(page.contains("<h1 "), "exactly one meaningful heading");
        assert_eq!(page.matches("<h1 ").count(), 1);
        assert!(page.contains("<time "), "a machine-readable date");
        assert!(
            page.contains("datetime=\"2025-11-"),
            "ISO date in datetime: {page}"
        );
        assert!(
            page.contains("/_app/immutable/assets/0.css"),
            "the site's stylesheet"
        );
    }

    #[test]
    fn a_title_cannot_inject_markup_or_break_the_structured_data() {
        let page = post_page(
            &sample_post(),
            false,
            false,
            &Chrome::default(),
            &styles(),
            "https://example.test",
        );
        assert!(!page.contains("<script>alert"), "{page}");
        assert!(
            page.contains("&lt;script&gt;"),
            "the title is escaped: {page}"
        );
        // The JSON-LD block stays parseable: one script tag, quotes escaped.
        assert_eq!(
            page.matches("<script type=\"application/ld+json\">")
                .count(),
            1
        );
    }

    #[test]
    fn pending_posts_say_so_on_their_page() {
        let page = post_page(
            &sample_post(),
            false,
            true,
            &Chrome::default(),
            &styles(),
            "https://example.test",
        );
        assert!(page.contains("waiting for a moderator"), "{page}");
        let published = post_page(
            &sample_post(),
            false,
            false,
            &Chrome::default(),
            &styles(),
            "https://example.test",
        );
        assert!(!published.contains("waiting for a moderator"));
    }

    #[test]
    fn the_feed_lists_posts_and_explains_the_rules() {
        let hint = PostingHint {
            mode: "open",
            signed_in: false,
            may_post: false,
            reason: Some("Sign in with your game account to submit a post.".to_string()),
        };
        let page = feed_page(&[sample_post()], hint, &Chrome::default());
        assert!(page.contains("href=\"/news/12-solo-the-surfside-bunker-script\""));
        assert!(page.contains("Guide"));
        assert!(page.contains("Sign in"), "the reason is shown");
        assert!(
            !page.contains("Write a post"),
            "no form link for anonymous readers"
        );

        let hint = PostingHint {
            mode: "open",
            signed_in: true,
            may_post: true,
            reason: None,
        };
        let page = feed_page(&[sample_post()], hint, &Chrome::default());
        assert!(page.contains("Write a post"), "{page}");
        assert!(page.contains("href=\"/news/submit\""));
    }

    #[test]
    fn an_empty_feed_still_renders_a_page() {
        let hint = PostingHint {
            mode: "open",
            signed_in: true,
            may_post: true,
            reason: None,
        };
        let page = feed_page(&[], hint, &Chrome::default());
        assert!(page.contains("Nothing published yet"), "{page}");
    }

    fn pending_post() -> Post {
        let mut post = sample_post();
        post.status = Status::Pending;
        post.title = "Awaiting a decision".to_string();
        post.slug = "awaiting-a-decision".to_string();
        post
    }

    #[test]
    fn a_moderator_sees_the_queue_on_the_feed_with_its_controls() {
        let queue = [pending_post()];
        let data = FeedData {
            approved: &[],
            pending: &queue,
            moderator: true,
            hint: hint(),
        };
        let page = feed(&data, &Chrome::default(), &styles(), "https://example.test");

        assert!(
            page.contains("Waiting for review"),
            "the section is on the feed"
        );
        assert!(page.contains("1 pending"));
        assert!(page.contains("Awaiting a decision"), "the post itself");
        assert!(
            page.contains("<h2>Prep</h2>"),
            "with its rendered body to judge"
        );
        assert!(page.contains("data-action=\"approve\""), "{page}");
        assert!(page.contains("data-action=\"reject\""));
        assert!(
            page.contains("data-note"),
            "a rejection needs somewhere to write the reason"
        );
        // The empty-feed line knows about the queue rather than contradicting it.
        assert!(
            page.contains("the queue above is waiting for your review"),
            "{page}"
        );
        assert!(!page.contains("The first post could be yours."));
    }

    #[test]
    fn a_reader_sees_their_own_pending_post_marked_as_waiting() {
        let mine = [pending_post()];
        let data = FeedData {
            approved: &[],
            pending: &mine,
            moderator: false,
            hint: hint(),
        };
        let page = feed(&data, &Chrome::default(), &styles(), "https://example.test");

        assert!(
            page.contains("Awaiting a decision"),
            "their own post is visible"
        );
        assert!(page.contains("Waiting for review"), "and labelled");
        assert!(
            page.contains("Only you (and a moderator) can see this"),
            "with the reason it is not public"
        );
        assert!(
            !page.contains("data-action=\"approve\""),
            "an author must not approve their own post: {page}"
        );
        assert!(!page.contains("data-note"));
    }

    #[test]
    fn a_reader_with_nothing_pending_sees_no_queue_and_no_notices() {
        let data = FeedData {
            approved: &[sample_post()],
            pending: &[],
            moderator: false,
            hint: hint(),
        };
        let page = feed(&data, &Chrome::default(), &styles(), "https://example.test");
        assert!(!page.contains("Waiting for review"));
        assert!(!page.contains("data-action"));
        assert!(page.contains("solo-the-surfside-bunker-script"));
    }

    #[test]
    fn the_queue_escapes_what_it_renders() {
        let mut post = pending_post();
        post.title = "<img src=x onerror=alert(1)>".to_string();
        post.author_name = "\"quoted\"".to_string();
        let queue = [post];
        let data = FeedData {
            approved: &[],
            pending: &queue,
            moderator: true,
            hint: hint(),
        };
        let page = feed(&data, &Chrome::default(), &styles(), "https://example.test");
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
        let queue = [pending_post()];
        let data = FeedData {
            approved: &[sample_post()],
            pending: &queue,
            moderator: true,
            hint: hint(),
        };
        let pages = [
            feed(&data, &Chrome::default(), &styles(), "https://example.test"),
            post_page(
                &sample_post(),
                false,
                false,
                &Chrome::default(),
                &styles(),
                "https://example.test",
            ),
        ];

        for page in pages {
            assert!(
                page.contains("<script src=\"/news-admin.js\" defer></script>"),
                "every page loads the shared script"
            );
            // The only inline script allowed is structured data, which is JSON.
            let inline: Vec<&str> = page
                .split("<script>")
                .skip(1)
                .filter_map(|rest| rest.split("</script>").next())
                .collect();
            assert!(
                inline.is_empty(),
                "no inline behaviour scripts: found {:?}",
                inline.first().map(|body| &body[..body.len().min(60)])
            );
        }
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
    fn a_video_post_embeds_through_the_privacy_preserving_domain() {
        let mut post = sample_post();
        post.kind = Kind::Video;
        post.video_id = Some("dQw4w9WgXcQ".to_string());
        post.link = Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string());
        let page = post_page(
            &post,
            false,
            false,
            &Chrome::default(),
            &styles(),
            "https://example.test",
        );
        assert!(
            page.contains("youtube-nocookie.com/embed/dQw4w9WgXcQ"),
            "{page}"
        );
        assert!(page.contains("Watch on YouTube"));
    }

    #[test]
    fn the_header_shows_who_is_signed_in() {
        // The bug this pins: the page rendered "Sign in" to a signed-in reader,
        // because the chrome was built without their name.
        let signed_in = Chrome {
            signed_in_as: Some("Agent07"),
            status: None,
        };
        let page = feed_page(&[sample_post()], hint(), &signed_in);
        assert!(
            page.contains("Agent07"),
            "the account name is in the header"
        );
        assert!(
            page.contains("href=\"/me\""),
            "the account chip links to the player page: {page}"
        );
        // Scoped to the header: the posting hint below can legitimately tell an
        // anonymous reader to sign in, and this test is about the account chip.
        let header = page
            .split("<header")
            .nth(1)
            .and_then(|rest| rest.split("</header>").next())
            .expect("the page has a header");
        assert!(
            !header.contains("Sign in"),
            "a signed-in reader is not offered sign-in: {header}"
        );

        let anonymous = feed_page(&[sample_post()], hint(), &Chrome::default());
        let anonymous_header = anonymous
            .split("<header")
            .nth(1)
            .and_then(|rest| rest.split("</header>").next())
            .expect("header");
        assert!(anonymous_header.contains("Sign in"), "{anonymous_header}");
    }

    #[test]
    fn the_header_carries_the_live_status_chip() {
        let live = Chrome {
            signed_in_as: None,
            status: Some("3 agents in-game"),
        };
        let page = feed_page(&[], hint(), &live);
        assert!(
            page.contains("3 agents in-game"),
            "the chip says what the server knows"
        );
        assert!(
            page.contains("href=\"/server\""),
            "and links to the status page"
        );

        let unknown = feed_page(&[], hint(), &Chrome::default());
        assert!(
            unknown.contains("linking"),
            "no status yet renders the placeholder, not a wrong count"
        );
    }

    #[test]
    fn the_chrome_is_the_sites_chrome_not_a_plainer_one() {
        // What was missing when the page first shipped: the sigil, the tagline,
        // the backdrop and the footer's third column.
        let page = feed_page(&[], hint(), &Chrome::default());
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
    }

    fn hint() -> PostingHint {
        PostingHint {
            mode: "open",
            signed_in: false,
            may_post: false,
            reason: Some("Sign in with your game account to submit a post.".to_string()),
        }
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

        // `{ href: '/play', label: 'Get started' }` entries in the links array.
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
            assert!(
                svelte_links.iter().any(|(h, _)| h == href),
                "{href} is in the news page header but not in Nav.svelte ({svelte_links:?})"
            );
            let svelte_label = svelte_links
                .iter()
                .find(|(h, _)| h == href)
                .map(|(_, l)| l.as_str())
                .unwrap_or_default();
            assert_eq!(
                svelte_label, label,
                "{href} is labelled {label:?} here and {svelte_label:?} in Nav.svelte"
            );
        }

        // The reverse direction: every Svelte link must exist here too, or a
        // visitor loses a way to navigate from the news pages.
        for (href, _) in &svelte_links {
            assert!(
                rust_links.iter().any(|(h, _)| h == href),
                "{href} is in Nav.svelte but missing from the news page header"
            );
        }
    }
}
