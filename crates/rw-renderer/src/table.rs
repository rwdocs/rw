//! Walker-private state for tracking table rendering.

use pulldown_cmark::Alignment;

/// State for tracking table rendering.
#[derive(Default)]
pub(crate) struct TableState {
    /// Whether we're inside the table header row.
    in_head: bool,
    /// Column alignments for current table.
    alignments: Vec<Alignment>,
    /// Current column index in table row.
    cell_index: usize,
}

impl TableState {
    /// Start a new table with column alignments.
    pub fn start(&mut self, alignments: Vec<Alignment>) {
        self.alignments = alignments;
        self.in_head = false;
        self.cell_index = 0;
    }

    /// Start the table header row.
    pub fn start_head(&mut self) {
        self.in_head = true;
        self.cell_index = 0;
    }

    /// End the table header row.
    pub fn end_head(&mut self) {
        self.in_head = false;
    }

    /// Start a new table row.
    pub fn start_row(&mut self) {
        self.cell_index = 0;
    }

    /// Move to the next cell.
    pub fn next_cell(&mut self) {
        self.cell_index += 1;
    }

    /// Check if we're in the table header.
    pub fn is_in_head(&self) -> bool {
        self.in_head
    }

    /// Get the alignment for the current cell.
    pub fn current_alignment(&self) -> Option<Alignment> {
        self.alignments.get(self.cell_index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_state() {
        let mut state = TableState::default();
        state.start(vec![Alignment::Left, Alignment::Center, Alignment::Right]);

        state.start_head();
        assert!(state.is_in_head());
        assert_eq!(state.current_alignment(), Some(Alignment::Left));

        state.next_cell();
        assert_eq!(state.current_alignment(), Some(Alignment::Center));

        state.next_cell();
        assert_eq!(state.current_alignment(), Some(Alignment::Right));

        state.end_head();
        assert!(!state.is_in_head());
    }
}
