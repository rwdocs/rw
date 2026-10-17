//! Error-chain formatting shared by RW's crates.
//!
//! RW's errors follow one convention: an error's `Display` is its own message
//! only, and its cause is reachable through [`Error::source`]. A wrapper that
//! adds nothing is `#[error(transparent)]`. The chain is joined once, by
//! [`error_chain`], where an error leaves the program: the CLI's `Error:` line,
//! an HTTP error body, a JS exception, a log line.
//!
//! An external error whose `Display` already embeds its cause breaks the
//! convention; wrap it so that it is not exposed as a `source()`, or the cause
//! prints twice.

use std::error::Error;

/// Format `err` and its whole `source()` chain as `"err: cause: root"`.
#[must_use]
pub fn error_chain(err: &dyn Error) -> String {
    let mut msg = err.to_string();
    let mut next = err.source();
    while let Some(cause) = next {
        msg.push_str(": ");
        msg.push_str(&cause.to_string());
        next = cause.source();
    }
    msg
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use super::*;

    #[derive(Debug)]
    struct Layer(&'static str, Option<Box<Layer>>);

    impl fmt::Display for Layer {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0)
        }
    }

    impl Error for Layer {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.1.as_deref().map(|e| e as &(dyn Error + 'static))
        }
    }

    #[test]
    fn an_error_without_a_source_is_its_message() {
        assert_eq!(error_chain(&Layer("disk gone", None)), "disk gone");
    }

    #[test]
    fn every_cause_is_joined_in_order() {
        let err = Layer(
            "dispatch failure",
            Some(Box::new(Layer(
                "connection refused",
                Some(Box::new(Layer("os error 61", None))),
            ))),
        );
        assert_eq!(
            error_chain(&err),
            "dispatch failure: connection refused: os error 61"
        );
    }
}
