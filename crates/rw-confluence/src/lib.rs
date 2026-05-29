//! Confluence rendering for RW.
//!
//! Converts `CommonMark` markdown to Confluence storage-format XHTML and
//! produces a publish-ready bundle on disk (page body + diagram PNGs).
//! Optional inline-comment-marker preservation carries
//! `<ac:inline-comment-marker>` tags from the current page's XHTML into
//! the freshly rendered XHTML.
//!
//! This crate does **not** talk to the Confluence REST API. Publishing
//! is the caller's responsibility — point `rw confluence render` (or this
//! library) at your markdown, then upload `<out>/page.xhtml` and the
//! PNGs in `<out>/` with the publisher of your choice.
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//! use rw_confluence::{render, RenderOptions};
//!
//! let output = render(
//!     "# Hello\n\nA paragraph.\n",
//!     Path::new("./dist"),
//!     RenderOptions {
//!         extract_title: true,
//!         prepend_toc: true,
//!         ..RenderOptions::default()
//!     },
//! ).expect("render");
//!
//! assert_eq!(output.title.as_deref(), Some("Hello"));
//! ```

mod backend;
mod renderer;
mod tags;

mod comment_preservation;
pub use comment_preservation::{PreserveResult, UnmatchedComment, preserve_comments};

mod render;
pub use render::{RenderOptions, RenderOutput, render};

mod error;
pub use error::{CommentPreservationError, ConfluenceError};
