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
            "{}  {}  {}  {}  {}",
            comment.id,
            comment.document_id,
            comment.status,
            short_timestamp(&comment.created_at),
            comment.author.name,
        )?;
        if let Some(quote) = first_quote(&comment.selectors) {
            writeln!(out, "  \"{quote}\"")?;
        } else {
            writeln!(out, "  (page comment)")?;
        }
        for line in comment.body.lines() {
            writeln!(out, "  > {line}")?;
        }
        writeln!(out)?;
    }
    Ok(())
}

pub(super) fn write_show(
    out: &mut dyn Write,
    parent: &Comment,
    replies: &[Comment],
) -> io::Result<()> {
    writeln!(out, "Thread {} ({})", parent.id, parent.status)?;
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
            reply.author.name, reply.created_at, reply.status
        )?;
        for line in reply.body.lines() {
            writeln!(out, "     {line}")?;
        }
    }
    Ok(())
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
            // A quote can span lines; a raw newline would break the indent.
            let flat = exact.split_whitespace().collect::<Vec<_>>().join(" ");
            let mut chars = flat.chars();
            let mut quote: String = chars.by_ref().take(60).collect();
            if chars.next().is_some() {
                quote.push('…');
            }
            Some(quote)
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use rw_comments::{Author, CommentStatus};
    use uuid::Uuid;

    use super::*;

    fn comment(id: &str, document_id: &str, body: &str, selectors: Vec<Selector>) -> Comment {
        Comment {
            id: Uuid::parse_str(id).unwrap(),
            document_id: document_id.to_owned(),
            parent_id: None,
            author: Author::local_ai(),
            body: body.to_owned(),
            selectors,
            status: CommentStatus::Open,
            created_at: "2026-04-17T13:22:54.757497+00:00".to_owned(),
            updated_at: "2026-04-17T13:22:54.757497+00:00".to_owned(),
            deleted_at: None,
        }
    }

    fn quote(exact: &str) -> Vec<Selector> {
        vec![Selector::TextQuoteSelector {
            exact: exact.to_owned(),
            prefix: String::new(),
            suffix: String::new(),
        }]
    }

    #[test]
    fn write_list_text_output() {
        let page = comment(
            "0193c1f2-0000-7000-8000-000000000001",
            "section:default/root#",
            "Looks good.",
            vec![],
        );
        let mut resolved = comment(
            "0193c1f2-0000-7000-8000-000000000002",
            "section:default/billing-platform#guides/payments/refunds-and-chargebacks",
            "First line.\nSecond line.",
            quote(
                "Refunds are issued to the original\n  payment method within five business days.",
            ),
        );
        resolved.status = CommentStatus::Resolved;
        let mut reply = comment(
            "0193c1f2-0000-7000-8000-000000000003",
            "section:default/root#guide",
            "Agreed.",
            quote("quick brown fox"),
        );
        reply.parent_id = Some(resolved.id);
        reply.author = Author::local_human();

        let mut out = Vec::new();
        write_list(&mut out, &[page, resolved, reply]).unwrap();

        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\
0193c1f2-0000-7000-8000-000000000001  section:default/root#  open  2026-04-17 13:22  AI
  (page comment)
  > Looks good.

0193c1f2-0000-7000-8000-000000000002  section:default/billing-platform#guides/payments/refunds-and-chargebacks  resolved  2026-04-17 13:22  AI
  \"Refunds are issued to the original payment method within fiv…\"
  > First line.
  > Second line.

0193c1f2-0000-7000-8000-000000000003  section:default/root#guide  open  2026-04-17 13:22  You
  \"quick brown fox\"
  > Agreed.

"
        );
    }

    #[test]
    fn first_quote_marks_only_actual_truncation() {
        let sixty = "é".repeat(60);
        assert_eq!(first_quote(&quote(&sixty)), Some(sixty.clone()));
        assert_eq!(
            first_quote(&quote(&format!("{sixty}é"))),
            Some(format!("{sixty}…"))
        );
        let fifty_eight = "é".repeat(58);
        assert_eq!(
            first_quote(&quote(&format!("{fifty_eight}\n\n é"))),
            Some(format!("{fifty_eight} é")),
        );
    }
}
