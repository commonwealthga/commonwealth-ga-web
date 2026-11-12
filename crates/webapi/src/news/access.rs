//! Who may post, and who may moderate.
//!
//! Roles are `ga_user_roles` rows in the game database — the same table that
//! gates the `spectator` chat commands — and they are **read-only here**. The
//! website never grants or revokes one: that happens in the dashboard, through
//! the control server's token-gated `set-role` action. Keeping writes out of this
//! process is deliberate, because it is the one container exposed to the
//! internet, and the admin token can do considerably more than news.
//!
//! Three role names matter:
//!
//! * `news_moderator` — sees the queue, approves and rejects. Granted by hand.
//! * `news_restricted` — may not submit, in any mode. The answer to an abusive
//!   account that keeps coming back; whole-account, not per-post.
//! * `news_allowlisted` — may submit when the deployment is in `allowlisted`
//!   mode, which is the switch to throw if bots ever find the form.
//!
//! Every lookup fails closed: a missing table, a missing account or a database
//! error means "ordinary user", never "moderator".

use sqlx::SqlitePool;

use super::{PostingMode, markdown};

pub const ROLE_MODERATOR: &str = "news_moderator";
pub const ROLE_RESTRICTED: &str = "news_restricted";
pub const ROLE_ALLOWLISTED: &str = "news_allowlisted";

/// Every role that means anything to the news feed, as granted in the dashboard.
pub const KNOWN_ROLES: [(&str, &str); 3] = [
    (
        ROLE_MODERATOR,
        "Reviews the news queue: approves submissions and rejects them with a note.",
    ),
    (
        ROLE_RESTRICTED,
        "May not submit news, whatever the posting mode.",
    ),
    (
        ROLE_ALLOWLISTED,
        "May submit news while posting is restricted to the allowlist.",
    ),
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Roles {
    pub moderator: bool,
    pub restricted: bool,
    pub allowlisted: bool,
}

impl Roles {
    /// An anonymous visitor, or any signed-in account with no news roles.
    pub const NONE: Roles = Roles {
        moderator: false,
        restricted: false,
        allowlisted: false,
    };
}

/// Which roles this account holds. Never fails: an unreadable role table means
/// the account has no special roles.
pub async fn roles_of(live: Option<&SqlitePool>, user_id: i64) -> Roles {
    let Some(pool) = live else {
        return Roles::NONE;
    };

    let rows: Result<Vec<String>, sqlx::Error> =
        sqlx::query_scalar("SELECT role FROM ga_user_roles WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(pool)
            .await;

    let names = match rows {
        Ok(names) => names,
        // The table is created by the game server; a database from before roles
        // existed, or a pool that cannot be read, gives nobody any rights.
        Err(error) => {
            tracing::debug!(%error, user_id, "could not read ga_user_roles; treating as ordinary user");
            return Roles::NONE;
        }
    };

    let mut roles = Roles::NONE;
    for name in names {
        match name.trim() {
            ROLE_MODERATOR => roles.moderator = true,
            ROLE_RESTRICTED => roles.restricted = true,
            ROLE_ALLOWLISTED => roles.allowlisted = true,
            _ => {}
        }
    }
    roles
}

/// Why a submission was refused. Each variant is shown to the would-be author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    /// `news_restricted`: this account may not submit.
    Restricted,
    /// Posting is allowlisted and this account is not on it.
    NotAllowlisted,
    /// Not signed in.
    Anonymous,
}

impl Denial {
    pub fn message(self) -> &'static str {
        match self {
            Denial::Restricted => {
                "Your account is not permitted to submit posts. Contact a moderator if you think this is a mistake."
            }
            Denial::NotAllowlisted => {
                "New submissions are limited to approved contributors right now."
            }
            Denial::Anonymous => "Sign in with your game account to submit a post.",
        }
    }
}

impl std::fmt::Display for Denial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// The single place the posting rules live.
///
/// A moderator is always allowed to post, including under `news_restricted`: the
/// alternative is a moderator locked out of their own tool. An operator who wants
/// somebody stopped should revoke the moderator role first, which is the only
/// way to reach that state anyway.
pub fn may_post(mode: PostingMode, roles: Roles) -> Result<(), Denial> {
    if roles.moderator {
        return Ok(());
    }
    if roles.restricted {
        return Err(Denial::Restricted);
    }
    match mode {
        PostingMode::Open => Ok(()),
        PostingMode::Allowlisted if roles.allowlisted => Ok(()),
        PostingMode::Allowlisted => Err(Denial::NotAllowlisted),
    }
}

/// Render a preview for the editor, using the same renderer the post will be
/// served with — the preview cannot flatter the result.
pub fn preview(markdown_source: &str) -> String {
    markdown::to_html(markdown_source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn live_with_roles(grants: &[(i64, &str)]) -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory database");
        sqlx::query(
            "CREATE TABLE ga_user_roles (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 user_id INTEGER NOT NULL,
                 role TEXT NOT NULL,
                 granted_at INTEGER NOT NULL DEFAULT 0,
                 UNIQUE(user_id, role)
             )",
        )
        .execute(&pool)
        .await
        .expect("schema");
        for (user_id, role) in grants {
            sqlx::query("INSERT INTO ga_user_roles (user_id, role) VALUES (?, ?)")
                .bind(user_id)
                .bind(role)
                .execute(&pool)
                .await
                .expect("grant");
        }
        pool
    }

    #[tokio::test]
    async fn roles_are_read_from_the_game_database() {
        let pool =
            live_with_roles(&[(7, ROLE_MODERATOR), (8, ROLE_ALLOWLISTED), (9, "spectator")]).await;
        let moderator = roles_of(Some(&pool), 7).await;
        assert_eq!(
            moderator,
            Roles {
                moderator: true,
                ..Roles::NONE
            }
        );
        assert!(roles_of(Some(&pool), 8).await.allowlisted);
        assert_eq!(
            roles_of(Some(&pool), 9).await,
            Roles::NONE,
            "a role the news feed does not know is not a right"
        );
        assert_eq!(roles_of(Some(&pool), 10).await, Roles::NONE);
    }

    #[tokio::test]
    async fn an_unreadable_role_table_grants_nothing() {
        // A database with no ga_user_roles table at all — older game data, or a
        // misconfigured mount. The failure must not become a permission.
        let bare = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory database");
        assert_eq!(roles_of(Some(&bare), 7).await, Roles::NONE);
        assert_eq!(roles_of(None, 7).await, Roles::NONE);
    }

    #[test]
    fn open_posting_admits_ordinary_accounts() {
        assert_eq!(may_post(PostingMode::Open, Roles::NONE), Ok(()));
    }

    #[test]
    fn restricted_accounts_are_refused_in_every_mode() {
        let restricted = Roles {
            restricted: true,
            ..Roles::NONE
        };
        assert_eq!(
            may_post(PostingMode::Open, restricted),
            Err(Denial::Restricted)
        );
        assert_eq!(
            may_post(PostingMode::Allowlisted, restricted),
            Err(Denial::Restricted)
        );
    }

    #[test]
    fn allowlisted_mode_admits_only_the_allowlist_and_moderators() {
        let mode = PostingMode::Allowlisted;
        assert_eq!(may_post(mode, Roles::NONE), Err(Denial::NotAllowlisted));
        assert_eq!(
            may_post(
                mode,
                Roles {
                    allowlisted: true,
                    ..Roles::NONE
                }
            ),
            Ok(())
        );
        assert_eq!(
            may_post(
                mode,
                Roles {
                    moderator: true,
                    ..Roles::NONE
                }
            ),
            Ok(()),
            "a moderator must not be locked out of their own queue"
        );
    }
}
