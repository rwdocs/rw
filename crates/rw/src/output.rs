//! Colored terminal output utilities.

use console::{Style, Term};

/// Terminal output formatter.
pub(crate) struct Output {
    term: Term,
    green: Style,
    yellow: Style,
    red: Style,
    cyan_bold: Style,
}

impl Output {
    /// Create a new output formatter.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            term: Term::stderr(),
            green: Style::new().green(),
            yellow: Style::new().yellow(),
            red: Style::new().red(),
            cyan_bold: Style::new().cyan().bold(),
        }
    }

    /// Print an info message.
    pub(crate) fn info(&self, msg: &str) {
        let _ = self.term.write_line(msg);
    }

    /// Print a success message (green).
    pub(crate) fn success(&self, msg: &str) {
        let _ = self.term.write_line(&self.green.apply_to(msg).to_string());
    }

    /// Print a warning message (yellow).
    pub(crate) fn warning(&self, msg: &str) {
        let _ = self.term.write_line(&self.yellow.apply_to(msg).to_string());
    }

    /// Print an error message (red).
    pub(crate) fn error(&self, msg: &str) {
        let _ = self.term.write_line(&self.red.apply_to(msg).to_string());
    }

    /// Print a highlighted message (cyan bold).
    pub(crate) fn highlight(&self, msg: &str) {
        let _ = self
            .term
            .write_line(&self.cyan_bold.apply_to(msg).to_string());
    }

    /// Print a separator line.
    pub(crate) fn separator(&self) {
        let _ = self.term.write_line(&"=".repeat(70));
    }
}
