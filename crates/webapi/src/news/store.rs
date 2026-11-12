//! Storage for the news feed, in `site.db` — the website's own writable database.
//!
//! Three tables, and the shape of the moderation record is the point:
//!
//! * `news_posts` keeps every submission, including rejected ones. Rejection
//!   sets a status and a note; it never removes the row. A moderator reviewing a
//!   pattern of abuse needs the removed content, and so does the author's appeal.
//! * `news_reactions` is one row per account per post, so a thumbs-up is a fact
//!   rather than a counter that can be inflated by clicking twice.
//! * `news_audit` records every moderation action with who, when and why, for the
//!   cases where the post itself is gone from view.

use sqlx::{Row, SqlitePool};

use super::{Kind, Status, Validated};

/// How long a moderation note may be. Long enough for a real explanation.
pub const NOTE_MAX: usize = 500;

pub async fn migrate(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS news_posts (
             id                INTEGER PRIMARY KEY AUTOINCREMENT,
             kind              TEXT    NOT NULL,
             title             TEXT    NOT NULL,
             slug              TEXT    NOT NULL,
             summary           TEXT    NOT NULL DEFAULT '',
             body_markdown     TEXT    NOT NULL DEFAULT '',
             body_html         TEXT    NOT NULL,
             link              TEXT,
             video_id          TEXT,
             status            TEXT    NOT NULL,
             author_id         INTEGER NOT NULL,
             author_name       TEXT    NOT NULL,
             created_at        INTEGER NOT NULL,
             reviewed_by_id    INTEGER,
             reviewed_by_name  TEXT,
             reviewed_at       INTEGER,
             moderation_note   TEXT
         )",
    )
    .execute(pool)
    .await?;

    // The feed query and the moderator queue both order by time within a status.
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_news_posts_status_created
             ON news_posts(status, created_at DESC)",
    )
    .execute(pool)
    .await?;

    // Rate limiting counts an author's recent submissions.
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_news_posts_author
             ON news_posts(author_id, created_at DESC)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS news_reactions (
             post_id    INTEGER NOT NULL REFERENCES news_posts(id),
             user_id    INTEGER NOT NULL,
             created_at INTEGER NOT NULL,
             PRIMARY KEY (post_id, user_id)
         )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS news_audit (
             id         INTEGER PRIMARY KEY AUTOINCREMENT,
             post_id    INTEGER,
             actor_id   INTEGER,
             actor_name TEXT    NOT NULL,
             action     TEXT    NOT NULL,
             note       TEXT    NOT NULL DEFAULT '',
             created_at INTEGER NOT NULL
         )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_news_audit_post ON news_audit(post_id, created_at DESC)",
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// One post, as the templates and the API need it.
#[derive(Debug, Clone)]
pub struct Post {
    pub id: i64,
    pub kind: Kind,
    pub title: String,
    pub slug: String,
    pub summary: String,
    pub body_markdown: String,
    pub body_html: String,
    pub link: Option<String>,
    pub video_id: Option<String>,
    pub status: Status,
    pub author_id: i64,
    pub author_name: String,
    pub created_at: i64,
    pub reviewed_by_name: Option<String>,
    pub reviewed_at: Option<i64>,
    pub moderation_note: Option<String>,
    pub thumbs_up: i64,
}

/// The canonical path of a post. The id makes it stable if the title is edited.
impl Post {
    pub fn path(&self) -> String {
        format!("/news/{}-{}", self.id, self.slug)
    }
}

const SELECT: &str = "SELECT p.id, p.kind, p.title, p.slug, p.summary, p.body_markdown,
                             p.body_html, p.link, p.video_id, p.status, p.author_id,
                             p.author_name, p.created_at, p.reviewed_by_name,
                             p.reviewed_at, p.moderation_note,
                             (SELECT COUNT(*) FROM news_reactions r WHERE r.post_id = p.id)
                                AS thumbs_up
                      FROM news_posts p";

fn row_to_post(row: &sqlx::sqlite::SqliteRow) -> Post {
    Post {
        id: row.get("id"),
        // A row written by a future version with an unknown kind still renders;
        // `News` is the neutral label.
        kind: Kind::parse(row.get::<String, _>("kind").as_str()).unwrap_or(Kind::News),
        title: row.get("title"),
        slug: row.get("slug"),
        summary: row.get("summary"),
        body_markdown: row.get("body_markdown"),
        body_html: row.get("body_html"),
        link: row.get("link"),
        video_id: row.get("video_id"),
        status: Status::parse(row.get::<String, _>("status").as_str()).unwrap_or(Status::Pending),
        author_id: row.get("author_id"),
        author_name: row.get("author_name"),
        created_at: row.get("created_at"),
        reviewed_by_name: row.get("reviewed_by_name"),
        reviewed_at: row.get("reviewed_at"),
        moderation_note: row.get("moderation_note"),
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
        "INSERT INTO news_posts (kind, title, slug, summary, body_markdown, body_html,
                                 link, video_id, status, author_id, author_name, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.kind.as_str())
    .bind(&entry.title)
    .bind(&entry.slug)
    .bind(&entry.summary)
    .bind(&entry.body_markdown)
    .bind(&entry.body_html)
    .bind(&entry.link)
    .bind(&entry.video_id)
    .bind(Status::Pending.as_str())
    .bind(author_id)
    .bind(author_name)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

/// Submissions by this account inside the window, not counting rejected ones:
/// a rejection is the moderator's answer, and counting it would penalise someone
/// whose posts were removed for a reason unrelated to volume.
pub async fn submissions_since(pool: &SqlitePool, author_id: i64, since: i64) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM news_posts
         WHERE author_id = ? AND created_at >= ? AND status != ?",
    )
    .bind(author_id)
    .bind(since)
    .bind(Status::Rejected.as_str())
    .fetch_one(pool)
    .await
}

/// Approved posts, newest first — the public feed.
pub async fn feed(pool: &SqlitePool, limit: i64, offset: i64) -> sqlx::Result<Vec<Post>> {
    let rows = sqlx::query(&format!(
        "{SELECT} WHERE p.status = ? ORDER BY p.created_at DESC, p.id DESC LIMIT ? OFFSET ?"
    ))
    .bind(Status::Approved.as_str())
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_post).collect())
}

/// Every approved post, for the sitemap and RSS. Deliberately unbounded: this
/// feed is a few hundred rows a year, and a crawler wants all of them.
pub async fn all_approved(pool: &SqlitePool) -> sqlx::Result<Vec<Post>> {
    let rows = sqlx::query(&format!(
        "{SELECT} WHERE p.status = ? ORDER BY p.created_at DESC, p.id DESC"
    ))
    .bind(Status::Approved.as_str())
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_post).collect())
}

/// The moderator queue: pending only. Rejected posts are absent by design — once
/// rejected, a post is out of every view but the database.
pub async fn pending(pool: &SqlitePool) -> sqlx::Result<Vec<Post>> {
    let rows = sqlx::query(&format!(
        "{SELECT} WHERE p.status = ? ORDER BY p.created_at ASC, p.id ASC"
    ))
    .bind(Status::Pending.as_str())
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_post).collect())
}

/// One approved post by id. Pending and rejected rows are not reachable here:
/// callers that need them (the author's own view, moderation) ask separately.
pub async fn approved_by_id(pool: &SqlitePool, id: i64) -> sqlx::Result<Option<Post>> {
    let row = sqlx::query(&format!("{SELECT} WHERE p.id = ? AND p.status = ?"))
        .bind(id)
        .bind(Status::Approved.as_str())
        .fetch_optional(pool)
        .await?;
    Ok(row.as_ref().map(row_to_post))
}

/// One post by id in any state — for the author's own view and for moderation.
pub async fn by_id(pool: &SqlitePool, id: i64) -> sqlx::Result<Option<Post>> {
    let row = sqlx::query(&format!("{SELECT} WHERE p.id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.as_ref().map(row_to_post))
}

/// Pending posts by one author, so they can see their own queue.
pub async fn pending_by_author(pool: &SqlitePool, author_id: i64) -> sqlx::Result<Vec<Post>> {
    let rows = sqlx::query(&format!(
        "{SELECT} WHERE p.status = ? AND p.author_id = ? ORDER BY p.created_at DESC"
    ))
    .bind(Status::Pending.as_str())
    .bind(author_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_post).collect())
}

pub async fn approve(
    pool: &SqlitePool,
    id: i64,
    reviewer_id: i64,
    reviewer_name: &str,
    now: i64,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE news_posts
         SET status = ?, reviewed_by_id = ?, reviewed_by_name = ?, reviewed_at = ?
         WHERE id = ? AND status = ?",
    )
    .bind(Status::Approved.as_str())
    .bind(reviewer_id)
    .bind(reviewer_name)
    .bind(now)
    .bind(id)
    .bind(Status::Pending.as_str())
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Soft delete. Allowed from pending *and* approved: a post that turns out to be
/// harmful after it went live comes down the same way, with the same note.
pub async fn reject(
    pool: &SqlitePool,
    id: i64,
    reviewer_id: i64,
    reviewer_name: &str,
    note: &str,
    now: i64,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE news_posts
         SET status = ?, reviewed_by_id = ?, reviewed_by_name = ?, reviewed_at = ?,
             moderation_note = ?
         WHERE id = ? AND status != ?",
    )
    .bind(Status::Rejected.as_str())
    .bind(reviewer_id)
    .bind(reviewer_name)
    .bind(now)
    .bind(note)
    .bind(id)
    .bind(Status::Rejected.as_str())
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Add or remove this account's thumbs-up. Returns whether it is set now.
pub async fn set_reaction(
    pool: &SqlitePool,
    post_id: i64,
    user_id: i64,
    active: bool,
    now: i64,
) -> sqlx::Result<bool> {
    if active {
        sqlx::query(
            "INSERT INTO news_reactions (post_id, user_id, created_at) VALUES (?, ?, ?)
             ON CONFLICT(post_id, user_id) DO NOTHING",
        )
        .bind(post_id)
        .bind(user_id)
        .bind(now)
        .execute(pool)
        .await?;
    } else {
        sqlx::query("DELETE FROM news_reactions WHERE post_id = ? AND user_id = ?")
            .bind(post_id)
            .bind(user_id)
            .execute(pool)
            .await?;
    }
    Ok(active)
}

pub async fn has_reacted(pool: &SqlitePool, post_id: i64, user_id: i64) -> sqlx::Result<bool> {
    let found: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM news_reactions WHERE post_id = ? AND user_id = ?")
            .bind(post_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(found.is_some())
}

pub async fn thumb_count(pool: &SqlitePool, post_id: i64) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM news_reactions WHERE post_id = ?")
        .bind(post_id)
        .fetch_one(pool)
        .await
}

/// Record a moderation action. Every approve and reject writes one; nothing in
/// this module writes to `news_posts` without a matching call.
pub async fn audit(
    pool: &SqlitePool,
    post_id: Option<i64>,
    actor_id: Option<i64>,
    actor_name: &str,
    action: &str,
    note: &str,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO news_audit (post_id, actor_id, actor_name, action, note, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(post_id)
    .bind(actor_id)
    .bind(actor_name)
    .bind(action)
    .bind(note)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// The audit trail for one post, oldest first — what a review reads.
pub async fn audit_for(pool: &SqlitePool, post_id: i64) -> sqlx::Result<Vec<AuditEntry>> {
    let rows = sqlx::query(
        "SELECT actor_id, actor_name, action, note, created_at FROM news_audit
         WHERE post_id = ? ORDER BY created_at ASC, id ASC",
    )
    .bind(post_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| AuditEntry {
            actor_id: row.get("actor_id"),
            actor_name: row.get("actor_name"),
            action: row.get("action"),
            note: row.get("note"),
            created_at: row.get("created_at"),
        })
        .collect())
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub actor_id: Option<i64>,
    pub actor_name: String,
    pub action: String,
    pub note: String,
    pub created_at: i64,
}

/// A note for a moderation action, or the reason it is not acceptable.
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory database");
        migrate(&pool).await.expect("migrate");
        pool
    }

    fn entry(title: &str) -> Validated {
        super::super::validate("news", title, "", "Body text", None).expect("valid entry")
    }

    #[tokio::test]
    async fn a_submission_is_pending_and_invisible_until_approved() {
        let pool = pool().await;
        let id = create(&pool, 7, "agent", &entry("First post"), 1_000)
            .await
            .expect("create");

        assert!(
            feed(&pool, 10, 0).await.expect("feed").is_empty(),
            "nothing public yet"
        );
        assert!(approved_by_id(&pool, id).await.expect("lookup").is_none());
        assert_eq!(
            pending(&pool).await.expect("queue").len(),
            1,
            "but it is in the queue"
        );
        assert_eq!(
            pending_by_author(&pool, 7).await.expect("own queue").len(),
            1
        );
        assert!(
            pending_by_author(&pool, 8)
                .await
                .expect("own queue")
                .is_empty()
        );

        assert!(approve(&pool, id, 1, "mod", 2_000).await.expect("approve"));
        assert_eq!(feed(&pool, 10, 0).await.expect("feed").len(), 1);
        assert!(pending(&pool).await.expect("queue").is_empty());
        assert!(approved_by_id(&pool, id).await.expect("lookup").is_some());

        let post = by_id(&pool, id).await.expect("lookup").expect("present");
        assert_eq!(post.status, Status::Approved);
        assert_eq!(post.reviewed_by_name.as_deref(), Some("mod"));
        assert_eq!(post.path(), format!("/news/{id}-first-post"));
    }

    #[tokio::test]
    async fn rejection_hides_the_post_but_keeps_it_and_the_reason() {
        let pool = pool().await;
        let id = create(&pool, 7, "agent", &entry("Spam post"), 1_000)
            .await
            .expect("create");
        approve(&pool, id, 1, "mod", 1_500).await.expect("approve");

        assert!(
            reject(&pool, id, 2, "mod2", "Advertising, not a news post", 2_000)
                .await
                .expect("reject")
        );

        // Gone from every view...
        assert!(feed(&pool, 10, 0).await.expect("feed").is_empty());
        assert!(approved_by_id(&pool, id).await.expect("lookup").is_none());
        assert!(pending(&pool).await.expect("queue").is_empty());
        assert!(
            pending_by_author(&pool, 7)
                .await
                .expect("own queue")
                .is_empty(),
            "the author's own pending list does not resurrect it either"
        );

        // ...but still on disk, with the note and the content intact.
        let post = by_id(&pool, id).await.expect("lookup").expect("kept");
        assert_eq!(post.status, Status::Rejected);
        assert_eq!(
            post.moderation_note.as_deref(),
            Some("Advertising, not a news post")
        );
        assert_eq!(post.title, "Spam post");
        assert_eq!(post.body_markdown, "Body text");
        assert_eq!(post.body_html, "<p>Body text</p>");
    }

    #[tokio::test]
    async fn a_rejected_post_cannot_be_rejected_twice_or_approved_back() {
        let pool = pool().await;
        let id = create(&pool, 7, "agent", &entry("Gone"), 1_000)
            .await
            .expect("create");
        assert!(
            reject(&pool, id, 1, "mod", "off topic", 2_000)
                .await
                .expect("reject")
        );
        assert!(
            !reject(&pool, id, 1, "mod", "again", 3_000)
                .await
                .expect("second reject"),
            "a second rejection would overwrite the original note"
        );
        assert!(
            !approve(&pool, id, 1, "mod", 4_000).await.expect("approve"),
            "rejection is final; reinstating is a new post"
        );
    }

    #[tokio::test]
    async fn only_pending_posts_can_be_approved() {
        let pool = pool().await;
        let id = create(&pool, 7, "agent", &entry("Twice"), 1_000)
            .await
            .expect("create");
        assert!(approve(&pool, id, 1, "mod", 2_000).await.expect("approve"));
        assert!(
            !approve(&pool, id, 1, "mod", 3_000)
                .await
                .expect("re-approve")
        );
        assert!(
            !approve(&pool, 9_999, 1, "mod", 3_000)
                .await
                .expect("missing post")
        );
    }

    #[tokio::test]
    async fn the_rate_limit_ignores_rejected_submissions() {
        let pool = pool().await;
        for i in 0..3 {
            let id = create(&pool, 7, "agent", &entry(&format!("Post {i}")), 1_000 + i)
                .await
                .expect("create");
            if i == 0 {
                reject(&pool, id, 1, "mod", "no", 1_100)
                    .await
                    .expect("reject");
            }
        }
        assert_eq!(submissions_since(&pool, 7, 0).await.expect("count"), 2);
        assert_eq!(
            submissions_since(&pool, 7, 2_000).await.expect("count"),
            0,
            "the window is respected"
        );
        assert_eq!(submissions_since(&pool, 8, 0).await.expect("count"), 0);
    }

    #[tokio::test]
    async fn thumbs_up_is_per_account_and_toggles() {
        let pool = pool().await;
        let id = create(&pool, 7, "agent", &entry("Popular"), 1_000)
            .await
            .expect("create");
        approve(&pool, id, 1, "mod", 1_100).await.expect("approve");

        assert!(set_reaction(&pool, id, 7, true, 2_000).await.expect("add"));
        assert!(
            set_reaction(&pool, id, 7, true, 2_100)
                .await
                .expect("add twice")
        );
        assert_eq!(
            thumb_count(&pool, id).await.expect("count"),
            1,
            "idempotent"
        );
        assert!(has_reacted(&pool, id, 7).await.expect("check"));

        set_reaction(&pool, id, 8, true, 2_200)
            .await
            .expect("other account");
        assert_eq!(thumb_count(&pool, id).await.expect("count"), 2);

        set_reaction(&pool, id, 7, false, 2_300)
            .await
            .expect("remove");
        assert_eq!(thumb_count(&pool, id).await.expect("count"), 1);
        assert!(!has_reacted(&pool, id, 7).await.expect("check"));

        // The feed carries the count without a second query per post.
        let feed = feed(&pool, 10, 0).await.expect("feed");
        assert_eq!(feed[0].thumbs_up, 1);
    }

    #[tokio::test]
    async fn every_moderation_action_leaves_a_trail() {
        let pool = pool().await;
        let id = create(&pool, 7, "agent", &entry("Audited"), 1_000)
            .await
            .expect("create");
        approve(&pool, id, 1, "mod", 2_000).await.expect("approve");
        audit(&pool, Some(id), Some(1), "mod", "approve", "", 2_000)
            .await
            .expect("audit");
        reject(&pool, id, 2, "mod2", "harmful", 3_000)
            .await
            .expect("reject");
        audit(&pool, Some(id), Some(2), "mod2", "reject", "harmful", 3_000)
            .await
            .expect("audit");

        let trail = audit_for(&pool, id).await.expect("trail");
        assert_eq!(trail.len(), 2);
        assert_eq!(trail[0].action, "approve");
        assert_eq!(trail[1].action, "reject");
        assert_eq!(trail[1].note, "harmful");
        assert_eq!(trail[1].actor_name, "mod2");
    }

    #[tokio::test]
    async fn a_moderation_note_is_mandatory_and_bounded() {
        assert!(validate_note("   ").is_err());
        assert!(validate_note("").is_err());
        assert_eq!(validate_note("  spam links  ").expect("note"), "spam links");
        assert!(validate_note(&"x".repeat(NOTE_MAX + 1)).is_err());
    }

    #[tokio::test]
    async fn the_feed_is_newest_first_and_paginates() {
        let pool = pool().await;
        for i in 0..3 {
            let id = create(
                &pool,
                7,
                "agent",
                &entry(&format!("Post {i}")),
                1_000 * (i + 1),
            )
            .await
            .expect("create");
            approve(&pool, id, 1, "mod", 9_000).await.expect("approve");
        }
        let first = feed(&pool, 2, 0).await.expect("feed");
        let second = feed(&pool, 2, 2).await.expect("feed");
        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 1);
        assert_eq!(first[0].title, "Post 2");
        assert_eq!(first[1].title, "Post 1");
        assert_eq!(second[0].title, "Post 0");
        assert_eq!(all_approved(&pool).await.expect("all").len(), 3);
    }
}
