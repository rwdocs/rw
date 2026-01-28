//! Confluence API types.

mod attachment;
mod comment;
mod page;

pub use attachment::{Attachment, AttachmentsResponse};
pub use comment::CommentsResponse;
pub use page::Page;
