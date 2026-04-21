use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Identity of a comment's author.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

impl Author {
    /// The default identity stamped on comments when the caller doesn't supply
    /// one — shared across `rw serve`, the `rw comment` CLI, and any other
    /// in-process consumer.
    #[must_use]
    pub fn local_human() -> Self {
        Self {
            id: "local:human".to_owned(),
            name: "You".to_owned(),
            avatar_url: None,
        }
    }
}

/// A selector that identifies a text range within a document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Selector {
    TextQuoteSelector {
        exact: String,
        prefix: String,
        suffix: String,
    },
    TextPositionSelector {
        start: usize,
        end: usize,
    },
    CSSSelector {
        value: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CommentStatus {
    Open,
    Resolved,
}

impl CommentStatus {
    /// The canonical string form stored in the database and used in query
    /// parameters. Matches the lowercase serde representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            CommentStatus::Open => "open",
            CommentStatus::Resolved => "resolved",
        }
    }
}

/// Returned when a string does not match any [`CommentStatus`] variant.
#[derive(Debug, thiserror::Error)]
#[error("unknown comment status: {0}")]
pub struct ParseCommentStatusError(pub String);

impl FromStr for CommentStatus {
    type Err = ParseCommentStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(CommentStatus::Open),
            "resolved" => Ok(CommentStatus::Resolved),
            other => Err(ParseCommentStatusError(other.to_owned())),
        }
    }
}

/// A comment attached to a document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: Uuid,
    pub document_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Uuid>,
    pub author: Author,
    pub body: String,
    pub selectors: Vec<Selector>,
    pub status: CommentStatus,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for creating a new comment at the storage layer. Selectors are
/// already resolved; see [`NewComment`] for the higher-level flow that takes a
/// `quote` string instead. When `author` is `None` the store stamps
/// [`Author::local_human`].
#[derive(Debug)]
pub struct CreateComment {
    pub document_id: String,
    pub parent_id: Option<Uuid>,
    pub author: Option<Author>,
    pub body: String,
    pub selectors: Vec<Selector>,
}

/// Input for the high-level creation flow ([`crate::create_comment`]). Accepts
/// either a pre-resolved `selectors` vector *or* a `quote` string that the
/// flow resolves against a rendered [`Site`](rw_site::Site); supplying both is
/// a client error.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewComment {
    pub document_id: String,
    pub parent_id: Option<Uuid>,
    pub author: Option<Author>,
    pub body: String,
    pub selectors: Option<Vec<Selector>>,
    pub quote: Option<String>,
}

/// Input for updating an existing comment.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateComment {
    pub body: Option<String>,
    pub status: Option<CommentStatus>,
    pub selectors: Option<Vec<Selector>>,
}

/// Filter criteria for listing comments. `parent_id` wins over
/// `top_level_only` when both are set; leave both at their defaults to include
/// every comment regardless of depth.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentFilter {
    pub document_id: Option<String>,
    pub status: Option<CommentStatus>,
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub top_level_only: bool,
}
