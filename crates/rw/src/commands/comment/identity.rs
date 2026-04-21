use rw_comments::Author;

use crate::error::CliError;

/// Resolve an optional author claim.
///
/// Returns:
/// - `Ok(Some(Author))` when both id and name are provided.
/// - `Ok(None)` when neither is provided (callers fall back to
///   [`Author::local_human`]).
/// - `Err(CliError::Validation)` when exactly one of the two is provided.
pub(crate) fn resolve_author(
    id: Option<&str>,
    name: Option<&str>,
) -> Result<Option<Author>, CliError> {
    match (id, name) {
        (Some(id), Some(name)) => Ok(Some(Author {
            id: id.to_owned(),
            name: name.to_owned(),
            avatar_url: None,
        })),
        (None, None) => Ok(None),
        _ => Err(CliError::Validation(
            "--author-id and --author-name must be set together (or both left unset \
             to use the default `local:human` identity)"
                .to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn both_set_returns_author() {
        let author = resolve_author(Some("local:claude-code"), Some("Claude Code")).unwrap();
        assert_eq!(
            author,
            Some(Author {
                id: "local:claude-code".to_owned(),
                name: "Claude Code".to_owned(),
                avatar_url: None,
            })
        );
    }

    #[test]
    fn neither_set_returns_none() {
        assert_eq!(resolve_author(None, None).unwrap(), None);
    }

    #[test]
    fn id_without_name_is_error() {
        assert!(matches!(
            resolve_author(Some("local:claude-code"), None),
            Err(CliError::Validation(_))
        ));
    }

    #[test]
    fn name_without_id_is_error() {
        assert!(matches!(
            resolve_author(None, Some("Claude Code")),
            Err(CliError::Validation(_))
        ));
    }
}
