//! The news feed: community posts, moderated before they appear.
//!
//! Shape of the thing, and why:
//!
//! * **Anyone signed in may submit.** Nothing appears on the site until a
//!   moderator approves it, so the failure mode of open posting is a queue
//!   nobody asked for, not a spam wall on the front page.
//! * **Rejection is the soft delete.** A rejected post keeps its body, author
//!   and the moderator's note; it just stops being visible — to the public and
//!   to moderators — so there is always a record of what was removed and why.
//! * **Roles are read, never written.** `news_moderator` is granted in the
//!   dashboard (it is a `ga_user_roles` row, the same mechanism as `spectator`),
//!   and this service only ever reads that table. That keeps the admin token out
//!   of the one container that faces the internet.
//! * **The posting mode flips per deployment.** `open` (default) or
//!   `allowlisted`, where only holders of `news_allowlisted` may submit — the
//!   switch to throw if bots ever find the form. `news_restricted` denies posting
//!   to a specific account regardless of mode.

pub mod access;
pub mod markdown;
pub mod pages;
pub mod store;

use std::fmt;

/// What kind of entry this is. Stored as text so a new kind needs no migration.
///
/// `Guide` is deliberately absent: guides get their own page later, and will be
/// surfaced in this feed as "posted a guide" without being a kind here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    News,
    Update,
    Article,
    Video,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::News, Kind::Update, Kind::Article, Kind::Video];

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "news" => Some(Kind::News),
            "update" => Some(Kind::Update),
            "article" => Some(Kind::Article),
            "video" => Some(Kind::Video),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::News => "news",
            Kind::Update => "update",
            Kind::Article => "article",
            Kind::Video => "video",
        }
    }

    /// What the feed shows next to the title, if there is one.
    pub fn label(self) -> &'static str {
        match self {
            Kind::News => "News",
            Kind::Update => "Server update",
            Kind::Article => "Article",
            Kind::Video => "Video",
        }
    }

    /// How the feed's header line reads: "Zaxik posted a video".
    pub fn phrase(self) -> &'static str {
        match self {
            Kind::News => "news",
            Kind::Update => "a server update",
            Kind::Article => "an article",
            Kind::Video => "a video",
        }
    }

    /// A video entry is links and nothing else.
    pub fn requires_link(self) -> bool {
        matches!(self, Kind::Video)
    }

    /// Everything except a video needs to say something in a body.
    pub fn requires_body(self) -> bool {
        !matches!(self, Kind::Video)
    }

    /// Whether the form should offer a title and an editor at all.
    pub fn has_editor(self) -> bool {
        self.requires_body()
    }
}

/// Moderation state. `Rejected` is the soft delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Approved,
    Rejected,
}

impl Status {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Status::Pending),
            "approved" => Some(Status::Approved),
            "rejected" => Some(Status::Rejected),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Approved => "approved",
            Status::Rejected => "rejected",
        }
    }
}

/// How posting is gated for this deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PostingMode {
    /// Anyone with an account may submit (still moderated).
    #[default]
    Open,
    /// Only holders of `news_allowlisted` may submit.
    Allowlisted,
}

impl PostingMode {
    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "open" => Ok(PostingMode::Open),
            "allowlisted" => Ok(PostingMode::Allowlisted),
            other => Err(ConfigError(format!(
                "unknown posting mode {other:?} (expected \"open\" or \"allowlisted\")"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PostingMode::Open => "open",
            PostingMode::Allowlisted => "allowlisted",
        }
    }
}

/// A configuration value that is wrong in a way only the operator can fix.
#[derive(Debug, Clone)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

pub const TITLE_MIN: usize = 3;
pub const TITLE_MAX: usize = 120;
pub const SUMMARY_MAX: usize = 300;
pub const BODY_MAX: usize = 20_000;
/// Slugs are for readers, not databases: long enough to read, short enough to fit.
const SLUG_MAX: usize = 60;

/// How many videos one post may embed. Two fit side by side on a wide screen;
/// six is already a scroll.
pub const MAX_VIDEOS: usize = 6;

/// Submissions allowed per account per `RATE_WINDOW`, rejected ones excluded.
pub const RATE_LIMIT: i64 = 5;
pub const RATE_WINDOW_SECS: i64 = 24 * 3600;

/// Why a draft was not accepted. Each variant is a sentence for the author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    UnknownKind,
    TitleLength,
    TitleControl,
    SummaryTooLong,
    BodyRequired,
    BodyTooLong,
    LinkRequired,
    LinkInvalid,
    TooManyVideos,
}

impl DraftError {
    pub fn message(self) -> &'static str {
        match self {
            DraftError::UnknownKind => "That entry type does not exist.",
            DraftError::TitleLength => "The title needs 3 to 120 characters.",
            DraftError::TitleControl => "The title contains characters that cannot be shown.",
            DraftError::SummaryTooLong => "The summary can be at most 300 characters.",
            DraftError::BodyRequired => "Write something in the body.",
            DraftError::BodyTooLong => "The body is too long (20,000 characters at most).",
            DraftError::LinkRequired => "A video entry needs a YouTube link.",
            DraftError::LinkInvalid => "That link is not a YouTube video address.",
            DraftError::TooManyVideos => "That is more videos than one post can hold.",
        }
    }
}

impl fmt::Display for DraftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for DraftError {}

/// A submission that passed validation, in the form the store writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    pub kind: Kind,
    pub title: String,
    pub summary: String,
    pub body_markdown: String,
    /// Rendered once, here, so what a moderator approves is exactly what is
    /// served later (`news::markdown` is the only thing that produces it).
    pub body_html: String,
    pub link: Option<String>,
    /// YouTube ids, in the order the author gave them. One for a video entry,
    /// none otherwise — but a list, because a single post may show several.
    pub videos: Vec<String>,
    /// Path fragment for `/news/{id}-{slug}`.
    pub slug: String,
}

/// Validate and normalise a submission. The returned value is what gets stored;
/// nothing downstream re-checks or trims.
pub fn validate(
    kind: &str,
    title: &str,
    summary: &str,
    body_markdown: &str,
    links: &[String],
) -> Result<Validated, DraftError> {
    let kind = Kind::parse(kind.trim()).ok_or(DraftError::UnknownKind)?;
    let links: Vec<&str> = links
        .iter()
        .map(|link| link.trim())
        .filter(|link| !link.is_empty())
        .collect();

    let summary = normalise_line(summary).ok_or(DraftError::SummaryTooLong)?;
    if summary.chars().count() > SUMMARY_MAX {
        return Err(DraftError::SummaryTooLong);
    }

    // A video entry is a set of links: the form asks for nothing else, and a
    // title would be invented rather than written.
    if kind.requires_link() {
        if links.is_empty() {
            return Err(DraftError::LinkRequired);
        }
        if links.len() > MAX_VIDEOS {
            return Err(DraftError::TooManyVideos);
        }
        let mut videos: Vec<String> = Vec::with_capacity(links.len());
        for link in &links {
            let id = youtube_id(link).ok_or(DraftError::LinkInvalid)?;
            // The same video twice in one post is a slip, not a choice.
            if !videos.contains(&id) {
                videos.push(id);
            }
        }
        return Ok(Validated {
            kind,
            title: String::new(),
            summary,
            body_markdown: String::new(),
            body_html: String::new(),
            link: videos
                .first()
                .map(|id| format!("https://www.youtube.com/watch?v={id}")),
            videos,
            // Stable and readable in a URL, though nothing links to it any more:
            // the feed is the page now.
            slug: "video".to_string(),
        });
    }

    let title = normalise_line(title).ok_or(DraftError::TitleControl)?;
    let title_len = title.chars().count();
    if !(TITLE_MIN..=TITLE_MAX).contains(&title_len) {
        return Err(DraftError::TitleLength);
    }

    let body_markdown = body_markdown.trim().to_string();
    if kind.requires_body() && body_markdown.is_empty() {
        return Err(DraftError::BodyRequired);
    }
    if body_markdown.chars().count() > BODY_MAX {
        return Err(DraftError::BodyTooLong);
    }

    // One outbound link is allowed, and only over https.
    let link = match links.first() {
        Some(value) => {
            if !is_https(value) {
                return Err(DraftError::LinkInvalid);
            }
            Some((*value).to_string())
        }
        None => None,
    };

    Ok(Validated {
        kind,
        slug: slugify(&title),
        title,
        summary,
        body_html: markdown::to_html(&body_markdown),
        body_markdown,
        link,
        videos: Vec::new(),
    })
}

/// One line, whitespace collapsed, control characters refused.
fn normalise_line(value: &str) -> Option<String> {
    let mut out = String::with_capacity(value.len());
    let mut pending_space = false;
    for ch in value.chars() {
        // Whitespace first: a title pasted with a line break should be
        // collapsed, not refused. Anything else a terminal can emit is not a
        // title.
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if ch.is_control() || ch == '\u{7f}' {
            return None;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }
    Some(out)
}

fn is_https(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    lowered.starts_with("https://") && lowered.len() > "https://".len()
}

/// `https://example.org/a?b` -> checked, lowercased host.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?; // drop userinfo
    let host = host.split(':').next()?; // drop port
    Some(host.to_ascii_lowercase())
}

/// YouTube id from the address forms people paste.
pub fn youtube_id(url: &str) -> Option<String> {
    if !is_https(url) {
        return None;
    }
    let host = host_of(url)?;
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    let rest = &url.split_once("://")?.1[url.split_once("://")?.1.find('/').unwrap_or(0)..];
    let path = rest.trim_start_matches('/');
    let mut segments = path.split(['/', '?', '#']);

    let candidate = match host.as_str() {
        "youtu.be" => segments.next()?.to_string(),
        "youtube.com"
        | "m.youtube.com"
        | "music.youtube.com"
        | "youtube-nocookie.com"
        | "m.youtube-nocookie.com" => match segments.next()? {
            "watch" => path
                .split(['?', '&'])
                .find_map(|pair| pair.strip_prefix("v="))?
                .to_string(),
            "embed" | "shorts" | "live" | "v" => segments.next()?.to_string(),
            _ => return None,
        },
        _ => return None,
    };

    let valid = (6..=20).contains(&candidate.len())
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    valid.then_some(candidate)
}

/// `HH:MM UTC` — for a sentence about when something becomes possible again.
pub fn clock_utc(ts: i64) -> String {
    let seconds = ts.rem_euclid(86_400);
    format!("{:02}:{:02} UTC", seconds / 3600, (seconds % 3600) / 60)
}

/// A URL-safe fragment of the title. Unused by any page now that the feed shows
/// entries in full, but stored with each post so a future detail view has one.
pub fn slugify(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut last_dash = true; // suppress a leading dash
    for ch in title.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash && matches!(ch, ' ' | '-' | '_' | '/' | '.' | ',') {
            out.push('-');
            last_dash = true;
        }
        if out.len() >= SLUG_MAX {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "post".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip() {
        for kind in Kind::ALL {
            assert_eq!(Kind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(Kind::parse("blog"), None);
    }

    #[test]
    fn a_valid_entry_normalises_and_renders() {
        let entry = validate(
            "news",
            "  Surfside   Atoll opens  ",
            "A new PvP map pool.",
            "**Live now**",
            &[],
        )
        .expect("valid");
        assert_eq!(entry.title, "Surfside Atoll opens");
        assert_eq!(entry.slug, "surfside-atoll-opens");
        assert_eq!(entry.body_html, "<p><strong>Live now</strong></p>");
        assert_eq!(entry.link, None);
        assert!(entry.videos.is_empty());
    }

    #[test]
    fn every_kind_has_a_phrase_for_the_feed_header() {
        assert_eq!(Kind::News.phrase(), "news");
        assert_eq!(Kind::Update.phrase(), "a server update");
        assert_eq!(Kind::Article.phrase(), "an article");
        assert_eq!(Kind::Video.phrase(), "a video");
        // Guides are not a kind here; they will arrive as their own page and be
        // surfaced in this feed separately.
        assert_eq!(Kind::parse("guide"), None);
    }

    #[test]
    fn titles_are_bounded_and_single_line() {
        assert_eq!(
            validate("news", "ab", "", "body", &[]),
            Err(DraftError::TitleLength)
        );
        let long = "x".repeat(TITLE_MAX + 1);
        assert_eq!(
            validate("news", &long, "", "body", &[]),
            Err(DraftError::TitleLength)
        );
        // A pasted line break is whitespace and collapses.
        let collapsed = validate("news", "ok\n  title", "", "body", &[]).expect("valid");
        assert_eq!(collapsed.title, "ok title");
        // An actual control character is refused rather than stored.
        assert_eq!(
            validate("news", "bad\u{7}title", "", "body", &[]),
            Err(DraftError::TitleControl)
        );
        assert_eq!(
            validate("news", "nul\u{0}byte", "", "body", &[]),
            Err(DraftError::TitleControl)
        );
    }

    #[test]
    fn written_entries_need_a_body_and_a_title() {
        assert_eq!(
            validate("article", "Some article", "", "   ", &[]),
            Err(DraftError::BodyRequired)
        );
        assert_eq!(
            validate("update", "", "", "body", &[]),
            Err(DraftError::TitleLength)
        );
    }

    #[test]
    fn a_video_needs_links_and_nothing_else() {
        // No title, no body: the form does not ask for them, and the feed shows
        // the author, the kind and the date.
        let entry = validate(
            "video",
            "",
            "",
            "",
            &["https://youtu.be/dQw4w9WgXcQ".to_string()],
        )
        .expect("valid");
        assert!(entry.title.is_empty(), "a title is never invented");
        assert!(entry.body_html.is_empty());
        assert_eq!(entry.videos, vec!["dQw4w9WgXcQ".to_string()]);
        assert_eq!(
            entry.link.as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );

        assert_eq!(
            validate("video", "", "", "", &[]),
            Err(DraftError::LinkRequired)
        );
        assert_eq!(
            validate("video", "", "", "", &["https://example.org/v".to_string()]),
            Err(DraftError::LinkInvalid)
        );
    }

    #[test]
    fn one_post_may_hold_several_videos() {
        let links = [
            "https://youtu.be/dQw4w9WgXcQ".to_string(),
            "https://www.youtube.com/watch?v=aaaaaaaaaaa".to_string(),
            // A repeat is dropped rather than shown twice.
            "https://youtu.be/dQw4w9WgXcQ".to_string(),
        ];
        let entry = validate("video", "", "", "", &links).expect("valid");
        assert_eq!(
            entry.videos,
            vec!["dQw4w9WgXcQ".to_string(), "aaaaaaaaaaa".to_string()]
        );

        let too_many: Vec<String> = (0..=MAX_VIDEOS)
            .map(|i| format!("https://youtu.be/video{i:04}"))
            .collect();
        assert_eq!(
            validate("video", "", "", "", &too_many),
            Err(DraftError::TooManyVideos)
        );
    }

    #[test]
    fn written_kinds_take_at_most_one_https_link() {
        assert_eq!(
            validate(
                "article",
                "An article",
                "",
                "body",
                &["http://plain.example".to_string()]
            ),
            Err(DraftError::LinkInvalid),
            "plain http is not accepted"
        );
        let entry = validate(
            "article",
            "An article",
            "",
            "body",
            &["https://example.org/a".to_string()],
        )
        .expect("valid");
        assert_eq!(entry.link.as_deref(), Some("https://example.org/a"));
        assert!(entry.videos.is_empty());
    }

    #[test]
    fn youtube_addresses_are_parsed_in_the_forms_people_paste() {
        for url in [
            "https://youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?v=dQw4w9WgXcQ&t=30s",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/embed/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ",
        ] {
            assert_eq!(
                youtube_id(url).as_deref(),
                Some("dQw4w9WgXcQ"),
                "should parse {url}"
            );
        }
        for url in [
            "https://example.org/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/",
            "https://www.youtube.com/watch",
            // Lookalike host: the check is on the whole host, not a substring.
            "https://youtube.com.evil.example/watch?v=dQw4w9WgXcQ",
            "http://youtu.be/dQw4w9WgXcQ",
            "https://youtu.be/../../etc/passwd",
        ] {
            assert_eq!(youtube_id(url), None, "should refuse {url}");
        }
    }

    #[test]
    fn slugs_are_short_ascii_and_never_empty() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  ---  "), "post");
        assert_eq!(slugify("Ünïcode déjà vu"), "ncode-dj-vu");
        assert!(slugify(&"word ".repeat(40)).len() <= SLUG_MAX);
        assert!(!slugify(&"a".repeat(200)).contains(' '));
    }

    #[test]
    fn summaries_are_optional_but_bounded() {
        let long = "s".repeat(SUMMARY_MAX + 1);
        assert_eq!(
            validate("news", "A title", &long, "body", &[]),
            Err(DraftError::SummaryTooLong)
        );
        assert!(validate("news", "A title", "", "body", &[]).is_ok());
    }

    #[test]
    fn posting_mode_refuses_typos_instead_of_guessing() {
        assert_eq!(PostingMode::parse("").expect("default"), PostingMode::Open);
        assert_eq!(PostingMode::parse("open").expect("open"), PostingMode::Open);
        assert_eq!(
            PostingMode::parse("Allowlisted").expect("case-insensitive"),
            PostingMode::Allowlisted
        );
        assert!(
            PostingMode::parse("opne").is_err(),
            "a typo must not open posting"
        );
    }
}
