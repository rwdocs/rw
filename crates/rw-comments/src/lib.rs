//! Inline comment storage for RW documentation engine.

mod anchoring;
mod creation;
mod error;
mod html_text;
mod model;
mod sqlite;

pub use anchoring::QuoteResolutionError;
pub use creation::create_comment;
pub use error::{CreateError, StoreError};
pub use model::{
    Author, Comment, CommentFilter, CommentStatus, CreateComment, NewComment,
    ParseCommentStatusError, Selector, UpdateComment,
};
pub use sqlite::SqliteCommentStore;
