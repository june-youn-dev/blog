//! Thin repository layer over the Cloudflare D1 binding.
//!
//! All SQL literals and row-to-struct mappings for the `posts` table live
//! here so that handlers in `src/lib.rs` never see raw SQL. Adding a new
//! query means adding a function here and exposing a DTO from `src/dto.rs`.

use uuid::Uuid;
use worker::Env;

use crate::dto::{CreatePost, Post, PostSummary, UpdatePost};
use crate::validation::{validate_create_post_input, validate_update_post_input};

/// Typed error for write operations, allowing handlers to map
/// specific failure modes to appropriate HTTP status codes without
/// relying on broad error-message string matching in the HTTP layer.
pub enum WriteError {
    /// Input validation failed (slug format, body size, etc.).
    Validation(String),
    /// A UNIQUE constraint was violated (duplicate slug).
    Conflict,
    /// An unexpected D1 or binding error.
    #[allow(dead_code)]
    Internal(worker::Error),
}

/// Explicit column list for the `posts` table.
///
/// Using a named constant instead of `SELECT *` decouples query
/// results from schema ordering and makes future column additions
/// a compile-time decision rather than a silent runtime change.
const POST_COLUMNS: &str = "\
    id, public_id, slug, title, summary, body_adoc, status, \
    published_at, created_at, updated_at, revision_no";

/// Explicit column list for public post listings.
const POST_SUMMARY_COLUMNS: &str = "public_id, slug, title, summary, published_at";

/// Lists every post for authenticated administrative tooling.
///
/// This query is intentionally unpaginated because the expected row
/// count for a personal blog is small, and administrative interfaces
/// benefit from receiving the full editable row including `body_adoc`.
/// Results are ordered by `updated_at DESC, created_at DESC` so recent
/// work appears first.
pub async fn list_all_posts(env: &Env) -> worker::Result<Vec<Post>> {
    let db = env.d1("DB")?;
    let stmt = db.prepare(format!(
        r#"
        SELECT {POST_COLUMNS}
        FROM posts
        ORDER BY updated_at DESC, created_at DESC
        "#,
    ));
    let result = stmt.all().await?;
    result.results::<Post>()
}

/// Looks up a single **public** post by its slug.
///
/// Only posts with `status = 'public'` are returned. Draft and private
/// posts are invisible to this function, preventing accidental leakage
/// through the public `GET /posts/by-slug/{slug}` endpoint.
///
/// Returns `Ok(Some(post))` when a public row with the given slug is
/// found, `Ok(None)` when the slug does not exist or is not public.
///
/// # Errors
///
/// Propagates any [`worker::Error`] raised along the query pipeline.
pub async fn get_post_by_slug(env: &Env, slug: &str) -> worker::Result<Option<Post>> {
    let db = env.d1("DB")?;
    let public_status = crate::dto::PostStatus::Public.as_db_str();
    let stmt = db
        .prepare(format!(
            r#"
            SELECT {POST_COLUMNS}
            FROM posts
            WHERE slug = ?1
                AND status = ?2
            LIMIT 1
            "#,
        ))
        .bind(&[slug.into(), public_status.into()])?;
    stmt.first::<Post>(None).await
}

/// Looks up a single **public** post by its stable public identifier.
pub async fn get_post_by_public_id(env: &Env, public_id: Uuid) -> worker::Result<Option<Post>> {
    let db = env.d1("DB")?;
    let public_status = crate::dto::PostStatus::Public.as_db_str();
    let stmt = db
        .prepare(format!(
            r#"
            SELECT {POST_COLUMNS}
            FROM posts
            WHERE public_id = ?1
                AND status = ?2
            LIMIT 1
            "#,
        ))
        .bind(&[public_id.to_string().into(), public_status.into()])?;
    stmt.first::<Post>(None).await
}

/// Lists every post whose `status` is [`crate::dto::PostStatus::Public`],
/// newest first.
///
/// Ordered by `published_at DESC` so that the most recently published
/// post appears at the head of the returned vector; `draft` and
/// `private` posts are filtered out at the database level.
///
/// Returns `Ok(Vec::new())` when there are no public posts in the
/// database — callers should treat the empty vector as a valid `200
/// OK` response rather than a `404`.
///
/// No pagination is applied: the expected row count for a personal
/// blog is small enough that returning every row is cheaper than
/// paying the complexity cost of a cursor.
///
/// # Errors
///
/// Propagates any [`worker::Error`] raised along the query pipeline.
pub async fn list_public_posts(env: &Env) -> worker::Result<Vec<PostSummary>> {
    let db = env.d1("DB")?;
    let public_status = crate::dto::PostStatus::Public.as_db_str();
    let stmt = db
        .prepare(format!(
            r#"
            SELECT {POST_SUMMARY_COLUMNS}
            FROM posts
            WHERE status = ?1
            ORDER BY published_at DESC
            "#,
        ))
        .bind(&[public_status.into()])?;
    let result = stmt.all().await?;
    result.results::<PostSummary>()
}

/// Inserts a new post and returns the created row.
///
/// Validates `slug` format and `body_adoc` size before touching the
/// database. When `status` is [`crate::dto::PostStatus::Public`],
/// `published_at` is populated with the current UTC timestamp. The
/// statement uses `ON CONFLICT DO NOTHING RETURNING ...` so duplicate
/// slugs are classified through SQL control flow instead of by parsing
/// database error strings.
///
/// # Errors
///
/// Returns [`WriteError::Validation`] on validation failure,
/// [`WriteError::Conflict`] when the slug already exists, and
/// [`WriteError::Internal`] for unexpected binding or D1 failures.
pub async fn create_post(env: &Env, p: &CreatePost) -> Result<Post, WriteError> {
    if let Err(msg) = validate_create_post_input(p) {
        return Err(WriteError::Validation(msg.into()));
    }

    let db = env.d1("DB").map_err(WriteError::Internal)?;
    let status_str = p.status.as_db_str();
    let public_status = crate::dto::PostStatus::Public.as_db_str();

    let published_at_expr =
        if status_str == public_status { "STRFTIME('%Y-%m-%dT%H:%M:%SZ', 'now')" } else { "NULL" };

    let insert = db
        .prepare(format!(
            r#"
            INSERT INTO posts (public_id, slug, title, summary, body_adoc, status, published_at)
            VALUES (?1, ?2, ?3, NULLIF(?4, ''), ?5, ?6, {published_at_expr})
            ON CONFLICT(slug) DO NOTHING
            RETURNING {POST_COLUMNS}
            "#,
        ))
        .bind(&[
            Uuid::new_v4().to_string().into(),
            p.slug.clone().into(),
            p.title.clone().into(),
            p.summary.clone().map(|v| v.into()).unwrap_or(worker::wasm_bindgen::JsValue::NULL),
            p.body_adoc.clone().into(),
            status_str.into(),
        ])
        .map_err(WriteError::Internal)?;

    match insert.first::<Post>(None).await.map_err(WriteError::Internal)? {
        Some(post) => Ok(post),
        None => Err(WriteError::Conflict),
    }
}

/// The outcome of an [`update_post`] call.
pub enum UpdateResult {
    /// The row was updated. Contains the refreshed post.
    Updated(Post),
    /// No row with the given slug exists.
    NotFound,
    /// The row exists but its `revision_no` does not match the one
    /// supplied by the caller, indicating a concurrent edit.
    Conflict,
}

/// Partially updates a post identified by `slug`.
///
/// Only the fields present (`Some`) in `payload` are written; absent
/// fields keep their current value via `COALESCE`. To explicitly clear
/// a nullable field (e.g. set `summary` to `NULL`), send the field as
/// `""` (empty string) — the UPDATE statement maps empty strings to
/// `NULL` for nullable text columns. `updated_at` is refreshed and
/// `revision_no` is bumped on every successful update.
///
/// If the post transitions to [`crate::dto::PostStatus::Public`] and
/// `published_at` is still `NULL`, it is set to the current UTC
/// timestamp.
///
/// # Errors
///
/// Returns a [`worker::Error`] on validation failure, binding,
/// execution, or deserialisation failure.
pub async fn update_post(
    env: &Env,
    slug: &str,
    payload: &UpdatePost,
) -> Result<UpdateResult, WriteError> {
    if let Err(msg) = validate_update_post_input(slug, payload) {
        return Err(WriteError::Validation(msg.into()));
    }

    let db = env.d1("DB").map_err(WriteError::Internal)?;

    let status_val = payload.status.map(crate::dto::PostStatus::as_db_str);
    let public_status = crate::dto::PostStatus::Public.as_db_str();

    // COALESCE(?N, col) keeps the existing value when the param is NULL.
    // NULLIF(..., '') converts empty strings to NULL for nullable columns,
    // allowing callers to explicitly clear `summary` by sending `""`.
    // published_at is set on the first transition to 'public'.
    let update = db
        .prepare(
            r#"
            UPDATE posts
            SET slug = COALESCE(?1, slug),
                title = COALESCE(?2, title),
                summary = CASE
                    WHEN ?3 IS NOT NULL THEN NULLIF(?3, '')
                    ELSE summary
                END,
                body_adoc = COALESCE(?4, body_adoc),
                status = COALESCE(?5, status),
                updated_at = STRFTIME('%Y-%m-%dT%H:%M:%SZ', 'now'),
                revision_no = revision_no + 1,
                published_at = CASE
                    WHEN COALESCE(?5, status) = ?8 AND published_at IS NULL
                    THEN STRFTIME('%Y-%m-%dT%H:%M:%SZ', 'now')
                    ELSE published_at
                END
            WHERE slug = ?6
                AND revision_no = ?7
            "#,
        )
        .bind(&[
            payload.slug.clone().map(|v| v.into()).unwrap_or(worker::wasm_bindgen::JsValue::NULL),
            payload.title.clone().map(|v| v.into()).unwrap_or(worker::wasm_bindgen::JsValue::NULL),
            payload
                .summary
                .clone()
                .map(|v| v.into())
                .unwrap_or(worker::wasm_bindgen::JsValue::NULL),
            payload
                .body_adoc
                .clone()
                .map(|v| v.into())
                .unwrap_or(worker::wasm_bindgen::JsValue::NULL),
            status_val.map(|v| v.into()).unwrap_or(worker::wasm_bindgen::JsValue::NULL),
            slug.into(),
            payload.revision_no.into(),
            public_status.into(),
        ])
        .map_err(WriteError::Internal)?;

    let result = match update.run().await {
        Ok(result) => result,
        Err(err) if is_slug_conflict(&err) => return Err(WriteError::Conflict),
        Err(err) => return Err(WriteError::Internal(err)),
    };

    let changes = result.meta().ok().flatten().and_then(|m| m.changes).unwrap_or(0);

    if changes == 0 {
        // Distinguish "slug not found" from "revision mismatch".
        let exists = db
            .prepare(
                r#"
                SELECT 1
                AS post_exists
                FROM posts
                WHERE slug = ?1
                LIMIT 1
                "#,
            )
            .bind(&[slug.into()])
            .map_err(WriteError::Internal)?
            .first::<i64>(Some("post_exists"))
            .await
            .map_err(WriteError::Internal)?;

        return if exists.is_some() {
            Ok(UpdateResult::Conflict)
        } else {
            Ok(UpdateResult::NotFound)
        };
    }

    let read_slug = payload.slug.as_deref().unwrap_or(slug);
    let post = db
        .prepare(format!(
            r#"
            SELECT {POST_COLUMNS}
            FROM posts
            WHERE slug = ?1
            "#,
        ))
        .bind(&[read_slug.into()])
        .map_err(WriteError::Internal)?
        .first::<Post>(None)
        .await
        .map_err(WriteError::Internal)?
        .ok_or_else(|| {
            WriteError::Internal(worker::Error::RustError(
                "updated row not found on read-back".into(),
            ))
        })?;

    Ok(UpdateResult::Updated(post))
}

fn is_slug_conflict(err: &worker::Error) -> bool {
    match err {
        worker::Error::D1(d1) => d1.cause().contains("UNIQUE constraint failed: posts.slug"),
        _ => false,
    }
}

/// The outcome of a [`trash_post`] call.
pub enum TrashResult {
    /// The row was moved to the trash.
    Trashed,
    /// No row with the given slug exists.
    NotFound,
    /// The row exists but its `revision_no` does not match.
    Conflict,
}

/// Moves a post to the trash by slug and revision number.
///
/// `revision_no` is required to prevent accidental deletion of a post
/// that has been edited since the caller last read it.
///
/// # Errors
///
/// Returns a [`worker::Error`] on binding or execution failure.
pub async fn trash_post(env: &Env, slug: &str, revision_no: i64) -> worker::Result<TrashResult> {
    let db = env.d1("DB")?;
    let trashed_status = crate::dto::PostStatus::Trashed.as_db_str();
    let result = db
        .prepare(
            r#"
            UPDATE posts
            SET status = ?1,
                updated_at = STRFTIME('%Y-%m-%dT%H:%M:%SZ', 'now'),
                revision_no = revision_no + 1
            WHERE slug = ?2
                AND revision_no = ?3
                AND status != ?1
            "#,
        )
        .bind(&[trashed_status.into(), slug.into(), revision_no.into()])?
        .run()
        .await?;

    let changes = result.meta().ok().flatten().and_then(|m| m.changes).unwrap_or(0);

    if changes > 0 {
        return Ok(TrashResult::Trashed);
    }

    let row = db
        .prepare(
            r#"
            SELECT revision_no, status
            FROM posts
            WHERE slug = ?1
            LIMIT 1
            "#,
        )
        .bind(&[slug.into()])?
        .first::<serde_json::Value>(None)
        .await?;

    let Some(row) = row else {
        return Ok(TrashResult::NotFound);
    };

    let current_revision =
        row.get("revision_no").and_then(|value| value.as_i64()).unwrap_or_default();
    let current_status = row.get("status").and_then(|value| value.as_str()).unwrap_or_default();

    if current_status == trashed_status && current_revision == revision_no {
        Ok(TrashResult::Trashed)
    } else {
        Ok(TrashResult::Conflict)
    }
}
