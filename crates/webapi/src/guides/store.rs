//! Storage for guides and builds, in `site.db`.
//!
//! Five tables, and the same moderation shape as the news feed on purpose:
//!
//! * `guides` holds every submission, including rejected ones. Rejection sets a
//!   status and a note; the row stays, because a moderator reviewing a pattern of
//!   abuse needs the content and the author may appeal.
//! * `guide_tags` and `guide_modes` are the multi-valued facts — a guide's tags,
//!   a build's queues — in their own tables, so a filter is an index lookup
//!   rather than a `LIKE` over a comma-separated string.
//! * `guide_reactions` is one row per account per entry, so a thumbs-up is a fact
//!   rather than a counter that doubles when you click twice.
//! * `guide_images` records the derivative that was stored: its hash, size and
//!   dimensions. The bytes live on disk under that hash and nowhere else.

use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use super::{Category, Class, Mode, Sort, Tag, Validated};

/// How long a moderation note may be. Long enough for a real explanation.
pub const NOTE_MAX: usize = 500;

/// One entry, as the API and the pages need it.
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: i64,
    pub category: Category,
    pub title: String,
    pub slug: String,
    pub summary: String,
    pub body_markdown: String,
    pub body_html: String,
    pub status: &'static str,
    pub author_id: i64,
    pub author_name: String,
    pub created_at: i64,
    pub reviewed_by_name: Option<String>,
    pub reviewed_at: Option<i64>,
    pub moderation_note: Option<String>,
    /// Builds only, and the id points at a row in `guide_images`.
    pub image_id: Option<i64>,
    /// The stored file's name on disk (`<hash>.webp`), when the entry has one.
    /// Selected with the row so a list of twenty builds is one query.
    pub image_hash: Option<String>,
    pub build_class: Option<Class>,
    pub discord_url: Option<String>,
    pub tags: Vec<Tag>,
    pub modes: Vec<Mode>,
    pub thumbs_up: i64,
}

impl Entry {
    /// The canonical path of an entry. The id makes it stable if the title is
    /// edited later.
    pub fn path(&self) -> String {
        format!("{}/{}-{}", self.category.path(), self.id, self.slug)
    }
}

const SELECT: &str = "SELECT g.id, g.category, g.title, g.slug, g.summary, g.body_markdown,
                             g.body_html, g.status, g.author_id, g.author_name, g.created_at,
                             g.reviewed_by_name, g.reviewed_at, g.moderation_note,
                             g.image_id,
                             (SELECT i.hash FROM guide_images i WHERE i.id = g.image_id)
                                AS image_hash,
                             g.build_class, g.discord_url,
                             (SELECT GROUP_CONCAT(tag, char(31)) FROM (
                                  SELECT tag FROM guide_tags t
                                  WHERE t.guide_id = g.id ORDER BY tag
                              )) AS tags,
                             (SELECT GROUP_CONCAT(mode, char(31)) FROM (
                                  SELECT mode FROM guide_modes m
                                  WHERE m.guide_id = g.id ORDER BY mode
                              )) AS modes,
                             (SELECT COUNT(*) FROM guide_reactions r WHERE r.guide_id = g.id)
                                AS thumbs_up
                      FROM guides g";

pub async fn migrate(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS guides (
             id                INTEGER PRIMARY KEY AUTOINCREMENT,
             category          TEXT    NOT NULL,
             title             TEXT    NOT NULL,
             slug              TEXT    NOT NULL,
             summary           TEXT    NOT NULL DEFAULT '',
             body_markdown     TEXT    NOT NULL DEFAULT '',
             body_html         TEXT    NOT NULL DEFAULT '',
             status            TEXT    NOT NULL,
             author_id         INTEGER NOT NULL,
             author_name       TEXT    NOT NULL,
             created_at        INTEGER NOT NULL,
             reviewed_by_id    INTEGER,
             reviewed_by_name  TEXT,
             reviewed_at       INTEGER,
             moderation_note   TEXT,
             image_id          INTEGER REFERENCES guide_images(id),
             build_class       TEXT,
             discord_url       TEXT
         )",
    )
    .execute(pool)
    .await?;

    // The list is always "approved, this category, in this order": both orders
    // end at `created_at`, so one index serves them.
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_guides_category_status
             ON guides(category, status, created_at DESC)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_guides_author
             ON guides(author_id, created_at DESC)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS guide_tags (
             guide_id INTEGER NOT NULL REFERENCES guides(id),
             tag      TEXT    NOT NULL,
             PRIMARY KEY (guide_id, tag)
         )",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_guide_tags_tag ON guide_tags(tag, guide_id)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS guide_modes (
             guide_id INTEGER NOT NULL REFERENCES guides(id),
             mode     TEXT    NOT NULL,
             PRIMARY KEY (guide_id, mode)
         )",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_guide_modes_mode ON guide_modes(mode, guide_id)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS guide_reactions (
             guide_id   INTEGER NOT NULL REFERENCES guides(id),
             user_id    INTEGER NOT NULL,
             created_at INTEGER NOT NULL,
             PRIMARY KEY (guide_id, user_id)
         )",
    )
    .execute(pool)
    .await?;

    // What was stored, not what was uploaded: the bytes are on disk under this
    // hash, and the original was never written anywhere.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS guide_images (
             id         INTEGER PRIMARY KEY AUTOINCREMENT,
             hash       TEXT    NOT NULL UNIQUE,
             width      INTEGER NOT NULL,
             height     INTEGER NOT NULL,
             bytes      INTEGER NOT NULL,
             author_id  INTEGER NOT NULL,
             created_at INTEGER NOT NULL
         )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_guide_images_author
             ON guide_images(author_id, created_at DESC)",
    )
    .execute(pool)
    .await?;

    // One announcement per guide in the news feed, at most. Created on approval
    // and removed on rejection or unpublishing, so the feed cannot point at a
    // page that is not there.
    Ok(())
}

fn split_codes(value: Option<String>) -> Vec<String> {
    value
        .map(|joined| {
            joined
                .split('\u{1f}')
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn from_row(row: &sqlx::sqlite::SqliteRow) -> Entry {
    Entry {
        id: row.get("id"),
        category: Category::parse(row.get::<String, _>("category").as_str())
            .unwrap_or(Category::Guide),
        title: row.get("title"),
        slug: row.get("slug"),
        summary: row.get("summary"),
        body_markdown: row.get("body_markdown"),
        body_html: row.get("body_html"),
        status: match row.get::<String, _>("status").as_str() {
            "approved" => "approved",
            "rejected" => "rejected",
            _ => "pending",
        },
        author_id: row.get("author_id"),
        author_name: row.get("author_name"),
        created_at: row.get("created_at"),
        reviewed_by_name: row.get("reviewed_by_name"),
        reviewed_at: row.get("reviewed_at"),
        moderation_note: row.get("moderation_note"),
        image_id: row.get("image_id"),
        image_hash: row.get("image_hash"),
        build_class: row
            .get::<Option<String>, _>("build_class")
            .and_then(|value| Class::parse(&value)),
        discord_url: row.get("discord_url"),
        tags: split_codes(row.get("tags"))
            .iter()
            .filter_map(|value| Tag::parse(value))
            .collect(),
        modes: split_codes(row.get("modes"))
            .iter()
            .filter_map(|value| Mode::parse(value))
            .collect(),
        thumbs_up: row.get("thumbs_up"),
    }
}

/// Store a submission as pending. Returns the new id.
pub async fn create(
    pool: &SqlitePool,
    author_id: i64,
    author_name: &str,
    entry: &Validated,
    now: i64,
) -> sqlx::Result<i64> {
    let result = sqlx::query(
        "INSERT INTO guides (category, title, slug, summary, body_markdown, body_html,
                             status, author_id, author_name, created_at,
                             image_id, build_class, discord_url)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.category.as_str())
    .bind(&entry.title)
    .bind(&entry.slug)
    .bind(&entry.summary)
    .bind(&entry.body_markdown)
    .bind(&entry.body_html)
    .bind("pending")
    .bind(author_id)
    .bind(author_name)
    .bind(now)
    .bind(entry.image_id)
    .bind(entry.build_class.map(|class| class.as_str()))
    .bind(&entry.discord_url)
    .execute(pool)
    .await?;

    let id = result.last_insert_rowid();
    attach_facets(pool, id, entry).await?;
    Ok(id)
}

/// Tags and modes, in the author's order.
pub async fn attach_facets(
    pool: &SqlitePool,
    guide_id: i64,
    entry: &Validated,
) -> sqlx::Result<()> {
    for tag in &entry.tags {
        sqlx::query("INSERT OR IGNORE INTO guide_tags (guide_id, tag) VALUES (?, ?)")
            .bind(guide_id)
            .bind(tag.as_str())
            .execute(pool)
            .await?;
    }
    for mode in &entry.modes {
        sqlx::query("INSERT OR IGNORE INTO guide_modes (guide_id, mode) VALUES (?, ?)")
            .bind(guide_id)
            .bind(mode.as_str())
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// What one list request asks for. Every field is optional: no tag means all
/// tags, an empty query means no search.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub tag: Option<Tag>,
    pub class: Option<Class>,
    pub mode: Option<Mode>,
    pub query: String,
    pub sort: Option<Sort>,
}

impl Filter {
    /// `%` and `_` mean something to `LIKE`; a search for "100%" should look for
    /// those characters, not for everything.
    fn pattern(&self) -> Option<String> {
        let query = self.query.trim().to_lowercase();
        if query.is_empty() {
            return None;
        }
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        Some(format!("%{escaped}%"))
    }
}

fn push_conditions<'a>(
    builder: &mut QueryBuilder<'a, Sqlite>,
    category: Category,
    filter: &'a Filter,
    status: &'a str,
) {
    builder
        .push(" WHERE g.category = ")
        .push_bind(category.as_str());
    builder.push(" AND g.status = ").push_bind(status);

    if let Some(tag) = filter.tag {
        builder
            .push(" AND EXISTS (SELECT 1 FROM guide_tags t WHERE t.guide_id = g.id AND t.tag = ")
            .push_bind(tag.as_str())
            .push(")");
    }
    if let Some(class) = filter.class {
        builder
            .push(" AND g.build_class = ")
            .push_bind(class.as_str());
    }
    if let Some(mode) = filter.mode {
        builder
            .push(" AND EXISTS (SELECT 1 FROM guide_modes m WHERE m.guide_id = g.id AND m.mode = ")
            .push_bind(mode.as_str())
            .push(")");
    }
    if let Some(pattern) = filter.pattern() {
        builder
            .push(" AND (lower(g.title) LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR lower(g.summary) LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR lower(g.body_markdown) LIKE ")
            .push_bind(pattern)
            .push(" ESCAPE '\\')");
    }
}

/// The approved entries of one category, filtered, in the requested order.
///
/// "Most helpful" is thumbs first, then newest: a tie at zero thumbs (everything
/// new) falls back to what a reader expects, not to insertion order.
pub async fn feed(
    pool: &SqlitePool,
    category: Category,
    filter: &Filter,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<Entry>> {
    let mut builder = QueryBuilder::new(SELECT);
    push_conditions(&mut builder, category, filter, "approved");
    match filter.sort.unwrap_or(Sort::Helpful) {
        Sort::Helpful => {
            builder.push(" ORDER BY thumbs_up DESC, g.created_at DESC, g.id DESC");
        }
        Sort::Newest => {
            builder.push(" ORDER BY g.created_at DESC, g.id DESC");
        }
    }
    builder.push(" LIMIT ").push_bind(limit);
    builder.push(" OFFSET ").push_bind(offset);

    let rows = builder.build().fetch_all(pool).await?;
    Ok(rows.iter().map(from_row).collect())
}

/// How many approved entries match, so the page knows when to stop asking.
pub async fn count_feed(
    pool: &SqlitePool,
    category: Category,
    filter: &Filter,
) -> sqlx::Result<i64> {
    let mut builder = QueryBuilder::new("SELECT COUNT(*) FROM guides g");
    push_conditions(&mut builder, category, filter, "approved");
    builder.build_query_scalar::<i64>().fetch_one(pool).await
}

/// Everything awaiting review, newest first. Moderators see both categories at
/// once: the decision is the same, and one queue is one place to look.
pub async fn pending(pool: &SqlitePool) -> sqlx::Result<Vec<Entry>> {
    let rows = sqlx::query(&format!(
        "{SELECT} WHERE g.status = 'pending' ORDER BY g.created_at ASC"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(from_row).collect())
}

/// One entry that readers may see: approved, and of this category.
pub async fn approved_by_id(
    pool: &SqlitePool,
    id: i64,
    category: Category,
) -> sqlx::Result<Option<Entry>> {
    let rows = sqlx::query(&format!(
        "{SELECT} WHERE g.id = ? AND g.category = ? AND g.status = 'approved'"
    ))
    .bind(id)
    .bind(category.as_str())
    .fetch_all(pool)
    .await?;
    Ok(rows.first().map(from_row))
}

/// One entry whatever its status — for moderators and its author.
pub async fn by_id(pool: &SqlitePool, id: i64) -> sqlx::Result<Option<Entry>> {
    let rows = sqlx::query(&format!("{SELECT} WHERE g.id = ?"))
        .bind(id)
        .fetch_all(pool)
        .await?;
    Ok(rows.first().map(from_row))
}

/// The author's own entries still in the queue, so they can see and fix them.
pub async fn pending_by_author(pool: &SqlitePool, author_id: i64) -> sqlx::Result<Vec<Entry>> {
    let rows = sqlx::query(&format!(
        "{SELECT} WHERE g.author_id = ? AND g.status = 'pending' ORDER BY g.created_at DESC"
    ))
    .bind(author_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(from_row).collect())
}

/// Everything approved, for the sitemap.
pub async fn all_approved(pool: &SqlitePool) -> sqlx::Result<Vec<Entry>> {
    let rows = sqlx::query(&format!(
        "{SELECT} WHERE g.status = 'approved' ORDER BY g.created_at DESC"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(from_row).collect())
}

/// Submissions by this account inside the window, not counting rejected ones —
/// the same arithmetic as the news feed, for the same reason.
pub async fn submissions_since(pool: &SqlitePool, author_id: i64, since: i64) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM guides
         WHERE author_id = ? AND created_at >= ? AND status != 'rejected'",
    )
    .bind(author_id)
    .bind(since)
    .fetch_one(pool)
    .await
}

/// Record a stored image and return its id. The same screenshot twice is one
/// row: the hash is unique, so a re-upload finds the existing record.
pub async fn create_image(
    pool: &SqlitePool,
    hash: &str,
    width: u32,
    height: u32,
    bytes: usize,
    author_id: i64,
    now: i64,
) -> sqlx::Result<i64> {
    sqlx::query(
        "INSERT INTO guide_images (hash, width, height, bytes, author_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(hash) DO UPDATE SET hash = hash",
    )
    .bind(hash)
    .bind(i64::from(width))
    .bind(i64::from(height))
    .bind(bytes as i64)
    .bind(author_id)
    .bind(now)
    .execute(pool)
    .await?;

    sqlx::query_scalar("SELECT id FROM guide_images WHERE hash = ?")
        .bind(hash)
        .fetch_one(pool)
        .await
}

/// Uploads by this account inside the window, so the disk is not free space.
pub async fn images_since(pool: &SqlitePool, author_id: i64, since: i64) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM guide_images WHERE author_id = ? AND created_at >= ?")
        .bind(author_id)
        .bind(since)
        .fetch_one(pool)
        .await
}

/// Who uploaded this image. A submission may only reference its own author's
/// uploads: an id guessed from the sequence would otherwise let one account
/// attach another's screenshot, or one that was never sanitised by them.
pub async fn image_owner(pool: &SqlitePool, id: i64) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT author_id FROM guide_images WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// An image record, for serving: the hash is the file name on disk.
pub async fn image_hash(pool: &SqlitePool, id: i64) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT hash FROM guide_images WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Approve. Clears any note: the request it described has been satisfied, and a
/// stale "please fix X" on a published page reads as a mistake.
pub async fn approve(
    pool: &SqlitePool,
    id: i64,
    reviewer_id: i64,
    reviewer_name: &str,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE guides
         SET status = 'approved', reviewed_by_id = ?, reviewed_by_name = ?, reviewed_at = ?,
             moderation_note = NULL
         WHERE id = ?",
    )
    .bind(reviewer_id)
    .bind(reviewer_name)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Reject: a soft delete. The row and its image stay for the record.
pub async fn reject(
    pool: &SqlitePool,
    id: i64,
    reviewer_id: i64,
    reviewer_name: &str,
    note: &str,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE guides
         SET status = 'rejected', reviewed_by_id = ?, reviewed_by_name = ?, reviewed_at = ?,
             moderation_note = ?
         WHERE id = ?",
    )
    .bind(reviewer_id)
    .bind(reviewer_name)
    .bind(now)
    .bind(note)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Unpublish: back to the queue with a note saying what to fix.
pub async fn unapprove(
    pool: &SqlitePool,
    id: i64,
    reviewer_id: i64,
    reviewer_name: &str,
    note: &str,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE guides
         SET status = 'pending', reviewed_by_id = ?, reviewed_by_name = ?, reviewed_at = ?,
             moderation_note = ?
         WHERE id = ?",
    )
    .bind(reviewer_id)
    .bind(reviewer_name)
    .bind(now)
    .bind(note)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Request changes without deciding: the entry stays pending, carrying a note.
pub async fn annotate(pool: &SqlitePool, id: i64, note: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE guides SET moderation_note = ? WHERE id = ? AND status = 'pending'")
        .bind(note)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// An author's edit, while the entry is still pending. Status, author and
/// timestamps are untouched: editing is not moderating.
pub async fn update_pending(
    pool: &SqlitePool,
    id: i64,
    author_id: i64,
    entry: &Validated,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE guides
         SET title = ?, slug = ?, summary = ?, body_markdown = ?, body_html = ?,
             image_id = ?, build_class = ?, discord_url = ?
         WHERE id = ? AND author_id = ? AND status = 'pending'",
    )
    .bind(&entry.title)
    .bind(&entry.slug)
    .bind(&entry.summary)
    .bind(&entry.body_markdown)
    .bind(&entry.body_html)
    .bind(entry.image_id)
    .bind(entry.build_class.map(|class| class.as_str()))
    .bind(&entry.discord_url)
    .bind(id)
    .bind(author_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(false);
    }

    // Facets are replaced wholesale: an edit that removes a tag should remove it,
    // not leave the old one behind.
    sqlx::query("DELETE FROM guide_tags WHERE guide_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM guide_modes WHERE guide_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    attach_facets(pool, id, entry).await?;
    Ok(true)
}

/// A moderation note: required, and short enough to be read in a queue row.
pub fn validate_note(note: &str) -> Result<String, &'static str> {
    let trimmed = note.trim();
    if trimmed.is_empty() {
        return Err("A note explaining the decision is required.");
    }
    if trimmed.chars().count() > NOTE_MAX {
        return Err("The note is too long (500 characters at most).");
    }
    Ok(trimmed.to_string())
}

/// Set or clear one account's thumbs-up. Returns the new count.
pub async fn set_reaction(
    pool: &SqlitePool,
    guide_id: i64,
    user_id: i64,
    active: bool,
    now: i64,
) -> sqlx::Result<i64> {
    if active {
        sqlx::query(
            "INSERT OR IGNORE INTO guide_reactions (guide_id, user_id, created_at)
             VALUES (?, ?, ?)",
        )
        .bind(guide_id)
        .bind(user_id)
        .bind(now)
        .execute(pool)
        .await?;
    } else {
        sqlx::query("DELETE FROM guide_reactions WHERE guide_id = ? AND user_id = ?")
            .bind(guide_id)
            .bind(user_id)
            .execute(pool)
            .await?;
    }
    thumb_count(pool, guide_id).await
}

pub async fn has_reacted(pool: &SqlitePool, guide_id: i64, user_id: i64) -> sqlx::Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM guide_reactions WHERE guide_id = ? AND user_id = ?",
    )
    .bind(guide_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

pub async fn thumb_count(pool: &SqlitePool, guide_id: i64) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM guide_reactions WHERE guide_id = ?")
        .bind(guide_id)
        .fetch_one(pool)
        .await
}
