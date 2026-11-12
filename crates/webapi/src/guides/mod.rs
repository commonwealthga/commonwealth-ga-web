//! Guides and builds: the site's long-form section.
//!
//! Two categories, one shape. A **guide** is written advice — a strategy, a
//! class in depth, a mode explained. A **build** is a skill tree screenshot with
//! the metadata that makes it findable: which class, which queues, and
//! optionally a link back to the Discord thread where it is discussed, because
//! that is where the community already posts them.
//!
//! They share authorship, moderation and the tiled list, so they share a table
//! and a status; what differs is which fields are required, which is decided in
//! [`validate`] rather than by either caller.
//!
//! Moderation is the news feed's, deliberately: same `news_moderator` role, same
//! pre-publication review, same soft-delete rejection. A moderator who can judge
//! a news post can judge a build.

pub mod images;
pub mod store;

use std::fmt;

use crate::news::markdown;
use crate::news::{host_of, is_https, normalise_line, slugify};

/// One clock for both sections: a limit that resets "at 14:00 UTC" must mean the
/// same time wherever it is mentioned.
pub use crate::news::clock_utc;

/// Which half of the section an entry belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Guide,
    Build,
}

impl Category {
    pub const ALL: [Category; 2] = [Category::Guide, Category::Build];

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "guide" => Some(Category::Guide),
            "build" => Some(Category::Build),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Category::Guide => "guide",
            Category::Build => "build",
        }
    }

    /// What one entry is called, for a heading or a card.
    pub fn label(self) -> &'static str {
        match self {
            Category::Guide => "guide",
            Category::Build => "build",
        }
    }

    pub fn plural(self) -> &'static str {
        match self {
            Category::Guide => "guides",
            Category::Build => "builds",
        }
    }

    /// The page this category lives on.
    pub fn path(self) -> &'static str {
        match self {
            Category::Guide => "/guides",
            Category::Build => "/builds",
        }
    }

    /// A build is a screenshot; without one there is nothing to look at, and a
    /// "build" that is only prose is a guide wearing the wrong category.
    pub fn requires_image(self) -> bool {
        matches!(self, Category::Build)
    }

    /// Tags are the guides' filter. Builds are filtered by class and queue
    /// instead, which are facts about the build rather than labels on it.
    pub fn uses_tags(self) -> bool {
        matches!(self, Category::Guide)
    }
}

/// The closed tag set. One word each, so a chip row stays readable, and small
/// enough that authors actually choose instead of inventing synonyms.
///
/// Deliberately absent: `loadout` (that is a build, and builds have their own
/// page) and `map` (folded into `strategy`; callout posts are rare enough that a
/// tag of their own would sit empty).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tag {
    Basics,
    Strategy,
    Class,
    Mode,
    Pve,
    Pvp,
}

impl Tag {
    pub const ALL: [Tag; 6] = [
        Tag::Basics,
        Tag::Strategy,
        Tag::Class,
        Tag::Mode,
        Tag::Pve,
        Tag::Pvp,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "basics" => Some(Tag::Basics),
            "strategy" => Some(Tag::Strategy),
            "class" => Some(Tag::Class),
            "mode" => Some(Tag::Mode),
            "pve" => Some(Tag::Pve),
            "pvp" => Some(Tag::Pvp),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Tag::Basics => "basics",
            Tag::Strategy => "strategy",
            Tag::Class => "class",
            Tag::Mode => "mode",
            Tag::Pve => "pve",
            Tag::Pvp => "pvp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tag::Basics => "Basics",
            Tag::Strategy => "Strategy",
            Tag::Class => "Class",
            Tag::Mode => "Mode",
            Tag::Pve => "PvE",
            Tag::Pvp => "PvP",
        }
    }
}

/// The four classes. Exactly one per build: a tree is spent on one agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Assault,
    Medic,
    Recon,
    Robotic,
}

impl Class {
    pub const ALL: [Class; 4] = [Class::Assault, Class::Medic, Class::Recon, Class::Robotic];

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "assault" => Some(Class::Assault),
            "medic" => Some(Class::Medic),
            "recon" => Some(Class::Recon),
            "robotic" => Some(Class::Robotic),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Class::Assault => "assault",
            Class::Medic => "medic",
            Class::Recon => "recon",
            Class::Robotic => "robotic",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Class::Assault => "Assault",
            Class::Medic => "Medic",
            Class::Recon => "Recon",
            Class::Robotic => "Robotic",
        }
    }
}

/// Which queues a build is for. Both may apply to one build: the same tree
/// often serves a mercenary queue and a mission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pvp,
    Pve,
}

impl Mode {
    pub const ALL: [Mode; 2] = [Mode::Pvp, Mode::Pve];

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "pvp" => Some(Mode::Pvp),
            "pve" => Some(Mode::Pve),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Pvp => "pvp",
            Mode::Pve => "pve",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Pvp => "PvP",
            Mode::Pve => "PvE",
        }
    }
}

/// How a list is ordered. "Helpful" is the default: the point of thumbs is that
/// the good ones float, rather than being buried by whoever posted most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Helpful,
    Newest,
}

impl Sort {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "newest" => Some(Sort::Newest),
            "helpful" | "" => Some(Sort::Helpful),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Sort::Helpful => "helpful",
            Sort::Newest => "newest",
        }
    }
}

pub const TITLE_MIN: usize = 3;
pub const TITLE_MAX: usize = 120;
pub const SUMMARY_MAX: usize = 300;
/// Guides carry the writing, so they get the news body limit.
pub const BODY_MAX: usize = 20_000;
/// A build's description is "suggested gear, general strategy" — a note beside
/// a screenshot, not an article.
pub const DESCRIPTION_MAX: usize = 4_000;

/// Submissions allowed per account per window, rejected ones excluded, the same
/// arithmetic as the news feed.
pub const RATE_LIMIT: i64 = 5;
pub const RATE_WINDOW_SECS: i64 = 24 * 3600;

/// Why a draft was not accepted. Each variant is a sentence for the author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    UnknownCategory,
    TitleLength,
    TitleControl,
    SummaryTooLong,
    BodyRequired,
    BodyTooLong,
    DescriptionTooLong,
    TagsRequired,
    UnknownTag,
    ImageRequired,
    ClassRequired,
    UnknownClass,
    ModeRequired,
    UnknownMode,
    DiscordInvalid,
}

impl DraftError {
    pub fn message(self) -> &'static str {
        match self {
            DraftError::UnknownCategory => "That is not a kind of entry this section holds.",
            DraftError::TitleLength => "The title needs 3 to 120 characters.",
            DraftError::TitleControl => "The title contains characters that cannot be shown.",
            DraftError::SummaryTooLong => "The summary can be at most 300 characters.",
            DraftError::BodyRequired => "Write the guide before submitting it.",
            DraftError::BodyTooLong => "The guide is too long (20,000 characters at most).",
            DraftError::DescriptionTooLong => {
                "The description is too long (4,000 characters at most)."
            }
            DraftError::TagsRequired => "Pick at least one tag, so people can find it.",
            DraftError::UnknownTag => "One of those tags does not exist.",
            DraftError::ImageRequired => "A build needs its skill tree screenshot.",
            DraftError::ClassRequired => "Pick the class this build is for.",
            DraftError::UnknownClass => "That class does not exist.",
            DraftError::ModeRequired => "Pick at least one of PvP or PvE.",
            DraftError::UnknownMode => "That mode does not exist.",
            DraftError::DiscordInvalid => {
                "That is not a Discord thread link — copy it from the thread itself."
            }
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
    pub category: Category,
    pub title: String,
    pub summary: String,
    pub body_markdown: String,
    /// Rendered once, here, so what a moderator approves is what is served
    /// later — the same restricted renderer the news body uses.
    pub body_html: String,
    /// Guides only, at least one. Builds have none.
    pub tags: Vec<Tag>,
    /// Builds only: the one class the tree is spent on.
    pub build_class: Option<Class>,
    /// Builds only: the queues it was made for.
    pub modes: Vec<Mode>,
    /// Optional, builds only: the Discord thread where it is discussed.
    pub discord_url: Option<String>,
    /// Builds only: the stored, already-sanitised screenshot.
    pub image_id: Option<i64>,
    /// Path fragment for `/guides/{id}-{slug}`.
    pub slug: String,
}

/// Validate and normalise a submission. The returned value is what gets stored;
/// nothing downstream re-checks or trims.
#[allow(clippy::too_many_arguments)]
pub fn validate(
    category: &str,
    title: &str,
    summary: &str,
    body: &str,
    tags: &[String],
    build_class: &str,
    modes: &[String],
    discord_url: &str,
    image_id: Option<i64>,
) -> Result<Validated, DraftError> {
    let category = Category::parse(category).ok_or(DraftError::UnknownCategory)?;

    let title = normalise_line(title).ok_or(DraftError::TitleControl)?;
    if !(TITLE_MIN..=TITLE_MAX).contains(&title.chars().count()) {
        return Err(DraftError::TitleLength);
    }

    let summary = normalise_line(summary).ok_or(DraftError::SummaryTooLong)?;
    if summary.chars().count() > SUMMARY_MAX {
        return Err(DraftError::SummaryTooLong);
    }

    // The body limit differs by category: a guide is the article, a build's
    // description is a note beside a picture.
    let body = body.trim();
    let limit = match category {
        Category::Guide => BODY_MAX,
        Category::Build => DESCRIPTION_MAX,
    };
    if body.chars().count() > limit {
        return Err(match category {
            Category::Guide => DraftError::BodyTooLong,
            Category::Build => DraftError::DescriptionTooLong,
        });
    }

    let mut parsed_tags = Vec::new();
    let mut parsed_modes = Vec::new();
    let mut class = None;
    let mut image = None;
    let mut discord = None;

    match category {
        Category::Guide => {
            if body.is_empty() {
                return Err(DraftError::BodyRequired);
            }
            for raw in tags {
                let tag = Tag::parse(raw).ok_or(DraftError::UnknownTag)?;
                if !parsed_tags.contains(&tag) {
                    parsed_tags.push(tag);
                }
            }
            if parsed_tags.is_empty() {
                return Err(DraftError::TagsRequired);
            }
        }
        Category::Build => {
            image = image_id.ok_or(DraftError::ImageRequired).map(Some)?;
            if build_class.trim().is_empty() {
                return Err(DraftError::ClassRequired);
            }
            class = Some(Class::parse(build_class).ok_or(DraftError::UnknownClass)?);
            for raw in modes {
                let mode = Mode::parse(raw).ok_or(DraftError::UnknownMode)?;
                if !parsed_modes.contains(&mode) {
                    parsed_modes.push(mode);
                }
            }
            if parsed_modes.is_empty() {
                return Err(DraftError::ModeRequired);
            }
            discord = discord_thread(discord_url)?;
        }
    }

    // Tags only order the guides list; a build carrying them would show up under
    // filters that do not describe it.
    let summary = if category == Category::Build {
        String::new()
    } else {
        summary
    };

    Ok(Validated {
        category,
        slug: slugify(&title),
        title,
        summary,
        body_markdown: body.to_string(),
        body_html: if body.is_empty() {
            String::new()
        } else {
            markdown::to_html(body)
        },
        tags: parsed_tags,
        build_class: class,
        modes: parsed_modes,
        discord_url: discord,
        image_id: image,
    })
}

/// A Discord thread or message link, or nothing if the field is empty.
///
/// Only `discord.com/channels/<guild>/<channel>[/<message>]` — the address a
/// "copy link" gives you on a thread. Invites are refused: they expire, and an
/// expired link on an approved build is worse than no link.
fn discord_thread(value: &str) -> Result<Option<String>, DraftError> {
    let value = normalise_line(value).ok_or(DraftError::DiscordInvalid)?;
    if value.is_empty() {
        return Ok(None);
    }
    if !is_https(&value) {
        return Err(DraftError::DiscordInvalid);
    }

    const HOSTS: [&str; 4] = [
        "discord.com",
        "ptb.discord.com",
        "canary.discord.com",
        "discordapp.com",
    ];
    let host = host_of(&value).ok_or(DraftError::DiscordInvalid)?;
    if !HOSTS.contains(&host.as_str()) {
        return Err(DraftError::DiscordInvalid);
    }

    let path = value
        .split_once("://")
        .and_then(|(_, rest)| rest.split_once('/'))
        .map(|(_, path)| path)
        .ok_or(DraftError::DiscordInvalid)?;
    let path = path.split(['?', '#']).next().unwrap_or_default();

    let mut segments = path.split('/');
    if segments.next() != Some("channels") {
        return Err(DraftError::DiscordInvalid);
    }
    let mut ids = 0;
    for segment in segments.filter(|segment| !segment.is_empty()) {
        if !segment.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(DraftError::DiscordInvalid);
        }
        ids += 1;
    }
    // guild/channel, and optionally the message inside it.
    if !(2..=3).contains(&ids) {
        return Err(DraftError::DiscordInvalid);
    }

    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guide(title: &str, body: &str, tags: &[&str]) -> Result<Validated, DraftError> {
        let tags: Vec<String> = tags.iter().map(|tag| tag.to_string()).collect();
        validate("guide", title, "", body, &tags, "", &[], "", None)
    }

    #[test]
    fn a_guide_needs_a_body_and_a_tag() {
        assert_eq!(
            guide("Assault basics", "", &["basics"]),
            Err(DraftError::BodyRequired)
        );
        assert_eq!(
            guide("Assault basics", "Body text", &[]),
            Err(DraftError::TagsRequired)
        );
    }

    #[test]
    fn tags_are_deduplicated_and_ordered_as_given() {
        let entry = guide(
            "Assault basics",
            "Body text",
            &["strategy", "basics", "strategy"],
        )
        .expect("valid");
        assert_eq!(entry.tags, vec![Tag::Strategy, Tag::Basics]);
        assert_eq!(entry.slug, "assault-basics");
    }

    #[test]
    fn an_unknown_tag_is_refused_rather_than_dropped() {
        assert_eq!(
            guide("Assault basics", "Body text", &["maps"]),
            Err(DraftError::UnknownTag)
        );
    }

    #[test]
    fn a_build_needs_an_image_a_class_and_a_mode() {
        let modes = vec!["pvp".to_string()];
        assert_eq!(
            validate(
                "build",
                "Burst",
                "",
                "notes",
                &[],
                "assault",
                &modes,
                "",
                None
            ),
            Err(DraftError::ImageRequired)
        );
        assert_eq!(
            validate("build", "Burst", "", "notes", &[], "", &modes, "", Some(1)),
            Err(DraftError::ClassRequired)
        );
        assert_eq!(
            validate(
                "build",
                "Burst",
                "",
                "notes",
                &[],
                "assault",
                &[],
                "",
                Some(1)
            ),
            Err(DraftError::ModeRequired)
        );
    }

    #[test]
    fn a_build_may_be_for_both_queues_and_keeps_its_description() {
        let modes = vec!["pvp".to_string(), "pve".to_string()];
        let entry = validate(
            "build",
            "Support medic",
            "",
            "Bring two heals.",
            &[],
            "medic",
            &modes,
            "",
            Some(7),
        )
        .expect("valid");
        assert_eq!(entry.modes, vec![Mode::Pvp, Mode::Pve]);
        assert_eq!(entry.build_class, Some(Class::Medic));
        assert_eq!(entry.body_markdown, "Bring two heals.");
        assert!(entry.summary.is_empty());
        assert_eq!(entry.image_id, Some(7));
    }

    #[test]
    fn a_discord_thread_link_is_accepted_and_an_invite_is_not() {
        let modes = vec!["pvp".to_string()];
        let good = validate(
            "build",
            "Support medic",
            "",
            "",
            &[],
            "medic",
            &modes,
            "https://discord.com/channels/123456789/987654321/555",
            Some(1),
        )
        .expect("valid");
        assert_eq!(
            good.discord_url.as_deref(),
            Some("https://discord.com/channels/123456789/987654321/555")
        );

        for bad in [
            "https://discord.gg/abcd",
            "https://example.com/channels/1/2",
            "http://discord.com/channels/1/2",
            "https://discord.com/channels/abc/2",
        ] {
            assert_eq!(
                validate(
                    "build",
                    "Support medic",
                    "",
                    "",
                    &[],
                    "medic",
                    &modes,
                    bad,
                    Some(1)
                ),
                Err(DraftError::DiscordInvalid),
                "{bad}"
            );
        }
    }

    #[test]
    fn an_empty_discord_field_means_no_link() {
        let modes = vec!["pve".to_string()];
        let entry = validate(
            "build",
            "Solo tree",
            "",
            "",
            &[],
            "recon",
            &modes,
            "   ",
            Some(2),
        )
        .expect("valid");
        assert_eq!(entry.discord_url, None);
    }
}
