//! Thin repository layer over the Cloudflare D1 binding.

use uuid::Uuid;
use worker::Env;

use crate::dto::{CreatePost, Post, PostSummary, UpdatePost};
use crate::validation::{validate_create_post_input, validate_update_post_input};

pub enum WriteError {
    Validation(String),
    Conflict,
    #[allow(dead_code)]
    Internal(worker::Error),
}

const POST_COLUMNS: &str = "\
    id, public_id, slug, title, summary, body_adoc, status, \
    published_at, created_at, updated_at, revision_no";

const POST_SUMMARY_COLUMNS: &str = "public_id, slug, title, summary, published_at";

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

pub enum UpdateResult {
    Updated(Post),
    NotFound,
    Conflict,
}

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

pub enum TrashResult {
    Trashed,
    NotFound,
    Conflict,
}

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
