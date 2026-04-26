//! Data-transfer objects that cross the HTTP boundary.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Post {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "string")]
    pub public_id: uuid::Uuid,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub body_adoc: String,
    pub status: PostStatus,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[ts(type = "number")]
    pub revision_no: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PostSummary {
    #[ts(type = "string")]
    pub public_id: uuid::Uuid,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreatePost {
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub body_adoc: String,
    pub status: PostStatus,
}

#[derive(Debug, Deserialize, TS)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FirebaseSessionRequest {
    pub id_token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename_all = "lowercase")]
pub enum PostStatus {
    Draft,
    Private,
    Public,
    Trashed,
}

impl PostStatus {
    pub const fn as_db_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Private => "private",
            Self::Public => "public",
            Self::Trashed => "trashed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionIssuedResponse {
    pub ok: bool,
    pub session: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
