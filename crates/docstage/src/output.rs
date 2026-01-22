//! Colored terminal output utilities.

use console::{Style, Term};

/// Terminal output formatter.
pub struct Output {
    term: Term,
    green: Style,
    yellow: Style,
    red: Style,
    cyan_bold: Style,
}

impl Output {
    /// Create a new output formatter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            term: Term::stderr(),
            green: Style::new().green(),
            yellow: Style::new().yellow(),
            red: Style::new().red(),
            cyan_bold: Style::new().cyan().bold(),
        }
    }

    /// Print an info message.
    pub fn info(&self, msg: &str) {
        let _ = self.term.write_line(msg);
    }

    /// Print a success message (green).
    pub fn success(&self, msg: &str) {
        let _ = self.term.write_line(&self.green.apply_to(msg).to_string());
    }

    /// Print a warning message (yellow).
    pub fn warning(&self, msg: &str) {
        let _ = self.term.write_line(&self.yellow.apply_to(msg).to_string());
    }

    /// Print an error message (red).
    pub fn error(&self, msg: &str) {
        let _ = self.term.write_line(&self.red.apply_to(msg).to_string());
    }

    /// Print a highlighted message (cyan bold).
    pub fn highlight(&self, msg: &str) {
        let _ = self
            .term
            .write_line(&self.cyan_bold.apply_to(msg).to_string());
    }

    /// Print a separator line.
    pub fn separator(&self) {
        let _ = self.term.write_line(&"=".repeat(70));
    }
}

impl Default for Output {
    fn default() -> Self {
        Self::new()
    }
}
