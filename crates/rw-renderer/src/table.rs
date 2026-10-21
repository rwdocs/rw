//! Walker-private state for tracking table rendering.

use rw_parser::Alignment;

#[derive(Default)]
pub(crate) struct TableState {
    in_head: bool,
    alignments: Vec<Alignment>,
    cell_index: usize,
}

impl TableState {
    pub fn start(&mut self, alignments: Vec<Alignment>) {
        self.alignments = alignments;
        self.in_head = false;
        self.cell_index = 0;
    }

    pub fn start_head(&mut self) {
        self.in_head = true;
        self.cell_index = 0;
    }

    pub fn end_head(&mut self) {
        self.in_head = false;
    }

    pub fn start_row(&mut self) {
        self.cell_index = 0;
    }

    pub fn next_cell(&mut self) {
        self.cell_index += 1;
    }

    pub fn is_in_head(&self) -> bool {
        self.in_head
    }

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
