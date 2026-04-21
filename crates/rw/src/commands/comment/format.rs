use std::io::{self, Write};

use rw_comments::{Comment, Selector};
use serde::Serialize;

use super::OutputFormat;

pub(super) fn print<T: Serialize + ?Sized>(
    format: OutputFormat,
    text_writer: impl FnOnce(&mut dyn Write) -> io::Result<()>,
    json_value: &T,
) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match format {
        OutputFormat::Text => text_writer(&mut out),
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut out, json_value).map_err(io::Error::other)?;
            writeln!(out)
        }
    }
}

pub(super) fn write_list(out: &mut dyn Write, comments: &[Comment]) -> io::Result<()> {
    for comment in comments {
        writeln!(
            out,
            "{:<10}{:<22}{:<10}{:<18}  {}",
            short_id(comment),
            comment.document_id,
            comment.status.as_str(),
            short_timestamp(&comment.created_at),
            comment.author.name,
        )?;
        if let Some(quote) = first_quote(&comment.selectors) {
            writeln!(out, "          \"{quote}…\"")?;
        } else {
            writeln!(out, "          (page comment)")?;
        }
        writeln!(out, "          > {}", comment.body)?;
        writeln!(out)?;
    }
    Ok(())
}

pub(super) fn write_show(
    out: &mut dyn Write,
    parent: &Comment,
    replies: &[Comment],
) -> io::Result<()> {
    writeln!(out, "Thread {} ({})", parent.id, parent.status.as_str())?;
    writeln!(out, "Document: {}", parent.document_id)?;
    writeln!(out, "Author:   {}", parent.author.name)?;
    writeln!(out, "Created:  {}", parent.created_at)?;
    if let Some(quote) = first_quote(&parent.selectors) {
        writeln!(out, "Quote:    \"{quote}\"")?;
    } else {
        writeln!(out, "(page comment)")?;
    }
    writeln!(out)?;
    writeln!(out, "{}", parent.body)?;
    for reply in replies {
        writeln!(out)?;
        writeln!(
            out,
            "  └─ {} ({}) — {}",
            reply.author.name,
            reply.created_at,
            reply.status.as_str()
        )?;
        for line in reply.body.lines() {
            writeln!(out, "     {line}")?;
        }
    }
    Ok(())
}

fn short_id(comment: &Comment) -> String {
    format!("{:.8}", comment.id)
}

/// Trim an RFC-3339 timestamp like `2026-04-17T13:22:54.757497+00:00` down to
/// `2026-04-17 13:22` for the list view. Falls back to the raw value if the
/// prefix doesn't look right.
fn short_timestamp(raw: &str) -> String {
    if raw.len() >= 16 && raw.as_bytes().get(10) == Some(&b'T') {
        format!("{} {}", &raw[..10], &raw[11..16])
    } else {
        raw.to_owned()
    }
}

fn first_quote(selectors: &[Selector]) -> Option<String> {
    selectors.iter().find_map(|s| match s {
        Selector::TextQuoteSelector { exact, .. } => {
            Some(exact.chars().take(60).collect::<String>())
        }
        _ => None,
    })
}
