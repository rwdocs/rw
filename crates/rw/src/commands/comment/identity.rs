use rw_comments::Author;

use crate::error::CliError;

/// Resolve the author claim for a CLI-created comment.
///
/// Returns:
/// - the explicit [`Author`] when both id and name are provided;
/// - the default [`Author::local_ai`] identity when neither is provided — the
///   `rw comment` CLI's primary user is an LLM agent, so an unattributed
///   comment is stamped as AI rather than as a human;
/// - [`CliError::Validation`] when exactly one of the two is provided.
pub(crate) fn resolve_author(id: Option<&str>, name: Option<&str>) -> Result<Author, CliError> {
    match (id, name) {
        (Some(id), Some(name)) => Ok(Author {
            id: id.to_owned(),
            name: name.to_owned(),
            avatar_url: None,
        }),
        (None, None) => Ok(Author::local_ai()),
        _ => Err(CliError::Validation(
            "--author-id and --author-name must be set together (or both left unset \
             to use the default `local:ai` identity)"
                .to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::assert_matches;

    #[test]
    fn both_set_returns_author() {
        let author = resolve_author(Some("local:claude-code"), Some("Claude Code")).unwrap();
        assert_eq!(
            author,
            Author {
                id: "local:claude-code".to_owned(),
                name: "Claude Code".to_owned(),
                avatar_url: None,
            }
        );
    }

    #[test]
    fn neither_set_returns_local_ai() {
        assert_eq!(resolve_author(None, None).unwrap(), Author::local_ai());
    }

    #[test]
    fn id_without_name_is_error() {
        assert_matches!(
            resolve_author(Some("local:claude-code"), None),
            Err(CliError::Validation(_))
        );
    }

    #[test]
    fn name_without_id_is_error() {
        assert_matches!(
            resolve_author(None, Some("Claude Code")),
            Err(CliError::Validation(_))
        );
    }
}
