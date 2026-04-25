//! Data-transfer objects that cross the HTTP boundary.
//!
//! Every type in this module is the single source of truth for both the
//! Rust handlers in `src/lib.rs` and the TypeScript code in the static
//! site. The TypeScript files are produced by `ts-rs` at `cargo test`
//! time and committed to the repo, so the frontend build does not need
//! a Rust toolchain. The destination directory is controlled by the
//! `TS_RS_EXPORT_DIR` environment variable declared in `.cargo/config.toml`.
//!
//! # Integer width convention
//!
//! Integer columns coming out of SQLite are naturally `i64`, which
//! `ts-rs` would otherwise render as TypeScript `bigint`. Forcing the
//! frontend into `BigInt` arithmetic is unnecessarily ceremonial given
//! the expected row counts, so every `i64` field in this module is
//! annotated with `#[ts(type = "number")]` to map to TypeScript
//! `number` instead. This is safe as long as no id grows past
//! JavaScript's 2^53 safe-integer range — revisit the override if that
//! ever becomes plausible.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utoipa::ToSchema;

/// A single blog post, mapping one-to-one to a row of the `posts`
/// table defined in `migrations/0001_init.sql`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct Post {
    /// Primary key of the row (`INTEGER PRIMARY KEY AUTOINCREMENT`).
    #[ts(type = "number")]
    pub id: i64,

    /// Stable public identifier that never changes and is safe to expose.
    ///
    /// This identifier is generated independently of timestamps so it
    /// does not leak creation order or publication timing. Use it for
    /// stable hyperlinks and long-lived external references.
    #[ts(type = "string")]
    pub public_id: uuid::Uuid,

    /// Human-readable URL slug for the post.
    ///
    /// Backed by a `UNIQUE` constraint at the database level and used
    /// for canonical friendly URLs. Unlike [`Post::public_id`], this
    /// value may change over time to improve readability.
    pub slug: String,

    /// Display title shown in the post header and in listings.
    pub title: String,

    /// Optional one-line teaser for listings and social previews.
    ///
    /// `None` when the author has not provided one; the frontend is
    /// free to fall back to a body excerpt or omit the field entirely.
    pub summary: Option<String>,

    /// Post body in AsciiDoc source form.
    ///
    /// This is the authoritative representation of the post content —
    /// all rendered artifacts (static HTML, RSS, search index, ...)
    /// are regenerable from `body_adoc` alone. The API never performs
    /// AsciiDoc → HTML conversion server-side, so clients must run
    /// their own renderer.
    pub body_adoc: String,

    /// Visibility state of the post. See [`PostStatus`] for the
    /// meaning of each variant.
    pub status: PostStatus,

    /// Timestamp at which the post first transitioned to
    /// [`PostStatus::Public`], or `None` if it has never been
    /// published.
    ///
    /// Stored and transmitted as an RFC 3339 string in UTC (e.g.
    /// `2026-04-12T02:04:09Z`), parsed into [`DateTime<Utc>`] on the
    /// Rust side by chrono's default serde impl and re-exported to
    /// TypeScript as `string` via ts-rs's `chrono-impl` feature.
    pub published_at: Option<DateTime<Utc>>,

    /// Timestamp of the row's initial insertion.
    ///
    /// Populated by the SQLite `DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ',
    /// 'now'))` clause on the `posts.created_at` column, so inserts
    /// never need to pass it explicitly. Immutable after creation.
    pub created_at: DateTime<Utc>,

    /// Timestamp of the most recent successful update to the row.
    ///
    /// Populated by the same SQLite `DEFAULT` expression as
    /// `created_at` on insertion, but is *not* refreshed automatically
    /// on `UPDATE` — there is no database trigger, so any write
    /// handler that mutates a row must set `updated_at` explicitly.
    pub updated_at: DateTime<Utc>,

    /// Monotonic edit counter that starts at `1` on insertion and is
    /// bumped by every successful update.
    ///
    /// Included in read responses so that clients can cache by
    /// `(id, revision_no)` and so that future write handlers can use
    /// it as an optimistic-concurrency token. Like `updated_at`, it
    /// is maintained by the application layer rather than a trigger.
    #[ts(type = "number")]
    pub revision_no: i64,
}

/// Summary data for the public post listing endpoint.
///
/// This is intentionally smaller than [`Post`]: callers that only need
/// the index view should not pay to transfer or deserialize the full
/// AsciiDoc source for every post.
#[derive(Debug, Clone, Serialize, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct PostSummary {
    #[ts(type = "string")]
    pub public_id: uuid::Uuid,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
}

/// Request body for `POST /posts`.
#[derive(Debug, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct CreatePost {
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub body_adoc: String,
    pub status: PostStatus,
}

/// Request body for `PUT /posts/by-slug/{slug}`.
///
/// Every field except `revision_no` is optional — only the fields
/// present in the request payload are updated. `revision_no` is
/// required and acts as an optimistic-concurrency token: the update
/// succeeds only when the value matches the current row.
#[derive(Debug, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct UpdatePost {
    pub slug: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub body_adoc: Option<String>,
    pub status: Option<PostStatus>,
    #[ts(type = "number")]
    pub revision_no: i64,
}

/// Request body for exchanging a Firebase ID token for an admin session cookie.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct FirebaseSessionRequest {
    pub id_token: String,
}

/// Visibility lifecycle of a [`Post`].
///
/// Enforced at the database level by a `CHECK (status IN ('draft',
/// 'private', 'public', 'trashed'))` constraint on the `posts.status`
/// column, so any value outside this enum is rejected before it
/// reaches Rust. The four-state lifecycle is intentionally minimal —
/// anything more elaborate (scheduled publishing, embargoed drafts,
/// per-reader access control) can be layered on later without a
/// wire-format change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, ToSchema)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename_all = "lowercase")]
pub enum PostStatus {
    /// Work in progress. Not listed on the public index and not
    /// reachable by slug from read-only handlers.
    Draft,
    /// Not listed on the public index and not reachable by slug from
    /// public read-only handlers. Reserved for future authenticated
    /// access or unlisted sharing.
    Private,
    /// Listed on the public index and reachable by slug. The only
    /// variant that counts as "published" for the purposes of
    /// [`Post::published_at`].
    Public,
    /// Content moved to the trash for future administrative review and
    /// possible restoration. Hidden from public read handlers.
    Trashed,
}

impl PostStatus {
    /// Canonical database representation used by SQLite and D1 writes.
    pub const fn as_db_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Private => "private",
            Self::Public => "public",
            Self::Trashed => "trashed",
        }
    }
}

/// Standard JSON error response shape.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

/// Session issuance response for `POST /auth/session`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionIssuedResponse {
    pub ok: bool,
    pub session: String,
}

/// Session status response for `GET /auth/session`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionStatusResponse {
    pub authenticated: bool,
}

#[cfg(test)]
mod tests {
    use super::PostStatus;

    #[test]
    fn post_status_database_strings_match_schema_values() {
        assert_eq!(PostStatus::Draft.as_db_str(), "draft");
        assert_eq!(PostStatus::Private.as_db_str(), "private");
        assert_eq!(PostStatus::Public.as_db_str(), "public");
        assert_eq!(PostStatus::Trashed.as_db_str(), "trashed");
    }
}
