//! Confluence API types.

mod attachment;
mod comment;
mod page;

pub use attachment::{Attachment, AttachmentsResponse};
pub use comment::{Comment, CommentsResponse, Extensions, InlineProperties, Resolution};
pub use page::{Body, Links, Page, Storage, Version};
